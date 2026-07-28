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

/// What the painter saw on the way through.
#[derive(Debug, Default, Clone, Copy)]
pub struct PaintStats {
    pub elements: usize,
    pub text_runs: usize,
    /// Text runs whose family could not be resolved to any face.
    pub unresolved_fonts: usize,
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
    };
    painter.node(NodeId::DOCUMENT);

    (list, stats)
}

struct Painter<'a> {
    dom: &'a Dom,
    layout: &'a mut LayoutEngine,
    inline: &'a mut InlineLayout,
    ctx: ResolveContext,
    list: &'a mut DisplayList,
    stats: &'a mut PaintStats,
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
