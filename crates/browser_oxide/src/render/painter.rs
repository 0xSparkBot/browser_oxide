//! Walk the box tree in paint order and emit a display list.
//!
//! Paint order is CSS 2.1 Appendix E, minimally: in-flow content in tree order
//! — background, then border, then children — and text on top of the box it
//! sits in. Stacking contexts and `z-index` are not modelled yet; getting paint
//! order wrong is the most common source of "looks subtly broken", so that gap
//! is worth stating rather than discovering.

use crate::css_cascade::ComputedStyle;
use crate::css_values::property::{CssValue, PropertyId};
use crate::css_values::types::color::Color as CssColor;
use crate::css_values::types::length::{LengthPercentage, LengthPercentageAuto};
use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;
use crate::layout::inline::InlineLayout;
use crate::layout::resolve::{resolve_length, ResolveContext};
use crate::layout::LayoutEngine;

use super::display_list::{DisplayItem, DisplayList, FontRef, Glyph, Rect, Rgba, SideOffsets};
use super::layer::{Layer, LayerId, LayerReason, LayerTree, Transform2D};

/// What the painter saw on the way through.
#[derive(Debug, Default, Clone, Copy)]
pub struct PaintStats {
    pub elements: usize,
    pub text_runs: usize,
    /// Text runs whose family could not be resolved to any face.
    pub unresolved_fonts: usize,
    /// Subtrees promoted to their own compositing layer.
    pub layers: usize,
}

/// A paintable area attributed back to the element that produced it.
///
/// The display list itself holds no DOM references — that is what makes it
/// cacheable — so hit-testing needs this parallel list. `RENDERER_DESIGN.md`
/// says hit-testing should reuse the fragment tree; there is no fragment tree
/// yet, and this is the honest interim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitRegion {
    pub rect: Rect,
    pub node: NodeId,
}

/// Build a display list for `dom`, using geometry and styles already computed
/// by `layout`.
///
/// `layout.compute()` must have run. The painter reads
/// `LayoutEngine::styles()` rather than re-resolving anything: two style
/// resolutions per element would be two chances to disagree, and a box painted
/// with a style that did not decide its geometry looks like a paint bug.
pub fn paint(
    dom: &Dom,
    layout: &mut LayoutEngine,
    inline: &mut InlineLayout,
    viewport_width: f32,
    viewport_height: f32,
) -> (DisplayList, PaintStats) {
    let mut list = DisplayList::default();
    let mut stats = PaintStats::default();

    // A browser's canvas starts white. Without this the page composites over
    // whatever was in the buffer.
    list.push(DisplayItem::Rect {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: viewport_width,
            height: viewport_height,
        },
        color: Rgba::WHITE,
    });

    let ctx = ResolveContext {
        font_size: 16.0,
        root_font_size: 16.0,
        viewport_w: viewport_width,
        viewport_h: viewport_height,
    };

    let mut painter = Painter {
        dom,
        layout,
        inline,
        ctx,
        list: &mut list,
        stats: &mut stats,
        extra_layers: Vec::new(),
        hit_regions: Vec::new(),
        next_layer_id: 1,
    };
    painter.node(NodeId::DOCUMENT);

    (list, stats)
}

/// Paint into a layer tree rather than a single list.
///
/// Subtrees are promoted when they have a reason to composite separately:
/// `opacity` below 1, a non-identity `transform`, or `position: fixed`. Every
/// promotion costs a surface, so the list of reasons is deliberately short.
pub fn paint_layered(
    dom: &Dom,
    layout: &mut LayoutEngine,
    inline: &mut InlineLayout,
    viewport_width: f32,
    viewport_height: f32,
) -> (LayerTree, Vec<HitRegion>, PaintStats) {
    let mut root_list = DisplayList::default();
    let mut stats = PaintStats::default();

    root_list.push(DisplayItem::Rect {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: viewport_width,
            height: viewport_height,
        },
        color: Rgba::WHITE,
    });

    let ctx = ResolveContext {
        font_size: 16.0,
        root_font_size: 16.0,
        viewport_w: viewport_width,
        viewport_h: viewport_height,
    };

    let mut painter = Painter {
        dom,
        layout,
        inline,
        ctx,
        list: &mut root_list,
        stats: &mut stats,
        extra_layers: Vec::new(),
        hit_regions: Vec::new(),
        next_layer_id: 1,
    };
    painter.node(NodeId::DOCUMENT);
    let extra = std::mem::take(&mut painter.extra_layers);
    let hit_regions = std::mem::take(&mut painter.hit_regions);

    // The root document height, for scroll clamping.
    let content_height = hit_regions
        .iter()
        .map(|r| r.rect.bottom())
        .fold(viewport_height, f32::max);

    let root_bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_width,
        height: viewport_height,
    };
    let mut layers = Vec::with_capacity(extra.len() + 1);
    layers.push(Layer {
        id: LayerId(0),
        reason: LayerReason::Root,
        display_list: root_list,
        bounds: root_bounds,
        transform: Transform2D::IDENTITY,
        opacity: 1.0,
        scrolls: true,
    });
    layers.extend(extra);
    stats.layers = layers.len();

    (
        LayerTree {
            layers,
            content_height,
        },
        hit_regions,
        stats,
    )
}

struct Painter<'a> {
    dom: &'a Dom,
    layout: &'a mut LayoutEngine,
    inline: &'a mut InlineLayout,
    ctx: ResolveContext,
    list: &'a mut DisplayList,
    stats: &'a mut PaintStats,
    /// Layers split off from the main list, in the order they were found.
    extra_layers: Vec<Layer>,
    hit_regions: Vec<HitRegion>,
    next_layer_id: u32,
}

impl Painter<'_> {
    fn node(&mut self, node_id: NodeId) {
        let Some(node) = self.dom.get(node_id) else {
            return;
        };

        match &node.data {
            NodeData::Document | NodeData::DocumentFragment => {
                for child in self.dom.children(node_id) {
                    self.node(child);
                }
            }
            NodeData::Element(_) => {
                let style = self.layout.styles().get_or_initial(node_id);
                if is_display_none(&style) || is_hidden(&style) {
                    return;
                }
                self.stats.elements += 1;

                let rect = self.rect_of(node_id);
                if !rect.is_empty() {
                    self.hit_regions.push(HitRegion {
                        rect,
                        node: node_id,
                    });
                }

                if let Some(reason) = promotion_reason(&style) {
                    self.promote(node_id, &style, rect, reason);
                    return;
                }

                self.box_decoration(&rect, &style);

                let clips = clips_overflow(&style);
                if clips {
                    self.list.push(DisplayItem::PushClip { rect });
                }
                for child in self.dom.children(node_id) {
                    self.node(child);
                }
                if clips {
                    self.list.push(DisplayItem::PopClip);
                }
            }
            NodeData::Text(_) => self.text(node_id),
            _ => {}
        }
    }

    /// Paint a subtree into its own layer.
    ///
    /// The layer's content is painted in the *page's* coordinate space and the
    /// layer's bounds record where it sits, so the compositor can place it
    /// without the painter having to rebase every coordinate.
    fn promote(&mut self, node_id: NodeId, style: &ComputedStyle, rect: Rect, reason: LayerReason) {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;

        let mut sub_list = DisplayList::default();
        let mut sub_extra = Vec::new();
        let mut sub_regions = Vec::new();
        std::mem::swap(self.list, &mut sub_list);
        std::mem::swap(&mut self.extra_layers, &mut sub_extra);
        std::mem::swap(&mut self.hit_regions, &mut sub_regions);

        self.box_decoration(&rect, style);
        for child in self.dom.children(node_id) {
            self.node(child);
        }

        std::mem::swap(self.list, &mut sub_list);
        std::mem::swap(&mut self.extra_layers, &mut sub_extra);
        std::mem::swap(&mut self.hit_regions, &mut sub_regions);
        // Regions from inside the layer still describe page coordinates, so
        // hit-testing does not need to know about layers at all.
        self.hit_regions.extend(sub_regions);

        let opacity = opacity_of(style);
        let transform = transform_of(style, &rect, &self.ctx);
        let scrolls = !matches!(
            style.get(&PropertyId::Position),
            Some(CssValue::Position(
                crate::css_values::types::display::Position::Fixed
            ))
        );

        self.extra_layers.push(Layer {
            id,
            reason,
            display_list: sub_list,
            bounds: rect,
            transform,
            opacity,
            scrolls,
        });
        // Layers found inside this one composite after it.
        self.extra_layers.extend(sub_extra);
    }

    fn rect_of(&mut self, node_id: NodeId) -> Rect {
        let r = self.layout.get_bounding_rect(self.dom, node_id);
        Rect {
            x: r.x as f32,
            y: r.y as f32,
            width: r.width as f32,
            height: r.height as f32,
        }
    }

    fn box_decoration(&mut self, rect: &Rect, style: &ComputedStyle) {
        if rect.is_empty() {
            return;
        }
        let background = color_of(style, PropertyId::BackgroundColor, Rgba::TRANSPARENT);
        if background.is_visible() {
            self.list.push(DisplayItem::Rect {
                rect: *rect,
                color: background,
            });
        }

        // Used widths, not specified widths: a border whose style is `none`
        // has a used width of zero, and layout reserved space accordingly.
        // Painting the specified width would draw outside the box.
        let widths = SideOffsets {
            top: self.border_width(
                style,
                PropertyId::BorderTopWidth,
                PropertyId::BorderTopStyle,
            ),
            right: self.border_width(
                style,
                PropertyId::BorderRightWidth,
                PropertyId::BorderRightStyle,
            ),
            bottom: self.border_width(
                style,
                PropertyId::BorderBottomWidth,
                PropertyId::BorderBottomStyle,
            ),
            left: self.border_width(
                style,
                PropertyId::BorderLeftWidth,
                PropertyId::BorderLeftStyle,
            ),
        };
        if !widths.any() {
            return;
        }
        let colors = [
            color_of(style, PropertyId::BorderTopColor, Rgba::BLACK),
            color_of(style, PropertyId::BorderRightColor, Rgba::BLACK),
            color_of(style, PropertyId::BorderBottomColor, Rgba::BLACK),
            color_of(style, PropertyId::BorderLeftColor, Rgba::BLACK),
        ];
        if colors.iter().any(Rgba::is_visible) {
            self.list.push(DisplayItem::Border {
                rect: *rect,
                widths,
                colors,
            });
        }
    }

    fn text(&mut self, node_id: NodeId) {
        let Some(node) = self.dom.get(node_id) else {
            return;
        };
        let NodeData::Text(raw) = &node.data else {
            return;
        };

        let parent_style = node
            .parent
            .map(|p| self.layout.styles().get_or_initial(p))
            .unwrap_or_else(|| ComputedStyle::resolve(&std::collections::HashMap::new(), None));

        // The same white-space processing layout applied. If the painter drew a
        // different string from the one that was measured, the text would not
        // fit the box reserved for it.
        let collapsed = crate::layout::engine::collapse_white_space(raw, &parent_style);
        if collapsed.is_empty() {
            return;
        }

        let rect = self.rect_of(node_id);
        let text_style = self.layout.text_style_for(&parent_style, &self.ctx);
        let (_summary, lines) =
            self.inline
                .layout_lines(&collapsed, &text_style, Some(rect.width.max(0.0)));
        if lines.is_empty() {
            return;
        }

        let Some(font) = resolve_font(&text_style) else {
            self.stats.unresolved_fonts += 1;
            return;
        };
        let color = color_of(&parent_style, PropertyId::Color, Rgba::BLACK);

        for line in &lines {
            if line.glyphs.is_empty() {
                continue;
            }
            self.stats.text_runs += 1;
            self.list.push(DisplayItem::Text {
                origin: (rect.x, rect.y + line.y + line.baseline),
                glyphs: line
                    .glyphs
                    .iter()
                    .map(|g| Glyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    })
                    .collect(),
                font,
                color,
            });
        }
    }

    fn border_width(
        &self,
        style: &ComputedStyle,
        width: PropertyId,
        style_prop: PropertyId,
    ) -> f32 {
        let draws = matches!(
            style.get(&style_prop),
            Some(CssValue::BorderStyle(s)) if s.is_visible()
        );
        if !draws {
            return 0.0;
        }
        length_of(style, width, &self.ctx)
    }
}

/// Resolve a text style's family chain to the face the shaper used.
fn resolve_font(style: &crate::layout::inline::TextStyle) -> Option<FontRef> {
    let db = crate::canvas::text::font_database::FontDatabase::get();
    let id = db.query_chain(&style.families, style.weight, style.italic, &style.os_name)?;
    let (data, face_index) = db.face_data(id)?;
    Some(FontRef {
        data,
        face_index,
        size_px: style.size_px,
    })
}

fn color_of(style: &ComputedStyle, property: PropertyId, fallback: Rgba) -> Rgba {
    match style.get(&property) {
        Some(CssValue::Color(c)) => css_color(c),
        _ => fallback,
    }
}

fn css_color(c: &CssColor) -> Rgba {
    let (r, g, b, a) = c.to_rgba();
    Rgba {
        r,
        g,
        b,
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

fn length_of(style: &ComputedStyle, property: PropertyId, ctx: &ResolveContext) -> f32 {
    match style.get(&property) {
        Some(CssValue::Length(l)) => resolve_length(l, ctx),
        Some(CssValue::LengthPercentage(LengthPercentage::Length(l))) => resolve_length(l, ctx),
        Some(CssValue::LengthPercentageAuto(LengthPercentageAuto::Length(l))) => {
            resolve_length(l, ctx)
        }
        _ => 0.0,
    }
}

fn is_display_none(style: &ComputedStyle) -> bool {
    use crate::css_values::types::display::Display;
    matches!(
        style.get(&PropertyId::Display),
        Some(CssValue::Display(Display::None))
    )
}

fn is_hidden(style: &ComputedStyle) -> bool {
    use crate::css_values::types::display::Visibility;
    matches!(
        style.get(&PropertyId::Visibility),
        Some(CssValue::Visibility(Visibility::Hidden))
    )
}

fn clips_overflow(style: &ComputedStyle) -> bool {
    use crate::css_values::types::display::Overflow;
    [PropertyId::OverflowX, PropertyId::OverflowY]
        .iter()
        .any(|p| {
            matches!(
                style.get(p),
                Some(CssValue::Overflow(
                    Overflow::Hidden | Overflow::Scroll | Overflow::Auto
                ))
            )
        })
}

/// Why this element needs its own compositing layer, if it does.
///
/// Every promotion costs a surface and the memory behind it, so the list is
/// short on purpose. `will-change` is not here because the engine has no such
/// property yet; when it arrives it belongs in this function and nowhere else.
fn promotion_reason(style: &ComputedStyle) -> Option<LayerReason> {
    use crate::css_values::types::display::Position;

    if matches!(
        style.get(&PropertyId::Position),
        Some(CssValue::Position(Position::Fixed))
    ) {
        // Not an optimisation: a fixed element must *not* move when the page
        // scrolls, and the only way to express that to a compositor that
        // scrolls by translating surfaces is to give it a surface of its own.
        return Some(LayerReason::Fixed);
    }
    if opacity_of(style) < 1.0 {
        // The subtree has to composite as a unit. Applying opacity per item
        // would let overlapping children show through each other.
        return Some(LayerReason::Opacity);
    }
    if let Some(CssValue::Transform(fns)) = style.get(&PropertyId::Transform) {
        if !fns.is_empty() {
            return Some(LayerReason::Transform);
        }
    }
    None
}

fn opacity_of(style: &ComputedStyle) -> f32 {
    match style.get(&PropertyId::Opacity) {
        Some(CssValue::Number(n)) => (*n as f32).clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// Flatten a CSS transform list to a 2D affine matrix.
///
/// Percentages resolve against the element's own border box, and the transform
/// origin is its centre — the CSS default. 3D functions contribute their 2D
/// part only; a correct implementation needs a 4x4 and a perspective-aware
/// compositor.
fn transform_of(style: &ComputedStyle, rect: &Rect, ctx: &ResolveContext) -> Transform2D {
    use crate::css_values::types::transform::TransformFunction as T;

    let Some(CssValue::Transform(fns)) = style.get(&PropertyId::Transform) else {
        return Transform2D::IDENTITY;
    };
    if fns.is_empty() {
        return Transform2D::IDENTITY;
    }

    let px = |lp: &LengthPercentage, base: f32| -> f32 {
        match lp {
            LengthPercentage::Length(l) => resolve_length(l, ctx),
            LengthPercentage::Percentage(p) => (*p as f32) / 100.0 * base,
            _ => 0.0,
        }
    };

    let mut m = Transform2D::IDENTITY;
    for f in fns {
        let step = match f {
            T::Translate(x, y) => Transform2D::translate(px(x, rect.width), px(y, rect.height)),
            T::TranslateX(x) => Transform2D::translate(px(x, rect.width), 0.0),
            T::TranslateY(y) => Transform2D::translate(0.0, px(y, rect.height)),
            T::Translate3d(x, y, _) => {
                Transform2D::translate(px(x, rect.width), px(y, rect.height))
            }
            T::Scale(x, y) => Transform2D {
                a: *x as f32,
                d: *y as f32,
                ..Transform2D::IDENTITY
            },
            T::ScaleX(x) => Transform2D {
                a: *x as f32,
                ..Transform2D::IDENTITY
            },
            T::ScaleY(y) => Transform2D {
                d: *y as f32,
                ..Transform2D::IDENTITY
            },
            T::Scale3d(x, y, _) => Transform2D {
                a: *x as f32,
                d: *y as f32,
                ..Transform2D::IDENTITY
            },
            T::Rotate(a) => {
                let r = (a.to_degrees() as f32).to_radians();
                Transform2D {
                    a: r.cos(),
                    b: r.sin(),
                    c: -r.sin(),
                    d: r.cos(),
                    e: 0.0,
                    f: 0.0,
                }
            }
            T::SkewX(a) => Transform2D {
                c: (a.to_degrees() as f32).to_radians().tan(),
                ..Transform2D::IDENTITY
            },
            T::SkewY(a) => Transform2D {
                b: (a.to_degrees() as f32).to_radians().tan(),
                ..Transform2D::IDENTITY
            },
            T::Matrix(a, b, c, d, e, f2) => Transform2D {
                a: *a as f32,
                b: *b as f32,
                c: *c as f32,
                d: *d as f32,
                e: *e as f32,
                f: *f2 as f32,
            },
            // 3D rotations and matrices need a 4x4 to be meaningful. Treating
            // them as identity is wrong but visible; silently approximating
            // them with their top-left 2x2 would be wrong and invisible.
            _ => Transform2D::IDENTITY,
        };
        m = m.then(&step);
    }

    // CSS `transform-origin` defaults to the element's centre. Translate to the
    // origin, transform, translate back — otherwise a rotation swings the
    // element around the page origin instead of spinning in place.
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    Transform2D::translate(-cx, -cy)
        .then(&m)
        .then(&Transform2D::translate(cx, cy))
}

/// The topmost element containing `point`, in page coordinates.
///
/// Regions are recorded in paint order, so the last one containing the point is
/// the one on top — which is what `document.elementFromPoint` means.
pub fn hit_test(regions: &[HitRegion], x: f32, y: f32) -> Option<NodeId> {
    regions
        .iter()
        .rev()
        .find(|r| x >= r.rect.x && x < r.rect.right() && y >= r.rect.y && y < r.rect.bottom())
        .map(|r| r.node)
}
