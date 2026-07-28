//! Rendering: box tree → display list → pixels.
//!
//! The half of a browser engine `browser_oxide` did not have. Before this,
//! Skia was a dependency used only by `<canvas>`, nothing painted a background
//! or a border or a text run belonging to a DOM element, and the CDP surface
//! implemented 48 methods of which `Page.captureScreenshot` was not one.
//!
//! Three stages, deliberately separable:
//!
//! ```text
//!   Dom + LayoutEngine ──► painter ──► DisplayList ──► raster ──► RGBA / PNG
//! ```
//!
//! The display list in the middle is the point. It is flat, ordered and holds
//! no references back into the DOM, which is what makes it cacheable and
//! diffable — repainting a scroll should replay a diff, not re-run layout.
//!
//! A fourth stage sits alongside them for anything that moves:
//!
//! ```text
//!   painter ──► LayerTree ──► Compositor ──► RGBA / PNG
//! ```
//!
//! The compositor keeps each layer's rasterized surface between frames, so a
//! scroll translates surfaces rather than repainting. `render_to_png` does not
//! use it — a one-off screenshot has nothing to reuse — but anything animating
//! or scrolling should.
//!
//! What is not here: GPU surfaces, images, SVG, form controls, and stacking
//! contexts. Paint order is tree order.

pub mod composite;
pub mod display_list;
pub mod layer;
pub mod painter;
pub mod raster;

pub use composite::{CompositeStats, Compositor};
pub use display_list::{DisplayItem, DisplayList, FontRef, Glyph, Rect, Rgba, SideOffsets};
pub use layer::{Layer, LayerId, LayerReason, LayerTree, Transform2D};
pub use painter::{hit_test, HitRegion, PaintStats};
pub use raster::Target;

use crate::dom::Dom;
use crate::layout::inline::InlineLayout;
use crate::layout::LayoutEngine;

/// Build a display list for an already-laid-out document.
pub fn build_display_list(
    dom: &Dom,
    layout: &mut LayoutEngine,
    inline: &mut InlineLayout,
    width: f32,
    height: f32,
) -> (DisplayList, PaintStats) {
    painter::paint(dom, layout, inline, width, height)
}

/// Lay out and rasterize a document, returning premultiplied RGBA8.
///
/// `layout` is computed here, so the caller does not have to remember to.
pub fn render_to_target(
    dom: &Dom,
    layout: &mut LayoutEngine,
    width: u32,
    height: u32,
) -> (Target, PaintStats) {
    layout.compute(dom);
    let mut inline = InlineLayout::new();
    let (list, stats) = build_display_list(dom, layout, &mut inline, width as f32, height as f32);
    let mut target = Target::new(width, height);
    target.paint(&list);
    (target, stats)
}

/// Build a layer tree, and the hit regions that go with it.
///
/// Use this rather than [`build_display_list`] when the result will be
/// scrolled, animated or hit-tested.
pub fn build_layer_tree(
    dom: &Dom,
    layout: &mut LayoutEngine,
    inline: &mut InlineLayout,
    width: f32,
    height: f32,
) -> (LayerTree, Vec<HitRegion>, PaintStats) {
    painter::paint_layered(dom, layout, inline, width, height)
}

/// Lay out and rasterize a document to PNG bytes.
pub fn render_to_png(
    dom: &Dom,
    layout: &mut LayoutEngine,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let (target, _) = render_to_target(dom, layout, width, height);
    target.to_png()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Viewport;

    fn render(html: &str, w: u32, h: u32) -> (Target, PaintStats, DisplayList) {
        let dom = crate::html_parser::parse_html(html);
        let mut layout = LayoutEngine::new(Viewport::new(w as f32, h as f32));
        layout.compute(&dom);
        let mut inline = InlineLayout::new();
        let (list, stats) = build_display_list(&dom, &mut layout, &mut inline, w as f32, h as f32);
        let mut target = Target::new(w, h);
        assert!(target.paint(&list), "raster surface must wrap the buffer");
        (target, stats, list)
    }

    /// Colour of one pixel, un-premultiplied.
    fn pixel(t: &Target, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let rgba = t.to_rgba8();
        let i = ((y * t.width + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])
    }

    #[test]
    fn a_background_colour_reaches_the_pixels() {
        // The end-to-end claim in one test: CSS in, coloured pixels out.
        let (target, _, _) = render(
            "<html><body style='margin:0'>\
             <div style='width:100px; height:100px; background-color:#ff0000'></div>\
             </body></html>",
            200,
            200,
        );
        assert_eq!(pixel(&target, 50, 50), (255, 0, 0, 255));
        // Outside the div is the white the canvas starts as.
        assert_eq!(pixel(&target, 150, 150), (255, 255, 255, 255));
    }

    #[test]
    fn a_stylesheet_reaches_the_pixels() {
        let (target, _, _) = render(
            "<html><head><style>.box { width:80px; height:80px; background-color:#0000ff }</style></head>\
             <body style='margin:0'><div class='box'></div></body></html>",
            200,
            200,
        );
        assert_eq!(pixel(&target, 40, 40), (0, 0, 255, 255));
    }

    #[test]
    fn borders_are_painted_in_their_own_colour() {
        let (target, _, _) = render(
            "<html><body style='margin:0'>\
             <div style='width:100px; height:100px; background-color:#ffffff; \
             border-style:solid; border-top-width:10px; border-top-color:#00ff00'></div>\
             </body></html>",
            200,
            200,
        );
        // Inside the top border.
        assert_eq!(pixel(&target, 50, 5), (0, 255, 0, 255));
        // Below it, the background.
        assert_eq!(pixel(&target, 50, 50), (255, 255, 255, 255));
    }

    #[test]
    fn a_border_with_no_style_paints_nothing() {
        // `border-width` alone has no effect in CSS. This is the defect the
        // engine had, checked at the pixel level.
        let (target, _, _) = render(
            "<html><body style='margin:0'>\
             <div style='width:100px; height:100px; background-color:#ffffff; \
             border-top-width:10px; border-top-color:#00ff00'></div>\
             </body></html>",
            200,
            200,
        );
        assert_eq!(pixel(&target, 50, 5), (255, 255, 255, 255));
    }

    #[test]
    fn text_produces_glyphs_and_dark_pixels() {
        let (target, stats, list) = render(
            "<html><body style='margin:0'><p style='width:300px; color:#000000'>\
             Hello from the engine</p></body></html>",
            400,
            200,
        );
        assert!(stats.text_runs > 0, "expected a text run: {stats:?}");
        assert_eq!(stats.unresolved_fonts, 0);
        assert!(
            list.items
                .iter()
                .any(|i| matches!(i, DisplayItem::Text { glyphs, .. } if !glyphs.is_empty())),
            "expected positioned glyphs in the display list"
        );

        // Somewhere in the first line there must be ink.
        let rgba = target.to_rgba8();
        let has_dark = rgba
            .chunks_exact(4)
            .any(|p| p[0] < 128 && p[1] < 128 && p[2] < 128 && p[3] > 0);
        assert!(has_dark, "text must actually darken some pixels");
    }

    #[test]
    fn display_none_paints_nothing() {
        let (target, _, _) = render(
            "<html><body style='margin:0'>\
             <div style='width:100px; height:100px; background-color:#ff0000; display:none'></div>\
             </body></html>",
            200,
            200,
        );
        assert_eq!(pixel(&target, 50, 50), (255, 255, 255, 255));
    }

    #[test]
    fn png_encodes() {
        let dom = crate::html_parser::parse_html(
            "<html><body><div style='background-color:#123456'>x</div></body></html>",
        );
        let mut layout = LayoutEngine::new(Viewport::new(120.0, 80.0));
        let png = render_to_png(&dom, &mut layout, 120, 80).expect("png encodes");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "PNG magic");
        assert!(png.len() > 100);
    }

    #[test]
    fn an_unbalanced_clip_does_not_corrupt_the_surface() {
        // A PopClip with no PushClip must not unbalance Skia's save stack.
        let mut list = DisplayList::default();
        list.push(DisplayItem::PopClip);
        list.push(DisplayItem::Rect {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            color: Rgba::WHITE,
        });
        list.push(DisplayItem::PushClip {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            },
        });
        let mut target = Target::new(10, 10);
        assert!(target.paint(&list));
        assert_eq!(pixel(&target, 1, 1), (255, 255, 255, 255));
    }

    #[test]
    fn a_zero_sized_target_is_refused_rather_than_panicking() {
        let mut target = Target::new(0, 0);
        assert!(!target.paint(&DisplayList::default()));
    }
}

/// Compositing over real documents.
///
/// `composite::tests` covers the compositor against hand-built layer trees.
/// These check the seam: that CSS actually produces the layers it should, and
/// that scrolling a real page rasterizes nothing.
#[cfg(test)]
mod compositing_integration {
    use super::*;
    use crate::layout::Viewport;

    fn layers(html: &str, w: f32, h: f32) -> (LayerTree, Vec<HitRegion>, PaintStats) {
        let dom = crate::html_parser::parse_html(html);
        let mut layout = LayoutEngine::new(Viewport::new(w, h));
        layout.compute(&dom);
        let mut inline = InlineLayout::new();
        build_layer_tree(&dom, &mut layout, &mut inline, w, h)
    }

    #[test]
    fn an_ordinary_page_is_one_layer() {
        // Promotion costs a surface each. A page with nothing to composite
        // separately must not pay for any.
        let (tree, _, _) = layers(
            "<html><body><div style='background-color:#eee'>plain</div>\
             <p>more content</p></body></html>",
            400.0,
            300.0,
        );
        assert_eq!(tree.layers.len(), 1);
        assert_eq!(tree.layers[0].reason, LayerReason::Root);
    }

    #[test]
    fn opacity_promotes() {
        let (tree, _, _) = layers(
            "<html><body><div style='opacity:0.5; width:50px; height:50px; \
             background-color:#f00'>x</div></body></html>",
            400.0,
            300.0,
        );
        assert_eq!(tree.layers.len(), 2);
        assert_eq!(tree.layers[1].reason, LayerReason::Opacity);
        assert!((tree.layers[1].opacity - 0.5).abs() < 0.01);
    }

    #[test]
    fn a_transform_promotes_and_is_flattened_to_a_matrix() {
        let (tree, _, _) = layers(
            "<html><body><div style='transform:translateX(20px); width:50px; \
             height:50px; background-color:#f00'>x</div></body></html>",
            400.0,
            300.0,
        );
        assert_eq!(tree.layers.len(), 2);
        assert_eq!(tree.layers[1].reason, LayerReason::Transform);
        let t = tree.layers[1].transform;
        assert!(!t.is_identity());
        assert!(
            (t.e - 20.0).abs() < 0.01,
            "expected a 20px x-translation, got {t:?}"
        );
    }

    #[test]
    fn a_rotation_spins_about_the_element_centre() {
        // CSS `transform-origin` defaults to the centre. Rotating about the
        // page origin instead swings the element across the screen, which is
        // the classic version of this bug.
        let (tree, _, _) = layers(
            "<html><body style='margin:0'><div style='transform:rotate(90deg); \
             width:100px; height:100px; background-color:#f00'>x</div></body></html>",
            400.0,
            300.0,
        );
        let layer = &tree.layers[1];
        let moved = layer.transform.transform_rect(&layer.bounds);
        // A square rotated about its own centre lands back on itself.
        assert!(
            (moved.x - layer.bounds.x).abs() < 1.0 && (moved.y - layer.bounds.y).abs() < 1.0,
            "a square rotated about its centre should not move: {:?} -> {:?}",
            layer.bounds,
            moved
        );
    }

    #[test]
    fn position_fixed_promotes_and_does_not_scroll() {
        let (tree, _, _) = layers(
            "<html><body><div style='position:fixed; width:50px; height:50px; \
             background-color:#f00'>x</div></body></html>",
            400.0,
            300.0,
        );
        let fixed = tree
            .layers
            .iter()
            .find(|l| l.reason == LayerReason::Fixed)
            .expect("a fixed element must get its own layer");
        assert!(
            !fixed.scrolls,
            "a fixed layer must not move with the page scroll"
        );
    }

    #[test]
    fn scrolling_a_real_page_rasterizes_nothing() {
        // The whole justification for compositing, end to end.
        let html = "<html><body><div style='height:2000px; background-color:#ddd'>\
                    <p>a long page that scrolls</p></div></body></html>";
        let (tree, _, _) = layers(html, 400.0, 300.0);

        let mut compositor = Compositor::new();
        let (_, first) = compositor.composite(&tree, 0.0, 400, 300);
        assert!(first.rasterized > 0, "the first frame must paint");

        let (_, second) = compositor.composite(&tree, 120.0, 400, 300);
        assert_eq!(
            second.rasterized,
            0,
            "scrolling must not rasterize: {}",
            second.summary()
        );
        assert!(second.reused > 0);
    }

    #[test]
    fn hit_testing_finds_the_topmost_element() {
        let (_, regions, _) = layers(
            "<html><body style='margin:0'>\
             <div style='width:200px; height:200px; background-color:#eee'>\
             <div id='inner' style='width:50px; height:50px; background-color:#f00'></div>\
             </div></body></html>",
            400.0,
            300.0,
        );
        let hit = hit_test(&regions, 25.0, 25.0).expect("something is under the point");
        // The inner div is painted after the outer, so it wins.
        let dom = crate::html_parser::parse_html(
            "<html><body style='margin:0'>\
             <div style='width:200px; height:200px; background-color:#eee'>\
             <div id='inner' style='width:50px; height:50px; background-color:#f00'></div>\
             </div></body></html>",
        );
        let inner = dom.get_element_by_id("inner");
        assert_eq!(Some(hit), inner, "the innermost element must win");
    }

    #[test]
    fn hit_testing_outside_everything_finds_nothing() {
        let (_, regions, _) = layers(
            "<html><body style='margin:0'><div style='width:10px; height:10px'></div>\
             </body></html>",
            400.0,
            300.0,
        );
        assert_eq!(hit_test(&regions, 380.0, 290.0), None);
    }

    #[test]
    fn a_composited_page_looks_like_an_uncomposited_one() {
        // Compositing is an optimisation, so it must not change the picture.
        let html = "<html><body style='margin:0'>\
                    <div style='width:100px;height:100px;background-color:#3366cc'></div>\
                    </body></html>";
        let dom = crate::html_parser::parse_html(html);

        let mut layout = LayoutEngine::new(Viewport::new(200.0, 200.0));
        let (direct, _) = render_to_target(&dom, &mut layout, 200, 200);

        let mut layout2 = LayoutEngine::new(Viewport::new(200.0, 200.0));
        layout2.compute(&dom);
        let mut inline = InlineLayout::new();
        let (tree, _, _) = build_layer_tree(&dom, &mut layout2, &mut inline, 200.0, 200.0);
        let (composited, _) = Compositor::new().composite(&tree, 0.0, 200, 200);

        let a = direct.to_rgba8();
        let b = composited.to_rgba8();
        let differing = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(p, q)| (0..3).any(|i| p[i].abs_diff(q[i]) > 2))
            .count();
        assert_eq!(
            differing, 0,
            "compositing changed {differing} pixels; it is an optimisation, not a rendering mode"
        );
    }
}
