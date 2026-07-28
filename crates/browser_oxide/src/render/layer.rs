//! The layer tree.
//!
//! A layer is a subtree that can be rasterized once and then moved, faded or
//! transformed without repainting its contents. That is the whole point:
//! scrolling a page should translate a surface, not re-run paint, and an
//! opacity animation should blend a cached bitmap rather than re-rasterize
//! every frame underneath it.
//!
//! Promotion is not free — every layer is a separate surface and separate
//! memory — so a subtree is only promoted when it has a reason.

use super::display_list::{DisplayList, Rect};

/// Identifies a layer within one tree. Stable across frames as long as the
/// document structure does not change, which is what lets the compositor reuse
/// a rasterized surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(pub u32);

/// Why a subtree was promoted.
///
/// Recorded rather than discarded because "why does this page have 400 layers"
/// is a question that gets asked, and because a reason that turns out not to
/// need a layer is the first thing to remove when memory is tight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerReason {
    /// The document itself. Always present, always first.
    Root,
    /// `opacity` below 1 — the subtree must composite as a unit, or overlapping
    /// children would show through each other.
    Opacity,
    /// A `transform` other than the identity.
    Transform,
    /// `position: fixed` — does not move when the page scrolls.
    Fixed,
}

/// A 2D affine transform, row-major: `[a c e; b d f]`.
///
/// 3D transform functions are flattened to their 2D part. A real
/// implementation needs a 4x4 and a perspective-correct compositor; this is
/// enough for translate, scale, rotate and skew, which is what pages actually
/// animate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// `self` then `other`.
    pub fn then(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Axis-aligned bounding box of a transformed rectangle.
    pub fn transform_rect(&self, r: &Rect) -> Rect {
        let corners = [
            self.apply(r.x, r.y),
            self.apply(r.right(), r.y),
            self.apply(r.right(), r.bottom()),
            self.apply(r.x, r.bottom()),
        ];
        let (mut min_x, mut min_y) = corners[0];
        let (mut max_x, mut max_y) = corners[0];
        for (x, y) in &corners[1..] {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
}

/// One compositing layer.
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: LayerId,
    pub reason: LayerReason,
    /// Content, in the layer's own coordinate space.
    pub display_list: DisplayList,
    /// Union of the content's bounds, before transform.
    pub bounds: Rect,
    /// Applied at composite time, not bake time — changing it does not
    /// invalidate the layer's rasterized surface.
    pub transform: Transform2D,
    pub opacity: f32,
    /// Does this layer move with the page scroll? `position: fixed` says no,
    /// and that is the whole reason it needs its own layer.
    pub scrolls: bool,
}

impl Layer {
    pub fn is_trivial(&self) -> bool {
        self.transform.is_identity() && self.opacity >= 1.0
    }
}

/// Layers in paint order: back to front.
#[derive(Debug, Clone, Default)]
pub struct LayerTree {
    pub layers: Vec<Layer>,
    /// Total document height, for clamping scroll.
    pub content_height: f32,
}

impl LayerTree {
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Total display items across all layers.
    pub fn item_count(&self) -> usize {
        self.layers.iter().map(|l| l.display_list.len()).sum()
    }

    pub fn summary(&self) -> String {
        let fixed = self.layers.iter().filter(|l| !l.scrolls).count();
        format!(
            "{} layers ({} fixed), {} items, content {}px",
            self.layers.len(),
            fixed,
            self.item_count(),
            self.content_height.round()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_composes_to_nothing() {
        let t = Transform2D::IDENTITY;
        assert!(t.then(&Transform2D::IDENTITY).is_identity());
        assert_eq!(t.apply(3.0, 4.0), (3.0, 4.0));
    }

    #[test]
    fn translation_composes() {
        let a = Transform2D::translate(10.0, 5.0);
        let b = Transform2D::translate(1.0, 2.0);
        assert_eq!(a.then(&b).apply(0.0, 0.0), (11.0, 7.0));
    }

    #[test]
    fn a_rotated_rect_gets_a_bigger_bounding_box() {
        // 45 degrees: a square's AABB grows by sqrt(2).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let rot = Transform2D {
            a: s,
            b: s,
            c: -s,
            d: s,
            e: 0.0,
            f: 0.0,
        };
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let out = rot.transform_rect(&r);
        assert!(
            (out.width - 10.0 * std::f32::consts::SQRT_2).abs() < 0.01,
            "got {}",
            out.width
        );
    }

    #[test]
    fn scale_then_translate_is_not_translate_then_scale() {
        // Composition order matters, and getting it backwards is the classic
        // transform bug.
        let scale = Transform2D {
            a: 2.0,
            d: 2.0,
            ..Transform2D::IDENTITY
        };
        let translate = Transform2D::translate(10.0, 0.0);
        assert_ne!(
            scale.then(&translate).apply(1.0, 0.0),
            translate.then(&scale).apply(1.0, 0.0)
        );
    }
}
