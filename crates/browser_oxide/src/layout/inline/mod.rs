//! Inline layout: shape text, find where it breaks, and report how big it is.
//!
//! This replaces the placeholder that measured every glyph as `0.6em` and never
//! wrapped. It is the piece [`ENGINE_GAP_ANALYSIS`] called the long pole, and
//! its algorithms are the ones PoC-1 measured against Chrome 150: 12/12
//! fixtures matching Chrome's line count with a worst geometric error of
//! 0.03 px against a 2 px budget, across English, German, CJK, Arabic, mixed
//! bidi, emoji, `pre-wrap`, soft hyphens and justification.
//!
//! What is here is *measurement and breaking*. What is not here is a full
//! inline formatting context: an inline box that wraps still produces one box
//! rather than several fragments, because the surrounding layout is taffy and
//! taffy's contract is one node, one `measure(available) -> size`, one rect.
//! Owning block+inline flow so that fragments are expressible is
//! [ADR-010](https://github.com/yfedoseev/browser_oxide_app) and comes next.
//!
//! [`ENGINE_GAP_ANALYSIS`]: https://github.com/yfedoseev/browser_oxide_app

pub mod breaks;
pub mod metrics;
pub mod shape;

use rustybuzz::Face as HbFace;
use unicode_script::UnicodeScript;

pub use metrics::{FontMetrics, MetricsProfile};
pub use shape::{CacheStats, ShapeCache};

use crate::canvas::text::font_database::FontDatabase;

/// How a run of text should be laid out.
#[derive(Debug, Clone)]
pub struct TextStyle {
    /// `font-family` list, in CSS order.
    pub families: Vec<String>,
    pub size_px: f32,
    pub weight: u16,
    pub italic: bool,
    /// `None` for `line-height: normal`.
    pub line_height: Option<f32>,
    /// May a too-long word be broken mid-word?
    pub break_word: bool,
    /// Does this text wrap at all? False for `white-space: nowrap` and `pre`.
    pub wraps: bool,
    /// Which platform's metric convention to follow.
    pub metrics: MetricsProfile,
    /// OS name for the engine's family-alias resolution (`Arial` →
    /// `Liberation Sans` on Linux, and so on).
    pub os_name: String,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            families: vec!["sans-serif".to_string()],
            size_px: 16.0,
            weight: 400,
            italic: false,
            line_height: None,
            break_word: false,
            wraps: true,
            metrics: MetricsProfile::default(),
            os_name: "linux".to_string(),
        }
    }
}

/// The result of laying out a run of text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    /// Width of the widest line, excluding trailing collapsible space.
    pub width: f32,
    /// Total height: `lines * line_height`.
    pub height: f32,
    pub line_count: usize,
    /// Used `line-height`.
    pub line_height: f32,
    /// Distance from the top of the first line box to its baseline.
    pub first_baseline: f32,
    /// Width the text would occupy if it never wrapped — CSS max-content.
    pub max_content_width: f32,
    /// Width of the widest unbreakable segment — CSS min-content.
    pub min_content_width: f32,
}

/// One line after fitting: the byte range it covers and how wide it is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FittedLine {
    start: usize,
    end: usize,
    width: f32,
}

/// A laid-out line, with the glyphs to draw it.
///
/// This is what paint consumes. It carries positioned glyphs rather than a
/// string, because a display list that held strings would have to reshape on
/// every replay and shaping is the expensive half.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBox {
    /// Top of this line box, relative to the first line's top.
    pub y: f32,
    /// Baseline offset from this line box's top.
    pub baseline: f32,
    pub width: f32,
    pub height: f32,
    /// Byte range of the source text this line covers.
    pub start: usize,
    pub end: usize,
    pub glyphs: Vec<PositionedGlyph>,
}

/// A glyph placed on a line, relative to the line's start and baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

/// Measures and breaks text. Holds the shaping cache, so keep one per document
/// and reuse it across relayouts — that is where the cache pays.
pub struct InlineLayout {
    cache: ShapeCache,
}

impl Default for InlineLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineLayout {
    pub fn new() -> Self {
        Self {
            cache: ShapeCache::new(),
        }
    }

    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Lay out `text` into `available_width`, or unconstrained when `None`.
    ///
    /// `text` must already have had CSS white-space processing applied.
    pub fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        available_width: Option<f32>,
    ) -> TextLayout {
        let db = FontDatabase::get();
        let Some(face_id) =
            db.query_chain(&style.families, style.weight, style.italic, &style.os_name)
        else {
            return self.empty(style, 0.0);
        };
        let Some((data, index)) = db.face_data(face_id) else {
            return self.empty(style, 0.0);
        };
        let Some(hb) = HbFace::from_slice(data, index) else {
            return self.empty(style, 0.0);
        };
        let Ok(ttf) = ttf_parser::Face::parse(data, index) else {
            return self.empty(style, 0.0);
        };

        let m = metrics::metrics_for(&ttf, style.size_px, style.metrics);
        let line_height = style.line_height.unwrap_or(m.line_spacing);
        let (ascent, _descent) = m.leaded(line_height);

        if text.is_empty() {
            return TextLayout {
                width: 0.0,
                height: 0.0,
                line_count: 0,
                line_height,
                first_baseline: ascent,
                max_content_width: 0.0,
                min_content_width: 0.0,
            };
        }

        // `face_data` returns a slice borrowed from the process-wide font
        // database, so its address is a stable identity for the cache.
        let face_key = data.as_ptr() as u64 ^ u64::from(index);
        let script = dominant_script(text);
        let rtl = is_rtl(text);

        let width_of = |cache: &mut ShapeCache, s: &str| -> f32 {
            cache.measure(&hb, face_key, style.size_px, script, rtl, s)
        };

        let max_content = width_of(&mut self.cache, text.trim_end());
        let ops = breaks::opportunities(text);
        let min_content = self.min_content(text, &ops, &hb, face_key, style, script, rtl);

        let fitted = self.fitted_lines(
            text,
            &ops,
            available_width,
            style,
            &hb,
            face_key,
            script,
            rtl,
            max_content,
        );
        let line_count = fitted.len();
        let widest = fitted.iter().map(|l| l.width).fold(0.0f32, f32::max);

        TextLayout {
            width: widest,
            height: line_height * line_count as f32,
            line_count,
            line_height,
            first_baseline: ascent,
            max_content_width: max_content,
            min_content_width: min_content,
        }
    }

    /// Fit into `available_width`, or into one line per forced break when the
    /// text does not wrap.
    #[allow(
        clippy::too_many_arguments,
        reason = "threading shaping state, not a public API"
    )]
    fn fitted_lines(
        &mut self,
        text: &str,
        ops: &[breaks::Opportunity],
        available_width: Option<f32>,
        style: &TextStyle,
        hb: &HbFace<'_>,
        face_key: u64,
        script: rustybuzz::Script,
        rtl: bool,
        max_content: f32,
    ) -> Vec<FittedLine> {
        match (available_width, style.wraps) {
            (Some(avail), true) => self.fit(text, ops, avail, hb, face_key, style, script, rtl),
            _ => {
                // No wrapping: only forced breaks split lines.
                let mut lines = Vec::new();
                let mut start = 0usize;
                for op in ops.iter().filter(|o| o.mandatory) {
                    let end = trim_trailing_spaces(text, op.offset);
                    let width = self.cache.measure(
                        hb,
                        face_key,
                        style.size_px,
                        script,
                        rtl,
                        &text[start..end],
                    );
                    lines.push(FittedLine { start, end, width });
                    start = op.offset;
                }
                if lines.is_empty() {
                    lines.push(FittedLine {
                        start: 0,
                        end: trim_trailing_spaces(text, text.len()),
                        width: max_content,
                    });
                }
                lines
            }
        }
    }

    /// Lay out text and return the lines with their glyphs, ready to paint.
    ///
    /// Same breaking as [`Self::layout`] — it is the same code path — plus the
    /// shaping needed to draw each line.
    pub fn layout_lines(
        &mut self,
        text: &str,
        style: &TextStyle,
        available_width: Option<f32>,
    ) -> (TextLayout, Vec<LineBox>) {
        let summary = self.layout(text, style, available_width);
        if text.is_empty() || summary.line_count == 0 {
            return (summary, Vec::new());
        }

        let db = FontDatabase::get();
        let Some(face_id) =
            db.query_chain(&style.families, style.weight, style.italic, &style.os_name)
        else {
            return (summary, Vec::new());
        };
        let Some((data, index)) = db.face_data(face_id) else {
            return (summary, Vec::new());
        };
        let Some(hb) = HbFace::from_slice(data, index) else {
            return (summary, Vec::new());
        };

        let face_key = data.as_ptr() as u64 ^ u64::from(index);
        let script = dominant_script(text);
        let rtl = is_rtl(text);
        let ops = breaks::opportunities(text);
        let fitted = self.fitted_lines(
            text,
            &ops,
            available_width,
            style,
            &hb,
            face_key,
            script,
            rtl,
            summary.max_content_width,
        );

        let mut boxes = Vec::with_capacity(fitted.len());
        let mut y = 0.0f32;
        for line in &fitted {
            let slice = &text[line.start..line.end.max(line.start)];
            let glyphs = self
                .cache
                .shape(&hb, face_key, style.size_px, script, rtl, slice);
            let mut x = 0.0f32;
            let positioned = glyphs
                .iter()
                .map(|g| {
                    let p = PositionedGlyph {
                        id: g.id,
                        x: x + g.x_offset,
                        y: -g.y_offset,
                    };
                    x += g.x_advance;
                    p
                })
                .collect();
            boxes.push(LineBox {
                y,
                baseline: summary.first_baseline,
                width: line.width,
                height: summary.line_height,
                start: line.start,
                end: line.end,
                glyphs: positioned,
            });
            y += summary.line_height;
        }
        (summary, boxes)
    }

    fn empty(&self, style: &TextStyle, width: f32) -> TextLayout {
        let line_height = style.line_height.unwrap_or(style.size_px * 1.2);
        TextLayout {
            width,
            height: 0.0,
            line_count: 0,
            line_height,
            first_baseline: line_height,
            max_content_width: width,
            min_content_width: width,
        }
    }

    /// Widest single unbreakable segment — the narrowest the text can be
    /// without overflowing, which is CSS `min-content`.
    #[allow(
        clippy::too_many_arguments,
        reason = "threading shaping state, not a public API"
    )]
    fn min_content(
        &mut self,
        text: &str,
        ops: &[breaks::Opportunity],
        hb: &HbFace<'_>,
        face_key: u64,
        style: &TextStyle,
        script: rustybuzz::Script,
        rtl: bool,
    ) -> f32 {
        let mut widest: f32 = 0.0;
        let mut start = 0usize;
        for op in ops {
            if op.offset <= start {
                continue;
            }
            let end = trim_trailing_spaces(text, op.offset);
            if end > start {
                let w =
                    self.cache
                        .measure(hb, face_key, style.size_px, script, rtl, &text[start..end]);
                widest = widest.max(w);
            }
            start = op.offset;
        }
        widest
    }

    /// Greedy first-fit. Chrome is greedy — it does not implement Knuth-Plass —
    /// so matching it needs the same measurement and the same tie-breaking, not
    /// a better algorithm.
    ///
    /// Returns `(line count, widest line)`.
    #[allow(
        clippy::too_many_arguments,
        reason = "threading shaping state, not a public API"
    )]
    fn fit(
        &mut self,
        text: &str,
        ops: &[breaks::Opportunity],
        avail: f32,
        hb: &HbFace<'_>,
        face_key: u64,
        style: &TextStyle,
        script: rustybuzz::Script,
        rtl: bool,
    ) -> Vec<FittedLine> {
        // Sub-pixel slack. Without it a line whose width lands on
        // `avail + 1e-4` wraps a word early and every line below it is wrong.
        const EPS: f32 = 0.01;

        let mut lines: Vec<FittedLine> = Vec::new();
        // The line under construction runs from `start` to `committed`, and
        // `committed_width` is its width. `committed == start` means empty.
        let mut start = 0usize;
        let mut committed = 0usize;
        let mut committed_width = 0.0f32;

        for op in ops {
            if op.offset <= start {
                continue;
            }
            let mut trimmed = trim_trailing_spaces(text, op.offset);
            let mut width = self.cache.measure(
                hb,
                face_key,
                style.size_px,
                script,
                rtl,
                &text[start..trimmed],
            );

            if width > avail + EPS && committed > start {
                // Take the last opportunity that fitted, then reconsider this
                // segment at the head of a fresh line.
                lines.push(FittedLine {
                    start,
                    end: trim_trailing_spaces(text, committed),
                    width: committed_width,
                });
                start = skip_leading_spaces(text, committed);
                trimmed = trim_trailing_spaces(text, op.offset);
                width = self.cache.measure(
                    hb,
                    face_key,
                    style.size_px,
                    script,
                    rtl,
                    &text[start..trimmed],
                );
            }

            if width > avail + EPS && style.break_word {
                // A single segment wider than the line. `overflow-wrap:
                // normal` lets it overflow, which is CSS's actual default and
                // what Chrome does with a long URL; `break-word` cuts it at the
                // last grapheme that fits, as many times as needed.
                for (cut, cut_width) in self.break_inside(
                    text, start, trimmed, avail, hb, face_key, style, script, rtl,
                ) {
                    lines.push(FittedLine {
                        start,
                        end: cut,
                        width: cut_width,
                    });
                    start = cut;
                }
                width = self.cache.measure(
                    hb,
                    face_key,
                    style.size_px,
                    script,
                    rtl,
                    &text[start..trimmed],
                );
            }

            committed = op.offset;
            committed_width = width;

            if op.mandatory {
                lines.push(FittedLine {
                    start,
                    end: trim_trailing_spaces(text, committed),
                    width: committed_width,
                });
                start = op.offset;
                committed = op.offset;
                committed_width = 0.0;
            }
        }

        if committed > start || lines.is_empty() {
            lines.push(FittedLine {
                start,
                end: trim_trailing_spaces(text, committed.max(start)),
                width: committed_width,
            });
        }
        lines
    }

    /// Cut an over-long segment at the last grapheme that fits, repeatedly.
    /// Returns `(end offset, width)` per emitted line, excluding the tail.
    #[allow(
        clippy::too_many_arguments,
        reason = "threading shaping state, not a public API"
    )]
    fn break_inside(
        &mut self,
        text: &str,
        start: usize,
        seg_end: usize,
        avail: f32,
        hb: &HbFace<'_>,
        face_key: u64,
        style: &TextStyle,
        script: rustybuzz::Script,
        rtl: bool,
    ) -> Vec<(usize, f32)> {
        let mut out = Vec::new();
        let mut cursor = start;
        loop {
            let mut last_fit: Option<(usize, f32)> = None;
            let mut probe = cursor;
            while probe < seg_end {
                let ch = text[probe..].chars().next().unwrap_or(' ');
                let next = probe + ch.len_utf8();
                let w = self.cache.measure(
                    hb,
                    face_key,
                    style.size_px,
                    script,
                    rtl,
                    &text[cursor..next],
                );
                if w <= avail + 0.01 {
                    last_fit = Some((next, w));
                    probe = next;
                } else {
                    break;
                }
            }
            match last_fit {
                Some((cut, w)) if cut < seg_end => {
                    out.push((cut, w));
                    cursor = cut;
                }
                // Everything fits, or not even one character does. Either way
                // the tail is the caller's problem.
                _ => return out,
            }
        }
    }
}

fn trim_trailing_spaces(text: &str, end: usize) -> usize {
    let mut e = end;
    while e > 0 {
        let prev = text[..e].chars().next_back().unwrap_or('x');
        if prev == ' ' || prev == '\t' || prev == '\n' {
            e -= prev.len_utf8();
        } else {
            break;
        }
    }
    e
}

fn skip_leading_spaces(text: &str, start: usize) -> usize {
    let mut s = start;
    while s < text.len() {
        let c = text[s..].chars().next().unwrap_or('x');
        if c == ' ' || c == '\t' {
            s += c.len_utf8();
        } else {
            break;
        }
    }
    s
}

/// The script to shape this text as.
///
/// A single script per run is a simplification: real itemisation splits on
/// script boundaries and shapes each separately. It is adequate for
/// measurement, where the error is confined to runs that mix scripts *and*
/// whose shaping depends on the tag — and it is not adequate for painting,
/// which is why the fragment tree will need real itemisation.
fn dominant_script(text: &str) -> rustybuzz::Script {
    use std::str::FromStr;
    for c in text.chars() {
        let s = c.script();
        if !matches!(
            s,
            unicode_script::Script::Common
                | unicode_script::Script::Inherited
                | unicode_script::Script::Unknown
        ) {
            if let Ok(script) = rustybuzz::Script::from_str(s.short_name()) {
                return script;
            }
        }
    }
    rustybuzz::script::LATIN
}

/// Does this text run right-to-left?
fn is_rtl(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c,
            '\u{0590}'..='\u{05FF}'   // Hebrew
            | '\u{0600}'..='\u{06FF}' // Arabic
            | '\u{0700}'..='\u{074F}' // Syriac
            | '\u{0750}'..='\u{077F}' // Arabic Supplement
            | '\u{08A0}'..='\u{08FF}' // Arabic Extended-A
            | '\u{FB1D}'..='\u{FDFF}' // Hebrew/Arabic presentation forms
            | '\u{FE70}'..='\u{FEFF}'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> TextStyle {
        TextStyle {
            families: vec!["sans-serif".to_string()],
            size_px: 16.0,
            ..Default::default()
        }
    }

    #[test]
    fn text_actually_wraps() {
        // The headline: the placeholder never wrapped, so a long paragraph was
        // one box thousands of pixels wide.
        let mut il = InlineLayout::new();
        let text = "the quick brown fox jumps over the lazy dog and keeps running \
                    for a considerable distance afterwards";
        let wide = il.layout(text, &style(), Some(2000.0));
        let narrow = il.layout(text, &style(), Some(200.0));
        assert_eq!(wide.line_count, 1);
        assert!(
            narrow.line_count > 3,
            "expected several lines at 200px, got {}",
            narrow.line_count
        );
        assert!(
            narrow.width <= 200.0,
            "no line may exceed the available width, got {}",
            narrow.width
        );
    }

    #[test]
    fn height_follows_line_count() {
        let mut il = InlineLayout::new();
        let text = "one two three four five six seven eight nine ten eleven twelve";
        let out = il.layout(text, &style(), Some(120.0));
        assert_eq!(out.height, out.line_height * out.line_count as f32);
    }

    #[test]
    fn glyph_widths_are_not_uniform() {
        // The placeholder gave every glyph 0.6em, so "iiii" and "WWWW"
        // measured the same. They must not.
        let mut il = InlineLayout::new();
        let narrow = il.layout("iiii", &style(), None);
        let wide = il.layout("WWWW", &style(), None);
        assert!(
            wide.width > narrow.width * 1.5,
            "W should be much wider than i: {} vs {}",
            wide.width,
            narrow.width
        );
    }

    #[test]
    fn a_long_word_overflows_rather_than_breaking() {
        // CSS default: `overflow-wrap: normal` does not break inside a word.
        let mut il = InlineLayout::new();
        let out = il.layout(
            "Donaudampfschifffahrtsgesellschaftskapitaen",
            &style(),
            Some(50.0),
        );
        assert_eq!(out.line_count, 1);
        assert!(out.width > 50.0, "the word must overflow, not break");
    }

    #[test]
    fn break_word_splits_a_long_word() {
        let mut il = InlineLayout::new();
        let s = TextStyle {
            break_word: true,
            ..style()
        };
        let out = il.layout(
            "Donaudampfschifffahrtsgesellschaftskapitaen",
            &s,
            Some(50.0),
        );
        assert!(
            out.line_count > 1,
            "overflow-wrap: break-word must break the word, got {} line(s)",
            out.line_count
        );
    }

    #[test]
    fn nowrap_never_wraps() {
        let mut il = InlineLayout::new();
        let s = TextStyle {
            wraps: false,
            ..style()
        };
        let text = "the quick brown fox jumps over the lazy dog";
        let out = il.layout(text, &s, Some(50.0));
        assert_eq!(out.line_count, 1);
    }

    #[test]
    fn min_content_is_the_widest_word() {
        let mut il = InlineLayout::new();
        let out = il.layout("a bb extraordinarily cc", &style(), None);
        let word = il.layout("extraordinarily", &style(), None);
        assert!(
            (out.min_content_width - word.max_content_width).abs() < 0.5,
            "min-content {} should equal the widest word {}",
            out.min_content_width,
            word.max_content_width
        );
    }

    #[test]
    fn max_content_is_the_unwrapped_width() {
        let mut il = InlineLayout::new();
        let text = "one two three";
        let out = il.layout(text, &style(), Some(30.0));
        let unwrapped = il.layout(text, &style(), None);
        assert!((out.max_content_width - unwrapped.width).abs() < 0.5);
    }

    #[test]
    fn empty_text_has_no_lines() {
        let mut il = InlineLayout::new();
        let out = il.layout("", &style(), Some(100.0));
        assert_eq!(out.line_count, 0);
        assert_eq!(out.height, 0.0);
    }

    #[test]
    fn line_height_overrides_the_font_metric() {
        let mut il = InlineLayout::new();
        let s = TextStyle {
            line_height: Some(40.0),
            ..style()
        };
        let out = il.layout("one two three", &s, Some(50.0));
        assert_eq!(out.line_height, 40.0);
        assert_eq!(out.height, 40.0 * out.line_count as f32);
    }

    #[test]
    fn a_url_does_not_wrap_at_slashes() {
        // The tailoring, end to end: Chrome puts the whole URL on one line and
        // lets it overflow.
        let mut il = InlineLayout::new();
        let out = il.layout(
            "https://example.com/very/long/path/segment/that/will/not/fit",
            &style(),
            Some(100.0),
        );
        assert_eq!(
            out.line_count, 1,
            "a URL must overflow rather than break at its slashes"
        );
    }

    #[test]
    fn cjk_wraps_between_characters() {
        let mut il = InlineLayout::new();
        let out = il.layout(
            "现代浏览器必须处理复杂的文本排版问题其中包括中文日文和韩文的断行规则",
            &style(),
            Some(100.0),
        );
        assert!(
            out.line_count > 2,
            "CJK has no spaces and must still wrap, got {} line(s)",
            out.line_count
        );
    }

    #[test]
    fn the_cache_is_reused_across_relayouts() {
        let mut il = InlineLayout::new();
        let text = "the quick brown fox jumps over the lazy dog";
        il.layout(text, &style(), Some(200.0));
        let after_first = il.cache_stats();
        il.layout(text, &style(), Some(200.0));
        let after_second = il.cache_stats();
        assert!(
            after_second.hits > after_first.hits,
            "a second identical layout must hit the cache"
        );
    }
}
