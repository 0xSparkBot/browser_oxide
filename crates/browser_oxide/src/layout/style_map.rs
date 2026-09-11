use crate::css_cascade::ComputedStyle;
use crate::css_values::property::{CssValue, PropertyId};
use crate::css_values::types::display::BorderStyle;
use crate::layout::resolve::{resolve_calc_expression, resolve_length, ResolveContext};

/// Convert a ComputedStyle into a taffy::Style.
pub fn computed_to_taffy(style: &ComputedStyle, ctx: &ResolveContext) -> taffy::Style {
    let mut ts = taffy::Style::default();

    // Display
    if let Some(CssValue::Display(d)) = style.get(&PropertyId::Display) {
        use crate::css_values::types::display::Display;
        ts.display = match d {
            Display::None => taffy::Display::None,
            Display::Flex | Display::InlineFlex => taffy::Display::Flex,
            Display::Grid | Display::InlineGrid => taffy::Display::Grid,
            _ => taffy::Display::Block,
        };
    }

    // Position
    if let Some(CssValue::Position(p)) = style.get(&PropertyId::Position) {
        use crate::css_values::types::display::Position;
        ts.position = match p {
            Position::Relative => taffy::Position::Relative,
            Position::Absolute | Position::Fixed => taffy::Position::Absolute,
            _ => taffy::Position::Relative,
        };
    }

    // Width / Height
    ts.size.width = css_to_dimension(style, &PropertyId::Width, ctx);
    ts.size.height = css_to_dimension(style, &PropertyId::Height, ctx);
    ts.min_size.width = css_to_dimension(style, &PropertyId::MinWidth, ctx);
    ts.min_size.height = css_to_dimension(style, &PropertyId::MinHeight, ctx);
    ts.max_size.width = css_to_dimension(style, &PropertyId::MaxWidth, ctx);
    ts.max_size.height = css_to_dimension(style, &PropertyId::MaxHeight, ctx);

    // Margin
    ts.margin.top = css_to_lpa(style, &PropertyId::MarginTop, ctx);
    ts.margin.right = css_to_lpa(style, &PropertyId::MarginRight, ctx);
    ts.margin.bottom = css_to_lpa(style, &PropertyId::MarginBottom, ctx);
    ts.margin.left = css_to_lpa(style, &PropertyId::MarginLeft, ctx);

    // Padding
    ts.padding.top = css_to_lp(style, &PropertyId::PaddingTop, ctx);
    ts.padding.right = css_to_lp(style, &PropertyId::PaddingRight, ctx);
    ts.padding.bottom = css_to_lp(style, &PropertyId::PaddingBottom, ctx);
    ts.padding.left = css_to_lp(style, &PropertyId::PaddingLeft, ctx);

    // Border
    ts.border.top = css_to_border(
        style,
        &PropertyId::BorderTopWidth,
        &PropertyId::BorderTopStyle,
        ctx,
    );
    ts.border.right = css_to_border(
        style,
        &PropertyId::BorderRightWidth,
        &PropertyId::BorderRightStyle,
        ctx,
    );
    ts.border.bottom = css_to_border(
        style,
        &PropertyId::BorderBottomWidth,
        &PropertyId::BorderBottomStyle,
        ctx,
    );
    ts.border.left = css_to_border(
        style,
        &PropertyId::BorderLeftWidth,
        &PropertyId::BorderLeftStyle,
        ctx,
    );

    // Flex
    if let Some(CssValue::FlexDirection(fd)) = style.get(&PropertyId::FlexDirection) {
        use crate::css_values::types::display::FlexDirection;
        ts.flex_direction = match fd {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
            FlexDirection::Column => taffy::FlexDirection::Column,
            FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
        };
    }
    if let Some(CssValue::FlexWrap(fw)) = style.get(&PropertyId::FlexWrap) {
        use crate::css_values::types::display::FlexWrap;
        ts.flex_wrap = match fw {
            FlexWrap::Nowrap => taffy::FlexWrap::NoWrap,
            FlexWrap::Wrap => taffy::FlexWrap::Wrap,
            FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        };
    }
    if let Some(CssValue::Number(v)) = style.get(&PropertyId::FlexGrow) {
        ts.flex_grow = *v as f32;
    }
    if let Some(CssValue::Number(v)) = style.get(&PropertyId::FlexShrink) {
        ts.flex_shrink = *v as f32;
    }

    // Gap
    if let Some(CssValue::LengthPercentage(lp)) = style.get(&PropertyId::RowGap) {
        ts.gap.height = css_lp_value_to_taffy(lp, ctx);
    }
    if let Some(CssValue::LengthPercentage(lp)) = style.get(&PropertyId::ColumnGap) {
        ts.gap.width = css_lp_value_to_taffy(lp, ctx);
    }

    // Box sizing
    if let Some(CssValue::BoxSizing(bs)) = style.get(&PropertyId::BoxSizing) {
        use crate::css_values::types::display::BoxSizing;
        ts.box_sizing = match bs {
            BoxSizing::ContentBox => taffy::BoxSizing::ContentBox,
            BoxSizing::BorderBox => taffy::BoxSizing::BorderBox,
        };
    }

    ts
}

fn css_to_dimension(
    style: &ComputedStyle,
    prop: &PropertyId,
    ctx: &ResolveContext,
) -> taffy::Dimension {
    match style.get(prop) {
        Some(CssValue::LengthPercentageAuto(lpa)) => {
            use crate::css_values::types::length::LengthPercentageAuto as CssLPA;
            match lpa {
                CssLPA::Length(l) => taffy::Dimension::length(resolve_length(l, ctx)),
                CssLPA::Percentage(p) => taffy::Dimension::percent(*p as f32 / 100.0),
                CssLPA::Auto => taffy::Dimension::auto(),
                CssLPA::Calc(expr) if expr.is_definite_length() => {
                    taffy::Dimension::length(resolve_calc_expression(expr, ctx, 0.0))
                }
                CssLPA::Calc(_) => taffy::Dimension::auto(),
            }
        }
        Some(CssValue::LengthPercentage(lp)) => {
            use crate::css_values::types::length::LengthPercentage as CssLP;
            match lp {
                CssLP::Length(l) => taffy::Dimension::length(resolve_length(l, ctx)),
                CssLP::Percentage(p) => taffy::Dimension::percent(*p as f32 / 100.0),
                CssLP::Calc(expr) if expr.is_definite_length() => {
                    taffy::Dimension::length(resolve_calc_expression(expr, ctx, 0.0))
                }
                CssLP::Calc(_) => taffy::Dimension::auto(),
            }
        }
        Some(CssValue::Length(l)) => taffy::Dimension::length(resolve_length(l, ctx)),
        _ => taffy::Dimension::auto(),
    }
}

fn css_to_lpa(
    style: &ComputedStyle,
    prop: &PropertyId,
    ctx: &ResolveContext,
) -> taffy::LengthPercentageAuto {
    use crate::css_values::types::length::LengthPercentageAuto as CssLPA;
    match style.get(prop) {
        Some(CssValue::LengthPercentageAuto(lpa)) => match lpa {
            CssLPA::Length(l) => taffy::LengthPercentageAuto::length(resolve_length(l, ctx)),
            CssLPA::Percentage(p) => taffy::LengthPercentageAuto::percent(*p as f32 / 100.0),
            CssLPA::Auto => taffy::LengthPercentageAuto::auto(),
            CssLPA::Calc(expr) if expr.is_definite_length() => {
                taffy::LengthPercentageAuto::length(resolve_calc_expression(expr, ctx, 0.0))
            }
            CssLPA::Calc(_) => taffy::LengthPercentageAuto::auto(),
        },
        _ => taffy::LengthPercentageAuto::length(0.0),
    }
}

fn css_to_lp(
    style: &ComputedStyle,
    prop: &PropertyId,
    ctx: &ResolveContext,
) -> taffy::LengthPercentage {
    match style.get(prop) {
        Some(CssValue::LengthPercentage(lp)) => css_lp_value_to_taffy(lp, ctx),
        _ => taffy::LengthPercentage::length(0.0),
    }
}

fn css_lp_value_to_taffy(
    lp: &crate::css_values::types::length::LengthPercentage,
    ctx: &ResolveContext,
) -> taffy::LengthPercentage {
    use crate::css_values::types::length::LengthPercentage as CssLP;
    match lp {
        CssLP::Length(l) => taffy::LengthPercentage::length(resolve_length(l, ctx)),
        // Keep a plain percentage symbolic so Taffy resolves it against the
        // real containing block instead of prematurely using the viewport.
        CssLP::Percentage(p) => taffy::LengthPercentage::percent(*p as f32 / 100.0),
        // TaffyTree 0.12 does not expose a custom calc resolver (its
        // resolve_calc_value is hard-coded to 0). Pure length/math calc can be
        // folded safely here; mixed percentage calc must stay deferred rather
        // than being evaluated against the wrong basis.
        CssLP::Calc(expr) if expr.is_definite_length() => {
            taffy::LengthPercentage::length(resolve_calc_expression(expr, ctx, 0.0))
        }
        CssLP::Calc(_) => taffy::LengthPercentage::length(0.0),
    }
}

fn css_to_border(
    style: &ComputedStyle,
    width_prop: &PropertyId,
    style_prop: &PropertyId,
    ctx: &ResolveContext,
) -> taffy::LengthPercentage {
    if matches!(
        style.get(style_prop),
        Some(CssValue::BorderStyle(
            BorderStyle::None | BorderStyle::Hidden
        ))
    ) {
        return taffy::LengthPercentage::length(0.0);
    }
    match style.get(width_prop) {
        Some(CssValue::Length(l)) => taffy::LengthPercentage::length(resolve_length(l, ctx)),
        _ => taffy::LengthPercentage::length(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_values::types::display::Display;
    use crate::css_values::types::length::{
        CalcExpr, CalcValue, Length, LengthPercentage, LengthPercentageAuto, LengthUnit,
    };
    use crate::layout::resolve::ResolveContext;
    use std::collections::HashMap;

    #[test]
    fn default_style_maps() {
        let style = ComputedStyle::resolve(&HashMap::new(), None);
        let ctx = ResolveContext::default();
        let ts = computed_to_taffy(&style, &ctx);
        assert!(matches!(ts.display, taffy::Display::Block));
    }

    #[test]
    fn flex_display_maps() {
        let mut cascaded = HashMap::new();
        cascaded.insert(PropertyId::Display, CssValue::Display(Display::Flex));
        let style = ComputedStyle::resolve(&cascaded, None);
        let ctx = ResolveContext::default();
        let ts = computed_to_taffy(&style, &ctx);
        assert!(matches!(ts.display, taffy::Display::Flex));
    }

    #[test]
    fn width_px_maps() {
        let mut cascaded = HashMap::new();
        cascaded.insert(
            PropertyId::Width,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Length(Length::Px(200.0))),
        );
        let style = ComputedStyle::resolve(&cascaded, None);
        let ctx = ResolveContext::default();
        let ts = computed_to_taffy(&style, &ctx);
        // taffy 0.8 made Dimension a newtype struct around CompactLength;
        // there are no pattern-matchable variants any more. Compare via
        // equality against the canonical constructor.
        assert_eq!(ts.size.width, taffy::Dimension::length(200.0));
    }

    #[test]
    fn pure_calc_width_maps_to_resolved_length() {
        let expr = CalcExpr::Add(
            Box::new(CalcExpr::Value(CalcValue::Length(100.0, LengthUnit::Px))),
            Box::new(CalcExpr::Value(CalcValue::Length(2.0, LengthUnit::Em))),
        );
        let mut cascaded = HashMap::new();
        cascaded.insert(
            PropertyId::Width,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Calc(Box::new(expr))),
        );
        let style = ComputedStyle::resolve(&cascaded, None);
        let ctx = ResolveContext {
            font_size: 20.0,
            ..ResolveContext::default()
        };
        let ts = computed_to_taffy(&style, &ctx);
        assert_eq!(ts.size.width, taffy::Dimension::length(140.0));
    }

    #[test]
    fn incompatible_calc_width_stays_deferred() {
        let expr = CalcExpr::Add(
            Box::new(CalcExpr::Value(CalcValue::Length(100.0, LengthUnit::Px))),
            Box::new(CalcExpr::Value(CalcValue::Number(2.0))),
        );
        let mut cascaded = HashMap::new();
        cascaded.insert(
            PropertyId::Width,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Calc(Box::new(expr))),
        );
        let style = ComputedStyle::resolve(&cascaded, None);
        let ts = computed_to_taffy(&style, &ResolveContext::default());
        assert_eq!(ts.size.width, taffy::Dimension::auto());
    }

    #[test]
    fn plain_gap_percentage_remains_symbolic() {
        let mut cascaded = HashMap::new();
        cascaded.insert(
            PropertyId::ColumnGap,
            CssValue::LengthPercentage(LengthPercentage::Percentage(25.0)),
        );
        let style = ComputedStyle::resolve(&cascaded, None);
        let ts = computed_to_taffy(&style, &ResolveContext::default());
        assert_eq!(ts.gap.width, taffy::LengthPercentage::percent(0.25));
    }

    #[test]
    fn margin_auto_maps() {
        let mut cascaded = HashMap::new();
        cascaded.insert(
            PropertyId::MarginLeft,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Auto),
        );
        let style = ComputedStyle::resolve(&cascaded, None);
        let ctx = ResolveContext::default();
        let ts = computed_to_taffy(&style, &ctx);
        assert_eq!(ts.margin.left, taffy::LengthPercentageAuto::auto());
    }

    #[test]
    fn border_none_and_hidden_suppress_computed_layout_width() {
        let ctx = ResolveContext::default();
        for border_style in [BorderStyle::None, BorderStyle::Hidden] {
            let mut cascaded = HashMap::new();
            cascaded.insert(
                PropertyId::BorderTopWidth,
                CssValue::Length(Length::Px(9.0)),
            );
            cascaded.insert(
                PropertyId::BorderTopStyle,
                CssValue::BorderStyle(border_style),
            );
            let style = ComputedStyle::resolve(&cascaded, None);
            let ts = computed_to_taffy(&style, &ctx);
            assert_eq!(ts.border.top, taffy::LengthPercentage::length(0.0));
        }
    }

    #[test]
    fn visible_border_style_preserves_computed_layout_width() {
        let mut cascaded = HashMap::new();
        cascaded.insert(
            PropertyId::BorderTopWidth,
            CssValue::Length(Length::Px(9.0)),
        );
        cascaded.insert(
            PropertyId::BorderTopStyle,
            CssValue::BorderStyle(BorderStyle::Solid),
        );
        let style = ComputedStyle::resolve(&cascaded, None);
        let ts = computed_to_taffy(&style, &ResolveContext::default());
        assert_eq!(ts.border.top, taffy::LengthPercentage::length(9.0));
    }
}
