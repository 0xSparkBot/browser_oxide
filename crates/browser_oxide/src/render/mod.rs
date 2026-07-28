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
//! What is not here: compositing, layer trees, damage tracking, incremental
//! invalidation, GPU surfaces, images, SVG and form controls. A screenshot
//! rasterizes the whole page every time.

pub mod display_list;
pub mod painter;
pub mod raster;

pub use display_list::{DisplayItem, DisplayList, FontRef, Glyph, Rect, Rgba, SideOffsets};
pub use painter::PaintStats;
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
