//! Turning a hit-tested node into something a browser can navigate to.
//!
//! [`crate::render::hit_test`] answers "which node is under this point". That
//! is not the same question a shell needs to answer when the user clicks, for
//! two reasons:
//!
//! * The node under the pointer is almost never the `<a>`. Clicking a link
//!   whose text is bold lands on the `<b>`, or on the text node inside it, or
//!   on an `<img>` in a card whose whole surface is wrapped in an anchor. The
//!   anchor is an ancestor, so finding it is a walk and not a lookup.
//! * `href` is whatever the author wrote — `/about`, `../x`, `#top`,
//!   `mailto:`, `javascript:void(0)`. Handing that to a navigation call
//!   unresolved is how a browser ends up fetching `https://host/#top`.
//!
//! This module answers the first question. [`crate::Page::link_for`] answers
//! the second, because resolution needs the document's own URL.

use super::arena::Dom;
use super::node::{NodeData, NodeId};

/// What an anchor points at, before resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// The `href` exactly as authored.
    pub href: String,
    /// The `<a>` element itself, not the node that was hit. A shell that wants
    /// to style the link it is about to follow needs this one.
    pub anchor: NodeId,
    /// `target="_blank"` and friends, lowercased. `None` when absent.
    pub target: Option<String>,
    /// `download`, present or not. A shell that cannot download should say so
    /// rather than navigating to the file and rendering bytes as text.
    pub download: bool,
}

/// Walk up from `node` to the nearest `<a href>`.
///
/// Returns `None` for a node that is not inside a link, and — deliberately —
/// for an `<a>` with no `href`. An anchor without an href is not a link; HTML
/// calls it a placeholder, it does not get link styling, and it must not
/// navigate.
///
/// The walk is unbounded up the tree, but a DOM is a tree and `parent` strictly
/// decreases in depth, so it terminates at the document. It stops at the first
/// anchor found: nested anchors are invalid HTML and the parser does not
/// produce them, but if one arrived some other way, the innermost is the one a
/// browser follows.
pub fn link_target(dom: &Dom, node: NodeId) -> Option<LinkTarget> {
    let mut current = Some(node);
    while let Some(id) = current {
        let n = dom.get(id)?;
        if let NodeData::Element(e) = &n.data {
            if e.name.local.eq_ignore_ascii_case("a") {
                let attr = |want: &str| {
                    e.attrs
                        .iter()
                        .find(|a| a.name.local.eq_ignore_ascii_case(want))
                };
                if let Some(href) = attr("href") {
                    return Some(LinkTarget {
                        href: href.value.trim().to_string(),
                        anchor: id,
                        target: attr("target").map(|a| a.value.trim().to_lowercase()),
                        download: attr("download").is_some(),
                    });
                }
            }
        }
        current = n.parent;
    }
    None
}

/// Whether `href` is a scheme a page is allowed to send us to on a click.
///
/// `javascript:` is refused outright. Running author script because the user
/// clicked a link is exactly the capability this browser exists to not hand
/// out casually, and a shell that navigated to it would either execute it or
/// display the source — both wrong.
///
/// `mailto:`, `tel:` and the like are *not* refused here: they are legitimate,
/// but they are the platform's business rather than the engine's. The
/// classification is returned so a shell can hand them to the OS instead of
/// trying to render them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// Navigate. The engine can load this.
    Navigable,
    /// A fragment on the current document: scroll, do not reload.
    SamePageFragment,
    /// Hand to the operating system — `mailto:`, `tel:`, `sms:`.
    External,
    /// Refuse. `javascript:` only.
    Refused,
}

/// Classify a raw `href`.
///
/// Case-insensitive on the scheme, because `MAILTO:` is a valid spelling and a
/// case-sensitive check here would be a way past the `javascript:` refusal.
/// Leading whitespace and embedded control characters are stripped first for
/// the same reason — `java\nscript:alert(1)` is a real historical bypass, and
/// HTML's own URL parser strips those characters, so a check that does not is
/// checking a different string than the one that would be navigated.
pub fn classify(href: &str) -> LinkKind {
    let cleaned: String = href
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{feff}')
        .collect();
    let trimmed = cleaned.trim();

    if trimmed.is_empty() {
        // `href=""` means the current document, per RFC 3986.
        return LinkKind::SamePageFragment;
    }
    if trimmed.starts_with('#') {
        return LinkKind::SamePageFragment;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("javascript:") {
        return LinkKind::Refused;
    }
    for scheme in ["mailto:", "tel:", "sms:", "callto:", "facetime:"] {
        if lower.starts_with(scheme) {
            return LinkKind::External;
        }
    }
    LinkKind::Navigable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::node::{Attribute, QualName};

    /// `<a href="/x"><b>click <i>here</i></b></a>` plus a bare `<p>text</p>`.
    fn fixture() -> (Dom, NodeId, NodeId, NodeId, NodeId) {
        let mut dom = Dom::new();
        let doc = dom.document();

        let anchor = dom.create_element(
            QualName::new("a"),
            vec![Attribute {
                name: QualName::new("href"),
                value: "/x".to_string(),
            }],
        );
        let bold = dom.create_element(QualName::new("b"), vec![]);
        let italic = dom.create_element(QualName::new("i"), vec![]);
        let deep = dom.create_text("here".to_string());
        let outside = dom.create_element(QualName::new("p"), vec![]);

        dom.append_child(doc, anchor);
        dom.append_child(anchor, bold);
        dom.append_child(bold, italic);
        dom.append_child(italic, deep);
        dom.append_child(doc, outside);

        (dom, anchor, bold, deep, outside)
    }

    #[test]
    fn a_deeply_nested_node_finds_its_anchor() {
        // The case that matters: nobody ever clicks the <a> itself.
        let (dom, anchor, _, deep, _) = fixture();
        let found = link_target(&dom, deep).expect("the text is inside a link");
        assert_eq!(found.href, "/x");
        assert_eq!(
            found.anchor, anchor,
            "the anchor, not the node that was hit"
        );
    }

    #[test]
    fn the_anchor_itself_also_works() {
        let (dom, anchor, _, _, _) = fixture();
        assert_eq!(link_target(&dom, anchor).unwrap().href, "/x");
    }

    #[test]
    fn a_node_outside_any_link_finds_nothing() {
        let (dom, _, _, _, outside) = fixture();
        assert!(link_target(&dom, outside).is_none());
    }

    #[test]
    fn an_anchor_without_href_is_not_a_link() {
        // HTML calls this a placeholder. It gets no link styling and it must
        // not navigate — a shell treating it as a link would send the user to
        // the current page on every click.
        let mut dom = Dom::new();
        let doc = dom.document();
        let anchor = dom.create_element(QualName::new("a"), vec![]);
        let text = dom.create_text("not a link".to_string());
        dom.append_child(doc, anchor);
        dom.append_child(anchor, text);

        assert!(link_target(&dom, text).is_none());
    }

    #[test]
    fn href_is_trimmed_and_extras_are_carried() {
        let mut dom = Dom::new();
        let doc = dom.document();
        let anchor = dom.create_element(
            QualName::new("a"),
            vec![
                Attribute {
                    name: QualName::new("HREF"),
                    value: "  /y  ".to_string(),
                },
                Attribute {
                    name: QualName::new("target"),
                    value: "_BLANK".to_string(),
                },
                Attribute {
                    name: QualName::new("download"),
                    value: String::new(),
                },
            ],
        );
        dom.append_child(doc, anchor);

        let found = link_target(&dom, anchor).unwrap();
        assert_eq!(found.href, "/y", "authors leave whitespace in href");
        assert_eq!(
            found.target.as_deref(),
            Some("_blank"),
            "attribute names and target values are both case-insensitive"
        );
        assert!(found.download);
    }

    #[test]
    fn javascript_urls_are_refused_however_they_are_spelled() {
        assert_eq!(classify("javascript:alert(1)"), LinkKind::Refused);
        assert_eq!(classify("JavaScript:alert(1)"), LinkKind::Refused);
        assert_eq!(classify("  javascript:alert(1)"), LinkKind::Refused);
        // The historical bypass: HTML's URL parser strips control characters,
        // so a check that does not strip them is checking a different string
        // than the one that would be navigated.
        assert_eq!(classify("java\nscript:alert(1)"), LinkKind::Refused);
        assert_eq!(classify("java\tscript:alert(1)"), LinkKind::Refused);
        assert_eq!(classify("\u{0}javascript:alert(1)"), LinkKind::Refused);
    }

    #[test]
    fn fragments_and_empty_hrefs_do_not_reload() {
        assert_eq!(classify("#top"), LinkKind::SamePageFragment);
        assert_eq!(classify(""), LinkKind::SamePageFragment);
        assert_eq!(classify("   "), LinkKind::SamePageFragment);
    }

    #[test]
    fn platform_schemes_are_the_operating_systems_business() {
        assert_eq!(classify("mailto:a@b.c"), LinkKind::External);
        assert_eq!(classify("MAILTO:a@b.c"), LinkKind::External);
        assert_eq!(classify("tel:+15551234"), LinkKind::External);
    }

    #[test]
    fn ordinary_links_are_navigable() {
        assert_eq!(classify("/about"), LinkKind::Navigable);
        assert_eq!(classify("../x"), LinkKind::Navigable);
        assert_eq!(classify("https://example.com/"), LinkKind::Navigable);
        // Not a scheme this refuses, and not one the OS wants either — the
        // engine should try, and fail honestly if it cannot.
        assert_eq!(classify("ftp://example.com/"), LinkKind::Navigable);
    }
}
