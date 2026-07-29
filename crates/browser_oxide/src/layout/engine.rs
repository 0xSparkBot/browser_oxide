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
/// What a text leaf carries into taffy's measure function.
///
/// taffy solves boxes; text is the one thing it cannot size on its own, so it
/// hands the node back and asks. This is the seam through which real shaped,
/// wrapped text reaches a layout that is otherwise entirely block/flex/grid.
#[derive(Debug, Clone)]
/// The inline formatting context a measured leaf holds.
///
/// One leaf per *context*, not per text node. A paragraph containing a link and
/// a bold word is one context of five runs sharing line boxes; giving each
/// piece its own leaf is what made inline elements start new lines.
pub struct TextContext {
    pub runs: Vec<crate::layout::inline::InlineRun>,
}

pub struct LayoutEngine {
    tree: TaffyTree<TextContext>,
    dom_to_taffy: HashMap<u32, taffy::NodeId>,
    /// Every inline node folded into a context, mapped to the node that context
    /// is keyed by. Members other than the anchor have no box of their own, so
    /// the painter needs this to know not to look for one.
    ifc_anchor: HashMap<u32, u32>,
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
    /// Shaping cache and the inline breaker. Lives on the engine so it
    /// survives across relayouts — the cache is worth 17-75x there and close
    /// to nothing on a first pass.
    inline: crate::layout::inline::InlineLayout,
    /// Which platform's text-metric convention to follow, and the OS name the
    /// font database uses for family aliasing. Both come from the identity the
    /// engine is presenting, not from the build host — a profile claiming
    /// Chrome on Linux while reporting Windows text metrics is exactly the
    /// internal inconsistency the fingerprint design exists to avoid.
    metrics_profile: crate::layout::inline::MetricsProfile,
    os_name: String,
    /// Author CSS the caller fetched (external `<link>` sheets). `<style>`
    /// blocks are found in the DOM and do not need to be passed in.
    extra_css: Vec<String>,
    /// What `@media (prefers-color-scheme: …)` resolves to for this document.
    color_scheme: crate::css_cascade::ColorScheme,
}

impl LayoutEngine {
    pub fn new(viewport: Viewport) -> Self {
        Self {
            tree: TaffyTree::new(),
            dom_to_taffy: HashMap::new(),
            ifc_anchor: HashMap::new(),
            viewport,
            dirty: true,
            root_taffy: None,
            styles: crate::style::StyleTree::default(),
            inline: crate::layout::inline::InlineLayout::new(),
            metrics_profile: crate::layout::inline::MetricsProfile::default(),
            os_name: "linux".to_string(),
            extra_css: Vec::new(),
            color_scheme: crate::css_cascade::ColorScheme::Light,
        }
    }

    /// Supply author CSS that is not inline in the document — the contents of
    /// external `<link rel=stylesheet>` sheets the embedder fetched. Marks
    /// layout dirty.
    pub fn set_extra_css(&mut self, css: Vec<String>) {
        self.extra_css = css;
        self.dirty = true;
    }

    /// Resize the viewport, which is a restyle and not just a relayout.
    ///
    /// Width and height are media features, so a resize can change *which rules
    /// match* — `@media (min-width: 768px)` is not a fixed answer for the
    /// document's life. That is why this marks dirty rather than merely
    /// stretching the boxes: the next `compute` re-runs the cascade against the
    /// new size, and a `min-width` block that stopped matching stops applying.
    ///
    /// Embedders that also run script must keep `matchMedia` in step — see
    /// `Page::set_viewport`, which does both.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.dirty = true;
    }

    /// The viewport the next layout will be solved into, in CSS pixels.
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// The media features this document's `@media` blocks are evaluated
    /// against: the real viewport, plus the per-document knobs set above.
    ///
    /// Built fresh on every `compute` rather than cached, because a cached copy
    /// is precisely how the viewport would go stale again.
    fn media_features(&self) -> crate::css_cascade::MediaFeatures {
        crate::css_cascade::MediaFeatures {
            width: self.viewport.width as f64,
            height: self.viewport.height as f64,
            device_pixel_ratio: self.viewport.device_pixel_ratio as f64,
            prefers_color_scheme: self.color_scheme,
            ..Default::default()
        }
    }

    /// Follow `os_name`'s text-metric convention and font aliasing.
    ///
    /// Call this with the active stealth profile's OS so laid-out geometry and
    /// the claimed identity agree.
    pub fn set_os_name(&mut self, os_name: &str) {
        self.metrics_profile = crate::layout::inline::MetricsProfile::for_os(os_name);
        self.os_name = os_name.to_string();
        self.dirty = true;
    }

    /// Resolve `@media (prefers-color-scheme: …)` against `scheme`.
    ///
    /// Defaults to `Light`, and the engine never reads the host's theme to
    /// change that. That default is a privacy position, so it is worth stating
    /// why rather than leaving it to look like an oversight.
    ///
    /// `prefers-color-scheme` is a fingerprinting surface — one bit, readable
    /// by any page with a `@media` block and a `getComputedStyle` call, no
    /// permission involved. The argument for exposing it anyway is that the bit
    /// is cheap and refusing it is not free either: every mainstream browser
    /// supports the feature, so a client where *neither* `dark` nor `light`
    /// matched would stand out far more than one that reports a value. But that
    /// argument only justifies *answering*; it does not justify answering with
    /// the host's real setting. Reporting `light` unconditionally still puts
    /// the client in the largest single bucket and leaks nothing, which is why
    /// it is the default here and why nothing in the engine changes it.
    ///
    /// A shell may still opt in, and for an interactive browser it should: the
    /// alternative is rendering a dark chrome around a blinding white page,
    /// which is a real accessibility failure, and a privacy browser people stop
    /// using protects nobody. That is a trade a UI with a human in front of it
    /// gets to make. A headless or scraping embedder — which is most of this
    /// engine's use — makes no such trade and keeps the default, so the one bit
    /// is spent only where it buys something.
    ///
    /// **Callers that opt in must keep JS in step.** `matchMedia` answers from
    /// the stealth profile's `prefers_color_scheme` field, and a document whose
    /// CSS says dark while its `matchMedia` says light is an
    /// inconsistency-class signal worth considerably more than the bit it was
    /// trying to protect. Set both from `ColorScheme::as_keyword`.
    pub fn set_color_scheme(&mut self, scheme: crate::css_cascade::ColorScheme) {
        self.color_scheme = scheme;
        self.dirty = true;
    }

    /// What `@media (prefers-color-scheme: …)` currently resolves to.
    pub fn color_scheme(&self) -> crate::css_cascade::ColorScheme {
        self.color_scheme
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
        self.ifc_anchor.clear();

        // Resolve the cascade before building boxes. Everything below reads
        // styles out of `self.styles`; nothing re-resolves them.
        //
        // This runs on every `compute`, not once per document, which is what
        // makes a resize change the breakpoints rather than only the box sizes.
        self.styles = crate::style::compute_styles(dom, &self.extra_css, &self.media_features());

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
            // Text is measured by shaping it, not by assuming 0.6em a glyph.
            // taffy calls back per text leaf with the width available to it,
            // which is what makes wrapping possible at all.
            let inline = &mut self.inline;
            self.tree
                .compute_layout_with_measure(
                    root_id,
                    avail,
                    |known, available, _node, context, _style| {
                        measure_text_leaf(inline, known, available, context)
                    },
                )
                .ok();
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
                    // Push children in reverse for document order on pop —
                    // except inline-level ones, which get no boxes of their own.
                    // They are gathered into one inline formatting context at
                    // Finish, which is the whole point: a link inside a sentence
                    // has to share the sentence's line boxes, and a child with
                    // its own taffy node cannot.
                    let kids = dom.children(node_id);
                    for c in kids.into_iter().rev() {
                        if self.is_inline_level(dom, c) {
                            continue;
                        }
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

    /// Is this node inline-level — does it flow into its parent's lines rather
    /// than take a box of its own?
    ///
    /// Text always is. An element is when its computed `display` says so.
    /// `inline-block` is deliberately absent: it joins its parent's line as an
    /// atomic box, which needs an inline-level box containing a block
    /// formatting context, and that does not exist yet. Better it keeps
    /// behaving as it did than be silently flattened into text it is not.
    fn is_inline_level(&self, dom: &Dom, node_id: NodeId) -> bool {
        let Some(node) = dom.get(node_id) else {
            return false;
        };
        match &node.data {
            NodeData::Text(_) => true,
            NodeData::Element(_) => {
                let computed = self.styles.get_or_initial(node_id);
                matches!(
                    computed.get(&PropertyId::Display),
                    Some(CssValue::Display(Display::Inline))
                )
            }
            _ => false,
        }
    }

    /// Children in document order, runs of inline-level siblings merged into
    /// one measured leaf each.
    fn children_with_inline_grouping(
        &mut self,
        dom: &Dom,
        node_id: NodeId,
        ctx: &ResolveContext,
    ) -> Vec<taffy::NodeId> {
        let kids = dom.children(node_id);
        let mut out: Vec<taffy::NodeId> = Vec::with_capacity(kids.len());
        let mut group: Vec<NodeId> = Vec::new();

        for cid in kids {
            if self.is_inline_level(dom, cid) {
                group.push(cid);
                continue;
            }
            if let Some(leaf) = self.build_ifc(dom, &group, ctx) {
                out.push(leaf);
            }
            group.clear();
            if let Some(id) = self.dom_to_taffy.get(&cid.to_raw()).copied() {
                out.push(id);
            }
        }
        if let Some(leaf) = self.build_ifc(dom, &group, ctx) {
            out.push(leaf);
        }
        out
    }

    /// Turn a run of inline-level siblings into one measured leaf.
    ///
    /// Keyed in `dom_to_taffy` by the group's first node, so
    /// `get_bounding_rect` on that node returns the context's box and the
    /// painter has something to anchor on. Every other member goes into
    /// `ifc_anchor`, so the painter knows it was absorbed and must not draw it
    /// a second time.
    fn build_ifc(
        &mut self,
        dom: &Dom,
        group: &[NodeId],
        ctx: &ResolveContext,
    ) -> Option<taffy::NodeId> {
        if group.is_empty() {
            return None;
        }
        let runs = self.collect_runs(dom, group, ctx);
        if runs.is_empty() {
            return None;
        }
        let anchor = group[0];
        let leaf = self
            .tree
            .new_leaf_with_context(taffy::Style::default(), TextContext { runs })
            .ok()?;
        self.dom_to_taffy.insert(anchor.to_raw(), leaf);
        for member in group {
            self.ifc_anchor.insert(member.to_raw(), anchor.to_raw());
        }
        Some(leaf)
    }

    /// Flatten an inline subtree into styled runs, in document order.
    ///
    /// White-space collapsing happens *across* the context, not per text node.
    /// `a <b>b</b> c` has a space at the end of one run and the start of the
    /// next; collapsing each separately keeps both, and the sentence gains a
    /// double space at every tag boundary.
    fn collect_runs(
        &mut self,
        dom: &Dom,
        group: &[NodeId],
        ctx: &ResolveContext,
    ) -> Vec<crate::layout::inline::InlineRun> {
        let mut runs: Vec<crate::layout::inline::InlineRun> = Vec::new();
        for root in group {
            self.collect_runs_from(dom, *root, ctx, &mut runs);
        }
        // A collapsible space at the very end of the context is dropped: it is
        // the seam to nothing.
        while let Some(last) = runs.last_mut() {
            if last.text.ends_with(' ') {
                last.text.pop();
            }
            if last.text.is_empty() {
                runs.pop();
                continue;
            }
            break;
        }
        runs
    }

    fn collect_runs_from(
        &mut self,
        dom: &Dom,
        node_id: NodeId,
        ctx: &ResolveContext,
        out: &mut Vec<crate::layout::inline::InlineRun>,
    ) {
        let Some(node) = dom.get(node_id) else {
            return;
        };
        match &node.data {
            NodeData::Text(text) => {
                let parent_style = node
                    .parent
                    .map(|p| self.styles.get_or_initial(p))
                    .unwrap_or_else(|| ComputedStyle::resolve(&HashMap::new(), None));
                let mut collapsed = collapse_white_space_inline(text, &parent_style);
                if collapsed.is_empty() {
                    return;
                }
                // Two collapsible spaces meeting at a seam are one space, and a
                // space at the very start of the context is dropped entirely —
                // the same rule a browser applies, just applied across the
                // context rather than inside a single node.
                let previous_ends_with_space = out.last().is_some_and(|r| r.text.ends_with(' '));
                if (out.is_empty() || previous_ends_with_space) && collapsed.starts_with(' ') {
                    collapsed.remove(0);
                }
                if collapsed.is_empty() {
                    return;
                }
                let font_ctx = ResolveContext {
                    font_size: self.font_size_of(&parent_style, ctx),
                    ..*ctx
                };
                out.push(crate::layout::inline::InlineRun {
                    text: collapsed,
                    style: self.text_style_for(&parent_style, &font_ctx),
                    // The *parent* element, not the text node: the painter wants
                    // the colour and decoration, and those live on the element
                    // that styled this run.
                    source: node.parent.map(|p| p.to_raw()).unwrap_or(0),
                });
            }
            NodeData::Element(_) => {
                let computed = self.styles.get_or_initial(node_id);
                if let Some(CssValue::Display(Display::None)) = computed.get(&PropertyId::Display) {
                    return;
                }
                for child in dom.children(node_id) {
                    self.collect_runs_from(dom, child, ctx, out);
                }
            }
            _ => {}
        }
    }

    /// The node owning the inline formatting context `node_id` was folded into,
    /// if it was folded into one.
    ///
    /// The painter uses this twice: to find the one node a context should be
    /// drawn from, and to skip every other member so the text is not drawn once
    /// per run.
    pub fn ifc_anchor_of(&self, node_id: NodeId) -> Option<NodeId> {
        self.ifc_anchor
            .get(&node_id.to_raw())
            .map(|raw| NodeId::from_raw(*raw))
    }

    /// The runs of the context anchored at `node_id`.
    pub fn runs_of(&self, node_id: NodeId) -> Option<&[crate::layout::inline::InlineRun]> {
        let taffy_id = self.dom_to_taffy.get(&node_id.to_raw())?;
        self.tree
            .get_node_context(*taffy_id)
            .map(|c| c.runs.as_slice())
    }

    /// Build the taffy node for `node_id` using already-built children
    /// recorded in `self.dom_to_taffy` (set by prior Finish calls in
    /// post-order). Returns nothing — the result lives in `dom_to_taffy`.
    fn finish_node(&mut self, dom: &Dom, node_id: NodeId, ctx: &ResolveContext) {
        let node = match dom.get(node_id) {
            Some(n) => n,
            None => return,
        };

        // Children in document order, with each maximal run of inline-level
        // siblings turned into ONE anonymous inline formatting context.
        // Block-level children keep the node they built on their own pass;
        // children that returned None (display:none, unsupported node type) are
        // absent from dom_to_taffy and drop out naturally.
        let children = self.children_with_inline_grouping(dom, node_id, ctx);

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
            // Text never reaches here any more. Every text node is
            // inline-level, so `build_node` no longer schedules one, and its
            // content arrives through `collect_runs` as part of the inline
            // formatting context its siblings share. A text node with a box of
            // its own is precisely the bug the context exists to fix.
            NodeData::Text(_) => return,
            _ => return,
        };
        self.dom_to_taffy.insert(node_id.to_raw(), taffy_id);
    }

    /// Build the inline layout style for text inheriting `computed`.
    /// Public so the painter can build exactly the style layout measured with.
    pub fn text_style_for(
        &self,
        computed: &ComputedStyle,
        ctx: &ResolveContext,
    ) -> crate::layout::inline::TextStyle {
        use crate::css_values::types::display::WhiteSpace;
        use crate::css_values::types::font::{FontFamily, FontStyle, FontWeight};

        let mut families: Vec<String> = Vec::new();
        if let Some(CssValue::FontFamily(list)) = computed.get(&PropertyId::FontFamily) {
            for f in list {
                match f {
                    FontFamily::Named(name) => families.push(name.clone()),
                    FontFamily::Generic(g) => families.push(generic_family_name(*g).to_string()),
                }
            }
        }
        if families.is_empty() {
            families.push("sans-serif".to_string());
        }

        let size_px = self.font_size_of(computed, ctx);

        let weight = match computed.get(&PropertyId::FontWeight) {
            Some(CssValue::FontWeight(FontWeight::Numeric(w))) => (*w).clamp(1.0, 1000.0) as u16,
            Some(CssValue::FontWeight(FontWeight::Bold)) => 700,
            // `bolder`/`lighter` are relative to the parent's weight, which
            // ComputedStyle does not resolve yet. 400 is the honest answer
            // until it does.
            _ => 400,
        };
        let italic = matches!(
            computed.get(&PropertyId::FontStyle),
            Some(CssValue::FontStyle(
                FontStyle::Italic | FontStyle::Oblique(_)
            ))
        );

        // `line-height: normal` stays `None` so the font's own metrics decide.
        let line_height = match computed.get(&PropertyId::LineHeight) {
            Some(CssValue::LineHeight(crate::css_values::property::LineHeight::Length(l))) => {
                Some(crate::layout::resolve::resolve_length(l, ctx))
            }
            Some(CssValue::LineHeight(crate::css_values::property::LineHeight::Number(n))) => {
                Some(size_px * (*n as f32))
            }
            _ => None,
        };

        let ws = match computed.get(&PropertyId::WhiteSpace) {
            Some(CssValue::WhiteSpace(w)) => *w,
            _ => WhiteSpace::Normal,
        };

        crate::layout::inline::TextStyle {
            families,
            size_px,
            weight,
            italic,
            line_height,
            // `overflow-wrap` is not a property the engine has yet; CSS's
            // default is `normal`, which does not break inside a word.
            break_word: false,
            wraps: matches!(
                ws,
                WhiteSpace::Normal | WhiteSpace::PreWrap | WhiteSpace::PreLine
            ),
            metrics: self.metrics_profile,
            os_name: self.os_name.clone(),
        }
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
/// Public because [`crate::render::painter`] must collapse identically — if
/// the painter drew a different string from the one measured, the text would
/// not fit the box reserved for it.
/// White-space collapsing for a text node *inside* an inline formatting
/// context, where a space at the edge of the node may be a real space between
/// two inline boxes.
///
/// [`collapse_white_space`] drops a trailing collapsible space, and its own
/// comment said why that was wrong and why it was tolerable: "a space between
/// two inline boxes survives as one space in a real inline formatting context —
/// but this layout has no inline formatting context to survive into." It has
/// one now, and the first render through it printed `Read moreabout the
/// project` and `withbold runs`, because every space before a tag had been
/// eaten.
///
/// So this keeps the edge spaces and leaves the trimming to the caller, which
/// is the only place that knows whether a given edge is the start or end of the
/// whole context or just the seam between two runs.
pub fn collapse_white_space_inline(text: &str, style: &ComputedStyle) -> String {
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
                if pending {
                    out.push(' ');
                }
                pending = false;
                out.push(c);
            }
        }
    }
    // The difference from `collapse_white_space`: an edge space survives, both
    // at the start (handled by the loop above pushing before the first glyph)
    // and here at the end.
    if pending {
        out.push(' ');
    }
    out
}

pub fn collapse_white_space(text: &str, style: &ComputedStyle) -> String {
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

/// taffy's measure callback for a text leaf.
///
/// `known` is any dimension already decided by the box's own style; `available`
/// is how much room there is. The distinction matters: taffy asks twice during
/// intrinsic sizing — once with `MinContent` and once with `MaxContent` — and
/// answering those with the wrapped width would make a text box size itself to
/// whatever it last wrapped to.
fn measure_text_leaf(
    inline: &mut crate::layout::inline::InlineLayout,
    known: taffy::Size<Option<f32>>,
    available: taffy::Size<AvailableSpace>,
    context: Option<&mut TextContext>,
) -> taffy::Size<f32> {
    let Some(context) = context else {
        return taffy::Size::ZERO;
    };
    if context.runs.is_empty() {
        return taffy::Size::ZERO;
    }

    let width_constraint = match (known.width, available.width) {
        (Some(w), _) => Some(w),
        (None, AvailableSpace::Definite(w)) => Some(w),
        // MinContent: break at every opportunity — the widest single word.
        // MaxContent: never break.
        (None, AvailableSpace::MinContent) => Some(0.0),
        (None, AvailableSpace::MaxContent) => None,
    };

    let Some(flow) = inline.layout_runs(&context.runs, width_constraint) else {
        return taffy::Size::ZERO;
    };
    taffy::Size {
        width: known.width.unwrap_or(flow.summary.width),
        height: known.height.unwrap_or(flow.summary.height),
    }
}

/// CSS generic family names, as the font database's alias table spells them.
fn generic_family_name(g: crate::css_values::types::font::GenericFamily) -> &'static str {
    use crate::css_values::types::font::GenericFamily as G;
    match g {
        G::Serif => "serif",
        G::SansSerif => "sans-serif",
        G::Monospace => "monospace",
        G::Cursive => "cursive",
        G::Fantasy => "fantasy",
        // The remaining generics have no alias configured; sans-serif is the
        // engine's global fallback and what the database resolves them to
        // anyway.
        _ => "sans-serif",
    }
}

/// Inline layout, through the full stack: HTML → cascade → taffy → geometry.
///
/// `layout::inline`'s own tests cover shaping and breaking in isolation. These
/// check the seam — that a text node reaches the shaper at all, with the right
/// style, and that taffy's answer comes back as a box of the right size.
#[cfg(test)]
mod inline_integration {
    use super::*;
    use crate::layout::viewport::Viewport;

    /// The defect a screenshot of the running Windows app found, as a test.
    ///
    /// A paragraph with a link in it is ONE inline formatting context. Before
    /// this, every text node and every inline element took a box of its own and
    /// the block parent stacked them, so the paragraph rendered as six lines
    /// with the comma and the full stop alone on lines of their own.
    ///
    /// The viewport is wide enough for the whole sentence, so a correct engine
    /// puts it on one line. Height is the check: one line box, not six.
    #[test]
    fn a_paragraph_with_a_link_is_one_line() {
        let html = "<html><body style='margin:0'>\
                    <p id='p' style='font-size:16px'>Read more <a href='/x'>about it</a>, \
                    or see the <b>benchmarks</b>.</p></body></html>";
        let (dom, mut engine) = laid_out(html, 1200.0);
        let box_ = rect(&dom, &mut engine, "p");

        // One 16px line is ~18-19px tall. Six would be over 100.
        assert!(
            box_.height < 40.0,
            "the paragraph is {}px tall — inline children are still taking boxes of their own",
            box_.height
        );
    }

    /// The bug the first fix exposed: `Read moreabout it`.
    ///
    /// A collapsible space at the end of a text node is a real space between
    /// two inline boxes, and dropping it welds the words together. It has to
    /// survive to the seam and be collapsed against the next run, not thrown
    /// away by whichever node happened to hold it.
    #[test]
    fn a_space_before_an_inline_tag_survives_into_the_context() {
        let html = "<html><body><p id='p'>Read more <a href='/x'>about it</a> and \
                    <b>more</b> still</p></body></html>";
        let dom = crate::html_parser::parse_html(html);
        let mut engine = LayoutEngine::new(Viewport::new(1200.0, 600.0));
        engine.compute(&dom);

        let p = *dom
            .get_elements_by_tag_name(dom.document(), "p")
            .first()
            .expect("the paragraph parsed");
        // The context is anchored on the paragraph's first inline child.
        let first = dom.children(p)[0];
        let runs = engine.runs_of(first).expect("the paragraph is one context");
        let joined: String = runs.iter().map(|r| r.text.as_str()).collect();

        assert_eq!(joined, "Read more about it and more still");
        assert!(
            !joined.contains("  "),
            "two collapsible spaces met at a seam and both survived: {joined:?}"
        );
    }

    /// Every run keeps the element that styled it, so the painter can colour a
    /// link differently from the text around it without a second tree walk.
    #[test]
    fn runs_carry_the_element_that_styled_them() {
        let html = "<html><body><p id='p'>plain <a href='/x'>linked</a></p></body></html>";
        let dom = crate::html_parser::parse_html(html);
        let mut engine = LayoutEngine::new(Viewport::new(1200.0, 600.0));
        engine.compute(&dom);

        let p = *dom
            .get_elements_by_tag_name(dom.document(), "p")
            .first()
            .expect("the paragraph parsed");
        let a = *dom
            .get_elements_by_tag_name(dom.document(), "a")
            .first()
            .expect("the anchor parsed");
        let first = dom.children(p)[0];
        let runs = engine.runs_of(first).expect("one context");

        let sources: Vec<u32> = runs.iter().map(|r| r.source).collect();
        assert!(
            sources.contains(&p.to_raw()),
            "the plain text is attributed to the paragraph"
        );
        assert!(
            sources.contains(&a.to_raw()),
            "and the link text to the anchor, not to the paragraph"
        );
    }

    fn laid_out(html: &str, width: f32) -> (Dom, LayoutEngine) {
        let dom = crate::html_parser::parse_html(html);
        let mut engine = LayoutEngine::new(Viewport::new(width, 600.0));
        engine.compute(&dom);
        (dom, engine)
    }

    fn rect(dom: &Dom, engine: &mut LayoutEngine, tag: &str) -> crate::layout::query::DOMRect {
        let id = *dom
            .get_elements_by_tag_name(NodeId::DOCUMENT, tag)
            .first()
            .unwrap_or_else(|| panic!("no <{tag}>"));
        engine.get_bounding_rect(dom, id)
    }

    const PROSE: &str = "Every real page is mostly inline text, which is why an engine \
                         that cannot wrap a paragraph cannot render anything at all.";

    #[test]
    fn a_paragraph_wraps_and_grows_taller() {
        // The defining behaviour of inline layout, and the one the placeholder
        // could not do at any width: text gets taller as its box gets narrower.
        let (wide_dom, mut wide) = laid_out(
            &format!("<html><body><p style='width:600px'>{PROSE}</p></body></html>"),
            800.0,
        );
        let (narrow_dom, mut narrow) = laid_out(
            &format!("<html><body><p style='width:200px'>{PROSE}</p></body></html>"),
            800.0,
        );
        let wide_rect = rect(&wide_dom, &mut wide, "p");
        let narrow_rect = rect(&narrow_dom, &mut narrow, "p");
        assert!(
            narrow_rect.height > wide_rect.height * 2.0,
            "a 200px paragraph must be much taller than a 600px one: {} vs {}",
            narrow_rect.height,
            wide_rect.height
        );
    }

    #[test]
    fn text_does_not_overflow_its_block() {
        // The placeholder made a paragraph one box thousands of pixels wide.
        let (dom, mut engine) = laid_out(
            &format!("<html><body><p style='width:300px'>{PROSE}</p></body></html>"),
            800.0,
        );
        let p = rect(&dom, &mut engine, "p");
        assert!(
            p.width <= 300.5,
            "the paragraph must respect its width, got {}",
            p.width
        );
        assert!(p.height > 0.0, "and must still have height");
    }

    #[test]
    fn text_is_measured_by_shaping_not_by_character_count() {
        // 0.6em per glyph made "iiii…" and "WWWW…" identical widths. They are
        // not, so at the same box width the wide-glyph string must wrap onto
        // more lines.
        //
        // Asserted through *height* rather than width on purpose: `display:
        // inline` still maps to a taffy block, so an inline box stretches to
        // its parent instead of shrinking to its content. Real inline flow is
        // ADR-010 and comes next; until it does, height is the honest handle
        // on how wide the shaper thinks the text is.
        let narrow_glyphs = "iiiiiiiiii iiiiiiiiii iiiiiiiiii iiiiiiiiii";
        let wide_glyphs = "WWWWWWWWWW WWWWWWWWWW WWWWWWWWWW WWWWWWWWWW";

        let (nd, mut ne) = laid_out(
            &format!("<html><body><p style='width:200px'>{narrow_glyphs}</p></body></html>"),
            800.0,
        );
        let (wd, mut we) = laid_out(
            &format!("<html><body><p style='width:200px'>{wide_glyphs}</p></body></html>"),
            800.0,
        );
        let narrow_h = rect(&nd, &mut ne, "p").height;
        let wide_h = rect(&wd, &mut we, "p").height;
        assert!(
            wide_h > narrow_h,
            "W is wider than i, so the same text must need more lines:              {wide_h} vs {narrow_h}"
        );
    }

    #[test]
    fn font_size_reaches_the_shaper() {
        // Same trick: bigger text needs more lines in the same box.
        let text = "measured by shaping the actual glyphs of the actual face";
        let (sd, mut se) = laid_out(
            &format!("<html><body><p style='width:200px; font-size:10px'>{text}</p></body></html>"),
            800.0,
        );
        let (ld, mut le) = laid_out(
            &format!("<html><body><p style='width:200px; font-size:30px'>{text}</p></body></html>"),
            800.0,
        );
        let small_h = rect(&sd, &mut se, "p").height;
        let large_h = rect(&ld, &mut le, "p").height;
        assert!(
            large_h > small_h * 2.0,
            "30px text must need far more height than 10px: {large_h} vs {small_h}"
        );
    }

    #[test]
    fn an_inline_box_still_stretches() {
        // Records a known gap rather than asserting correctness. `display:
        // inline` maps to a taffy block, so a <span> fills its parent instead
        // of hugging its text. Chrome would give this the width of the word.
        // When real inline flow lands (ADR-010) this test should start failing
        // and be replaced by its opposite.
        let (dom, mut engine) = laid_out("<html><body><span>short</span></body></html>", 400.0);
        let span = rect(&dom, &mut engine, "span");
        assert!(
            span.width > 300.0,
            "expected the current stretch-to-fill behaviour, got {}",
            span.width
        );
    }

    #[test]
    fn nowrap_keeps_one_line() {
        let (dom, mut engine) = laid_out(
            &format!(
                "<html><body><p style='width:100px; white-space:nowrap'>{PROSE}</p></body></html>"
            ),
            800.0,
        );
        let p = rect(&dom, &mut engine, "p");
        // The block is still 100px, but its single text line overflows it —
        // which is what `nowrap` means.
        assert!(
            p.height < 40.0,
            "nowrap must not wrap, got height {}",
            p.height
        );
    }

    #[test]
    fn an_empty_paragraph_has_no_text_height() {
        let (dom, mut engine) = laid_out("<html><body><p></p></body></html>", 800.0);
        let p = rect(&dom, &mut engine, "p");
        assert_eq!(p.height, 0.0);
    }
}

/// `prefers-color-scheme`, end to end through the engine rather than through
/// the media-query evaluator.
///
/// The evaluator always handled the feature; what did not exist was any way for
/// a caller to say which value it should be evaluated against, so a dark rule
/// was unreachable no matter how the page was written.
#[cfg(test)]
mod colour_scheme {
    use super::*;
    use crate::css_cascade::ColorScheme;
    use crate::layout::viewport::Viewport;

    const THEMED: &str = "<html><head><style>\
         body { background-color: white }\
         @media (prefers-color-scheme: dark) { body { background-color: black; width: 123px } }\
         </style></head><body>x</body></html>";

    fn body_of(dom: &Dom) -> NodeId {
        *dom.get_elements_by_tag_name(NodeId::DOCUMENT, "body")
            .first()
            .expect("parsed document has a <body>")
    }

    fn background(engine: &LayoutEngine, dom: &Dom) -> Option<CssValue> {
        engine
            .styles()
            .get(body_of(dom))
            .and_then(|s| s.get(&PropertyId::BackgroundColor))
            .cloned()
    }

    fn black() -> CssValue {
        CssValue::Color(crate::css_values::types::color::Color::Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 1.0,
        })
    }

    #[test]
    fn the_engine_defaults_to_light() {
        let dom = crate::html_parser::parse_html(THEMED);
        let mut engine = LayoutEngine::new(Viewport::new(800.0, 600.0));
        engine.compute(&dom);
        assert_eq!(engine.color_scheme(), ColorScheme::Light);
        assert_ne!(
            background(&engine, &dom),
            Some(black()),
            "a fresh engine must not follow the host's theme"
        );
    }

    #[test]
    fn a_dark_preference_reaches_the_computed_style() {
        let dom = crate::html_parser::parse_html(THEMED);
        let mut engine = LayoutEngine::new(Viewport::new(800.0, 600.0));
        engine.set_color_scheme(ColorScheme::Dark);
        engine.compute(&dom);
        assert_eq!(background(&engine, &dom), Some(black()));
    }

    #[test]
    fn changing_the_preference_after_a_layout_takes_effect() {
        // `set_color_scheme` has to mark the tree dirty. Without that the shell
        // would apply the user's theme, see the previous frame's styles, and
        // look like the feature simply did not work.
        let dom = crate::html_parser::parse_html(THEMED);
        let mut engine = LayoutEngine::new(Viewport::new(800.0, 600.0));
        engine.compute(&dom);
        let light_width = engine.get_bounding_rect(&dom, body_of(&dom)).width;

        engine.set_color_scheme(ColorScheme::Dark);
        let dark_width = engine.get_bounding_rect(&dom, body_of(&dom)).width;

        assert_eq!(background(&engine, &dom), Some(black()));
        assert!(
            (dark_width - light_width).abs() > 1.0,
            "the dark rule sets width:123px, so geometry must move: \
             {light_width} -> {dark_width}"
        );
    }
}
