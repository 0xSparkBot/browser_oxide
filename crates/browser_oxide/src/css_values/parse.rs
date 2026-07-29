use crate::css_parser::{ComponentValue, Token, TokenKind};
use crate::css_values::error::ValueError;
use crate::css_values::property::*;
use crate::css_values::types::color::{self, Color};
use crate::css_values::types::display::*;
use crate::css_values::types::font::*;
use crate::css_values::types::length::*;

/// Parse a property name + value tokens into a typed PropertyDeclaration.
pub fn parse_property(
    name: &str,
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let value_trimmed = trim_whitespace(value);
    let name_lower = name.to_ascii_lowercase();

    // CSS-wide keywords. On a shorthand the keyword applies to every longhand
    // in the family, not to a property named after the shorthand — without
    // that, `button { font: inherit }`, which every reset stylesheet on the web
    // contains, lands on a `Custom("font")` entry that nothing reads.
    if let Some(keyword) = try_css_wide_keyword(value_trimmed) {
        if let Some(longhands) = shorthand_longhands(&name_lower) {
            return Ok(longhands
                .into_iter()
                .map(|property| PropertyDeclaration {
                    property,
                    value: keyword.clone(),
                    important,
                })
                .collect());
        }
        return Ok(vec![PropertyDeclaration {
            property: PropertyId::from_name(name),
            value: keyword,
            important,
        }]);
    }

    // Shorthand expansion
    match name_lower.as_str() {
        "margin" => return parse_box_shorthand(value_trimmed, important, "margin"),
        "padding" => return parse_box_shorthand(value_trimmed, important, "padding"),
        "overflow" => return parse_overflow_shorthand(value_trimmed, important),
        "background" => return parse_background_shorthand(value_trimmed, important),
        "font" => return parse_font_shorthand(value_trimmed, important),
        "flex" => return parse_flex_shorthand(value_trimmed, important),
        "flex-flow" => return parse_flex_flow_shorthand(value_trimmed, important),
        "gap" => return parse_gap_shorthand(value_trimmed, important),
        "place-items" | "place-content" | "place-self" => {
            return parse_place_shorthand(&name_lower, value_trimmed, important)
        }
        "border-width" => {
            return parse_border_side_shorthand(value_trimmed, important, BorderSide::Width)
        }
        "border-style" => {
            return parse_border_side_shorthand(value_trimmed, important, BorderSide::Style)
        }
        "border-color" => {
            return parse_border_side_shorthand(value_trimmed, important, BorderSide::Color)
        }
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            return parse_border_shorthand(&name_lower, value_trimmed, important)
        }
        _ => {}
    }

    // Longhand properties
    let css_value = match name_lower.as_str() {
        "display" => parse_display(value_trimmed)?,
        "position" => parse_position(value_trimmed)?,
        "width" | "height" | "min-width" | "min-height" => {
            parse_length_percentage_auto(value_trimmed)?
        }
        "max-width" | "max-height" => parse_length_percentage_auto_none(value_trimmed)?,
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
            parse_length_percentage_auto(value_trimmed)?
        }
        "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
            parse_length_percentage(value_trimmed)?
        }
        "border-top-width" | "border-right-width" | "border-bottom-width" | "border-left-width" => {
            parse_border_width(value_trimmed)?
        }
        "border-top-style" | "border-right-style" | "border-bottom-style" | "border-left-style" => {
            parse_border_style(value_trimmed)?
        }
        "border-top-color" | "border-right-color" | "border-bottom-color" | "border-left-color" => {
            parse_color(value_trimmed)?
        }
        "box-sizing" => parse_box_sizing(value_trimmed)?,
        "overflow-x" | "overflow-y" => parse_overflow(value_trimmed)?,
        "float" => parse_float(value_trimmed)?,
        "clear" => parse_clear(value_trimmed)?,
        "flex-direction" => parse_flex_direction(value_trimmed)?,
        "flex-wrap" => parse_flex_wrap(value_trimmed)?,
        "flex-grow" | "flex-shrink" => parse_number(value_trimmed)?,
        "flex-basis" => parse_length_percentage_auto(value_trimmed)?,
        "align-items" | "align-self" | "align-content" | "justify-content" | "justify-items"
        | "justify-self" => parse_alignment(value_trimmed)?,
        "row-gap" | "column-gap" => parse_length_percentage(value_trimmed)?,
        "font-size" => parse_font_size(value_trimmed)?,
        "font-family" => parse_font_family(value_trimmed)?,
        "font-weight" => parse_font_weight(value_trimmed)?,
        "font-style" => parse_font_style(value_trimmed)?,
        "line-height" => parse_line_height(value_trimmed)?,
        "text-align" => parse_text_align(value_trimmed)?,
        "white-space" => parse_white_space(value_trimmed)?,
        "color" | "background-color" => parse_color(value_trimmed)?,
        "visibility" => parse_visibility(value_trimmed)?,
        "opacity" => parse_number(value_trimmed)?,
        "transform" => parse_transform(value_trimmed)?,
        "z-index" => parse_z_index(value_trimmed)?,
        "content-visibility" => parse_content_visibility(value_trimmed)?,
        _ if name_lower.starts_with("--") => {
            CssValue::CustomValue(component_values_to_string(value_trimmed))
        }
        _ => return Err(ValueError::UnknownProperty(name_lower)),
    };

    Ok(vec![PropertyDeclaration {
        property: PropertyId::from_name(name),
        value: css_value,
        important,
    }])
}

// --- Individual parsers ---

fn try_css_wide_keyword(value: &[ComponentValue<'_>]) -> Option<CssValue> {
    if value.len() != 1 {
        return None;
    }
    match &value[0] {
        ComponentValue::Token(Token {
            kind: TokenKind::Ident(name),
            ..
        }) => match name.to_ascii_lowercase().as_str() {
            "initial" => Some(CssValue::Initial),
            "inherit" => Some(CssValue::Inherit),
            "unset" => Some(CssValue::Unset),
            "revert" => Some(CssValue::Revert),
            "revert-layer" => Some(CssValue::RevertLayer),
            _ => None,
        },
        _ => None,
    }
}

fn parse_display(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let d = match ident.to_ascii_lowercase().as_str() {
        "none" => Display::None,
        "block" => Display::Block,
        "inline" => Display::Inline,
        "inline-block" => Display::InlineBlock,
        "flex" => Display::Flex,
        "inline-flex" => Display::InlineFlex,
        "grid" => Display::Grid,
        "inline-grid" => Display::InlineGrid,
        "table" => Display::Table,
        "inline-table" => Display::InlineTable,
        "list-item" => Display::ListItem,
        "flow-root" => Display::FlowRoot,
        "contents" => Display::Contents,
        "table-row" => Display::TableRow,
        "table-cell" => Display::TableCell,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid display: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Display(d))
}

fn parse_position(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let p = match ident.to_ascii_lowercase().as_str() {
        "static" => Position::Static,
        "relative" => Position::Relative,
        "absolute" => Position::Absolute,
        "fixed" => Position::Fixed,
        "sticky" => Position::Sticky,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid position: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Position(p))
}

fn parse_length_percentage_auto(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            if ident.eq_ignore_ascii_case("auto") {
                return Ok(CssValue::LengthPercentageAuto(LengthPercentageAuto::Auto));
            }
        }
        if let Some(lp) = try_length_percentage(&value[0]) {
            return Ok(CssValue::LengthPercentageAuto(match lp {
                LengthPercentage::Length(l) => LengthPercentageAuto::Length(l),
                LengthPercentage::Percentage(p) => LengthPercentageAuto::Percentage(p),
                LengthPercentage::Calc(c) => LengthPercentageAuto::Calc(c),
            }));
        }
    }
    Err(ValueError::InvalidValue(
        "expected length, percentage, or auto".into(),
    ))
}

fn parse_length_percentage_auto_none(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            if ident.eq_ignore_ascii_case("none") {
                // max-width: none → no limit (represented as auto)
                return Ok(CssValue::LengthPercentageAuto(LengthPercentageAuto::Auto));
            }
        }
    }
    parse_length_percentage_auto(value)
}

fn parse_length_percentage(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(lp) = try_length_percentage(&value[0]) {
            return Ok(CssValue::LengthPercentage(lp));
        }
    }
    Err(ValueError::InvalidValue(
        "expected length or percentage".into(),
    ))
}

fn parse_border_width(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            let px = match ident.to_ascii_lowercase().as_str() {
                "thin" => 1.0,
                "medium" => 3.0,
                "thick" => 5.0,
                _ => {
                    return Err(ValueError::InvalidValue(format!(
                        "invalid border-width: {}",
                        ident
                    )))
                }
            };
            return Ok(CssValue::Length(Length::Px(px)));
        }
        if let Some(lp) = try_length_percentage(&value[0]) {
            return Ok(match lp {
                LengthPercentage::Length(l) => CssValue::Length(l),
                _ => {
                    return Err(ValueError::InvalidValue(
                        "border-width must be a length".into(),
                    ))
                }
            });
        }
    }
    Err(ValueError::InvalidValue(
        "expected border-width value".into(),
    ))
}

fn parse_box_sizing(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    match ident.to_ascii_lowercase().as_str() {
        "content-box" => Ok(CssValue::BoxSizing(BoxSizing::ContentBox)),
        "border-box" => Ok(CssValue::BoxSizing(BoxSizing::BorderBox)),
        _ => Err(ValueError::InvalidValue(format!(
            "invalid box-sizing: {}",
            ident
        ))),
    }
}

fn parse_overflow(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let o = match ident.to_ascii_lowercase().as_str() {
        "visible" => Overflow::Visible,
        "hidden" => Overflow::Hidden,
        "scroll" => Overflow::Scroll,
        "auto" => Overflow::Auto,
        "clip" => Overflow::Clip,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid overflow: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Overflow(o))
}

fn parse_float(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let f = match ident.to_ascii_lowercase().as_str() {
        "none" => Float::None,
        "left" => Float::Left,
        "right" => Float::Right,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid float: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Float(f))
}

fn parse_clear(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let c = match ident.to_ascii_lowercase().as_str() {
        "none" => Clear::None,
        "left" => Clear::Left,
        "right" => Clear::Right,
        "both" => Clear::Both,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid clear: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Clear(c))
}

fn parse_flex_direction(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let d = match ident.to_ascii_lowercase().as_str() {
        "row" => FlexDirection::Row,
        "row-reverse" => FlexDirection::RowReverse,
        "column" => FlexDirection::Column,
        "column-reverse" => FlexDirection::ColumnReverse,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid flex-direction: {}",
                ident
            )))
        }
    };
    Ok(CssValue::FlexDirection(d))
}

fn parse_flex_wrap(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let w = match ident.to_ascii_lowercase().as_str() {
        "nowrap" => FlexWrap::Nowrap,
        "wrap" => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid flex-wrap: {}",
                ident
            )))
        }
    };
    Ok(CssValue::FlexWrap(w))
}

fn parse_alignment(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let a = match ident.to_ascii_lowercase().as_str() {
        "normal" => AlignmentValue::Normal,
        "stretch" => AlignmentValue::Stretch,
        "center" => AlignmentValue::Center,
        "start" => AlignmentValue::Start,
        "end" => AlignmentValue::End,
        "flex-start" => AlignmentValue::FlexStart,
        "flex-end" => AlignmentValue::FlexEnd,
        "baseline" => AlignmentValue::Baseline,
        "space-between" => AlignmentValue::SpaceBetween,
        "space-around" => AlignmentValue::SpaceAround,
        "space-evenly" => AlignmentValue::SpaceEvenly,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid alignment: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Alignment(a))
}

fn parse_number(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let ComponentValue::Token(Token {
            kind: TokenKind::Number { value: v, .. },
            ..
        }) = &value[0]
        {
            return Ok(CssValue::Number(*v));
        }
    }
    Err(ValueError::InvalidValue("expected number".into()))
}

fn parse_font_size(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            let px = match ident.to_ascii_lowercase().as_str() {
                "xx-small" => 9.0,
                "x-small" => 10.0,
                "small" => 13.0,
                "medium" => 16.0,
                "large" => 18.0,
                "x-large" => 24.0,
                "xx-large" => 32.0,
                "xxx-large" => 48.0,
                "smaller" | "larger" => {
                    // Relative sizes — need parent context, store as keyword for now
                    return Ok(CssValue::LengthPercentage(LengthPercentage::Length(
                        Length::Em(if ident.eq_ignore_ascii_case("larger") {
                            1.2
                        } else {
                            0.833
                        }),
                    )));
                }
                _ => {
                    return Err(ValueError::InvalidValue(format!(
                        "invalid font-size: {}",
                        ident
                    )))
                }
            };
            return Ok(CssValue::LengthPercentage(LengthPercentage::Length(
                Length::Px(px),
            )));
        }
        if let Some(lp) = try_length_percentage(&value[0]) {
            return Ok(CssValue::LengthPercentage(lp));
        }
    }
    Err(ValueError::InvalidValue("expected font-size value".into()))
}

fn parse_font_family(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let mut families = Vec::new();
    let mut current_name_parts: Vec<String> = Vec::new();

    for cv in value {
        match cv {
            ComponentValue::Token(Token {
                kind: TokenKind::Comma,
                ..
            }) if !current_name_parts.is_empty() => {
                let name = current_name_parts.join(" ");
                families.push(match_generic_or_named(&name));
                current_name_parts.clear();
            }
            ComponentValue::Token(Token {
                kind: TokenKind::Ident(name),
                ..
            }) => {
                current_name_parts.push(name.to_string());
            }
            ComponentValue::Token(Token {
                kind: TokenKind::String(name),
                ..
            }) => {
                families.push(FontFamily::Named(name.to_string()));
            }
            ComponentValue::Token(Token {
                kind: TokenKind::Whitespace,
                ..
            }) => {}
            _ => {}
        }
    }

    if !current_name_parts.is_empty() {
        let name = current_name_parts.join(" ");
        families.push(match_generic_or_named(&name));
    }

    if families.is_empty() {
        return Err(ValueError::InvalidValue("expected font-family".into()));
    }

    Ok(CssValue::FontFamily(families))
}

fn match_generic_or_named(name: &str) -> FontFamily {
    match name.to_ascii_lowercase().as_str() {
        "serif" => FontFamily::Generic(GenericFamily::Serif),
        "sans-serif" => FontFamily::Generic(GenericFamily::SansSerif),
        "monospace" => FontFamily::Generic(GenericFamily::Monospace),
        "cursive" => FontFamily::Generic(GenericFamily::Cursive),
        "fantasy" => FontFamily::Generic(GenericFamily::Fantasy),
        "system-ui" => FontFamily::Generic(GenericFamily::SystemUi),
        "ui-serif" => FontFamily::Generic(GenericFamily::UiSerif),
        "ui-sans-serif" => FontFamily::Generic(GenericFamily::UiSansSerif),
        "ui-monospace" => FontFamily::Generic(GenericFamily::UiMonospace),
        "ui-rounded" => FontFamily::Generic(GenericFamily::UiRounded),
        "emoji" => FontFamily::Generic(GenericFamily::Emoji),
        "math" => FontFamily::Generic(GenericFamily::Math),
        _ => FontFamily::Named(name.to_string()),
    }
}

fn parse_font_weight(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            let w = match ident.to_ascii_lowercase().as_str() {
                "normal" => FontWeight::Normal,
                "bold" => FontWeight::Bold,
                "bolder" => FontWeight::Bolder,
                "lighter" => FontWeight::Lighter,
                _ => {
                    return Err(ValueError::InvalidValue(format!(
                        "invalid font-weight: {}",
                        ident
                    )))
                }
            };
            return Ok(CssValue::FontWeight(w));
        }
        if let ComponentValue::Token(Token {
            kind: TokenKind::Number { value: v, .. },
            ..
        }) = &value[0]
        {
            return Ok(CssValue::FontWeight(FontWeight::Numeric(*v)));
        }
    }
    Err(ValueError::InvalidValue(
        "expected font-weight value".into(),
    ))
}

fn parse_font_style(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let s = match ident.to_ascii_lowercase().as_str() {
        "normal" => FontStyle::Normal,
        "italic" => FontStyle::Italic,
        "oblique" => FontStyle::Oblique(None),
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid font-style: {}",
                ident
            )))
        }
    };
    Ok(CssValue::FontStyle(s))
}

fn parse_line_height(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            if ident.eq_ignore_ascii_case("normal") {
                return Ok(CssValue::LineHeight(LineHeight::Normal));
            }
        }
        if let ComponentValue::Token(Token {
            kind: TokenKind::Number { value: v, .. },
            ..
        }) = &value[0]
        {
            return Ok(CssValue::LineHeight(LineHeight::Number(*v)));
        }
        if let Some(lp) = try_length_percentage(&value[0]) {
            return Ok(match lp {
                LengthPercentage::Length(l) => CssValue::LineHeight(LineHeight::Length(l)),
                LengthPercentage::Percentage(p) => CssValue::LineHeight(LineHeight::Percentage(p)),
                _ => return Err(ValueError::InvalidValue("invalid line-height".into())),
            });
        }
    }
    Err(ValueError::InvalidValue(
        "expected line-height value".into(),
    ))
}

fn parse_text_align(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let a = match ident.to_ascii_lowercase().as_str() {
        "left" => TextAlign::Left,
        "right" => TextAlign::Right,
        "center" => TextAlign::Center,
        "justify" => TextAlign::Justify,
        "start" => TextAlign::Start,
        "end" => TextAlign::End,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid text-align: {}",
                ident
            )))
        }
    };
    Ok(CssValue::TextAlign(a))
}

fn parse_white_space(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let w = match ident.to_ascii_lowercase().as_str() {
        "normal" => WhiteSpace::Normal,
        "nowrap" => WhiteSpace::Nowrap,
        "pre" => WhiteSpace::Pre,
        "pre-wrap" => WhiteSpace::PreWrap,
        "pre-line" => WhiteSpace::PreLine,
        "break-spaces" => WhiteSpace::BreakSpaces,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid white-space: {}",
                ident
            )))
        }
    };
    Ok(CssValue::WhiteSpace(w))
}

fn parse_color(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    // Named color or currentcolor
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            if let Some(c) = color::named_color(&ident) {
                return Ok(CssValue::Color(c));
            }
        }
        // Hex color
        if let ComponentValue::Token(Token {
            kind: TokenKind::Hash { value: hex, .. },
            ..
        }) = &value[0]
        {
            if let Some(c) = parse_hex_color(hex) {
                return Ok(CssValue::Color(c));
            }
        }
    }
    // rgb() / rgba() / hsl() / hsla() functions
    if value.len() == 1 {
        if let ComponentValue::Function(f) = &value[0] {
            match f.name.to_ascii_lowercase().as_str() {
                "rgb" | "rgba" => return parse_rgb_function(&f.arguments),
                "hsl" | "hsla" => return parse_hsl_function(&f.arguments),
                _ => {}
            }
        }
    }
    Err(ValueError::InvalidValue("expected color value".into()))
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim();
    let chars: Vec<u8> = hex.bytes().collect();
    match chars.len() {
        3 => {
            let r = hex_digit(chars[0])? * 17;
            let g = hex_digit(chars[1])? * 17;
            let b = hex_digit(chars[2])? * 17;
            Some(Color::Rgba { r, g, b, a: 1.0 })
        }
        4 => {
            let r = hex_digit(chars[0])? * 17;
            let g = hex_digit(chars[1])? * 17;
            let b = hex_digit(chars[2])? * 17;
            let a = hex_digit(chars[3])? as f32 * 17.0 / 255.0;
            Some(Color::Rgba { r, g, b, a })
        }
        6 => {
            let r = hex_digit(chars[0])? * 16 + hex_digit(chars[1])?;
            let g = hex_digit(chars[2])? * 16 + hex_digit(chars[3])?;
            let b = hex_digit(chars[4])? * 16 + hex_digit(chars[5])?;
            Some(Color::Rgba { r, g, b, a: 1.0 })
        }
        8 => {
            let r = hex_digit(chars[0])? * 16 + hex_digit(chars[1])?;
            let g = hex_digit(chars[2])? * 16 + hex_digit(chars[3])?;
            let b = hex_digit(chars[4])? * 16 + hex_digit(chars[5])?;
            let a = (hex_digit(chars[6])? * 16 + hex_digit(chars[7])?) as f32 / 255.0;
            Some(Color::Rgba { r, g, b, a })
        }
        _ => None,
    }
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_rgb_function(args: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let nums = extract_numbers(args);
    if nums.len() >= 3 {
        let r = nums[0].clamp(0.0, 255.0) as u8;
        let g = nums[1].clamp(0.0, 255.0) as u8;
        let b = nums[2].clamp(0.0, 255.0) as u8;
        let a = if nums.len() >= 4 { nums[3] as f32 } else { 1.0 };
        Ok(CssValue::Color(Color::Rgba { r, g, b, a }))
    } else {
        Err(ValueError::InvalidValue("invalid rgb() arguments".into()))
    }
}

fn parse_hsl_function(args: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let nums = extract_numbers(args);
    if nums.len() >= 3 {
        let h = nums[0];
        let s = nums[1];
        let l = nums[2];
        let a = if nums.len() >= 4 { nums[3] as f32 } else { 1.0 };
        Ok(CssValue::Color(Color::Hsl { h, s, l, a }))
    } else {
        Err(ValueError::InvalidValue("invalid hsl() arguments".into()))
    }
}

fn parse_visibility(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let v = match ident.to_ascii_lowercase().as_str() {
        "visible" => Visibility::Visible,
        "hidden" => Visibility::Hidden,
        "collapse" => Visibility::Collapse,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid visibility: {}",
                ident
            )))
        }
    };
    Ok(CssValue::Visibility(v))
}

fn parse_z_index(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            if ident.eq_ignore_ascii_case("auto") {
                return Ok(CssValue::Integer(0));
            }
        }
        if let ComponentValue::Token(Token {
            kind: TokenKind::Number {
                int_value: Some(v), ..
            },
            ..
        }) = &value[0]
        {
            return Ok(CssValue::Integer(*v as i32));
        }
    }
    Err(ValueError::InvalidValue("expected z-index value".into()))
}

fn parse_content_visibility(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    let ident = expect_single_ident(value)?;
    let cv = match ident.to_ascii_lowercase().as_str() {
        "visible" => ContentVisibility::Visible,
        "hidden" => ContentVisibility::Hidden,
        "auto" => ContentVisibility::Auto,
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid content-visibility: {}",
                ident
            )))
        }
    };
    Ok(CssValue::ContentVisibility(cv))
}

// --- Shorthand parsers ---

fn parse_box_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
    prefix: &str,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    // Collect non-whitespace values
    let parts: Vec<&ComponentValue<'_>> = value
        .iter()
        .filter(|v| {
            !matches!(
                v,
                ComponentValue::Token(Token {
                    kind: TokenKind::Whitespace,
                    ..
                })
            )
        })
        .collect();

    let (top, right, bottom, left) = match parts.len() {
        1 => (parts[0], parts[0], parts[0], parts[0]),
        2 => (parts[0], parts[1], parts[0], parts[1]),
        3 => (parts[0], parts[1], parts[2], parts[1]),
        4 => (parts[0], parts[1], parts[2], parts[3]),
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "invalid {} shorthand",
                prefix
            )))
        }
    };

    let sides = ["top", "right", "bottom", "left"];
    let values = [top, right, bottom, left];
    let mut declarations = Vec::new();

    for (side, val) in sides.iter().zip(values.iter()) {
        let prop_name = format!("{}-{}", prefix, side);
        let cv = if let Some(ident) = try_ident(val) {
            if ident.eq_ignore_ascii_case("auto") {
                CssValue::LengthPercentageAuto(LengthPercentageAuto::Auto)
            } else {
                return Err(ValueError::InvalidValue(format!(
                    "invalid {} value",
                    prefix
                )));
            }
        } else if let Some(lp) = try_length_percentage(val) {
            match lp {
                LengthPercentage::Length(l) => {
                    if prefix == "margin" {
                        CssValue::LengthPercentageAuto(LengthPercentageAuto::Length(l))
                    } else {
                        CssValue::LengthPercentage(LengthPercentage::Length(l))
                    }
                }
                LengthPercentage::Percentage(p) => {
                    if prefix == "margin" {
                        CssValue::LengthPercentageAuto(LengthPercentageAuto::Percentage(p))
                    } else {
                        CssValue::LengthPercentage(LengthPercentage::Percentage(p))
                    }
                }
                other => CssValue::LengthPercentage(other),
            }
        } else {
            return Err(ValueError::InvalidValue(format!(
                "invalid {} value",
                prefix
            )));
        };

        declarations.push(PropertyDeclaration {
            property: PropertyId::from_name(&prop_name),
            value: cv,
            important,
        });
    }

    Ok(declarations)
}

fn parse_overflow_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let parts: Vec<&ComponentValue<'_>> = value
        .iter()
        .filter(|v| {
            !matches!(
                v,
                ComponentValue::Token(Token {
                    kind: TokenKind::Whitespace,
                    ..
                })
            )
        })
        .collect();

    let (x_val, y_val) = match parts.len() {
        1 => (parts[0], parts[0]),
        2 => (parts[0], parts[1]),
        _ => {
            return Err(ValueError::InvalidValue(
                "invalid overflow shorthand".into(),
            ))
        }
    };

    let parse_single = |v: &ComponentValue<'_>| -> Result<CssValue, ValueError> {
        if try_ident(v).is_some() {
            parse_overflow(std::slice::from_ref(v))
        } else {
            Err(ValueError::InvalidValue("expected overflow keyword".into()))
        }
    };

    Ok(vec![
        PropertyDeclaration {
            property: PropertyId::OverflowX,
            value: parse_single(x_val)?,
            important,
        },
        PropertyDeclaration {
            property: PropertyId::OverflowY,
            value: parse_single(y_val)?,
            important,
        },
    ])
}

// --- Helpers ---

fn trim_whitespace<'a, 'b>(value: &'a [ComponentValue<'b>]) -> &'a [ComponentValue<'b>] {
    let start = value
        .iter()
        .position(|v| {
            !matches!(
                v,
                ComponentValue::Token(Token {
                    kind: TokenKind::Whitespace,
                    ..
                })
            )
        })
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|v| {
            !matches!(
                v,
                ComponentValue::Token(Token {
                    kind: TokenKind::Whitespace,
                    ..
                })
            )
        })
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        &[]
    } else {
        &value[start..end]
    }
}

fn expect_single_ident(value: &[ComponentValue<'_>]) -> Result<String, ValueError> {
    if value.len() == 1 {
        if let Some(ident) = try_ident(&value[0]) {
            return Ok(ident);
        }
    }
    Err(ValueError::InvalidValue(
        "expected single identifier".into(),
    ))
}

fn try_ident(cv: &ComponentValue<'_>) -> Option<String> {
    match cv {
        ComponentValue::Token(Token {
            kind: TokenKind::Ident(name),
            ..
        }) => Some(name.to_string()),
        _ => None,
    }
}

fn try_length_percentage(cv: &ComponentValue<'_>) -> Option<LengthPercentage> {
    match cv {
        ComponentValue::Token(Token {
            kind: TokenKind::Dimension { value, unit, .. },
            ..
        }) => {
            let length = match unit.to_ascii_lowercase().as_str() {
                "px" => Length::Px(*value),
                "em" => Length::Em(*value),
                "rem" => Length::Rem(*value),
                "vw" => Length::Vw(*value),
                "vh" => Length::Vh(*value),
                "vmin" => Length::Vmin(*value),
                "vmax" => Length::Vmax(*value),
                "cm" => Length::Cm(*value),
                "mm" => Length::Mm(*value),
                "in" => Length::In(*value),
                "pt" => Length::Pt(*value),
                "pc" => Length::Pc(*value),
                "ch" => Length::Ch(*value),
                "ex" => Length::Ex(*value),
                "cqw" => Length::Cqw(*value),
                "cqh" => Length::Cqh(*value),
                _ => return None,
            };
            Some(LengthPercentage::Length(length))
        }
        ComponentValue::Token(Token {
            kind: TokenKind::Percentage { value, .. },
            ..
        }) => Some(LengthPercentage::Percentage(*value)),
        ComponentValue::Token(Token {
            kind: TokenKind::Number { value, .. },
            ..
        }) if *value == 0.0 => Some(LengthPercentage::Length(Length::Zero)),
        // calc(), min(), max(), clamp(), and the CSS Values 4 math
        // functions. The parser builds a CalcExpr tree; resolution to
        // a concrete pixel value happens at computed-style time via
        // CalcExpr::evaluate(&CalcContext). Carrying the unresolved
        // tree means the parsed property value is correctly populated
        // even when the math depends on layout context (vw/em/etc.) —
        // and getComputedStyle output stays correct under the
        // challenge vendor's calc-fingerprint probe.
        ComponentValue::Function(f) => match crate::css_values::calc::parse_math_function(f) {
            Ok(Some(expr)) => Some(LengthPercentage::Calc(Box::new(expr))),
            _ => None,
        },
        _ => None,
    }
}

fn extract_numbers(args: &[ComponentValue<'_>]) -> Vec<f64> {
    let mut nums = Vec::new();
    for cv in args {
        match cv {
            ComponentValue::Token(Token {
                kind: TokenKind::Number { value, .. },
                ..
            }) => nums.push(*value),
            ComponentValue::Token(Token {
                kind: TokenKind::Percentage { value, .. },
                ..
            }) => nums.push(*value),
            _ => {}
        }
    }
    nums
}

fn component_values_to_string(value: &[ComponentValue<'_>]) -> String {
    let mut s = String::new();
    for cv in value {
        match cv {
            ComponentValue::Token(Token { kind, .. }) => match kind {
                TokenKind::Ident(v) => s.push_str(v),
                TokenKind::String(v) => {
                    s.push('"');
                    s.push_str(v);
                    s.push('"');
                }
                TokenKind::Number { value, .. } => s.push_str(&value.to_string()),
                TokenKind::Dimension { value, unit, .. } => {
                    s.push_str(&value.to_string());
                    s.push_str(unit);
                }
                TokenKind::Percentage { value, .. } => {
                    s.push_str(&value.to_string());
                    s.push('%');
                }
                TokenKind::Whitespace => s.push(' '),
                TokenKind::Comma => s.push_str(", "),
                TokenKind::Colon => s.push(':'),
                TokenKind::Semicolon => s.push(';'),
                TokenKind::Delim(c) => s.push(*c),
                TokenKind::Hash { value, .. } => {
                    s.push('#');
                    s.push_str(value);
                }
                _ => {}
            },
            ComponentValue::Function(f) => {
                s.push_str(f.name);
                s.push('(');
                s.push_str(&component_values_to_string(&f.arguments));
                s.push(')');
            }
            ComponentValue::SimpleBlock(b) => {
                s.push(b.token);
                s.push_str(&component_values_to_string(&b.value));
                match b.token {
                    '{' => s.push('}'),
                    '[' => s.push(']'),
                    '(' => s.push(')'),
                    _ => {}
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_parser::parse_declaration_list;

    fn parse_decl(css: &str) -> Vec<PropertyDeclaration> {
        let (decls, _errors) = parse_declaration_list(css);
        let mut results = Vec::new();
        for d in &decls {
            match parse_property(d.name, &d.value, d.important) {
                Ok(props) => results.extend(props),
                Err(e) => panic!("Parse error for '{}': {}", d.name, e),
            }
        }
        results
    }

    #[test]
    fn parse_display_block() {
        let decls = parse_decl("display: block");
        assert_eq!(decls[0].value, CssValue::Display(Display::Block));
    }

    #[test]
    fn parse_display_flex() {
        let decls = parse_decl("display: flex");
        assert_eq!(decls[0].value, CssValue::Display(Display::Flex));
    }

    #[test]
    fn parse_position_absolute() {
        let decls = parse_decl("position: absolute");
        assert_eq!(decls[0].value, CssValue::Position(Position::Absolute));
    }

    #[test]
    fn parse_width_px() {
        let decls = parse_decl("width: 100px");
        assert_eq!(
            decls[0].value,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Length(Length::Px(100.0)))
        );
    }

    #[test]
    fn parse_width_auto() {
        let decls = parse_decl("width: auto");
        assert_eq!(
            decls[0].value,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Auto)
        );
    }

    #[test]
    fn parse_width_percentage() {
        let decls = parse_decl("width: 50%");
        assert_eq!(
            decls[0].value,
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Percentage(50.0))
        );
    }

    #[test]
    fn parse_margin_shorthand() {
        let decls = parse_decl("margin: 10px 20px");
        assert_eq!(decls.len(), 4);
        assert_eq!(decls[0].property, PropertyId::MarginTop);
        assert_eq!(decls[1].property, PropertyId::MarginRight);
        assert_eq!(decls[2].property, PropertyId::MarginBottom);
        assert_eq!(decls[3].property, PropertyId::MarginLeft);
    }

    #[test]
    fn parse_color_named() {
        let decls = parse_decl("color: red");
        assert!(matches!(
            decls[0].value,
            CssValue::Color(Color::Rgba {
                r: 255,
                g: 0,
                b: 0,
                ..
            })
        ));
    }

    #[test]
    fn parse_color_hex() {
        let decls = parse_decl("color: #ff0000");
        assert!(matches!(
            decls[0].value,
            CssValue::Color(Color::Rgba {
                r: 255,
                g: 0,
                b: 0,
                ..
            })
        ));
    }

    #[test]
    fn parse_color_hex_short() {
        let decls = parse_decl("color: #f00");
        assert!(matches!(
            decls[0].value,
            CssValue::Color(Color::Rgba {
                r: 255,
                g: 0,
                b: 0,
                ..
            })
        ));
    }

    #[test]
    fn parse_color_rgb() {
        let decls = parse_decl("color: rgb(255, 128, 0)");
        assert!(matches!(
            decls[0].value,
            CssValue::Color(Color::Rgba {
                r: 255,
                g: 128,
                b: 0,
                ..
            })
        ));
    }

    #[test]
    fn parse_font_family_generic() {
        let decls = parse_decl("font-family: sans-serif");
        assert_eq!(
            decls[0].value,
            CssValue::FontFamily(vec![FontFamily::Generic(GenericFamily::SansSerif)])
        );
    }

    #[test]
    fn parse_font_family_multiple() {
        let decls = parse_decl("font-family: \"Helvetica Neue\", Arial, sans-serif");
        if let CssValue::FontFamily(families) = &decls[0].value {
            assert_eq!(families.len(), 3);
            assert_eq!(families[0], FontFamily::Named("Helvetica Neue".into()));
            assert_eq!(families[1], FontFamily::Named("Arial".into()));
            assert_eq!(families[2], FontFamily::Generic(GenericFamily::SansSerif));
        } else {
            panic!("Expected FontFamily");
        }
    }

    #[test]
    fn parse_font_weight_bold() {
        let decls = parse_decl("font-weight: bold");
        assert_eq!(decls[0].value, CssValue::FontWeight(FontWeight::Bold));
    }

    #[test]
    fn parse_font_weight_numeric() {
        let decls = parse_decl("font-weight: 600");
        assert_eq!(
            decls[0].value,
            CssValue::FontWeight(FontWeight::Numeric(600.0))
        );
    }

    #[test]
    fn parse_opacity() {
        let decls = parse_decl("opacity: 0.5");
        assert_eq!(decls[0].value, CssValue::Number(0.5));
    }

    #[test]
    fn parse_z_index() {
        let decls = parse_decl("z-index: 10");
        assert_eq!(decls[0].value, CssValue::Integer(10));
    }

    #[test]
    fn parse_css_wide_keyword_inherit() {
        let decls = parse_decl("color: inherit");
        assert_eq!(decls[0].value, CssValue::Inherit);
    }

    #[test]
    fn parse_custom_property() {
        let decls = parse_decl("--my-color: red");
        assert_eq!(decls[0].property, PropertyId::Custom("--my-color".into()));
        assert!(matches!(decls[0].value, CssValue::CustomValue(_)));
    }

    #[test]
    fn parse_important_flag() {
        let (raw_decls, _) = parse_declaration_list("color: red !important");
        let decls = parse_property(
            raw_decls[0].name,
            &raw_decls[0].value,
            raw_decls[0].important,
        )
        .unwrap();
        assert!(decls[0].important);
    }

    #[test]
    fn parse_line_height_normal() {
        let decls = parse_decl("line-height: normal");
        assert_eq!(decls[0].value, CssValue::LineHeight(LineHeight::Normal));
    }

    #[test]
    fn parse_line_height_number() {
        let decls = parse_decl("line-height: 1.5");
        assert_eq!(
            decls[0].value,
            CssValue::LineHeight(LineHeight::Number(1.5))
        );
    }

    #[test]
    fn parse_zero_margin() {
        let decls = parse_decl("margin: 0");
        assert_eq!(decls.len(), 4);
    }
}

/// Split a value list into whitespace-separated groups, each a slice of the
/// original so the existing single-value parsers can be reused unchanged.
fn split_on_whitespace<'a, 'b>(value: &'b [ComponentValue<'a>]) -> Vec<Vec<ComponentValue<'a>>> {
    let mut groups: Vec<Vec<ComponentValue<'a>>> = Vec::new();
    let mut current: Vec<ComponentValue<'a>> = Vec::new();
    for v in value {
        if matches!(
            v,
            ComponentValue::Token(Token {
                kind: TokenKind::Whitespace,
                ..
            })
        ) {
            if !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
        } else {
            current.push(v.clone());
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn single_ident(value: &[ComponentValue<'_>]) -> Option<String> {
    expect_single_ident(value).ok()
}

/// Which longhand family a box-style shorthand expands into.
#[derive(Clone, Copy)]
enum BorderSide {
    Width,
    Style,
    Color,
}

impl BorderSide {
    fn ids(self) -> [PropertyId; 4] {
        match self {
            Self::Width => [
                PropertyId::BorderTopWidth,
                PropertyId::BorderRightWidth,
                PropertyId::BorderBottomWidth,
                PropertyId::BorderLeftWidth,
            ],
            Self::Style => [
                PropertyId::BorderTopStyle,
                PropertyId::BorderRightStyle,
                PropertyId::BorderBottomStyle,
                PropertyId::BorderLeftStyle,
            ],
            Self::Color => [
                PropertyId::BorderTopColor,
                PropertyId::BorderRightColor,
                PropertyId::BorderBottomColor,
                PropertyId::BorderLeftColor,
            ],
        }
    }

    fn parse_one(self, value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
        match self {
            Self::Width => parse_border_width(value),
            Self::Style => parse_border_style(value),
            Self::Color => parse_color(value),
        }
    }
}

/// `border-style: solid dashed` and friends — one to four values, applied in
/// the usual top / right / bottom / left order.
fn parse_border_side_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
    side: BorderSide,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let groups = split_on_whitespace(value);
    if groups.is_empty() || groups.len() > 4 {
        return Err(ValueError::InvalidValue(
            "expected one to four values".into(),
        ));
    }
    let parsed: Vec<CssValue> = groups
        .iter()
        .map(|g| side.parse_one(g))
        .collect::<Result<_, _>>()?;

    let (top, right, bottom, left) = match parsed.len() {
        1 => (
            parsed[0].clone(),
            parsed[0].clone(),
            parsed[0].clone(),
            parsed[0].clone(),
        ),
        2 => (
            parsed[0].clone(),
            parsed[1].clone(),
            parsed[0].clone(),
            parsed[1].clone(),
        ),
        3 => (
            parsed[0].clone(),
            parsed[1].clone(),
            parsed[2].clone(),
            parsed[1].clone(),
        ),
        _ => (
            parsed[0].clone(),
            parsed[1].clone(),
            parsed[2].clone(),
            parsed[3].clone(),
        ),
    };

    let ids = side.ids();
    Ok(vec![
        PropertyDeclaration {
            property: ids[0].clone(),
            value: top,
            important,
        },
        PropertyDeclaration {
            property: ids[1].clone(),
            value: right,
            important,
        },
        PropertyDeclaration {
            property: ids[2].clone(),
            value: bottom,
            important,
        },
        PropertyDeclaration {
            property: ids[3].clone(),
            value: left,
            important,
        },
    ])
}

/// `border: 1px solid red` and the per-side variants.
///
/// Order-independent, per the spec: each component is identified by what it
/// parses as, not by where it sits. Omitted components reset to their initial
/// value — which is why `border: solid` alone produces a visible border of
/// `medium` width, and why `border: 1px` alone produces no border at all.
fn parse_border_shorthand(
    name: &str,
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let mut width: Option<CssValue> = None;
    let mut style: Option<CssValue> = None;
    let mut color: Option<CssValue> = None;

    for group in split_on_whitespace(value) {
        if style.is_none() {
            if let Ok(v) = parse_border_style(&group) {
                style = Some(v);
                continue;
            }
        }
        if width.is_none() {
            if let Ok(v) = parse_border_width(&group) {
                width = Some(v);
                continue;
            }
        }
        if color.is_none() {
            if let Ok(v) = parse_color(&group) {
                color = Some(v);
                continue;
            }
        }
        return Err(ValueError::InvalidValue(format!(
            "unrecognised {name} component"
        )));
    }
    if width.is_none() && style.is_none() && color.is_none() {
        return Err(ValueError::InvalidValue(format!("empty {name}")));
    }

    let width =
        width.unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::BorderTopWidth));
    let style =
        style.unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::BorderTopStyle));
    let color =
        color.unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::BorderTopColor));

    let sides = border_sides(name);

    let mut out = Vec::with_capacity(sides.len() * 3);
    for &i in sides {
        out.push(PropertyDeclaration {
            property: BorderSide::Width.ids()[i].clone(),
            value: width.clone(),
            important,
        });
        out.push(PropertyDeclaration {
            property: BorderSide::Style.ids()[i].clone(),
            value: style.clone(),
            important,
        });
        out.push(PropertyDeclaration {
            property: BorderSide::Color.ids()[i].clone(),
            value: color.clone(),
            important,
        });
    }
    Ok(out)
}

fn parse_border_style(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    use crate::css_values::types::display::BorderStyle;
    if let Some(ident) = single_ident(value) {
        let style = match ident.to_ascii_lowercase().as_str() {
            "none" => BorderStyle::None,
            "hidden" => BorderStyle::Hidden,
            "dotted" => BorderStyle::Dotted,
            "dashed" => BorderStyle::Dashed,
            "solid" => BorderStyle::Solid,
            "double" => BorderStyle::Double,
            "groove" => BorderStyle::Groove,
            "ridge" => BorderStyle::Ridge,
            "inset" => BorderStyle::Inset,
            "outset" => BorderStyle::Outset,
            other => {
                return Err(ValueError::InvalidValue(format!(
                    "invalid border-style: {other}"
                )))
            }
        };
        return Ok(CssValue::BorderStyle(style));
    }
    Err(ValueError::InvalidValue("expected border-style".into()))
}

/// `transform: translateX(20px) rotate(45deg) scale(2)`.
///
/// `PropertyId::Transform`, `CssValue::Transform` and `TransformFunction` all
/// existed; nothing parsed into them, so `transform` was silently dropped on
/// every page that used it. Compositing found that, because a transform is one
/// of the few reasons to promote a layer and no page ever produced one.
///
/// `none` is the initial value and parses to an empty list rather than an
/// error, so `transform: none` correctly un-does an inherited-looking value.
fn parse_transform(value: &[ComponentValue<'_>]) -> Result<CssValue, ValueError> {
    use crate::css_values::types::transform::TransformFunction as T;

    if let Some(ident) = single_ident(value) {
        if ident.eq_ignore_ascii_case("none") {
            return Ok(CssValue::Transform(Vec::new()));
        }
    }

    let mut out = Vec::new();
    for group in split_on_whitespace(value) {
        for cv in &group {
            let ComponentValue::Function(f) = cv else {
                return Err(ValueError::InvalidValue("transform takes functions".into()));
            };
            let args = split_on_comma(&f.arguments);
            let name = f.name.to_ascii_lowercase();

            let lp = |i: usize| -> Option<LengthPercentage> {
                args.get(i)
                    .and_then(|g| g.first().and_then(try_length_percentage))
            };
            let num = |i: usize| -> Option<f64> {
                args.get(i)
                    .and_then(|g| g.first().and_then(try_number_value))
            };
            let ang = |i: usize| -> Option<Angle> {
                args.get(i).and_then(|g| g.first().and_then(try_angle))
            };
            let zero = LengthPercentage::Length(Length::Zero);

            let parsed = match name.as_str() {
                "translate" => T::Translate(
                    lp(0).ok_or_else(|| bad("translate"))?,
                    lp(1).unwrap_or(zero),
                ),
                "translatex" => T::TranslateX(lp(0).ok_or_else(|| bad("translateX"))?),
                "translatey" => T::TranslateY(lp(0).ok_or_else(|| bad("translateY"))?),
                "scale" => {
                    let x = num(0).ok_or_else(|| bad("scale"))?;
                    // `scale(2)` is uniform; `scale(2, 3)` is not.
                    T::Scale(x, num(1).unwrap_or(x))
                }
                "scalex" => T::ScaleX(num(0).ok_or_else(|| bad("scaleX"))?),
                "scaley" => T::ScaleY(num(0).ok_or_else(|| bad("scaleY"))?),
                "rotate" => T::Rotate(ang(0).ok_or_else(|| bad("rotate"))?),
                "skewx" => T::SkewX(ang(0).ok_or_else(|| bad("skewX"))?),
                "skewy" => T::SkewY(ang(0).ok_or_else(|| bad("skewY"))?),
                "matrix" => {
                    let mut m = [0.0f64; 6];
                    for (i, slot) in m.iter_mut().enumerate() {
                        *slot = num(i).ok_or_else(|| bad("matrix"))?;
                    }
                    T::Matrix(m[0], m[1], m[2], m[3], m[4], m[5])
                }
                other => {
                    // An unknown or 3D function makes the whole declaration
                    // invalid, per CSS. Dropping just the one function would
                    // apply a transform the author did not write.
                    return Err(ValueError::InvalidValue(format!(
                        "unsupported transform function {other}()"
                    )));
                }
            };
            out.push(parsed);
        }
    }

    if out.is_empty() {
        return Err(ValueError::InvalidValue("empty transform".into()));
    }
    Ok(CssValue::Transform(out))
}

fn bad(name: &str) -> ValueError {
    ValueError::InvalidValue(format!("invalid arguments to {name}()"))
}

fn try_number_value(cv: &ComponentValue<'_>) -> Option<f64> {
    match cv {
        ComponentValue::Token(Token {
            kind: TokenKind::Number { value, .. },
            ..
        }) => Some(*value),
        _ => None,
    }
}

fn try_angle(cv: &ComponentValue<'_>) -> Option<Angle> {
    match cv {
        ComponentValue::Token(Token {
            kind: TokenKind::Dimension { value, unit, .. },
            ..
        }) => match unit.to_ascii_lowercase().as_str() {
            "deg" => Some(Angle::Deg(*value)),
            "rad" => Some(Angle::Rad(*value)),
            "grad" => Some(Angle::Grad(*value)),
            "turn" => Some(Angle::Turn(*value)),
            _ => None,
        },
        // `rotate(0)` is legal.
        ComponentValue::Token(Token {
            kind: TokenKind::Number { value, .. },
            ..
        }) if *value == 0.0 => Some(Angle::Deg(0.0)),
        _ => None,
    }
}

/// Shorthands, checked through the *computed* style rather than the parser's
/// output.
///
/// This is deliberate. `background` was missing from `parse_property` for the
/// whole life of the module, and a test asserting the parser had produced some
/// token would have passed throughout: the declaration was dropped later, at
/// the cascade. Only a computed value proves a shorthand actually arrived.
#[cfg(test)]
mod shorthand_cascade {
    use crate::css_cascade::{ComputedStyle, MediaFeatures};
    use crate::css_values::property::{CssValue, LineHeight, PropertyId};
    use crate::css_values::types::color::Color;
    use crate::css_values::types::display::{FlexDirection, FlexWrap};
    use crate::css_values::types::font::{FontFamily, FontStyle, FontWeight};
    use crate::css_values::types::length::{Length, LengthPercentage, LengthPercentageAuto};
    use crate::dom::node::NodeId;

    /// Computed style of the first `tag` in `html`, with author CSS applied.
    fn computed(html: &str, tag: &str) -> ComputedStyle {
        let dom = crate::html_parser::parse_html(html);
        let tree = crate::style::compute_styles(&dom, &[], &MediaFeatures::default());
        let node = *dom
            .get_elements_by_tag_name(NodeId::DOCUMENT, tag)
            .first()
            .unwrap_or_else(|| panic!("no <{tag}> in the document"));
        tree.get(node)
            .unwrap_or_else(|| panic!("<{tag}> has no computed style"))
            .clone()
    }

    /// Computed style of the `<body>` after a `<style>` block.
    fn body_with(css: &str) -> ComputedStyle {
        computed(
            &format!("<html><head><style>{css}</style></head><body>x</body></html>"),
            "body",
        )
    }

    fn background_of(style: &ComputedStyle) -> CssValue {
        style.get_or_initial(&PropertyId::BackgroundColor)
    }

    fn rgb(r: u8, g: u8, b: u8) -> CssValue {
        CssValue::Color(Color::Rgba { r, g, b, a: 1.0 })
    }

    #[test]
    fn background_black_computes_to_a_black_background_colour() {
        assert_eq!(
            background_of(&body_with("body { background: black }")),
            rgb(0, 0, 0)
        );
    }

    #[test]
    fn background_keeps_the_colour_that_sits_beside_an_image() {
        // The shape every real page uses. The image is dropped; the colour is
        // exactly what must not be.
        assert_eq!(
            background_of(&body_with(
                "body { background: #fff url(bg.png) no-repeat }"
            )),
            rgb(255, 255, 255)
        );
    }

    #[test]
    fn a_later_background_colour_overrides_the_shorthand() {
        assert_eq!(
            background_of(&body_with(
                "body { background: black } body { background-color: red }"
            )),
            rgb(255, 0, 0)
        );
    }

    #[test]
    fn a_later_background_shorthand_resets_an_earlier_background_colour() {
        // The classic shorthand bug: `background: url(x)` names no colour, so
        // it must reset the colour longhand to transparent rather than leave
        // the earlier red standing.
        assert_eq!(
            background_of(&body_with(
                "body { background-color: red } body { background: url(x.png) }"
            )),
            CssValue::Color(Color::Transparent)
        );
    }

    #[test]
    fn a_gradient_background_resets_the_colour_rather_than_being_rejected() {
        // A gradient is dropped, not honoured — but the declaration is still
        // valid, so the earlier red must not survive it. If the shorthand were
        // rejected instead, red would stay.
        assert_eq!(
            background_of(&body_with(
                "body { background-color: red } \
                 body { background: linear-gradient(red, blue) }"
            )),
            CssValue::Color(Color::Transparent)
        );
    }

    #[test]
    fn a_genuinely_invalid_background_leaves_the_earlier_value_alone() {
        // The other side of the previous test: garbage must be rejected, and a
        // rejected declaration cascades as if it were never written.
        assert_eq!(
            background_of(&body_with(
                "body { background-color: red } body { background: notacolour }"
            )),
            rgb(255, 0, 0)
        );
    }

    #[test]
    fn a_more_specific_longhand_beats_an_earlier_shorthand() {
        // Specificity must still decide, even though the shorthand is written
        // later in source order.
        let style = computed(
            "<html><head><style>\
             #p { background-color: white } p { background: black }\
             </style></head><body><p id=p>x</p></body></html>",
            "p",
        );
        assert_eq!(background_of(&style), rgb(255, 255, 255));
    }

    #[test]
    fn important_on_a_shorthand_carries_to_its_longhands() {
        assert_eq!(
            background_of(&body_with(
                "body { background: black !important } body { background-color: red }"
            )),
            rgb(0, 0, 0)
        );
    }

    #[test]
    fn a_css_wide_keyword_on_a_shorthand_reaches_every_longhand() {
        // `button { font: inherit }` is in every reset stylesheet there is.
        let style = computed(
            "<html><head><style>\
             body { font-weight: bold; font-size: 30px } button { font: inherit }\
             </style></head><body><button>x</button></body></html>",
            "button",
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FontWeight),
            CssValue::FontWeight(FontWeight::Bold)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FontSize),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(30.0)))
        );
    }

    #[test]
    fn font_shorthand_expands_and_resets_its_siblings() {
        let style = body_with("body { font-style: italic } body { font: bold 12px/1.5 Arial }");
        assert_eq!(
            style.get_or_initial(&PropertyId::FontWeight),
            CssValue::FontWeight(FontWeight::Bold)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FontSize),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(12.0)))
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::LineHeight),
            CssValue::LineHeight(LineHeight::Number(1.5))
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FontFamily),
            CssValue::FontFamily(vec![FontFamily::Named("Arial".into())])
        );
        // The shorthand named no style, so the earlier italic is reset.
        assert_eq!(
            style.get_or_initial(&PropertyId::FontStyle),
            CssValue::FontStyle(FontStyle::Normal)
        );
    }

    #[test]
    fn a_font_system_keyword_is_rejected_whole() {
        // No host font metrics, so nothing is guessed — the earlier value
        // stands rather than being replaced by an invention.
        let style = body_with("body { font-size: 30px } body { font: caption }");
        assert_eq!(
            style.get_or_initial(&PropertyId::FontSize),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(30.0)))
        );
    }

    #[test]
    fn gap_reaches_the_longhands_layout_actually_reads() {
        let style = body_with("body { gap: 10px 20px }");
        assert_eq!(
            style.get_or_initial(&PropertyId::RowGap),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(10.0)))
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::ColumnGap),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(20.0)))
        );
    }

    #[test]
    fn a_later_column_gap_overrides_the_gap_shorthand() {
        let style = body_with("body { gap: 10px } body { column-gap: 4px }");
        assert_eq!(
            style.get_or_initial(&PropertyId::RowGap),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(10.0)))
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::ColumnGap),
            CssValue::LengthPercentage(LengthPercentage::Length(Length::Px(4.0)))
        );
    }

    #[test]
    fn flex_one_means_grow_one_shrink_one_basis_zero() {
        let style = body_with("body { flex: 1 }");
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexGrow),
            CssValue::Number(1.0)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexShrink),
            CssValue::Number(1.0)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexBasis),
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Percentage(0.0))
        );
    }

    #[test]
    fn flex_with_a_basis_alone_still_grows() {
        // `flex-grow`'s own initial is 0; the shorthand's omitted default is 1.
        let style = body_with("body { flex: 200px }");
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexGrow),
            CssValue::Number(1.0)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexBasis),
            CssValue::LengthPercentageAuto(LengthPercentageAuto::Length(Length::Px(200.0)))
        );
    }

    #[test]
    fn flex_shorthand_resets_an_earlier_grow() {
        let style = body_with("body { flex-grow: 7 } body { flex: none }");
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexGrow),
            CssValue::Number(0.0)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexShrink),
            CssValue::Number(0.0)
        );
    }

    #[test]
    fn flex_flow_expands_and_resets() {
        let style = body_with("body { flex-wrap: wrap } body { flex-flow: column }");
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexDirection),
            CssValue::FlexDirection(FlexDirection::Column)
        );
        // Wrap was not named by the shorthand, so it reverts to initial.
        assert_eq!(
            style.get_or_initial(&PropertyId::FlexWrap),
            CssValue::FlexWrap(FlexWrap::Nowrap)
        );
    }

    #[test]
    fn place_items_fills_both_axes() {
        use crate::css_values::types::display::AlignmentValue;
        let style = body_with("body { place-items: center }");
        assert_eq!(
            style.get_or_initial(&PropertyId::AlignItems),
            CssValue::Alignment(AlignmentValue::Center)
        );
        assert_eq!(
            style.get_or_initial(&PropertyId::JustifyItems),
            CssValue::Alignment(AlignmentValue::Center)
        );
    }
}

/// Which physical sides a `border`-family shorthand writes to.
fn border_sides(name: &str) -> &'static [usize] {
    match name {
        "border-top" => &[0],
        "border-right" => &[1],
        "border-bottom" => &[2],
        "border-left" => &[3],
        _ => &[0, 1, 2, 3],
    }
}

/// Every longhand each supported shorthand controls.
///
/// Only used for the CSS-wide keywords, which name no component values of
/// their own and so cannot go through the ordinary per-shorthand parser. Kept
/// next to the shorthands themselves so adding one and forgetting the keyword
/// path is a visible omission rather than a silent one.
fn shorthand_longhands(name: &str) -> Option<Vec<PropertyId>> {
    use PropertyId as P;
    let ids = match name {
        "margin" => vec![P::MarginTop, P::MarginRight, P::MarginBottom, P::MarginLeft],
        "padding" => vec![
            P::PaddingTop,
            P::PaddingRight,
            P::PaddingBottom,
            P::PaddingLeft,
        ],
        "overflow" => vec![P::OverflowX, P::OverflowY],
        "border-width" => BorderSide::Width.ids().to_vec(),
        "border-style" => BorderSide::Style.ids().to_vec(),
        "border-color" => BorderSide::Color.ids().to_vec(),
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            let widths = BorderSide::Width.ids();
            let styles = BorderSide::Style.ids();
            let colors = BorderSide::Color.ids();
            border_sides(name)
                .iter()
                .flat_map(|&i| [widths[i].clone(), styles[i].clone(), colors[i].clone()])
                .collect()
        }
        // The engine's only background longhand — see `parse_background_shorthand`.
        "background" => vec![P::BackgroundColor],
        "font" => vec![
            P::FontStyle,
            P::FontWeight,
            P::FontSize,
            P::LineHeight,
            P::FontFamily,
        ],
        "flex" => vec![P::FlexGrow, P::FlexShrink, P::FlexBasis],
        "flex-flow" => vec![P::FlexDirection, P::FlexWrap],
        "gap" => vec![P::RowGap, P::ColumnGap],
        "place-items" => vec![P::AlignItems, P::JustifyItems],
        "place-content" => vec![P::AlignContent, P::JustifyContent],
        "place-self" => vec![P::AlignSelf, P::JustifySelf],
        _ => return None,
    };
    Some(ids)
}

/// `background: #fff url(bg.png) no-repeat center / cover`.
///
/// The engine has exactly one background longhand — `background-color` — and
/// `render::painter` fills a solid rect and nothing else. There is no
/// `background-image`, `-repeat`, `-attachment`, `-position`, `-size`,
/// `-origin` or `-clip` property to expand into, so those components are
/// recognised (which keeps the declaration valid, so the colour beside them
/// survives) and then deliberately discarded. Nothing here invents a property
/// layout or paint could not honour: an image or gradient is lost exactly as it
/// already was, but the colour under it is no longer lost with it.
///
/// The reset is the half that bites. A shorthand sets *every* longhand in its
/// family, so `background: url(x)` after `background-color: red` must leave the
/// box transparent rather than red — when no colour component appears the
/// initial value is written out explicitly rather than merely omitted.
fn parse_background_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let layers = split_on_comma(value);
    // `split_on_comma` always yields at least one group.
    let last = layers.len() - 1;
    let mut color: Option<CssValue> = None;

    for (index, layer) in layers.iter().enumerate() {
        if layer.is_empty() {
            return Err(ValueError::InvalidValue("empty background layer".into()));
        }
        for item in layer {
            // Only the final layer may carry the colour: it paints beneath
            // every layer, so a colour anywhere else is invalid rather than
            // merely unhonoured.
            if index == last && color.is_none() {
                if let Ok(c) = parse_color(std::slice::from_ref(item)) {
                    color = Some(c);
                    continue;
                }
            }
            if is_background_component(item) {
                continue;
            }
            return Err(ValueError::InvalidValue(
                "unrecognised background component".into(),
            ));
        }
    }

    Ok(vec![PropertyDeclaration {
        property: PropertyId::BackgroundColor,
        value: color
            .unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::BackgroundColor)),
        important,
    }])
}

/// Everything in the `background` grammar other than `<background-color>`.
///
/// Matching these keeps the declaration valid so its colour is kept; the value
/// itself is dropped, because the engine has no longhand to put it in. See
/// `parse_background_shorthand`.
fn is_background_component(cv: &ComponentValue<'_>) -> bool {
    match cv {
        ComponentValue::Token(Token { kind, .. }) => match kind {
            TokenKind::Url(_) => true,
            // Separates `<position>` from `<size>`.
            TokenKind::Delim('/') => true,
            // Positions and sizes.
            TokenKind::Number { .. }
            | TokenKind::Percentage { .. }
            | TokenKind::Dimension { .. } => true,
            TokenKind::Ident(name) => matches!(
                name.to_ascii_lowercase().as_str(),
                "none"
                    | "repeat"
                    | "repeat-x"
                    | "repeat-y"
                    | "no-repeat"
                    | "space"
                    | "round"
                    | "scroll"
                    | "fixed"
                    | "local"
                    | "border-box"
                    | "padding-box"
                    | "content-box"
                    | "text"
                    | "left"
                    | "right"
                    | "top"
                    | "bottom"
                    | "center"
                    | "cover"
                    | "contain"
                    | "auto"
            ),
            _ => false,
        },
        ComponentValue::Function(f) => matches!(
            f.name.to_ascii_lowercase().as_str(),
            "url"
                | "image-set"
                | "-webkit-image-set"
                | "cross-fade"
                | "element"
                | "linear-gradient"
                | "radial-gradient"
                | "conic-gradient"
                | "repeating-linear-gradient"
                | "repeating-radial-gradient"
                | "repeating-conic-gradient"
                | "-webkit-linear-gradient"
                | "-webkit-radial-gradient"
                | "-webkit-gradient"
                | "calc"
                | "min"
                | "max"
                | "clamp"
        ),
        _ => false,
    }
}

/// `font: italic bold 14px/1.4 "Helvetica Neue", sans-serif`.
///
/// Expands to font-style, font-weight, font-size, line-height and font-family.
/// `font-variant` and `font-stretch` are recognised in the prelude and then
/// dropped: the engine has no property for either, so there is nowhere for them
/// to go, and inventing one would only move the loss downstream.
///
/// The system font keywords (`caption`, `menu`, `status-bar`, …) are rejected
/// outright rather than approximated. Resolving them needs host font metrics
/// this engine deliberately never reads, and guessing would restyle the element
/// to something the platform never said.
fn parse_font_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let mut style: Option<CssValue> = None;
    let mut weight: Option<CssValue> = None;
    let mut line_height: Option<CssValue> = None;

    // Prelude: style / variant / weight / stretch in any order, then the size.
    let mut i = 0usize;
    while i < value.len() {
        if is_whitespace_token(&value[i]) {
            i += 1;
            continue;
        }
        let one = std::slice::from_ref(&value[i]);
        if style.is_none() {
            if let Ok(v) = parse_font_style(one) {
                style = Some(v);
                i += 1;
                continue;
            }
        }
        if weight.is_none() {
            if let Ok(v) = parse_font_weight(one) {
                weight = Some(v);
                i += 1;
                continue;
            }
        }
        if is_dropped_font_prelude(&value[i]) {
            i += 1;
            continue;
        }
        break;
    }

    // Size is mandatory, and it is where a system font keyword lands and fails.
    let Some(size_token) = value.get(i) else {
        return Err(ValueError::InvalidValue(
            "font shorthand needs a size".into(),
        ));
    };
    let size = parse_font_size(std::slice::from_ref(size_token))?;
    i += 1;

    // Optional `/ <line-height>`.
    while i < value.len() && is_whitespace_token(&value[i]) {
        i += 1;
    }
    if value.get(i).is_some_and(is_slash_token) {
        i += 1;
        while i < value.len() && is_whitespace_token(&value[i]) {
            i += 1;
        }
        let Some(lh) = value.get(i) else {
            return Err(ValueError::InvalidValue(
                "font shorthand: `/` with no line-height".into(),
            ));
        };
        line_height = Some(parse_line_height(std::slice::from_ref(lh))?);
        i += 1;
    }

    // Family is mandatory and takes the rest, commas and all.
    let family_tokens = trim_whitespace(&value[i..]);
    if family_tokens.is_empty() {
        return Err(ValueError::InvalidValue(
            "font shorthand needs a family".into(),
        ));
    }
    let family = parse_font_family(family_tokens)?;

    Ok(vec![
        PropertyDeclaration {
            property: PropertyId::FontStyle,
            value: style
                .unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::FontStyle)),
            important,
        },
        PropertyDeclaration {
            property: PropertyId::FontWeight,
            value: weight
                .unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::FontWeight)),
            important,
        },
        PropertyDeclaration {
            property: PropertyId::FontSize,
            value: size,
            important,
        },
        PropertyDeclaration {
            property: PropertyId::LineHeight,
            value: line_height
                .unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::LineHeight)),
            important,
        },
        PropertyDeclaration {
            property: PropertyId::FontFamily,
            value: family,
            important,
        },
    ])
}

/// `font-variant` and `font-stretch` keywords: legal in the `font` prelude,
/// unrepresentable in this engine, so consumed and thrown away.
fn is_dropped_font_prelude(cv: &ComponentValue<'_>) -> bool {
    try_ident(cv).is_some_and(|ident| {
        matches!(
            ident.to_ascii_lowercase().as_str(),
            "normal"
                | "small-caps"
                | "ultra-condensed"
                | "extra-condensed"
                | "condensed"
                | "semi-condensed"
                | "semi-expanded"
                | "expanded"
                | "extra-expanded"
                | "ultra-expanded"
        )
    })
}

/// `flex: 1`, `flex: 0 0 200px`, `flex: auto`, `flex: none`.
///
/// The omitted-component defaults are the shorthand's own, not the longhands':
/// `flex: 200px` means grow 1, though `flex-grow`'s initial value is 0. Reading
/// the initial values off the longhands instead makes every `flex: <basis>`
/// item refuse to grow.
///
/// `flex-basis: content` is not supported — the engine's `LengthPercentageAuto`
/// has no such variant — so `flex: 1 1 content` is rejected whole rather than
/// silently becoming something else.
fn parse_flex_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let groups = split_on_whitespace(value);
    if groups.is_empty() || groups.len() > 3 {
        return Err(ValueError::InvalidValue("invalid flex shorthand".into()));
    }

    if groups.len() == 1 {
        if let Some(ident) = single_ident(&groups[0]) {
            if ident.eq_ignore_ascii_case("none") {
                return Ok(flex_declarations(
                    0.0,
                    0.0,
                    CssValue::LengthPercentageAuto(LengthPercentageAuto::Auto),
                    important,
                ));
            }
        }
    }

    let mut grow: Option<f64> = None;
    let mut shrink: Option<f64> = None;
    let mut basis: Option<CssValue> = None;

    for group in &groups {
        // A unitless number is always a flex factor, never a basis — which is
        // why `flex: 0` means grow 0 and not basis 0px.
        if let Ok(CssValue::Number(n)) = parse_number(group) {
            if grow.is_none() {
                grow = Some(n);
            } else if shrink.is_none() {
                shrink = Some(n);
            } else {
                return Err(ValueError::InvalidValue("too many flex factors".into()));
            }
            continue;
        }
        if basis.is_none() {
            if let Ok(v) = parse_length_percentage_auto(group) {
                basis = Some(v);
                continue;
            }
        }
        return Err(ValueError::InvalidValue(
            "unrecognised flex component".into(),
        ));
    }

    Ok(flex_declarations(
        grow.unwrap_or(1.0),
        shrink.unwrap_or(1.0),
        basis.unwrap_or(CssValue::LengthPercentageAuto(
            LengthPercentageAuto::Percentage(0.0),
        )),
        important,
    ))
}

fn flex_declarations(
    grow: f64,
    shrink: f64,
    basis: CssValue,
    important: bool,
) -> Vec<PropertyDeclaration> {
    vec![
        PropertyDeclaration {
            property: PropertyId::FlexGrow,
            value: CssValue::Number(grow),
            important,
        },
        PropertyDeclaration {
            property: PropertyId::FlexShrink,
            value: CssValue::Number(shrink),
            important,
        },
        PropertyDeclaration {
            property: PropertyId::FlexBasis,
            value: basis,
            important,
        },
    ]
}

/// `flex-flow: column wrap` — direction and wrap in either order.
fn parse_flex_flow_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let groups = split_on_whitespace(value);
    if groups.is_empty() || groups.len() > 2 {
        return Err(ValueError::InvalidValue(
            "invalid flex-flow shorthand".into(),
        ));
    }

    let mut direction: Option<CssValue> = None;
    let mut wrap: Option<CssValue> = None;
    for group in &groups {
        if direction.is_none() {
            if let Ok(v) = parse_flex_direction(group) {
                direction = Some(v);
                continue;
            }
        }
        if wrap.is_none() {
            if let Ok(v) = parse_flex_wrap(group) {
                wrap = Some(v);
                continue;
            }
        }
        return Err(ValueError::InvalidValue(
            "unrecognised flex-flow component".into(),
        ));
    }

    Ok(vec![
        PropertyDeclaration {
            property: PropertyId::FlexDirection,
            value: direction
                .unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::FlexDirection)),
            important,
        },
        PropertyDeclaration {
            property: PropertyId::FlexWrap,
            value: wrap.unwrap_or_else(|| crate::css_cascade::initial_value(&PropertyId::FlexWrap)),
            important,
        },
    ])
}

/// `gap: 12px 20px` — row gap, then column gap.
///
/// `PropertyId::Gap` existed and nothing ever read it: `layout::style_map` asks
/// for `RowGap` and `ColumnGap` only. So every `gap` on every flex container
/// parsed cleanly into a value no consumer looked at. Expanding to the two
/// longhands is what makes the property reach layout at all.
fn parse_gap_shorthand(
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let groups = split_on_whitespace(value);
    let (row, column) = match groups.len() {
        1 => (
            parse_length_percentage(&groups[0])?,
            parse_length_percentage(&groups[0])?,
        ),
        2 => (
            parse_length_percentage(&groups[0])?,
            parse_length_percentage(&groups[1])?,
        ),
        _ => {
            return Err(ValueError::InvalidValue(
                "expected one or two gap values".into(),
            ))
        }
    };
    Ok(vec![
        PropertyDeclaration {
            property: PropertyId::RowGap,
            value: row,
            important,
        },
        PropertyDeclaration {
            property: PropertyId::ColumnGap,
            value: column,
            important,
        },
    ])
}

/// `place-items`, `place-content`, `place-self`: align first, justify second,
/// the second defaulting to the first.
fn parse_place_shorthand(
    name: &str,
    value: &[ComponentValue<'_>],
    important: bool,
) -> Result<Vec<PropertyDeclaration>, ValueError> {
    let groups = split_on_whitespace(value);
    let (align, justify) = match groups.len() {
        1 => (parse_alignment(&groups[0])?, parse_alignment(&groups[0])?),
        2 => (parse_alignment(&groups[0])?, parse_alignment(&groups[1])?),
        _ => {
            return Err(ValueError::InvalidValue(format!(
                "expected one or two {name} values"
            )))
        }
    };
    let (align_id, justify_id) = match name {
        "place-items" => (PropertyId::AlignItems, PropertyId::JustifyItems),
        "place-content" => (PropertyId::AlignContent, PropertyId::JustifyContent),
        _ => (PropertyId::AlignSelf, PropertyId::JustifySelf),
    };
    Ok(vec![
        PropertyDeclaration {
            property: align_id,
            value: align,
            important,
        },
        PropertyDeclaration {
            property: justify_id,
            value: justify,
            important,
        },
    ])
}

fn is_whitespace_token(cv: &ComponentValue<'_>) -> bool {
    matches!(
        cv,
        ComponentValue::Token(Token {
            kind: TokenKind::Whitespace,
            ..
        })
    )
}

fn is_slash_token(cv: &ComponentValue<'_>) -> bool {
    matches!(
        cv,
        ComponentValue::Token(Token {
            kind: TokenKind::Delim('/'),
            ..
        })
    )
}

/// Split a function's arguments on commas.
fn split_on_comma<'a>(value: &[ComponentValue<'a>]) -> Vec<Vec<ComponentValue<'a>>> {
    let mut groups = Vec::new();
    let mut current: Vec<ComponentValue<'a>> = Vec::new();
    for cv in value {
        match cv {
            ComponentValue::Token(Token {
                kind: TokenKind::Comma,
                ..
            }) => groups.push(std::mem::take(&mut current)),
            ComponentValue::Token(Token {
                kind: TokenKind::Whitespace,
                ..
            }) => {}
            other => current.push(other.clone()),
        }
    }
    groups.push(current);
    groups
}
