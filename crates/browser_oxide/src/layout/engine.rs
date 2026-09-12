use crate::css_cascade::{cascade_sort, CascadeEntry, ComputedStyle, Origin};
use crate::css_selectors::{SelectorList, Specificity};
use crate::css_values::property::{CssValue, PropertyDeclaration, PropertyId};
use crate::css_values::types::display::{Display, Position};
use crate::css_values::types::length::{LengthPercentage, LengthPercentageAuto};
use crate::dom::node::{NodeData, NodeId};
use crate::dom::{Dom, DomElement};
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

#[derive(Clone)]
struct AuthorRule {
    selectors: SelectorList,
    declarations: Vec<PropertyDeclaration>,
    source_order: u32,
}

/// The layout engine. Converts a DOM + styles into positioned elements.
pub struct LayoutEngine {
    tree: TaffyTree,
    dom_to_taffy: HashMap<u32, taffy::NodeId>,
    viewport: Viewport,
    author_rules: Vec<AuthorRule>,
    dirty: bool,
    root_taffy: Option<taffy::NodeId>,
}

impl LayoutEngine {
    pub fn new(viewport: Viewport) -> Self {
        Self {
            tree: TaffyTree::new(),
            dom_to_taffy: HashMap::new(),
            viewport,
            author_rules: Vec::new(),
            dirty: true,
            root_taffy: None,
        }
    }

    /// Update the viewport used as the initial containing block.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        if self.viewport.width != viewport.width || self.viewport.height != viewport.height {
            self.viewport = viewport;
            self.dirty = true;
        }
    }

    /// Compile author stylesheets into typed rules consumed by layout.
    pub fn set_author_stylesheets(&mut self, stylesheets: &[String]) {
        self.author_rules.clear();
        let mut source_order = 0u32;

        for css_text in stylesheets {
            let (stylesheet, _errors) = crate::css_parser::parse_stylesheet(css_text);
            for rule in &stylesheet.rules {
                let crate::css_parser::ast::Rule::Qualified(rule) = rule else {
                    continue;
                };
                let selector_text = crate::js_runtime::utils::tokens_to_string(&rule.prelude);
                let Ok(selectors) = crate::css_selectors::parse_selector_list(&selector_text)
                else {
                    source_order = source_order.saturating_add(1);
                    continue;
                };

                let mut declarations = Vec::new();
                for declaration in &rule.declarations {
                    if let Ok(mut parsed) = crate::css_values::parse_property(
                        declaration.name,
                        &declaration.value,
                        declaration.important,
                    ) {
                        declarations.append(&mut parsed);
                    }
                }
                if !declarations.is_empty() {
                    self.author_rules.push(AuthorRule {
                        selectors,
                        declarations,
                        source_order,
                    });
                }
                source_order = source_order.saturating_add(1);
            }
        }
        self.dirty = true;
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

        let ctx = ResolveContext {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_w: self.viewport.width,
            viewport_h: self.viewport.height,
        };

        // Resolve the author cascade top-down first so inherited values are
        // available before the post-order Taffy tree construction.
        let computed_styles = self.compute_styles(dom);

        // Build taffy tree from DOM
        let root = self.build_node(dom, NodeId::DOCUMENT, &ctx, &computed_styles);
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

    /// Hit-test element layout boxes at a viewport coordinate.
    ///
    /// This follows the paint-order subset our Taffy-backed layout engine can
    /// represent: descendants paint above ancestors and later DOM siblings
    /// paint above earlier siblings. Elements excluded from layout (for
    /// example `display:none`) cannot be hit. Full Blink stacking-context,
    /// transform and clipping semantics remain a separate layout concern.
    pub fn elements_from_point(&mut self, dom: &Dom, x: f64, y: f64) -> Vec<NodeId> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= self.viewport.width as f64
            || y >= self.viewport.height as f64
        {
            return Vec::new();
        }

        self.ensure_computed(dom);

        let mut document_order = Vec::new();
        let mut stack = vec![NodeId::DOCUMENT];
        while let Some(node_id) = stack.pop() {
            if dom.get(node_id).is_some_and(|node| node.is_element())
                && self.dom_to_taffy.contains_key(&node_id.to_raw())
            {
                document_order.push(node_id);
            }

            let children = dom.children(node_id);
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        document_order
            .into_iter()
            .rev()
            .filter(|node_id| {
                let rect = self.get_bounding_rect(dom, *node_id);
                rect.width > 0.0
                    && rect.height > 0.0
                    && x >= rect.left
                    && x < rect.right
                    && y >= rect.top
                    && y < rect.bottom
            })
            .collect()
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
        computed_styles: &HashMap<NodeId, ComputedStyle>,
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
                    self.finish_node(dom, node_id, ctx, computed_styles);
                }
            }
        }
        self.dom_to_taffy.get(&root.to_raw()).copied()
    }

    fn compute_styles(&self, dom: &Dom) -> HashMap<NodeId, ComputedStyle> {
        let mut styles = HashMap::new();
        let mut stack = vec![NodeId::DOCUMENT];

        while let Some(node_id) = stack.pop() {
            if dom.get(node_id).is_some_and(|node| node.is_element()) {
                let parent_style = dom
                    .get(node_id)
                    .and_then(|node| node.parent)
                    .and_then(|parent| styles.get(&parent));
                let cascaded = self.cascade_for_element(dom, node_id);
                styles.insert(node_id, ComputedStyle::resolve(&cascaded, parent_style));
            }

            for child in dom.children(node_id).into_iter().rev() {
                stack.push(child);
            }
        }
        styles
    }

    fn cascade_for_element(&self, dom: &Dom, node_id: NodeId) -> HashMap<PropertyId, CssValue> {
        let Some(element) = DomElement::new(dom, node_id) else {
            return HashMap::new();
        };
        let mut entries = Vec::new();

        for rule in &self.author_rules {
            let specificity = rule
                .selectors
                .iter()
                .filter(|selector| crate::css_selectors::matches_selector(&element, selector))
                .map(crate::css_selectors::compute_specificity)
                .max();
            let Some(specificity) = specificity else {
                continue;
            };
            for declaration in &rule.declarations {
                entries.push(CascadeEntry {
                    declaration: declaration.clone(),
                    origin: Origin::Author,
                    layer: None,
                    specificity,
                    source_order: rule.source_order,
                });
            }
        }

        if let Some(style_text) = dom
            .get(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| {
                element
                    .attrs
                    .iter()
                    .find(|attr| attr.name.local.eq_ignore_ascii_case("style"))
                    .map(|attr| attr.value.as_str())
            })
        {
            let (declarations, _errors) = crate::css_parser::parse_declaration_list(style_text);
            for declaration in declarations {
                if let Ok(parsed) = crate::css_values::parse_property(
                    declaration.name,
                    &declaration.value,
                    declaration.important,
                ) {
                    for declaration in parsed {
                        entries.push(CascadeEntry {
                            declaration,
                            origin: Origin::Author,
                            layer: None,
                            specificity: Specificity::new(u32::MAX, 0, 0),
                            source_order: u32::MAX,
                        });
                    }
                }
            }
        }

        cascade_sort(&mut entries)
    }

    fn has_positioned_ancestor(
        &self,
        dom: &Dom,
        node_id: NodeId,
        computed_styles: &HashMap<NodeId, ComputedStyle>,
    ) -> bool {
        let mut current = dom.get(node_id).and_then(|node| node.parent);
        while let Some(ancestor) = current {
            if computed_styles.get(&ancestor).is_some_and(|style| {
                matches!(
                    style.get(&PropertyId::Position),
                    Some(CssValue::Position(position)) if *position != Position::Static
                )
            }) {
                return true;
            }
            current = dom.get(ancestor).and_then(|node| node.parent);
        }
        false
    }

    fn initial_containing_block_dimension(
        style: &ComputedStyle,
        property: &PropertyId,
        ctx: &ResolveContext,
        reference: f32,
    ) -> Option<f32> {
        match style.get(property) {
            Some(CssValue::LengthPercentageAuto(LengthPercentageAuto::Percentage(value))) => {
                Some(*value as f32 / 100.0 * reference)
            }
            Some(CssValue::LengthPercentageAuto(LengthPercentageAuto::Calc(expr))) => Some(
                crate::layout::resolve::resolve_calc_expression(expr, ctx, reference),
            ),
            Some(CssValue::LengthPercentage(LengthPercentage::Percentage(value))) => {
                Some(*value as f32 / 100.0 * reference)
            }
            Some(CssValue::LengthPercentage(LengthPercentage::Calc(expr))) => Some(
                crate::layout::resolve::resolve_calc_expression(expr, ctx, reference),
            ),
            _ => None,
        }
    }

    /// Build the taffy node for `node_id` using already-built children
    /// recorded in `self.dom_to_taffy` (set by prior Finish calls in
    /// post-order). Returns nothing — the result lives in `dom_to_taffy`.
    fn finish_node(
        &mut self,
        dom: &Dom,
        node_id: NodeId,
        ctx: &ResolveContext,
        computed_styles: &HashMap<NodeId, ComputedStyle>,
    ) {
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
                let computed = computed_styles
                    .get(&node_id)
                    .cloned()
                    .unwrap_or_else(|| ComputedStyle::resolve(&HashMap::new(), None));
                if let Some(CssValue::Display(Display::None)) = computed.get(&PropertyId::Display) {
                    return;
                }
                let mut taffy_style = computed_to_taffy(&computed, ctx);

                // CSS2 absolute positioning: if an absolutely positioned box
                // has no positioned ancestor, percentage width/height resolve
                // against the initial containing block (the viewport). Taffy
                // otherwise sees the immediate DOM parent, whose auto block
                // height is indefinite, and resolves `height:100%` to zero.
                // Fixed-position boxes use the viewport regardless of DOM
                // ancestry (transform-established fixed containing blocks are
                // outside the subset this engine models today).
                let position = match computed.get(&PropertyId::Position) {
                    Some(CssValue::Position(position)) => *position,
                    _ => Position::Static,
                };
                let uses_initial_containing_block = position == Position::Fixed
                    || (position == Position::Absolute
                        && !self.has_positioned_ancestor(dom, node_id, computed_styles));
                if uses_initial_containing_block {
                    if let Some(width) = Self::initial_containing_block_dimension(
                        &computed,
                        &PropertyId::Width,
                        ctx,
                        ctx.viewport_w,
                    ) {
                        taffy_style.size.width = Dimension::length(width);
                    }
                    if let Some(height) = Self::initial_containing_block_dimension(
                        &computed,
                        &PropertyId::Height,
                        ctx,
                        ctx.viewport_h,
                    ) {
                        taffy_style.size.height = Dimension::length(height);
                    }
                }
                match self.tree.new_with_children(taffy_style, &children) {
                    Ok(id) => id,
                    Err(_) => return,
                }
            }
            NodeData::Text(text) => {
                let char_count = text.chars().count() as f32;
                let width = char_count * ctx.font_size * 0.6;
                let height = ctx.font_size * 1.2;
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
    fn layout_pure_calc_width_is_not_treated_as_auto() {
        let dom = make_dom_with_styled_div("width: calc(100px + 50px); height: 20px");
        let viewport = Viewport::new(800.0, 600.0);
        let mut engine = LayoutEngine::new(viewport);
        engine.compute(&dom);

        let html = dom.child_elements(NodeId::DOCUMENT)[0];
        let body = dom.child_elements(html)[0];
        let div = dom.child_elements(body)[0];
        let rect = engine.get_bounding_rect(&dom, div);

        assert!(
            (rect.width - 150.0).abs() < 0.1,
            "pure calc width should resolve to 150px, got {}",
            rect.width
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

    #[test]
    fn elements_from_point_prefers_descendant_and_skips_display_none() {
        let mut dom = Dom::new();
        let html = dom.create_element(QualName::new("html"), vec![]);
        dom.append_child(NodeId::DOCUMENT, html);
        let body = dom.create_element(QualName::new("body"), vec![]);
        dom.append_child(html, body);

        let parent = dom.create_element(
            QualName::new("div"),
            vec![Attribute {
                name: QualName::new("style"),
                value: "width: 120px; height: 120px".into(),
            }],
        );
        dom.append_child(body, parent);
        let child = dom.create_element(
            QualName::new("span"),
            vec![Attribute {
                name: QualName::new("style"),
                value: "width: 40px; height: 40px".into(),
            }],
        );
        dom.append_child(parent, child);
        let hidden = dom.create_element(
            QualName::new("div"),
            vec![Attribute {
                name: QualName::new("style"),
                value: "display: none; width: 500px; height: 500px".into(),
            }],
        );
        dom.append_child(body, hidden);

        let mut engine = LayoutEngine::new(Viewport::new(800.0, 600.0));
        let hits = engine.elements_from_point(&dom, 10.0, 10.0);
        assert_eq!(hits.first().copied(), Some(child));
        assert!(hits.contains(&parent));
        assert!(!hits.contains(&hidden));
        assert!(engine.elements_from_point(&dom, -1.0, 10.0).is_empty());
        assert!(engine.elements_from_point(&dom, 800.0, 10.0).is_empty());
    }
}
