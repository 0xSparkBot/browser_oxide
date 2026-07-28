//! Line break opportunities: UAX #14, bent into what browsers actually do.
//!
//! `unicode-linebreak` implements UAX #14 faithfully. Browsers do not — they
//! use ICU's *tailored* rules, and the difference shows up on the most ordinary
//! content on the web.

use unicode_linebreak::BreakOpportunity;

/// A break opportunity: the byte offset at which a new line would start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opportunity {
    pub offset: usize,
    /// A forced break (newline), as opposed to a permitted one.
    pub mandatory: bool,
}

/// Break opportunities for `text`, tailored to match Chrome.
pub fn opportunities(text: &str) -> Vec<Opportunity> {
    unicode_linebreak::linebreaks(text)
        .filter_map(|(offset, op)| {
            let mandatory = op == BreakOpportunity::Mandatory;
            if !mandatory && !allowed_by_tailoring(text, offset) {
                return None;
            }
            Some(Opportunity { offset, mandatory })
        })
        .collect()
}

/// The tailorings. Currently one, measured rather than assumed.
///
/// SOLIDUS (`/`) is class SY. UAX #14 forbids a break *before* it (LB13) and
/// says nothing about after, so LB31 permits one — meaning a faithful
/// implementation wraps `https://example.com/very/long/path` at every slash.
/// Chrome breaks there **zero times**; it lets the URL overflow, which is
/// precisely why authors reach for `overflow-wrap: anywhere`.
///
/// Measured against Chrome 150 this single tailoring was worth two fixtures out
/// of twelve. There will be more of these, and each is invisible until a
/// fixture exposes it — the list should grow from a real-site corpus, not from
/// reading the spec.
fn allowed_by_tailoring(text: &str, offset: usize) -> bool {
    let before = text[..offset].chars().next_back();
    let after = text[offset..].chars().next();
    // Keep the opportunity when whitespace follows: that break belongs to the
    // space, not to the slash.
    !matches!(before, Some('/')) || matches!(after, None | Some(' ') | Some('\t') | Some('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offsets(text: &str) -> Vec<usize> {
        opportunities(text).iter().map(|o| o.offset).collect()
    }

    #[test]
    fn breaks_after_spaces() {
        // "one two" — opportunity after the space, and at end of text.
        let o = offsets("one two");
        assert!(o.contains(&4), "expected a break before 'two', got {o:?}");
    }

    #[test]
    fn does_not_break_after_a_slash_inside_a_url() {
        // The tailoring. Untailored UAX #14 offers a break after every slash.
        let url = "https://example.com/very/long/path";
        let o = offsets(url);
        let slash_breaks: Vec<usize> = o
            .iter()
            .copied()
            .filter(|&i| i > 0 && i < url.len() && url[..i].ends_with('/'))
            .collect();
        assert!(
            slash_breaks.is_empty(),
            "Chrome does not break after '/' in a URL; got breaks at {slash_breaks:?}"
        );
    }

    #[test]
    fn still_breaks_after_a_slash_followed_by_a_space() {
        // The tailoring must not swallow the space's own opportunity.
        let text = "and/ or";
        let o = offsets(text);
        assert!(o.contains(&5), "expected a break before 'or', got {o:?}");
    }

    #[test]
    fn newline_is_mandatory() {
        let ops = opportunities("a\nb");
        assert!(
            ops.iter().any(|o| o.mandatory && o.offset == 2),
            "expected a mandatory break after the newline, got {ops:?}"
        );
    }

    #[test]
    fn cjk_breaks_between_characters() {
        // No spaces, so every inter-character position is an opportunity.
        let text = "现代浏览器";
        let o = offsets(text);
        assert!(
            o.len() > 2,
            "CJK must offer per-character breaks, got {o:?}"
        );
    }
}
