use crate::css_cascade::ComputedStyle;
use crate::css_values::property::{CssValue, PropertyId};
use crate::css_values::types::display::Display;
use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;
use crate::layout::query::DOMRect;
use crate::layout::resolve::ResolveContext;
use crate::layout::style_map::computed_to_taffy;
use crate::layout::viewport::Viewport;
use std::collections::{HashMap, HashSet};
use taffy::prelude::*;

/// Step limit for the iterative DOM walk in `build_node`. A correct DOM has
/// at most `nodes.len()` unique ids; if the walker takes more steps than this
/// it is iterating a cycle and we panic with a clear message rather than
/// running until OS abort. 100K is several orders of magnitude beyond any
/// real document.
const LAYOUT_BUILD_LIMIT: usize = 100_000;

/// The layout engine. Converts a DOM + styles into positioned elements.
pub struct LayoutEngine {
    tree: TaffyTree,
    dom_to_taffy: HashMap<u32, taffy::NodeId>,
    viewport: Viewport,
    dirty: bool,
    root_taffy: Option<taffy::NodeId>,
    /// Computed styles for the tree being laid out.
    ///
    /// Layout used to resolve each element's style itself, from an *empty*
    /// cascaded map plus the `style` attribute — so stylesheets had no effect
    /// on geometry. Styles are now resolved once by `crate::style` and read
    /// from here, which is also what makes `font-size` inherit.
    styles: crate::style::StyleTree,
    /// Author CSS the caller fetched (external `<link>` sheets). `<style>`
    /// blocks are found in the DOM and do not need to be passed in.
    extra_css: Vec<String>,
}

impl LayoutEngine {
    pub fn new(viewport: Viewport) -> Self {
        Self {
            tree: TaffyTree::new(),
            dom_to_taffy: HashMap::new(),
            viewport,
            dirty: true,
            root_taffy: None,
            styles: crate::style::StyleTree::default(),
            extra_css: Vec::new(),
        }
    }

    /// Supply author CSS that is not inline in the document — the contents of
    /// external `<link rel=stylesheet>` sheets the embedder fetched. Marks
    /// layout dirty.
    pub fn set_extra_css(&mut self, css: Vec<String>) {
        self.extra_css = css;
        self.dirty = true;
    }

    /// The computed styles from the last `compute`. Empty before the first one.
    pub fn styles(&self) -> &crate::style::StyleTree {
        &self.styles
    }

    /// Mark layout as dirty (needs recomputation).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Compute layout for the entire DOM tree.
    pub fn compute(&mut self, dom: &Dom) {
        // Clear previous tree
        self.tree = TaffyTree::new();
        self.dom_to_taffy.clear();

        // Resolve the cascade before building boxes. Everything below reads
        // styles out of `self.styles`; nothing re-resolves them.
        self.styles = crate::style::compute_styles(dom, &self.extra_css);

        let ctx = ResolveContext {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_w: self.viewport.width,
            viewport_h: self.viewport.height,
        };

        // Build taffy tree from DOM
        let root = self.build_node(dom, NodeId::DOCUMENT, &ctx);
        self.root_taffy = root;

        // Run layout
        if let Some(root_id) = self.root_taffy {
            let avail = taffy::Size {
                width: AvailableSpace::Definite(self.viewport.width),
                height: AvailableSpace::Definite(self.viewport.height),
            };
            self.tree.compute_layout(root_id, avail).ok();
        }

        self.dirty = false;
    }

    /// Ensure layout is computed (lazy).
    pub fn ensure_computed(&mut self, dom: &Dom) {
        if self.dirty {
            self.compute(dom);
        }
    }

    /// Get the bounding rect of a node.
    pub fn get_bounding_rect(&mut self, dom: &Dom, node_id: NodeId) -> DOMRect {
        self.ensure_computed(dom);

        // Accumulate absolute position by walking up the taffy tree
        let taffy_id = match self.dom_to_taffy.get(&node_id.to_raw()) {
            Some(id) => *id,
            None => return DOMRect::default(),
        };

        let layout = match self.tree.layout(taffy_id) {
            Ok(l) => *l,
            Err(_) => return DOMRect::default(),
        };

        // Get absolute position by summing ancestor positions
        let (abs_x, abs_y) = self.absolute_position(taffy_id);

        // DOMRect::new quantizes to 1/64 px via LayoutUnit (Blink-coherent).
        DOMRect::new(
            abs_x as f64,
            abs_y as f64,
            layout.size.width as f64,
            layout.size.height as f64,
        )
    }

    /// Get offsetWidth (width including padding + border).
    pub fn get_offset_width(&mut self, dom: &Dom, node_id: NodeId) -> f64 {
        self.ensure_computed(dom);
        self.taffy_size(node_id).0
    }

    /// Get offsetHeight.
    pub fn get_offset_height(&mut self, dom: &Dom, node_id: NodeId) -> f64 {
        self.ensure_computed(dom);
        self.taffy_size(node_id).1
    }

    /// Get offsetTop (position relative to offsetParent).
    pub fn get_offset_top(&mut self, dom: &Dom, node_id: NodeId) -> f64 {
        self.ensure_computed(dom);
        self.taffy_position(node_id).1
    }

    /// Get offsetLeft.
    pub fn get_offset_left(&mut self, dom: &Dom, node_id: NodeId) -> f64 {
        self.ensure_computed(dom);
        self.taffy_position(node_id).0
    }

    // --- Internal ---

    /// Build a taffy subtree rooted at `root`. Iterative post-order DFS:
    /// each node is "visited" first to enqueue its children, then "finished"
    /// after all descendants are processed so children's taffy IDs are
    /// available via `self.dom_to_taffy` when we call `tree.new_with_children`.
    /// `visited` + step counter guard against arena cycles (impossible given
    /// the cycle assertions in `Dom::append_child`/`insert_before`, but
    /// provides a clear panic if state ever becomes corrupt).
    fn build_node(
        &mut self,
        dom: &Dom,
        root: NodeId,
        ctx: &ResolveContext,
    ) -> Option<taffy::NodeId> {
        enum Work {
            Visit(NodeId),
            Finish(NodeId),
        }
        let mut stack: Vec<Work> = vec![Work::Visit(root)];
        let mut visited: HashSet<NodeId> = HashSet::with_capacity(64);
        let mut steps: usize = 0;
        while let Some(work) = stack.pop() {
            match work {
                Work::Visit(node_id) => {
                    if !visited.insert(node_id) {
                        continue;
                    }
                    steps += 1;
                    if steps > LAYOUT_BUILD_LIMIT {
                        panic!(
                            "Layout build cycle from {:?} — visited {} unique nodes",
                            root,
                            visited.len()
                        );
                    }
                    // Schedule Finish first so it pops after all children.
                    stack.push(Work::Finish(node_id));
                    // Push children in reverse for document order on pop.
                    let kids = dom.children(node_id);
                    for c in kids.into_iter().rev() {
                        stack.push(Work::Visit(c));
                    }
                }
                Work::Finish(node_id) => {
                    self.finish_node(dom, node_id, ctx);
                }
            }
        }
        self.dom_to_taffy.get(&root.to_raw()).copied()
    }

    /// Build the taffy node for `node_id` using already-built children
    /// recorded in `self.dom_to_taffy` (set by prior Finish calls in
    /// post-order). Returns nothing — the result lives in `dom_to_taffy`.
    fn finish_node(&mut self, dom: &Dom, node_id: NodeId, ctx: &ResolveContext) {
        let node = match dom.get(node_id) {
            Some(n) => n,
            None => return,
        };

        // Collect already-built children's taffy IDs in document order.
        // Children that returned None (e.g. display:none, unsupported node
        // type) are absent from dom_to_taffy and naturally filtered out.
        let children: Vec<taffy::NodeId> = dom
            .children(node_id)
            .into_iter()
            .filter_map(|cid| self.dom_to_taffy.get(&cid.to_raw()).copied())
            .collect();

        let taffy_id = match &node.data {
            NodeData::Document | NodeData::DocumentFragment => {
                let style = taffy::Style {
                    display: taffy::Display::Block,
                    size: taffy::Size {
                        width: Dimension::length(ctx.viewport_w),
                        height: Dimension::auto(),
                    },
                    ..Default::default()
                };
                match self.tree.new_with_children(style, &children) {
                    Ok(id) => id,
                    Err(_) => return,
                }
            }
            NodeData::Element(_) => {
                let computed = self.styles.get_or_initial(node_id);
                if let Some(CssValue::Display(Display::None)) = computed.get(&PropertyId::Display) {
                    return;
                }
                // `font-size` is inherited, so `em`, `rem` and `ex` lengths on
                // this element must resolve against *its* size, not against a
                // fixed 16px for the whole document.
                let ctx = ResolveContext {
                    font_size: self.font_size_of(&computed, ctx),
                    ..*ctx
                };
                let taffy_style = computed_to_taffy(&computed, &ctx);
                match self.tree.new_with_children(taffy_style, &children) {
                    Ok(id) => id,
                    Err(_) => return,
                }
            }
            NodeData::Text(text) => {
                // CSS white-space processing, before measurement. A text node
                // holding only the newline and indentation between two tags
                // collapses to nothing and must not produce a box — without
                // this every such node became a full line box, and an ordinary
                // document gained one per element.
                let parent_style = dom
                    .get(node_id)
                    .and_then(|n| n.parent)
                    .map(|p| self.styles.get_or_initial(p))
                    .unwrap_or_else(|| ComputedStyle::resolve(&HashMap::new(), None));
                let collapsed = collapse_white_space(text, &parent_style);
                if collapsed.is_empty() {
                    return;
                }

                let font_size = self.font_size_of(&parent_style, ctx);
                // Still the 0.6em-per-glyph placeholder: this measures nothing
                // real and is what `browser_oxide_render`'s inline layout
                // replaces. Only the *inputs* are fixed here.
                let char_count = collapsed.chars().count() as f32;
                let width = char_count * font_size * 0.6;
                let height = font_size * 1.2;
                let style = taffy::Style {
                    size: taffy::Size {
                        width: Dimension::length(width),
                        height: Dimension::length(height),
                    },
                    ..Default::default()
                };
                match self.tree.new_leaf(style) {
                    Ok(id) => id,
                    Err(_) => return,
                }
            }
            _ => return,
        };
        self.dom_to_taffy.insert(node_id.to_raw(), taffy_id);
    }

    /// The used `font-size` for an element, in px.
    ///
    /// `font-size` is itself a length that can be relative, and it is resolved
    /// against the *parent's* size — but `ComputedStyle::resolve` has already
    /// applied inheritance, so an inherited value arrives here absolute. A
    /// relative value written directly on this element resolves against the
    /// context passed in, which is the parent's size.
    fn font_size_of(&self, computed: &ComputedStyle, ctx: &ResolveContext) -> f32 {
        use crate::css_values::types::length::LengthPercentage;
        match computed.get(&PropertyId::FontSize) {
            Some(CssValue::Length(l)) => crate::layout::resolve::resolve_length(l, ctx),
            Some(CssValue::LengthPercentage(LengthPercentage::Length(l))) => {
                crate::layout::resolve::resolve_length(l, ctx)
            }
            Some(CssValue::LengthPercentage(LengthPercentage::Percentage(p))) => {
                ctx.font_size * (*p as f32) / 100.0
            }
            _ => ctx.font_size,
        }
    }

    fn absolute_position(&self, taffy_id: taffy::NodeId) -> (f32, f32) {
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut current = taffy_id;
        loop {
            if let Ok(layout) = self.tree.layout(current) {
                x += layout.location.x;
                y += layout.location.y;
            }
            match self.tree.parent(current) {
                Some(parent) => current = parent,
                None => break,
            }
        }
        (x, y)
    }

    fn taffy_size(&self, node_id: NodeId) -> (f64, f64) {
        match self.dom_to_taffy.get(&node_id.to_raw()) {
            Some(taffy_id) => match self.tree.layout(*taffy_id) {
                Ok(layout) => (
                    crate::layout::layout_unit::LayoutUnit::from_taffy_f32(layout.size.width)
                        .to_f64_px(),
                    crate::layout::layout_unit::LayoutUnit::from_taffy_f32(layout.size.height)
                        .to_f64_px(),
                ),
                Err(_) => (0.0, 0.0),
            },
            None => (0.0, 0.0),
        }
    }

    fn taffy_position(&self, node_id: NodeId) -> (f64, f64) {
        match self.dom_to_taffy.get(&node_id.to_raw()) {
            Some(taffy_id) => match self.tree.layout(*taffy_id) {
                Ok(layout) => (
                    crate::layout::layout_unit::LayoutUnit::from_taffy_f32(layout.location.x)
                        .to_f64_px(),
                    crate::layout::layout_unit::LayoutUnit::from_taffy_f32(layout.location.y)
                        .to_f64_px(),
                ),
                Err(_) => (0.0, 0.0),
            },
            None => (0.0, 0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::node::{Attribute, QualName};

    fn make_dom_with_styled_div(style: &str) -> Dom {
        let mut dom = Dom::new();
        let html = dom.create_element(QualName::new("html"), vec![]);
        dom.append_child(NodeId::DOCUMENT, html);
        let body = dom.create_element(QualName::new("body"), vec![]);
        dom.append_child(html, body);
        let div = dom.create_element(
            QualName::new("div"),
            vec![Attribute {
                name: QualName::new("style"),
                value: style.to_string(),
            }],
        );
        dom.append_child(body, div);
        dom
    }

    #[test]
    fn layout_basic_div() {
        let dom = make_dom_with_styled_div("width: 200px; height: 100px");
        let viewport = Viewport::new(1920.0, 1080.0);
        let mut engine = LayoutEngine::new(viewport);
        engine.compute(&dom);

        // Find the div (it's the child of body, which is child of html, which is child of document)
        let html = dom.child_elements(NodeId::DOCUMENT)[0];
        let body = dom.child_elements(html)[0];
        let div = dom.child_elements(body)[0];

        let rect = engine.get_bounding_rect(&dom, div);
        // Width includes border (default 3px medium border on each side)
        // Content: 200px + border: 3+3 = 206px (content-box)
        assert!(
            rect.width >= 200.0,
            "width should be >= 200, got {}",
            rect.width
        );
        assert!(
            rect.height >= 100.0,
            "height should be >= 100, got {}",
            rect.height
        );
    }

    #[test]
    fn layout_text_node_has_size() {
        let mut dom = Dom::new();
        let html = dom.create_element(QualName::new("html"), vec![]);
        dom.append_child(NodeId::DOCUMENT, html);
        let body = dom.create_element(QualName::new("body"), vec![]);
        dom.append_child(html, body);
        let text = dom.create_text("Hello world".to_string());
        dom.append_child(body, text);

        let viewport = Viewport::new(1920.0, 1080.0);
        let mut engine = LayoutEngine::new(viewport);
        engine.compute(&dom);

        let (w, h) = engine.taffy_size(text);
        assert!(w > 0.0, "text width should be > 0, got {}", w);
        assert!(h > 0.0, "text height should be > 0, got {}", h);
    }

    #[test]
    fn layout_offset_width() {
        let dom = make_dom_with_styled_div("width: 300px; height: 150px");
        let viewport = Viewport::new(1920.0, 1080.0);
        let mut engine = LayoutEngine::new(viewport);

        let html = dom.child_elements(NodeId::DOCUMENT)[0];
        let body = dom.child_elements(html)[0];
        let div = dom.child_elements(body)[0];

        let w = engine.get_offset_width(&dom, div);
        assert!(w >= 300.0, "offsetWidth should be >= 300, got {}", w);
        let h = engine.get_offset_height(&dom, div);
        assert!(h >= 150.0, "offsetHeight should be >= 150, got {}", h);
    }

    #[test]
    fn dirty_tracking() {
        let dom = make_dom_with_styled_div("width: 100px");
        let viewport = Viewport::new(1920.0, 1080.0);
        let mut engine = LayoutEngine::new(viewport);

        assert!(engine.dirty);
        engine.compute(&dom);
        assert!(!engine.dirty);
        engine.mark_dirty();
        assert!(engine.dirty);
    }

    #[test]
    fn dom_rect_from_layout() {
        let layout = taffy::Layout::new();
        let rect = DOMRect::from_taffy_layout(&layout);
        assert_eq!(rect.width, 0.0);
    }
}

/// CSS white-space processing, applied before a text node is measured.
///
/// Only the collapsing half — the part that decides whether a text node has a
/// box at all and how wide it is. Where lines *break* is inline layout's job
/// and lives in the render crate.
///
/// The case that matters most is the least interesting one: the text node
/// between `</div>` and `<div>` holds a newline and some indentation, collapses
/// to a single space, and — because it is the whole node — to nothing at all.
/// Measuring it unprocessed gave every one of them a full line box.
pub(crate) fn collapse_white_space(text: &str, style: &ComputedStyle) -> String {
    use crate::css_values::types::display::WhiteSpace;

    let ws = match style.get(&PropertyId::WhiteSpace) {
        Some(CssValue::WhiteSpace(w)) => *w,
        _ => WhiteSpace::Normal,
    };
    let collapses = matches!(
        ws,
        WhiteSpace::Normal | WhiteSpace::Nowrap | WhiteSpace::PreLine
    );
    if !collapses {
        return text.to_string();
    }
    let keeps_newlines = matches!(ws, WhiteSpace::PreLine);

    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    for c in text.chars() {
        match c {
            '\n' if keeps_newlines => {
                pending = false;
                out.push('\n');
            }
            ' ' | '\t' | '\n' | '\r' => pending = true,
            _ => {
                if pending && !out.is_empty() {
                    out.push(' ');
                }
                pending = false;
                out.push(c);
            }
        }
    }
    // A trailing collapsible space at the end of the node is dropped. This is
    // not quite the spec — a space between two inline boxes survives as one
    // space in a real inline formatting context — but this layout has no inline
    // formatting context to survive into, and dropping it is much closer than
    // keeping a whole line box.
    out
}

/// Regressions for the five layout defects PoC-2 surfaced by rendering a page.
///
/// Every one of these was invisible while the engine was headless, and every
/// one of them moves `getBoundingClientRect()` — which is a fingerprint surface
/// as well as a rendering one, so they are geometry assertions rather than
/// style assertions on purpose.
#[cfg(test)]
mod render_regressions {
    use super::*;
    use crate::layout::viewport::Viewport;

    fn laid_out(html: &str) -> (Dom, LayoutEngine) {
        let dom = crate::html_parser::parse_html(html);
        let mut engine = LayoutEngine::new(Viewport::new(800.0, 600.0));
        engine.compute(&dom);
        (dom, engine)
    }

    fn rect_of(dom: &Dom, engine: &mut LayoutEngine, tag: &str) -> crate::layout::query::DOMRect {
        let id = *dom
            .get_elements_by_tag_name(NodeId::DOCUMENT, tag)
            .first()
            .unwrap_or_else(|| panic!("no <{tag}> element"));
        engine.get_bounding_rect(dom, id)
    }

    #[test]
    fn a_stylesheet_changes_geometry() {
        // The headline defect: layout resolved styles from an empty cascaded
        // map, so a <style> block could not move anything.
        let (dom, mut engine) = laid_out(
            "<html><head><style>div { width: 250px; height: 40px }</style></head>\
             <body><div></div></body></html>",
        );
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!((rect.width, rect.height), (250.0, 40.0));
    }

    #[test]
    fn border_width_without_border_style_is_not_used() {
        // `border-width: medium` is the initial value and the initial
        // `border-style` is `none`, so an element with neither set must have no
        // border. The engine applied 3px to every element on every page.
        let (dom, mut engine) =
            laid_out("<html><body><div style='width: 100px; height: 100px'></div></body></html>");
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!(
            (rect.width, rect.height),
            (100.0, 100.0),
            "a div with no border-style must not gain one"
        );
    }

    #[test]
    fn border_width_with_border_style_is_used() {
        // The other half: the gate must not swallow real borders. Default
        // `box-sizing: content-box`, so a 10px border grows the box by 20px.
        let (dom, mut engine) = laid_out(
            "<html><body><div style='width: 100px; height: 100px; \
             border-style: solid; border-top-width: 10px; border-right-width: 10px; \
             border-bottom-width: 10px; border-left-width: 10px'></div></body></html>",
        );
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!((rect.width, rect.height), (120.0, 120.0));
    }

    #[test]
    fn border_shorthand_expands() {
        let (dom, mut engine) = laid_out(
            "<html><body><div style='width: 100px; height: 100px; \
             border: 5px solid red'></div></body></html>",
        );
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!((rect.width, rect.height), (110.0, 110.0));
    }

    #[test]
    fn border_shorthand_without_a_style_draws_nothing() {
        // `border: 5px` sets a width but leaves style at its initial `none`,
        // so the used width is zero. This is the case that reads wrong until
        // you check it against a browser.
        let (dom, mut engine) = laid_out(
            "<html><body><div style='width: 100px; height: 100px; \
             border: 5px'></div></body></html>",
        );
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!((rect.width, rect.height), (100.0, 100.0));
    }

    #[test]
    fn head_is_not_laid_out() {
        // Without a UA stylesheet, <head> and <title> were visible blocks and
        // pushed <body> down the page.
        let (dom, mut engine) =
            laid_out("<html><head><title>a title</title></head><body></body></html>");
        let head = rect_of(&dom, &mut engine, "head");
        assert_eq!(
            (head.width, head.height),
            (0.0, 0.0),
            "head must not occupy space"
        );
    }

    #[test]
    fn whitespace_between_elements_has_no_box() {
        // Each newline-plus-indent text node used to become a full line box, so
        // an ordinary document gained one per element. Two divs separated by a
        // newline must stack with nothing between them.
        let (dom, mut engine) = laid_out(
            "<html><body>\n  <div style='height: 10px'></div>\n  \
             <div style='height: 10px'></div>\n</body></html>",
        );
        let ids = dom.get_elements_by_tag_name(NodeId::DOCUMENT, "div");
        assert_eq!(ids.len(), 2);
        let first = engine.get_bounding_rect(&dom, ids[0]);
        let second = engine.get_bounding_rect(&dom, ids[1]);
        assert_eq!(
            second.y - first.y,
            10.0,
            "the second div must sit directly below the first, not a line box lower"
        );
    }

    #[test]
    fn body_has_the_default_margin() {
        // The UA stylesheet's `body { margin: 8px }`. Without it every element
        // on an unstyled page sits 8px off from where a browser puts it.
        let (dom, mut engine) = laid_out("<html><body><div></div></body></html>");
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!((rect.x, rect.y), (8.0, 8.0));
    }

    #[test]
    fn font_size_inherits_and_em_resolves_against_it() {
        // `font-size` used to be a fixed 16px for the whole document, so an
        // `em` length anywhere resolved against the wrong number.
        let (dom, mut engine) = laid_out(
            "<html><head><style>body { font-size: 20px }</style></head>\
             <body><div style='width: 2em; height: 1em'></div></body></html>",
        );
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!(
            (rect.width, rect.height),
            (40.0, 20.0),
            "2em must be 40px under an inherited 20px font-size, not 32px"
        );
    }

    #[test]
    fn display_none_from_a_stylesheet_removes_the_box() {
        let (dom, mut engine) = laid_out(
            "<html><head><style>.gone { display: none }</style></head>\
             <body><div class='gone' style='height: 50px'></div></body></html>",
        );
        let rect = rect_of(&dom, &mut engine, "div");
        assert_eq!((rect.width, rect.height), (0.0, 0.0));
    }
}
