//! Replay a display list onto a Skia raster surface.
//!
//! CPU only. The Skia setup mirrors `canvas/canvas2d.rs`, which already proves
//! the engine can drive `SkCanvas` correctly: a raster surface wrapping a plain
//! pixel buffer via `surfaces::wrap_pixels`, RGBA8888 premultiplied.
//!
//! Glyph rasterization uses the same `SkFont` settings as the 2D canvas —
//! grayscale antialiasing, subpixel positioning, no hinting. Matching them is
//! not cosmetic: text rasterization is a fingerprint surface, and a browser
//! whose page text and `<canvas>` text were rasterized differently would be
//! reporting two different renderers.

use skia_safe::{
    surfaces, AlphaType, Canvas as SkCanvas, Color4f, ColorType, Font, FontHinting, FontMgr,
    ImageInfo, Paint, PathBuilder, Point, Rect as SkRect,
};

use super::display_list::{DisplayItem, DisplayList, Rect, Rgba};

/// A CPU raster target. `pixels` is RGBA8888 premultiplied, top-down.
pub struct Target {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Target {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    /// Paint a display list into this target.
    ///
    /// Returns false if Skia would not wrap the buffer — a zero dimension, or a
    /// size that overflows.
    pub fn paint(&mut self, list: &DisplayList) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }
        let info = ImageInfo::new(
            (self.width as i32, self.height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = self.width as usize * 4;
        let Some(mut surface) =
            surfaces::wrap_pixels(&info, &mut self.pixels, Some(row_bytes), None)
        else {
            return false;
        };
        replay(surface.canvas(), list);
        true
    }

    /// Un-premultiplied RGBA8, which is what an image encoder expects.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = self.pixels.clone();
        for px in out.chunks_exact_mut(4) {
            let a = px[3];
            if a != 0 && a != 255 {
                for c in px[..3].iter_mut() {
                    *c = ((u32::from(*c) * 255) / u32::from(a)).min(255) as u8;
                }
            }
        }
        out
    }

    /// Encode as PNG.
    pub fn to_png(&self) -> Option<Vec<u8>> {
        let rgba = self.to_rgba8();
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&rgba).ok()?;
        }
        Some(out)
    }
}

fn sk_color(c: Rgba) -> Color4f {
    Color4f::new(
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        f32::from(c.a) / 255.0,
    )
}

fn sk_rect(r: &Rect) -> SkRect {
    SkRect::from_xywh(r.x, r.y, r.width, r.height)
}

pub fn replay(canvas: &SkCanvas, list: &DisplayList) {
    // A `PopClip` with no matching `PushClip` would unbalance Skia's save
    // stack, so track depth rather than trusting the list.
    let mut clip_depth = 0usize;

    for item in &list.items {
        match item {
            DisplayItem::Rect { rect, color } => {
                let mut paint = Paint::new(sk_color(*color), None);
                paint.set_anti_alias(true);
                canvas.draw_rect(sk_rect(rect), &paint);
            }

            DisplayItem::Border {
                rect,
                widths,
                colors,
            } => {
                // Four trapezoids, not four rectangles: adjacent sides meet at
                // a mitre, and overlapping rectangles paint one colour over the
                // other at every corner.
                let (l, t, r, b) = (rect.x, rect.y, rect.right(), rect.bottom());
                let (il, it, ir, ib) = (
                    l + widths.left,
                    t + widths.top,
                    r - widths.right,
                    b - widths.bottom,
                );
                let sides: [(Rgba, [(f32, f32); 4]); 4] = [
                    (colors[0], [(l, t), (r, t), (ir, it), (il, it)]),
                    (colors[1], [(r, t), (r, b), (ir, ib), (ir, it)]),
                    (colors[2], [(r, b), (l, b), (il, ib), (ir, ib)]),
                    (colors[3], [(l, b), (l, t), (il, it), (il, ib)]),
                ];
                let side_widths = [widths.top, widths.right, widths.bottom, widths.left];
                for (i, (color, pts)) in sides.iter().enumerate() {
                    if side_widths[i] <= 0.0 || !color.is_visible() {
                        continue;
                    }
                    let mut builder = PathBuilder::new();
                    builder.move_to(Point::new(pts[0].0, pts[0].1));
                    for p in &pts[1..] {
                        builder.line_to(Point::new(p.0, p.1));
                    }
                    builder.close();
                    let mut paint = Paint::new(sk_color(*color), None);
                    paint.set_anti_alias(true);
                    canvas.draw_path(&builder.detach(), &paint);
                }
            }

            DisplayItem::Text {
                origin,
                glyphs,
                font,
                color,
            } => {
                let Some(typeface) =
                    FontMgr::new().new_from_data(font.data, Some(font.face_index as usize))
                else {
                    continue;
                };
                let mut sk_font = Font::from_typeface(typeface, Some(font.size_px));
                sk_font.set_edging(skia_safe::font::Edging::AntiAlias);
                sk_font.set_subpixel(true);
                sk_font.set_hinting(FontHinting::None);

                let ids: Vec<u16> = glyphs.iter().map(|g| g.id).collect();
                let pos: Vec<Point> = glyphs
                    .iter()
                    .map(|g| Point::new(origin.0 + g.x, origin.1 + g.y))
                    .collect();
                let mut paint = Paint::new(sk_color(*color), None);
                paint.set_anti_alias(true);
                canvas.draw_glyphs_at(&ids, &pos[..], Point::new(0.0, 0.0), &sk_font, &paint);
            }

            DisplayItem::PushClip { rect } => {
                canvas.save();
                clip_depth += 1;
                canvas.clip_rect(sk_rect(rect), None, Some(true));
            }
            DisplayItem::PopClip => {
                if clip_depth > 0 {
                    canvas.restore();
                    clip_depth -= 1;
                }
            }
        }
    }

    for _ in 0..clip_depth {
        canvas.restore();
    }
}
