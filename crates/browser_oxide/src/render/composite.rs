//! The compositor: rasterize layers once, then move them.
//!
//! This is what makes scrolling cheap. A scroll changes no layer's *content*,
//! so a compositor that keeps each layer's rasterized surface can answer the
//! next frame by blitting at a new offset — no paint, no shaping, no Skia.
//! Without it every scroll frame re-rasterizes the page, which on a CPU
//! surface is ~90% of the frame budget spent redrawing pixels that did not
//! change.
//!
//! Damage is decided by comparing each layer's display list against the one it
//! was last rasterized from. That comparison is only cheap because the display
//! list is flat and holds no DOM references — which is the reason it is
//! structured that way.

use std::collections::HashMap;

use skia_safe::{surfaces, AlphaType, ColorType, ImageInfo, Paint, SamplingOptions};

use super::display_list::{DisplayList, Rgba};
use super::layer::{Layer, LayerId, LayerTree};
use super::raster::Target;

/// What the compositor did this frame. The numbers exist so "is compositing
/// actually helping" is answerable rather than assumed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CompositeStats {
    pub layers: usize,
    /// Layers rasterized this frame because their content changed.
    pub rasterized: usize,
    /// Layers served from a cached surface.
    pub reused: usize,
    /// Layers skipped because they fall outside the viewport.
    pub culled: usize,
}

impl CompositeStats {
    pub fn summary(&self) -> String {
        format!(
            "{} layers: {} rasterized, {} reused, {} culled",
            self.layers, self.rasterized, self.reused, self.culled
        )
    }
}

/// A rasterized layer, kept between frames.
struct CachedSurface {
    /// The list this was rasterized from. Compared by content, not identity —
    /// a relayout rebuilds the list even when nothing about it changed.
    list: DisplayList,
    width: u32,
    height: u32,
    /// Premultiplied RGBA8, the layer's own coordinate space.
    pixels: Vec<u8>,
}

/// Keeps rasterized layers across frames.
#[derive(Default)]
pub struct Compositor {
    surfaces: HashMap<LayerId, CachedSurface>,
}

impl Compositor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every cached surface. Call when the document changes wholesale.
    pub fn invalidate_all(&mut self) {
        self.surfaces.clear();
    }

    pub fn cached_layers(&self) -> usize {
        self.surfaces.len()
    }

    /// Composite `tree` into a viewport of `width` x `height`, scrolled to
    /// `scroll_y`.
    ///
    /// Layers whose content is unchanged since the last call are reused rather
    /// than rasterized — which is the point, and which is why scrolling is
    /// nearly free.
    pub fn composite(
        &mut self,
        tree: &LayerTree,
        scroll_y: f32,
        width: u32,
        height: u32,
    ) -> (Target, CompositeStats) {
        let mut stats = CompositeStats {
            layers: tree.layers.len(),
            ..Default::default()
        };
        let mut out = Target::new(width, height);
        if width == 0 || height == 0 {
            return (out, stats);
        }

        let scroll_y = scroll_y.max(0.0);
        // Drop surfaces for layers that no longer exist, so a long-lived
        // compositor over a changing document does not grow without bound.
        let live: std::collections::HashSet<LayerId> = tree.layers.iter().map(|l| l.id).collect();
        self.surfaces.retain(|id, _| live.contains(id));

        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = width as usize * 4;
        let Some(mut surface) =
            surfaces::wrap_pixels(&info, &mut out.pixels, Some(row_bytes), None)
        else {
            return (out, stats);
        };
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);

        for layer in &tree.layers {
            // A layer's on-screen position: its own transform, then the scroll
            // offset if it participates in scrolling. Fixed layers do not.
            let offset_y = if layer.scrolls { -scroll_y } else { 0.0 };
            let placed = layer
                .transform
                .then(&super::layer::Transform2D::translate(0.0, offset_y));
            let on_screen = placed.transform_rect(&layer.bounds);

            // Cull anything entirely outside the viewport. On a long document
            // this is most of it.
            if on_screen.bottom() < 0.0
                || on_screen.y > height as f32
                || on_screen.right() < 0.0
                || on_screen.x > width as f32
            {
                stats.culled += 1;
                continue;
            }

            let (w, h) = layer_surface_size(layer, width, height);
            if w == 0 || h == 0 {
                continue;
            }

            // Reuse only if the surface is the right size *and* the drawing is
            // unchanged. Size alone is not enough and content alone is not
            // either — a resized viewport changes the root layer's surface
            // without changing a single display item.
            let reused = matches!(
                self.surfaces.get(&layer.id),
                Some(cached)
                    if cached.width == w
                        && cached.height == h
                        && lists_equal(&cached.list, &layer.display_list)
            );
            if reused {
                stats.reused += 1;
            } else {
                stats.rasterized += 1;
                let mut target = Target::new(w, h);
                // The list holds page coordinates; the surface starts at the
                // layer's own origin.
                target.paint_translated(&layer.display_list, -layer.bounds.x, -layer.bounds.y);
                self.surfaces.insert(
                    layer.id,
                    CachedSurface {
                        list: layer.display_list.clone(),
                        width: w,
                        height: h,
                        pixels: target.pixels,
                    },
                );
            }

            let Some(cached) = self.surfaces.get(&layer.id) else {
                continue;
            };
            let Some(image) = image_from_pixels(cached) else {
                continue;
            };

            canvas.save();
            canvas.concat(&skia_safe::Matrix::new_all(
                placed.a, placed.c, placed.e, placed.b, placed.d, placed.f, 0.0, 0.0, 1.0,
            ));
            let mut paint = Paint::default();
            if layer.opacity < 1.0 {
                paint.set_alpha_f(layer.opacity.clamp(0.0, 1.0));
            }
            canvas.draw_image_with_sampling_options(
                &image,
                (layer.bounds.x, layer.bounds.y),
                SamplingOptions::default(),
                Some(&paint),
            );
            canvas.restore();
        }

        drop(surface);
        (out, stats)
    }
}

/// How big a surface a layer needs.
///
/// The root layer is the viewport; everything else is its own bounds, clamped
/// so one absurd element cannot ask for a gigabyte.
fn layer_surface_size(layer: &Layer, viewport_w: u32, viewport_h: u32) -> (u32, u32) {
    const MAX: f32 = 8192.0;
    match layer.reason {
        super::layer::LayerReason::Root => (viewport_w, viewport_h),
        _ => (
            layer.bounds.width.clamp(0.0, MAX).ceil() as u32,
            layer.bounds.height.clamp(0.0, MAX).ceil() as u32,
        ),
    }
}

fn image_from_pixels(cached: &CachedSurface) -> Option<skia_safe::Image> {
    let info = ImageInfo::new(
        (cached.width as i32, cached.height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let row_bytes = cached.width as usize * 4;
    let data = skia_safe::Data::new_copy(&cached.pixels);
    skia_safe::images::raster_from_data(&info, data, row_bytes)
}

/// Are two display lists the same drawing?
///
/// Structural comparison, not pointer identity: a relayout rebuilds the list
/// from scratch even when the page did not change, and treating that as damage
/// would defeat the cache entirely.
fn lists_equal(a: &DisplayList, b: &DisplayList) -> bool {
    if a.items.len() != b.items.len() {
        return false;
    }
    a.items
        .iter()
        .zip(b.items.iter())
        .all(|(x, y)| item_eq(x, y))
}

fn item_eq(a: &super::display_list::DisplayItem, b: &super::display_list::DisplayItem) -> bool {
    use super::display_list::DisplayItem as D;
    match (a, b) {
        (
            D::Rect {
                rect: r1,
                color: c1,
            },
            D::Rect {
                rect: r2,
                color: c2,
            },
        ) => r1 == r2 && c1 == c2,
        (
            D::Border {
                rect: r1,
                widths: w1,
                colors: k1,
            },
            D::Border {
                rect: r2,
                widths: w2,
                colors: k2,
            },
        ) => r1 == r2 && w1 == w2 && k1 == k2,
        (
            D::Text {
                origin: o1,
                glyphs: g1,
                font: f1,
                color: c1,
            },
            D::Text {
                origin: o2,
                glyphs: g2,
                font: f2,
                color: c2,
            },
        ) => {
            o1 == o2
                && c1 == c2
                && (f1.size_px - f2.size_px).abs() < f32::EPSILON
                && f1.face_index == f2.face_index
                && std::ptr::eq(f1.data.as_ptr(), f2.data.as_ptr())
                && g1 == g2
        }
        (D::PushClip { rect: r1 }, D::PushClip { rect: r2 }) => r1 == r2,
        (D::PopClip, D::PopClip) => true,
        _ => false,
    }
}

/// Blend a colour over white, for tests that want a plain expected value.
pub fn over_white(c: Rgba) -> (u8, u8, u8) {
    let a = f32::from(c.a) / 255.0;
    let blend = |v: u8| ((f32::from(v) * a) + 255.0 * (1.0 - a)).round() as u8;
    (blend(c.r), blend(c.g), blend(c.b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::display_list::{DisplayItem, Rect};
    use crate::render::layer::{LayerReason, Transform2D};

    fn solid_layer(id: u32, reason: LayerReason, rect: Rect, color: Rgba) -> Layer {
        let mut list = DisplayList::default();
        list.push(DisplayItem::Rect { rect, color });
        Layer {
            id: LayerId(id),
            reason,
            display_list: list,
            bounds: rect,
            transform: Transform2D::IDENTITY,
            opacity: 1.0,
            scrolls: true,
        }
    }

    fn pixel(t: &Target, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let rgba = t.to_rgba8();
        let i = ((y * t.width + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
    }

    const RED: Rgba = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };

    #[test]
    fn a_layer_reaches_the_screen() {
        let tree = LayerTree {
            layers: vec![solid_layer(
                0,
                LayerReason::Root,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 50.0,
                },
                RED,
            )],
            content_height: 50.0,
        };
        let mut c = Compositor::new();
        let (target, stats) = c.composite(&tree, 0.0, 100, 100);
        assert_eq!(pixel(&target, 10, 10), (255, 0, 0, 255));
        assert_eq!(stats.rasterized, 1);
    }

    #[test]
    fn scrolling_reuses_every_surface() {
        // The claim compositing exists to make: a scroll rasterizes nothing.
        let tree = LayerTree {
            layers: vec![solid_layer(
                0,
                LayerReason::Root,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                RED,
            )],
            content_height: 1000.0,
        };
        let mut c = Compositor::new();
        let (_, first) = c.composite(&tree, 0.0, 100, 100);
        assert_eq!(first.rasterized, 1);

        for scroll in [10.0, 20.0, 40.0] {
            let (_, s) = c.composite(&tree, scroll, 100, 100);
            assert_eq!(
                (s.rasterized, s.reused),
                (0, 1),
                "scrolling to {scroll} must not rasterize anything"
            );
        }
    }

    #[test]
    fn a_scrolled_layer_moves_and_a_fixed_one_does_not() {
        let scrolling = solid_layer(
            0,
            LayerReason::Root,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            RED,
        );
        let mut fixed = solid_layer(
            1,
            LayerReason::Fixed,
            Rect {
                x: 0.0,
                y: 80.0,
                width: 100.0,
                height: 20.0,
            },
            Rgba {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            },
        );
        fixed.scrolls = false;

        let tree = LayerTree {
            layers: vec![scrolling, fixed],
            content_height: 500.0,
        };
        let mut c = Compositor::new();

        let (before, _) = c.composite(&tree, 0.0, 100, 100);
        assert_eq!(
            pixel(&before, 50, 10),
            (255, 0, 0, 255),
            "red band at the top"
        );
        assert_eq!(
            pixel(&before, 50, 90),
            (0, 0, 255, 255),
            "blue band at the bottom"
        );

        let (after, _) = c.composite(&tree, 30.0, 100, 100);
        // White, not red: comparing only the red channel would pass on white
        // too, which is exactly the wrong thing to be reassured by.
        assert_eq!(
            pixel(&after, 50, 10),
            (255, 255, 255, 255),
            "the scrolling layer must have moved off the top"
        );
        assert_eq!(
            pixel(&after, 50, 90),
            (0, 0, 255, 255),
            "the fixed layer must not have moved"
        );
    }

    #[test]
    fn changed_content_is_rasterized_again() {
        let make = |color: Rgba| LayerTree {
            layers: vec![solid_layer(
                0,
                LayerReason::Root,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 50.0,
                },
                color,
            )],
            content_height: 50.0,
        };
        let mut c = Compositor::new();
        c.composite(&make(RED), 0.0, 100, 100);
        let (target, stats) = c.composite(
            &make(Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            }),
            0.0,
            100,
            100,
        );
        assert_eq!(stats.rasterized, 1, "a colour change is damage");
        assert_eq!(pixel(&target, 10, 10), (0, 255, 0, 255));
    }

    #[test]
    fn an_identical_relayout_is_not_damage() {
        // A relayout rebuilds the display list from scratch. Comparing by
        // identity rather than content would treat that as damage and the
        // cache would never hit.
        let tree = || LayerTree {
            layers: vec![solid_layer(
                0,
                LayerReason::Root,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 50.0,
                },
                RED,
            )],
            content_height: 50.0,
        };
        let mut c = Compositor::new();
        c.composite(&tree(), 0.0, 100, 100);
        let (_, stats) = c.composite(&tree(), 0.0, 100, 100);
        assert_eq!(stats.rasterized, 0);
        assert_eq!(stats.reused, 1);
    }

    #[test]
    fn offscreen_layers_are_culled() {
        let mut far = solid_layer(
            1,
            LayerReason::Transform,
            Rect {
                x: 0.0,
                y: 5000.0,
                width: 50.0,
                height: 50.0,
            },
            RED,
        );
        far.scrolls = true;
        let tree = LayerTree {
            layers: vec![far],
            content_height: 6000.0,
        };
        let mut c = Compositor::new();
        let (_, stats) = c.composite(&tree, 0.0, 100, 100);
        assert_eq!(stats.culled, 1);
        assert_eq!(stats.rasterized, 0, "a culled layer costs no rasterization");
    }

    #[test]
    fn opacity_blends_without_repainting() {
        let mut half = solid_layer(
            0,
            LayerReason::Opacity,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            RED,
        );
        half.opacity = 0.5;
        let tree = LayerTree {
            layers: vec![half],
            content_height: 100.0,
        };
        let mut c = Compositor::new();
        let (target, _) = c.composite(&tree, 0.0, 100, 100);
        let (r, g, b, _) = pixel(&target, 50, 50);
        // Red at 50% over white.
        assert!(
            r > 200 && g > 100 && g < 160 && b > 100 && b < 160,
            "got {r},{g},{b}"
        );
    }

    #[test]
    fn surfaces_for_removed_layers_are_dropped() {
        let two = LayerTree {
            layers: vec![
                solid_layer(
                    0,
                    LayerReason::Root,
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    RED,
                ),
                solid_layer(
                    1,
                    LayerReason::Opacity,
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    RED,
                ),
            ],
            content_height: 10.0,
        };
        let one = LayerTree {
            layers: vec![two.layers[0].clone()],
            content_height: 10.0,
        };
        let mut c = Compositor::new();
        c.composite(&two, 0.0, 100, 100);
        assert_eq!(c.cached_layers(), 2);
        c.composite(&one, 0.0, 100, 100);
        assert_eq!(
            c.cached_layers(),
            1,
            "a long-lived compositor must not accumulate dead surfaces"
        );
    }
}
