//! The display list: a flat, ordered, paint-only description of a page.
//!
//! Kept separate from the box tree on purpose. This is the thing that gets
//! cached, diffed and replayed on scroll without re-running layout, and that
//! only works if it holds no references back into the DOM.
//!
//! Text carries **positioned glyphs, never strings**. A list that held strings
//! would have to reshape on every replay, and shaping is the expensive half.

/// A rectangle in page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub fn is_visible(&self) -> bool {
        self.a > 0
    }
}

/// Per-side lengths, in CSS order: top, right, bottom, left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SideOffsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SideOffsets {
    pub fn any(&self) -> bool {
        self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0
    }
}

/// Identifies the face a glyph run was shaped with.
///
/// Carries the face's bytes rather than a family name, deliberately: glyph ids
/// index into *this* face. A rasterizer handed a family name may resolve to a
/// different file — a different version, a bold variant — and the same ids then
/// draw different letters. Silent, and invisible in a screenshot.
#[derive(Debug, Clone, Copy)]
pub struct FontRef {
    pub data: &'static [u8],
    pub face_index: u32,
    pub size_px: f32,
}

/// A glyph positioned relative to a text run's origin, which sits on the
/// baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub enum DisplayItem {
    Rect {
        rect: Rect,
        color: Rgba,
    },
    /// Painted as four trapezoids, so a mitre between two differently coloured
    /// sides comes out right — four overlapping rectangles paint one colour
    /// over the other at every corner.
    Border {
        rect: Rect,
        widths: SideOffsets,
        colors: [Rgba; 4],
    },
    Text {
        /// Baseline origin.
        origin: (f32, f32),
        glyphs: Vec<Glyph>,
        font: FontRef,
        color: Rgba,
    },
    PushClip {
        rect: Rect,
    },
    PopClip,
}

#[derive(Debug, Default, Clone)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
}

impl DisplayList {
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Counts by kind. Useful in tests and in the CDP screenshot path's debug
    /// output; not a stable format.
    pub fn summary(&self) -> String {
        let (mut rects, mut borders, mut texts, mut glyphs, mut clips) = (0, 0, 0, 0, 0);
        for item in &self.items {
            match item {
                DisplayItem::Rect { .. } => rects += 1,
                DisplayItem::Border { .. } => borders += 1,
                DisplayItem::Text { glyphs: g, .. } => {
                    texts += 1;
                    glyphs += g.len();
                }
                DisplayItem::PushClip { .. } => clips += 1,
                DisplayItem::PopClip => {}
            }
        }
        format!(
            "{rects} rects, {borders} borders, {texts} text runs ({glyphs} glyphs), {clips} clips"
        )
    }
}
