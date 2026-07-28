//! An inline formatting context: several styled runs sharing one set of lines.
//!
//! [`super::InlineLayout::layout`] takes one style for an entire string, which
//! is enough for a text node on its own and not enough for a browser. A
//! paragraph is almost never one style:
//!
//! ```html
//! <p>Read more <a href="/about">about the project</a>, or see the
//! <b>benchmarks</b>.</p>
//! ```
//!
//! Those five pieces — text, link, text, bold, text — are one *inline
//! formatting context*. They share line boxes: the comma after the link sits on
//! whatever line the link ended on, and only the paragraph's width decides
//! where anything breaks. Laying each piece out on its own is not a smaller
//! version of the right answer, it is a different and visibly wrong one. Before
//! this module the engine did exactly that, and a captured screenshot of the
//! Windows app showed the paragraph above rendering as six stacked lines with
//! an orphaned comma and a lone full stop on lines of their own.
//!
//! # How this relates to the single-style path
//!
//! It is a deliberate second implementation of the greedy fit rather than a
//! generalisation of the first. The single-style path is the one PoC-1
//! validated at 12/12 against Chrome 150 with a worst geometric error of
//! 0.03 px, and threading span-awareness through it would have put that result
//! at risk for the sake of avoiding sixty lines.
//!
//! The two are pinned together instead: `one_run_matches_the_single_style_path`
//! asserts that an IFC of exactly one run produces the same line count and
//! width as [`super::InlineLayout::layout`] on the same text. Divergence is a
//! test failure rather than a slow drift nobody notices.
//!
//! # What is still missing
//!
//! Vertical alignment other than the baseline, floats, and inline-block
//! atomics. The strut is the tallest run on the line, which is right for
//! `vertical-align: baseline` — the initial value and the overwhelming
//! majority of real text — and wrong for anything else.

use rustybuzz::Face as HbFace;

use super::breaks;
use super::metrics;
use super::{dominant_script, is_rtl, skip_leading_spaces, trim_trailing_spaces};
use super::{InlineLayout, LineBox, PositionedGlyph, TextLayout, TextStyle};
use crate::canvas::text::font_database::FontDatabase;

/// One styled piece of an inline formatting context.
///
/// `text` must already have had CSS white-space processing applied, and the
/// collapsing must have been done across the whole context rather than per
/// run — a space at the end of one run and the start of the next collapses to
/// one space, and a caller that collapses each run separately keeps both.
#[derive(Debug, Clone)]
pub struct InlineRun {
    pub text: String,
    pub style: TextStyle,
    /// Opaque to layout; handed back on every glyph so the painter can find the
    /// colour, decoration and DOM node this run came from.
    pub source: u32,
}

/// A run with its font resolved and its place in the concatenated text.
struct Span {
    start: usize,
    end: usize,
    face: HbFace<'static>,
    face_key: u64,
    style: TextStyle,
    source: u32,
    /// Ascent and line height for the strut.
    ascent: f32,
    line_height: f32,
}

/// The lines an inline formatting context produced.
pub struct InlineFlow {
    pub summary: TextLayout,
    pub lines: Vec<LineBox>,
}

impl InlineLayout {
    /// Lay out several styled runs as one inline formatting context.
    ///
    /// Returns line boxes whose glyphs are tagged with the `source` of the run
    /// they came from, because a single line can carry glyphs from several
    /// fonts at several sizes and the painter needs to know which is which.
    pub fn layout_runs(
        &mut self,
        runs: &[InlineRun],
        available_width: Option<f32>,
    ) -> Option<InlineFlow> {
        let (text, spans) = resolve(runs)?;
        if spans.is_empty() {
            return None;
        }

        // Breaking is a property of the text, not of the styles: the same
        // string breaks in the same places whether or not part of it is bold.
        // So opportunities come from the whole concatenation, which is also the
        // only way a break between two runs can be found at all.
        let ops = breaks::opportunities(&text);

        let wraps = spans.iter().all(|s| s.style.wraps);
        let break_word = spans.iter().any(|s| s.style.break_word);

        let max_content =
            self.measure_range(&text, &spans, 0, trim_trailing_spaces(&text, text.len()));
        let fitted = match (available_width, wraps) {
            (Some(avail), true) => self.fit_runs(&text, &spans, &ops, avail, break_word),
            _ => self.forced_only(&text, &spans, &ops, max_content),
        };

        // The strut: a line is as tall as the tallest run on it, and its
        // baseline sits at the deepest ascent. Correct for `vertical-align:
        // baseline`, which is the initial value; anything else needs a real
        // alignment pass.
        let mut lines = Vec::with_capacity(fitted.len());
        let mut y = 0.0f32;
        let mut widest = 0.0f32;
        for (start, end, width) in fitted {
            let (ascent, height) = self.strut(&spans, start, end);
            let glyphs = self.glyphs_for(&text, &spans, start, end);
            widest = widest.max(width);
            lines.push(LineBox {
                y,
                baseline: ascent,
                width,
                height,
                start,
                end,
                glyphs,
            });
            y += height;
        }

        let first = lines.first();
        Some(InlineFlow {
            summary: TextLayout {
                width: widest,
                height: y,
                line_count: lines.len(),
                line_height: first.map(|l| l.height).unwrap_or(0.0),
                first_baseline: first.map(|l| l.baseline).unwrap_or(0.0),
                max_content_width: max_content,
                min_content_width: self.min_content_runs(&text, &spans, &ops),
            },
            lines,
        })
    }

    /// Width of `text[a..b]`, charging each byte to the style that owns it.
    ///
    /// This is the whole difference from the single-style path. A range that
    /// crosses a style boundary is measured as the sum of its parts, because
    /// the same characters are a different width in a different font at a
    /// different size — and a line that straddles a boundary is the normal
    /// case, not an edge one.
    fn measure_range(&mut self, text: &str, spans: &[Span], a: usize, b: usize) -> f32 {
        if b <= a {
            return 0.0;
        }
        let mut total = 0.0f32;
        for span in spans {
            let lo = span.start.max(a);
            let hi = span.end.min(b);
            if hi <= lo {
                continue;
            }
            let slice = &text[lo..hi];
            if slice.is_empty() {
                continue;
            }
            total += self.cache_measure(span, slice);
        }
        total
    }

    fn cache_measure(&mut self, span: &Span, slice: &str) -> f32 {
        // Script and direction are taken per span rather than per context: a
        // Latin caption inside an Arabic paragraph shapes as Latin, and asking
        // the whole concatenation would give both the same answer.
        let script = dominant_script(slice);
        let rtl = is_rtl(slice);
        self.cache.measure(
            &span.face,
            span.face_key,
            span.style.size_px,
            script,
            rtl,
            slice,
        )
    }

    /// Greedy first-fit over the whole context. Chrome is greedy; matching it
    /// needs the same measurement and tie-breaking, not a better algorithm.
    fn fit_runs(
        &mut self,
        text: &str,
        spans: &[Span],
        ops: &[breaks::Opportunity],
        avail: f32,
        break_word: bool,
    ) -> Vec<(usize, usize, f32)> {
        // Sub-pixel slack, for the same reason the single-style path has it: a
        // line whose width lands on `avail + 1e-4` would wrap a word early and
        // every line below it would be wrong.
        const EPS: f32 = 0.01;

        let mut lines: Vec<(usize, usize, f32)> = Vec::new();
        let mut start = 0usize;
        let mut committed = 0usize;
        let mut committed_width = 0.0f32;

        for op in ops {
            if op.offset <= start {
                continue;
            }
            let mut trimmed = trim_trailing_spaces(text, op.offset);
            let mut width = self.measure_range(text, spans, start, trimmed);

            if width > avail + EPS && committed > start {
                let end = trim_trailing_spaces(text, committed);
                lines.push((start, end, committed_width));
                start = skip_leading_spaces(text, committed);
                trimmed = trim_trailing_spaces(text, op.offset);
                width = self.measure_range(text, spans, start, trimmed);
            }

            if op.mandatory {
                lines.push((start, trimmed, width));
                start = skip_leading_spaces(text, op.offset);
                committed = start;
                committed_width = 0.0;
                continue;
            }

            if width <= avail + EPS {
                committed = op.offset;
                committed_width = width;
            } else if committed == start && break_word {
                // A single segment wider than the line, and the style allows
                // cutting it. `overflow-wrap: normal` lets it overflow instead,
                // which is CSS's actual default and what Chrome does with a
                // long URL.
                let cut = self.break_inside_runs(text, spans, start, trimmed, avail);
                if cut > start {
                    let w = self.measure_range(text, spans, start, cut);
                    lines.push((start, cut, w));
                    start = cut;
                    committed = start;
                    committed_width = 0.0;
                }
            }
        }

        let tail = trim_trailing_spaces(text, text.len());
        if tail > start {
            let w = self.measure_range(text, spans, start, tail);
            lines.push((start, tail, w));
        } else if lines.is_empty() {
            lines.push((0, tail, 0.0));
        }
        lines
    }

    /// Cut an over-long segment at the last character that still fits.
    fn break_inside_runs(
        &mut self,
        text: &str,
        spans: &[Span],
        start: usize,
        end: usize,
        avail: f32,
    ) -> usize {
        let mut last_fitting = start;
        for (offset, _) in text[start..end].char_indices() {
            let at = start + offset;
            if at == start {
                continue;
            }
            if self.measure_range(text, spans, start, at) > avail {
                break;
            }
            last_fitting = at;
        }
        last_fitting
    }

    /// One line per forced break, for `white-space: nowrap` and `pre`.
    fn forced_only(
        &mut self,
        text: &str,
        spans: &[Span],
        ops: &[breaks::Opportunity],
        max_content: f32,
    ) -> Vec<(usize, usize, f32)> {
        let mut lines = Vec::new();
        let mut start = 0usize;
        for op in ops.iter().filter(|o| o.mandatory) {
            let end = trim_trailing_spaces(text, op.offset);
            let width = self.measure_range(text, spans, start, end);
            lines.push((start, end, width));
            start = op.offset;
        }
        if lines.is_empty() {
            lines.push((0, trim_trailing_spaces(text, text.len()), max_content));
        }
        lines
    }

    fn min_content_runs(&mut self, text: &str, spans: &[Span], ops: &[breaks::Opportunity]) -> f32 {
        let mut widest = 0.0f32;
        let mut start = 0usize;
        for op in ops {
            if op.offset <= start {
                continue;
            }
            let end = trim_trailing_spaces(text, op.offset);
            if end > start {
                widest = widest.max(self.measure_range(text, spans, start, end));
            }
            start = op.offset;
        }
        widest
    }

    /// The tallest run on a line decides its height and baseline.
    fn strut(&self, spans: &[Span], start: usize, end: usize) -> (f32, f32) {
        let mut ascent = 0.0f32;
        let mut height = 0.0f32;
        for span in spans {
            if span.end <= start || span.start >= end.max(start) {
                continue;
            }
            ascent = ascent.max(span.ascent);
            height = height.max(span.line_height);
        }
        if height == 0.0 {
            // An empty line still occupies the first span's strut, which is
            // what an empty <p> does in every browser.
            if let Some(first) = spans.first() {
                return (first.ascent, first.line_height);
            }
        }
        (ascent, height)
    }

    /// Shape a line, span by span, laying each piece down after the last.
    fn glyphs_for(
        &mut self,
        text: &str,
        spans: &[Span],
        start: usize,
        end: usize,
    ) -> Vec<PositionedGlyph> {
        let mut out = Vec::new();
        let mut x = 0.0f32;
        for (index, span) in spans.iter().enumerate() {
            let lo = span.start.max(start);
            let hi = span.end.min(end);
            if hi <= lo {
                continue;
            }
            let slice = &text[lo..hi];
            if slice.is_empty() {
                continue;
            }
            let script = dominant_script(slice);
            let rtl = is_rtl(slice);
            let shaped = self.cache.shape(
                &span.face,
                span.face_key,
                span.style.size_px,
                script,
                rtl,
                slice,
            );
            for g in shaped.iter() {
                out.push(PositionedGlyph {
                    id: g.id,
                    x: x + g.x_offset,
                    y: -g.y_offset,
                    run: index as u16,
                    source: span.source,
                });
                x += g.x_advance;
            }
        }
        out
    }
}

/// Concatenate the runs and resolve each one's font.
///
/// Runs whose font cannot be resolved are dropped rather than silently
/// measured with somebody else's metrics — a missing font is a visible gap, not
/// a reason to render the wrong widths.
fn resolve(runs: &[InlineRun]) -> Option<(String, Vec<Span>)> {
    let db = FontDatabase::get();
    let mut text = String::new();
    let mut spans = Vec::with_capacity(runs.len());

    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        let start = text.len();
        text.push_str(&run.text);
        let end = text.len();

        let Some(face_id) = db.query_chain(
            &run.style.families,
            run.style.weight,
            run.style.italic,
            &run.style.os_name,
        ) else {
            continue;
        };
        let Some((data, index)) = db.face_data(face_id) else {
            continue;
        };
        let Some(face) = HbFace::from_slice(data, index) else {
            continue;
        };
        let Ok(ttf) = ttf_parser::Face::parse(data, index) else {
            continue;
        };

        let m = metrics::metrics_for(&ttf, run.style.size_px, run.style.metrics);
        let line_height = run.style.line_height.unwrap_or(m.line_spacing);
        let (ascent, _) = m.leaded(line_height);

        spans.push(Span {
            start,
            end,
            // `face_data` borrows from the process-wide font database, which is
            // a `OnceLock` static — so the slice really does live for the
            // program and the address is a stable cache identity.
            face,
            face_key: data.as_ptr() as u64 ^ u64::from(index),
            style: run.style.clone(),
            source: run.source,
            ascent,
            line_height,
        });
    }

    if text.is_empty() {
        return None;
    }
    Some((text, spans))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f32) -> TextStyle {
        TextStyle {
            families: vec!["DejaVu Sans".into(), "sans-serif".into()],
            size_px: size,
            ..Default::default()
        }
    }

    fn run(text: &str, size: f32, source: u32) -> InlineRun {
        InlineRun {
            text: text.to_string(),
            style: style(size),
            source,
        }
    }

    /// The defect this module exists to fix, stated as a test.
    ///
    /// Five runs, one of them a link, and the whole thing narrow enough to wrap
    /// once. A correct inline formatting context puts the comma on whatever
    /// line the link ended on and never gives it a line of its own.
    #[test]
    fn a_link_inside_a_sentence_does_not_break_the_sentence() {
        let mut layout = InlineLayout::new();
        let runs = vec![
            run("Read more ", 16.0, 0),
            run("about the project", 16.0, 1),
            run(", or see the ", 16.0, 2),
            run("benchmarks", 16.0, 3),
            run(".", 16.0, 4),
        ];

        let flow = layout.layout_runs(&runs, Some(400.0)).expect("laid out");

        // The whole context is ~40 characters at 16px, so it wraps once or
        // twice at 400px — but never once per run, which is what five lines
        // would mean and what the bug produced.
        assert!(
            flow.lines.len() <= 3,
            "five runs became {} lines — the runs are being laid out separately",
            flow.lines.len()
        );

        // No line may consist only of punctuation. That is the orphaned comma
        // from the screenshot, and it is the visible signature of the bug.
        for line in &flow.lines {
            let slice: String = flow_text(&runs)[line.start..line.end].to_string();
            let trimmed = slice.trim();
            assert!(
                trimmed.is_empty() || trimmed.chars().any(|c| c.is_alphanumeric()),
                "a line holds only {trimmed:?}"
            );
        }
    }

    /// Glyphs carry the run they came from, so the painter can find the colour.
    #[test]
    fn every_glyph_knows_which_run_it_belongs_to() {
        let mut layout = InlineLayout::new();
        let runs = vec![run("plain ", 16.0, 7), run("linked", 16.0, 9)];
        let flow = layout.layout_runs(&runs, None).expect("laid out");

        let sources: Vec<u32> = flow
            .lines
            .iter()
            .flat_map(|l| l.glyphs.iter().map(|g| g.source))
            .collect();
        assert!(sources.contains(&7), "the plain run is attributed");
        assert!(sources.contains(&9), "and so is the link");
    }

    /// A larger run makes the whole line taller — the strut.
    #[test]
    fn the_tallest_run_sets_the_line_height() {
        let mut layout = InlineLayout::new();
        let small = layout
            .layout_runs(&[run("aa ", 12.0, 0), run("bb", 12.0, 1)], None)
            .unwrap();
        let mixed = layout
            .layout_runs(&[run("aa ", 12.0, 0), run("bb", 32.0, 1)], None)
            .unwrap();

        assert!(
            mixed.lines[0].height > small.lines[0].height,
            "a 32px run on the line must make the line taller than an all-12px one"
        );
        assert!(
            mixed.lines[0].baseline > small.lines[0].baseline,
            "and push the baseline down"
        );
    }

    /// Text in a bigger font is wider, and a range crossing the boundary is the
    /// sum of its parts rather than either style applied to everything.
    #[test]
    fn a_range_across_a_style_boundary_is_charged_to_both() {
        let mut layout = InlineLayout::new();
        let all_small = layout
            .layout_runs(&[run("aaaaaaaa", 12.0, 0)], None)
            .unwrap();
        let all_big = layout
            .layout_runs(&[run("aaaaaaaa", 24.0, 0)], None)
            .unwrap();
        let half = layout
            .layout_runs(&[run("aaaa", 12.0, 0), run("aaaa", 24.0, 1)], None)
            .unwrap();

        let (small, big, mixed) = (
            all_small.summary.width,
            all_big.summary.width,
            half.summary.width,
        );
        assert!(
            mixed > small && mixed < big,
            "mixed {mixed} should sit between all-12px {small} and all-24px {big}"
        );
    }

    /// The pin between this path and the Chrome-validated single-style one.
    ///
    /// One run must produce what `layout` produces. If the two ever disagree,
    /// this fails rather than the difference showing up months later as a
    /// paragraph that wraps one word early.
    #[test]
    fn one_run_matches_the_single_style_path() {
        let mut layout = InlineLayout::new();
        let text = "The quick brown fox jumps over the lazy dog, and then it does \
                    the whole thing again at some length so that this wraps.";

        for width in [120.0f32, 250.0, 400.0, 800.0] {
            let single = layout.layout(text, &style(16.0), Some(width));
            let flow = layout
                .layout_runs(&[run(text, 16.0, 0)], Some(width))
                .expect("laid out");

            assert_eq!(
                flow.summary.line_count, single.line_count,
                "line count diverged at width {width}"
            );
            assert!(
                (flow.summary.width - single.width).abs() < 0.5,
                "width diverged at {width}: runs {} vs single {}",
                flow.summary.width,
                single.width
            );
        }
    }

    #[test]
    fn an_empty_context_is_none_rather_than_a_zero_height_line() {
        let mut layout = InlineLayout::new();
        assert!(layout.layout_runs(&[], None).is_none());
        assert!(layout.layout_runs(&[run("", 16.0, 0)], None).is_none());
    }

    fn flow_text(runs: &[InlineRun]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }
}
