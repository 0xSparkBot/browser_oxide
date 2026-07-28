//! Style resolution: DOM + stylesheets → a `ComputedStyle` per element.
//!
//! Every piece of this already existed — `css_parser` produces rules,
//! `css_selectors` matches them, `css_cascade` sorts and inherits. What did not
//! exist was anything that *assembled* them for layout. `LayoutEngine` built
//! each element's style as
//!
//! ```ignore
//! let computed = ComputedStyle::resolve(&HashMap::new(), None);
//! ```
//!
//! — an empty cascaded map, with inline `style` attributes overlaid — so a
//! `<style>` block had no effect on geometry at all, and neither did a
//! stylesheet. ~8,800 LOC of CSS machinery fed a 1,004-LOC layout that could
//! not see it. (`getComputedStyle` had its own separate path in
//! `js_runtime::state`, which is why the gap was not obvious: the *reported*
//! style was right while the *laid out* style was not.)
//!
//! This module is that missing assembly, and nothing more. It does no layout
//! and knows nothing about taffy.

use std::collections::HashMap;

use crate::css_cascade::{cascade_sort, CascadeEntry, ComputedStyle, Origin};
use crate::css_parser::ast::{Block, ComponentValue, Declaration, Rule};
use crate::css_selectors::{
    compute_specificity, matches_selector, parse_selector_list, Selector, Specificity,
};
use crate::css_values::property::PropertyDeclaration;
use crate::dom::element::DomElement;
use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;

/// The user-agent stylesheet, compiled in.
pub const UA_STYLESHEET: &str = include_str!("ua.css");

/// Style-attribute declarations must beat every author rule regardless of the
/// rule's specificity. The cascade in `css_cascade` has origin, layer,
/// specificity and source order but no separate style-attribute tier, so the
/// attribute is expressed as an unreachable specificity instead. Any real
/// selector would need `u32::MAX` id selectors to tie.
const INLINE_SPECIFICITY: Specificity = Specificity {
    a: u32::MAX,
    b: 0,
    c: 0,
};

/// A parsed style rule, ready to match.
struct StyleRule {
    selectors: Vec<Selector>,
    declarations: Vec<PropertyDeclaration>,
    origin: Origin,
    source_order: u32,
}

/// Computed styles for a document, keyed by node.
#[derive(Debug, Default, Clone)]
pub struct StyleTree {
    styles: HashMap<u32, ComputedStyle>,
}

impl StyleTree {
    pub fn get(&self, node_id: NodeId) -> Option<&ComputedStyle> {
        self.styles.get(&node_id.to_raw())
    }

    /// The style for a node, or a fresh initial style if it has none (a text
    /// node, or an element that was never visited).
    pub fn get_or_initial(&self, node_id: NodeId) -> ComputedStyle {
        self.styles
            .get(&node_id.to_raw())
            .cloned()
            .unwrap_or_else(|| ComputedStyle::resolve(&HashMap::new(), None))
    }

    pub fn len(&self) -> usize {
        self.styles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }
}

/// Resolve styles for every element in `dom`.
///
/// `extra_css` is author CSS the caller already has in hand — external
/// stylesheets it fetched, or a sheet injected by a test. `<style>` blocks in
/// the document are found here and do not need to be passed in.
pub fn compute_styles(dom: &Dom, extra_css: &[String]) -> StyleTree {
    let mut rules = Vec::new();
    let mut order = 0u32;

    collect_rules(UA_STYLESHEET, Origin::UserAgent, &mut order, &mut rules);

    for entry in crate::stylesheet_collector::find_stylesheets(dom) {
        if let crate::stylesheet_collector::StylesheetEntry::Inline(css) = entry {
            collect_rules(&css, Origin::Author, &mut order, &mut rules);
        }
        // External sheets are the caller's job to fetch; they arrive via
        // `extra_css`. Silently ignoring them here is what the engine already
        // does elsewhere, and pretending otherwise would be worse.
    }
    for css in extra_css {
        collect_rules(css, Origin::Author, &mut order, &mut rules);
    }

    let mut tree = StyleTree::default();
    let root_style = ComputedStyle::resolve(&HashMap::new(), None);
    resolve_subtree(dom, NodeId::DOCUMENT, &root_style, &rules, &mut tree);
    tree
}

/// Parse one stylesheet's qualified rules, descending through `@media` blocks
/// whose queries currently evaluate true.
fn collect_rules(css: &str, origin: Origin, order: &mut u32, out: &mut Vec<StyleRule>) {
    let (stylesheet, _errors) = crate::css_parser::parse_stylesheet(css);
    collect_from_rules(&stylesheet.rules, origin, order, out);
}

fn collect_from_rules(
    rules: &[Rule<'_>],
    origin: Origin,
    order: &mut u32,
    out: &mut Vec<StyleRule>,
) {
    let features = crate::css_cascade::MediaFeatures::default();
    for rule in rules {
        match rule {
            Rule::Qualified(qr) => {
                let selector_text = crate::js_runtime::utils::tokens_to_string(&qr.prelude);
                let selector_text = selector_text.trim();
                if selector_text.is_empty() {
                    continue;
                }
                let Ok(selectors) = parse_selector_list(selector_text) else {
                    continue;
                };
                let declarations = parse_declarations(&qr.declarations);
                if declarations.is_empty() || selectors.is_empty() {
                    continue;
                }
                *order += 1;
                out.push(StyleRule {
                    selectors,
                    declarations,
                    origin,
                    source_order: *order,
                });
            }
            Rule::At(at) => {
                // `@media` and `@layer`/`@supports` blocks contain rules. Media
                // queries are evaluated; the others are descended into so their
                // contents are not silently dropped. Layer *ordering* is not
                // modelled here — `css_cascade::layers` exists for that and is
                // not yet wired up.
                let applies = if at.name.eq_ignore_ascii_case("media") {
                    crate::css_cascade::evaluate_media_query(&at.prelude, &features)
                } else {
                    true
                };
                if !applies {
                    continue;
                }
                match &at.block {
                    Some(Block::RuleList(inner)) => collect_from_rules(inner, origin, order, out),
                    Some(Block::DeclarationBlock { rules: inner, .. }) => {
                        collect_from_rules(inner, origin, order, out)
                    }
                    None => {}
                }
            }
        }
    }
}

fn parse_declarations(decls: &[Declaration<'_>]) -> Vec<PropertyDeclaration> {
    let mut out = Vec::new();
    for decl in decls {
        if let Ok(props) = crate::css_values::parse_property(decl.name, &decl.value, decl.important)
        {
            out.extend(props);
        }
    }
    out
}

/// Depth-first walk, resolving each element against its parent's computed
/// style so inheritance falls out naturally.
fn resolve_subtree(
    dom: &Dom,
    node_id: NodeId,
    parent_style: &ComputedStyle,
    rules: &[StyleRule],
    tree: &mut StyleTree,
) {
    let Some(node) = dom.get(node_id) else {
        return;
    };

    let style = match &node.data {
        NodeData::Element(_) => {
            let computed = resolve_element(dom, node_id, parent_style, rules);
            tree.styles.insert(node_id.to_raw(), computed.clone());
            computed
        }
        // Text and comment nodes inherit wholesale; storing a copy per text
        // node would double the map for no gain, so they are not stored and
        // callers read the parent's.
        _ => parent_style.clone(),
    };

    for child in dom.children(node_id) {
        resolve_subtree(dom, child, &style, rules, tree);
    }
}

fn resolve_element(
    dom: &Dom,
    node_id: NodeId,
    parent_style: &ComputedStyle,
    rules: &[StyleRule],
) -> ComputedStyle {
    let mut entries: Vec<CascadeEntry> = Vec::new();

    if let Some(element) = DomElement::new(dom, node_id) {
        for rule in rules {
            // A rule contributes at the specificity of its *most specific
            // matching* selector, not of the list.
            let mut best: Option<Specificity> = None;
            for selector in &rule.selectors {
                if matches_selector(&element, selector) {
                    let spec = compute_specificity(selector);
                    best = Some(match best {
                        Some(current) => current.max(spec),
                        None => spec,
                    });
                }
            }
            let Some(specificity) = best else {
                continue;
            };
            for declaration in &rule.declarations {
                entries.push(CascadeEntry {
                    declaration: declaration.clone(),
                    origin: rule.origin,
                    layer: None,
                    specificity,
                    source_order: rule.source_order,
                });
            }
        }
    }

    for declaration in inline_declarations(dom, node_id) {
        entries.push(CascadeEntry {
            declaration,
            origin: Origin::Author,
            layer: None,
            specificity: INLINE_SPECIFICITY,
            source_order: u32::MAX,
        });
    }

    let cascaded = cascade_sort(&mut entries);
    ComputedStyle::resolve(&cascaded, Some(parent_style))
}

/// Declarations from an element's `style` attribute.
pub fn inline_declarations(dom: &Dom, node_id: NodeId) -> Vec<PropertyDeclaration> {
    let Some(node) = dom.get(node_id) else {
        return Vec::new();
    };
    let NodeData::Element(elem) = &node.data else {
        return Vec::new();
    };
    let Some(attr) = elem.attrs.iter().find(|a| a.name.local == *"style") else {
        return Vec::new();
    };
    let (decls, _) = crate::css_parser::parse_declaration_list(&attr.value);
    parse_declarations(&decls)
}

/// `tokens_to_string` lives in `js_runtime::utils`, which is an odd home for a
/// CSS helper. Re-exported here so style code does not have to reach into the
/// JS runtime to serialise a selector prelude.
pub fn selector_text(prelude: &[ComponentValue<'_>]) -> String {
    crate::js_runtime::utils::tokens_to_string(prelude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_values::property::{CssValue, PropertyId};
    use crate::css_values::types::display::Display;

    fn styles_for(html: &str) -> (Dom, StyleTree) {
        let dom = crate::html_parser::parse_html(html);
        let tree = compute_styles(&dom, &[]);
        (dom, tree)
    }

    fn first(dom: &Dom, tag: &str) -> NodeId {
        *dom.get_elements_by_tag_name(NodeId::DOCUMENT, tag)
            .first()
            .unwrap_or_else(|| panic!("no <{tag}> in {}", dom.serialize_html(NodeId::DOCUMENT)))
    }

    #[test]
    fn stylesheet_reaches_layout_style() {
        // The regression this module exists for: before it, a <style> block
        // had no effect on anything layout could see.
        let (dom, tree) = styles_for(
            "<html><head><style>div { width: 300px }</style></head>\
             <body><div id=a>x</div></body></html>",
        );
        let div = first(&dom, "div");
        let style = tree.get(div).expect("div has a computed style");
        assert!(
            matches!(
                style.get(&PropertyId::Width),
                Some(CssValue::LengthPercentageAuto(_))
            ),
            "width from a stylesheet should be present, got {:?}",
            style.get(&PropertyId::Width)
        );
    }

    #[test]
    fn ua_stylesheet_hides_head() {
        let (dom, tree) = styles_for("<html><head><title>t</title></head><body>x</body></html>");
        let head = first(&dom, "head");
        assert_eq!(
            tree.get(head).and_then(|s| s.get(&PropertyId::Display)),
            Some(&CssValue::Display(Display::None)),
            "the UA stylesheet must make <head> display:none"
        );
    }

    #[test]
    fn inline_style_beats_a_more_specific_selector() {
        let (dom, tree) = styles_for(
            "<html><head><style>#a.b.c.d { width: 100px }</style></head>\
             <body><div id=a class='b c d' style='width: 999px'>x</div></body></html>",
        );
        let div = first(&dom, "div");
        let width = tree.get(div).and_then(|s| s.get(&PropertyId::Width));
        let text = format!("{width:?}");
        assert!(
            text.contains("999"),
            "style attribute must win over any selector, got {text}"
        );
    }

    #[test]
    fn inheritance_flows_through_non_element_nodes() {
        let (dom, tree) = styles_for(
            "<html><head><style>body { color: rgb(1,2,3) }</style></head>\
             <body><div><span>x</span></div></body></html>",
        );
        let span = first(&dom, "span");
        let color = tree.get(span).and_then(|s| s.get(&PropertyId::Color));
        assert!(
            format!("{color:?}").contains('1'),
            "color must inherit from body through div, got {color:?}"
        );
    }

    #[test]
    fn author_rules_beat_the_ua_stylesheet() {
        let (dom, tree) = styles_for(
            "<html><head><style>div { display: inline }</style></head>\
             <body><div>x</div></body></html>",
        );
        let div = first(&dom, "div");
        assert_eq!(
            tree.get(div).and_then(|s| s.get(&PropertyId::Display)),
            Some(&CssValue::Display(Display::Inline)),
            "author origin must outrank user-agent origin"
        );
    }
}
