use crate::css_values::calc::resolve_computed_value;
use crate::css_values::types::length::CalcContext;
use crate::dom::node::{NodeData, NodeId};
use crate::dom::DomElement;
use crate::js_runtime::native_fns::{
    install_native_fp_tostring, IframePropertyBridge, IframeRealmStore,
};
use crate::js_runtime::state::DomState;
use crate::js_runtime::utils::tokens_to_string;
use deno_core::op2;
use deno_core::v8;
use deno_core::JsRuntime;
use deno_core::OpState;
use std::collections::HashMap;

fn compile_realm_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
) -> Option<v8::Global<v8::Function>> {
    let source = v8::String::new(scope, source)?;
    let value = v8::Script::compile(scope, source, None)?.run(scope)?;
    let function = v8::Local::<v8::Function>::try_from(value).ok()?;
    Some(v8::Global::new(scope, function))
}

/// Build a `CalcContext` from the current DOM state's stealth profile.
/// Provides viewport + font-size + container dimensions so calc()
/// math functions can resolve relative units (vw, em, etc.) correctly
/// for `getComputedStyle` resolution.
fn calc_context_from(state: &DomState) -> CalcContext {
    let mut ctx = CalcContext::default();
    if let Some(p) = state.stealth_profile.as_ref() {
        ctx.viewport_w = p.inner_width as f64;
        ctx.viewport_h = p.inner_height as f64;
        ctx.container_w = p.inner_width as f64;
        ctx.container_h = p.inner_height as f64;
        // 16px is Chrome's default; profiles don't currently override.
        ctx.root_font_size_px = 16.0;
        ctx.font_size_px = 16.0;
    }
    ctx
}

// Convention: ops that return "nullable NodeId" return i64.
// -1 means null/not found. JS bootstrap converts -1 → null.

// --- Read ops ---

#[op2(fast)]
#[smi]
pub fn op_dom_document_node() -> i32 {
    NodeId::DOCUMENT.to_raw() as i32
}

#[op2]
#[string]
pub fn op_dom_get_tag_name(state: &mut OpState, #[smi] node_id: i32) -> String {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    state
        .dom
        .get(id)
        .and_then(|n| n.as_element())
        .map(|e| e.name.local.clone())
        .unwrap_or_default()
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_node_type(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    state.dom.node_type(NodeId::from_raw(node_id as u32)) as i32
}

#[op2]
#[string]
pub fn op_dom_get_text_content(state: &mut OpState, #[smi] node_id: i32) -> String {
    let state = state.borrow::<DomState>();
    state.dom.text_content(NodeId::from_raw(node_id as u32))
}

#[op2]
#[string]
pub fn op_dom_get_inner_html(state: &mut OpState, #[smi] node_id: i32) -> String {
    let state = state.borrow::<DomState>();
    state
        .dom
        .serialize_inner_html(NodeId::from_raw(node_id as u32))
}

#[op2]
#[string]
pub fn op_dom_get_outer_html(state: &mut OpState, #[smi] node_id: i32) -> String {
    let state = state.borrow::<DomState>();
    state.dom.serialize_html(NodeId::from_raw(node_id as u32))
}

#[op2]
#[string]
pub fn op_dom_get_attribute(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] name: &str,
) -> Option<String> {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    state
        .dom
        .get(id)
        .and_then(|n| n.as_element())
        .and_then(|e| {
            e.attrs
                .iter()
                .find(|a| a.name.local.eq_ignore_ascii_case(name))
                .map(|a| a.value.clone())
        })
}

#[op2(fast)]
pub fn op_dom_has_attribute(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] name: &str,
) -> bool {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    state
        .dom
        .get(id)
        .and_then(|n| n.as_element())
        .is_some_and(|e| {
            e.attrs
                .iter()
                .any(|a| a.name.local.eq_ignore_ascii_case(name))
        })
}

/// Returns the names of all attributes on `node_id`, in source order.
/// Used by Proxy ownKeys traps for `element.attributes` and `element.dataset`.
#[op2]
#[serde]
pub fn op_dom_get_attribute_names(state: &mut OpState, #[smi] node_id: i32) -> Vec<String> {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    state
        .dom
        .get(id)
        .and_then(|n| n.as_element())
        .map(|e| e.attrs.iter().map(|a| a.name.local.clone()).collect())
        .unwrap_or_default()
}

/// Returns parent NodeId or -1 if no parent.
#[op2(fast)]
#[smi]
pub fn op_dom_get_parent(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    state
        .dom
        .get(id)
        .and_then(|n| n.parent)
        .map(|p| p.to_raw() as i32)
        .unwrap_or(-1)
}

#[op2]
#[serde]
pub fn op_dom_get_children(state: &mut OpState, #[smi] node_id: i32) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    state
        .dom
        .children(NodeId::from_raw(node_id as u32))
        .iter()
        .map(|id| id.to_raw() as i32)
        .collect()
}

#[op2]
#[serde]
pub fn op_dom_get_children_with_types(state: &mut OpState, #[smi] node_id: i32) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    let children = state.dom.children(id);
    let mut res = Vec::with_capacity(children.len() * 2);
    for cid in children {
        res.push(cid.to_raw() as i32);
        res.push(state.dom.node_type(cid) as i32);
    }
    res
}

#[op2]
#[serde]
pub fn op_dom_get_child_elements(state: &mut OpState, #[smi] node_id: i32) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    state
        .dom
        .child_elements(NodeId::from_raw(node_id as u32))
        .iter()
        .map(|id| id.to_raw() as i32)
        .collect()
}

#[op2]
#[serde]
pub fn op_dom_get_child_elements_with_types(state: &mut OpState, #[smi] node_id: i32) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    let children = state.dom.child_elements(id);
    let mut res = Vec::with_capacity(children.len() * 2);
    for cid in children {
        res.push(cid.to_raw() as i32);
        res.push(state.dom.node_type(cid) as i32);
    }
    res
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_first_child(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.first_child)
        .map(|id| id.to_raw() as i32)
        .unwrap_or(-1)
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_last_child(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.last_child)
        .map(|id| id.to_raw() as i32)
        .unwrap_or(-1)
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_next_sibling(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.next_sibling)
        .map(|id| id.to_raw() as i32)
        .unwrap_or(-1)
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_prev_sibling(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.prev_sibling)
        .map(|id| id.to_raw() as i32)
        .unwrap_or(-1)
}

#[op2(fast)]
#[smi]
pub fn op_dom_query_selector(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] selector: &str,
) -> i32 {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    let element = match DomElement::new(&state.dom, id) {
        Some(el) => el,
        None => {
            // For Document node, search from first element child
            let children = state.dom.child_elements(id);
            if children.is_empty() {
                return -1;
            }
            match DomElement::new(&state.dom, children[0]) {
                Some(el) => {
                    // Search from root element
                    if let Ok(Some(found)) = crate::css_selectors::query_selector(&el, selector) {
                        return found.node_id().to_raw() as i32;
                    }
                    return -1;
                }
                None => return -1,
            }
        }
    };
    match crate::css_selectors::query_selector(&element, selector) {
        Ok(Some(found)) => found.node_id().to_raw() as i32,
        _ => -1,
    }
}

#[op2]
#[serde]
pub fn op_dom_query_selector_all(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] selector: String,
) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    // For document or element, try to build a DomElement for querying
    let root_el = DomElement::new(&state.dom, id).or_else(|| {
        let children = state.dom.child_elements(id);
        children
            .first()
            .and_then(|&c| DomElement::new(&state.dom, c))
    });
    match root_el {
        Some(el) => crate::css_selectors::query_selector_all(&el, &selector)
            .unwrap_or_default()
            .iter()
            .map(|e| e.node_id().to_raw() as i32)
            .collect(),
        None => vec![],
    }
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_element_by_id(state: &mut OpState, #[string] id: &str) -> i32 {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get_element_by_id(id)
        .map(|n| n.to_raw() as i32)
        .unwrap_or(-1)
}

#[op2]
#[serde]
pub fn op_dom_get_elements_by_tag_name(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] tag: String,
) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get_elements_by_tag_name(NodeId::from_raw(node_id as u32), &tag)
        .iter()
        .map(|id| id.to_raw() as i32)
        .collect()
}

#[op2]
#[serde]
pub fn op_dom_get_elements_by_class_name(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] class: String,
) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    state
        .dom
        .get_elements_by_class_name(NodeId::from_raw(node_id as u32), &class)
        .iter()
        .map(|id| id.to_raw() as i32)
        .collect()
}

#[op2]
#[serde]
pub fn op_dom_collect_insert_targets(state: &mut OpState, #[smi] node_id: i32) -> Vec<i32> {
    let state = state.borrow::<DomState>();
    state
        .dom
        .collect_insert_targets(NodeId::from_raw(node_id as u32))
        .iter()
        .map(|id| id.to_raw() as i32)
        .collect()
}

#[op2(fast)]
pub fn op_dom_matches(state: &mut OpState, #[smi] node_id: i32, #[string] selector: &str) -> bool {
    let state = state.borrow::<DomState>();
    let el = match DomElement::new(&state.dom, NodeId::from_raw(node_id as u32)) {
        Some(e) => e,
        None => return false,
    };
    match crate::css_selectors::parse_selector_list(selector) {
        Ok(list) => crate::css_selectors::matches_any(&el, &list),
        Err(_) => false,
    }
}

#[op2(fast)]
#[smi]
pub fn op_dom_closest(state: &mut OpState, #[smi] node_id: i32, #[string] selector: &str) -> i32 {
    let state = state.borrow::<DomState>();
    let list = match crate::css_selectors::parse_selector_list(selector) {
        Ok(l) => l,
        Err(_) => return -1,
    };
    let mut cur = Some(NodeId::from_raw(node_id as u32));
    while let Some(id) = cur {
        if let Some(el) = DomElement::new(&state.dom, id) {
            if crate::css_selectors::matches_any(&el, &list) {
                return id.to_raw() as i32;
            }
        }
        cur = state.dom.get(id).and_then(|n| n.parent);
    }
    -1
}

#[op2(fast)]
pub fn op_dom_contains(state: &mut OpState, #[smi] ancestor: i32, #[smi] descendant: i32) -> bool {
    if ancestor == descendant {
        return true;
    }
    let state = state.borrow::<DomState>();
    let anc = NodeId::from_raw(ancestor as u32);
    let mut cur = state
        .dom
        .get(NodeId::from_raw(descendant as u32))
        .and_then(|n| n.parent);
    while let Some(id) = cur {
        if id == anc {
            return true;
        }
        cur = state.dom.get(id).and_then(|n| n.parent);
    }
    false
}

#[op2(fast)]
pub fn op_dom_is_connected(state: &mut OpState, #[smi] node_id: i32) -> bool {
    let state = state.borrow::<DomState>();
    let mut cur = Some(NodeId::from_raw(node_id as u32));
    while let Some(id) = cur {
        if id == NodeId::DOCUMENT {
            return true;
        }
        cur = state.dom.get(id).and_then(|node| match &node.data {
            // A shadow root is not a normal child of its host, but nodes in a
            // shadow tree are connected whenever the host is connected.
            NodeData::ShadowRoot { host, .. } => Some(*host),
            _ => node.parent,
        });
    }
    false
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_first_element_child(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    let mut cur = state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.first_child);
    while let Some(id) = cur {
        let node = match state.dom.get(id) {
            Some(n) => n,
            None => return -1,
        };
        if node.is_element() {
            return id.to_raw() as i32;
        }
        cur = node.next_sibling;
    }
    -1
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_last_element_child(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    let mut cur = state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.last_child);
    while let Some(id) = cur {
        let node = match state.dom.get(id) {
            Some(n) => n,
            None => return -1,
        };
        if node.is_element() {
            return id.to_raw() as i32;
        }
        cur = node.prev_sibling;
    }
    -1
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_next_element_sibling(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    let mut cur = state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.next_sibling);
    while let Some(id) = cur {
        let node = match state.dom.get(id) {
            Some(n) => n,
            None => return -1,
        };
        if node.is_element() {
            return id.to_raw() as i32;
        }
        cur = node.next_sibling;
    }
    -1
}

#[op2(fast)]
#[smi]
pub fn op_dom_get_prev_element_sibling(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    let mut cur = state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.prev_sibling);
    while let Some(id) = cur {
        let node = match state.dom.get(id) {
            Some(n) => n,
            None => return -1,
        };
        if node.is_element() {
            return id.to_raw() as i32;
        }
        cur = node.prev_sibling;
    }
    -1
}

#[op2(fast)]
pub fn op_dom_get_child_element_count(state: &mut OpState, #[smi] node_id: i32) -> u32 {
    let state = state.borrow::<DomState>();
    let mut count = 0u32;
    let mut cur = state
        .dom
        .get(NodeId::from_raw(node_id as u32))
        .and_then(|n| n.first_child);
    while let Some(id) = cur {
        let node = match state.dom.get(id) {
            Some(n) => n,
            None => break,
        };
        if node.is_element() {
            count += 1;
        }
        cur = node.next_sibling;
    }
    count
}

// --- Mutation ops ---

#[op2(fast)]
#[smi]
pub fn op_dom_create_element(state: &mut OpState, #[string] tag: &str) -> i32 {
    let state = state.borrow_mut::<DomState>();
    state
        .dom
        .create_element(crate::dom::node::QualName::new(tag), vec![])
        .to_raw() as i32
}

#[op2(fast)]
#[smi]
pub fn op_dom_create_text_node(state: &mut OpState, #[string] text: &str) -> i32 {
    let state = state.borrow_mut::<DomState>();
    state.dom.create_text(text.to_string()).to_raw() as i32
}

#[op2(fast)]
#[smi]
pub fn op_dom_create_document_fragment(state: &mut OpState) -> i32 {
    let state = state.borrow_mut::<DomState>();
    state.dom.create_document_fragment().to_raw() as i32
}

#[op2(fast)]
pub fn op_dom_append_child(state: &mut OpState, #[smi] parent: i32, #[smi] child: i32) {
    let state = state.borrow_mut::<DomState>();
    state.dom.append_child(
        NodeId::from_raw(parent as u32),
        NodeId::from_raw(child as u32),
    );
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
}

#[op2(fast)]
pub fn op_dom_insert_before(
    state: &mut OpState,
    #[smi] parent: i32,
    #[smi] child: i32,
    #[smi] reference: i32,
) {
    let state = state.borrow_mut::<DomState>();
    state.dom.insert_before(
        NodeId::from_raw(parent as u32),
        NodeId::from_raw(child as u32),
        NodeId::from_raw(reference as u32),
    );
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
}

#[op2(fast)]
pub fn op_dom_remove_child(state: &mut OpState, #[smi] _parent: i32, #[smi] child: i32) {
    let state = state.borrow_mut::<DomState>();
    state.dom.detach(NodeId::from_raw(child as u32));
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
}

#[op2(fast)]
pub fn op_dom_set_attribute(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] name: &str,
    #[string] value: &str,
) {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    if let Some(node) = state.dom.get_mut(id) {
        if let Some(elem) = node.as_element_mut() {
            if let Some(attr) = elem
                .attrs
                .iter_mut()
                .find(|a| a.name.local.eq_ignore_ascii_case(name))
            {
                attr.value = value.to_string();
            } else {
                elem.attrs.push(crate::dom::node::Attribute {
                    name: crate::dom::node::QualName::new(name),
                    value: value.to_string(),
                });
            }
        }
    }
    if name.eq_ignore_ascii_case("style") || name.eq_ignore_ascii_case("class") {
        state.layout_engine.mark_dirty();
    }
}

#[op2(fast)]
pub fn op_dom_remove_attribute(state: &mut OpState, #[smi] node_id: i32, #[string] name: &str) {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    if let Some(node) = state.dom.get_mut(id) {
        if let Some(elem) = node.as_element_mut() {
            elem.attrs
                .retain(|a| !a.name.local.eq_ignore_ascii_case(name));
        }
    }
    if name.eq_ignore_ascii_case("style") || name.eq_ignore_ascii_case("class") {
        state.layout_engine.mark_dirty();
    }
}

#[op2(fast)]
pub fn op_dom_set_text_content(state: &mut OpState, #[smi] node_id: i32, #[string] text: &str) {
    let state = state.borrow_mut::<DomState>();
    state
        .dom
        .set_text_content(NodeId::from_raw(node_id as u32), text);
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
}

#[op2(fast)]
pub fn op_dom_set_inner_html(state: &mut OpState, #[smi] node_id: i32, #[string] html: &str) {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    let fragment_dom = crate::html_parser::parse_html(&format!("<body>{}</body>", html));
    let body = fragment_dom
        .get_elements_by_tag_name(NodeId::DOCUMENT, "body")
        .into_iter()
        .next();

    // Remove existing children
    let old_children: Vec<NodeId> = state.dom.children(id);
    for child in old_children {
        state.dom.remove(child);
    }

    // Merge fragment children
    if let Some(body_id) = body {
        for child_id in fragment_dom.children(body_id) {
            let new_child = state.dom.merge_subtree(&fragment_dom, child_id);
            state.dom.append_child(id, new_child);
        }
    }
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
}

/// Clone a node. If deep=true, clone all descendants too.
#[op2(fast)]
#[smi]
pub fn op_dom_clone_node(state: &mut OpState, #[smi] node_id: i32, deep: bool) -> i32 {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    if deep {
        // merge_subtree does a deep copy from the same DOM
        let cloned = {
            // We need to read from &self and write to &mut self.
            // merge_subtree takes &Dom for source. Build a snapshot of the subtree.
            // Actually, we can use a two-pass: first collect the tree shape, then rebuild.
            clone_subtree_deep(&mut state.dom, id)
        };
        cloned.to_raw() as i32
    } else {
        // Shallow: copy just this node (no children)
        let node = match state.dom.get(id) {
            Some(n) => n,
            None => return -1,
        };
        let new_id = match &node.data {
            crate::dom::node::NodeData::Element(elem) => state
                .dom
                .create_element(elem.name.clone(), elem.attrs.clone()),
            crate::dom::node::NodeData::Text(t) => state.dom.create_text(t.clone()),
            crate::dom::node::NodeData::Comment(t) => state.dom.create_comment(t.clone()),
            _ => state.dom.create_document_fragment(),
        };
        new_id.to_raw() as i32
    }
}

/// Deep clone a subtree within the same Dom.
fn clone_subtree_deep(dom: &mut crate::dom::Dom, root: NodeId) -> NodeId {
    // Collect the tree structure first (read phase)
    let snapshot = collect_subtree(dom, root);
    // Rebuild from snapshot (write phase)
    rebuild_from_snapshot(dom, &snapshot)
}

#[derive(Debug)]
enum SnapshotNode {
    Element {
        name: crate::dom::node::QualName,
        attrs: Vec<crate::dom::node::Attribute>,
        children: Vec<SnapshotNode>,
    },
    Text(String),
    Comment(String),
    Fragment(Vec<SnapshotNode>),
}

fn collect_subtree(dom: &crate::dom::Dom, id: NodeId) -> SnapshotNode {
    let node = match dom.get(id) {
        Some(n) => n,
        None => return SnapshotNode::Fragment(vec![]),
    };
    let children: Vec<SnapshotNode> = dom
        .children(id)
        .iter()
        .map(|&child_id| collect_subtree(dom, child_id))
        .collect();
    match &node.data {
        crate::dom::node::NodeData::Element(elem) => SnapshotNode::Element {
            name: elem.name.clone(),
            attrs: elem.attrs.clone(),
            children,
        },
        crate::dom::node::NodeData::Text(t) => SnapshotNode::Text(t.clone()),
        crate::dom::node::NodeData::Comment(t) => SnapshotNode::Comment(t.clone()),
        _ => SnapshotNode::Fragment(children),
    }
}

fn rebuild_from_snapshot(dom: &mut crate::dom::Dom, snapshot: &SnapshotNode) -> NodeId {
    match snapshot {
        SnapshotNode::Element {
            name,
            attrs,
            children,
        } => {
            let id = dom.create_element(name.clone(), attrs.clone());
            for child in children {
                let child_id = rebuild_from_snapshot(dom, child);
                dom.append_child(id, child_id);
            }
            id
        }
        SnapshotNode::Text(t) => dom.create_text(t.clone()),
        SnapshotNode::Comment(t) => dom.create_comment(t.clone()),
        SnapshotNode::Fragment(children) => {
            let id = dom.create_document_fragment();
            for child in children {
                let child_id = rebuild_from_snapshot(dom, child);
                dom.append_child(id, child_id);
            }
            id
        }
    }
}

/// Insert HTML at a position relative to an element.
/// position: "beforebegin", "afterbegin", "beforeend", "afterend"
#[op2(fast)]
pub fn op_dom_insert_adjacent_html(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] position: &str,
    #[string] html: &str,
) {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    let fragment_dom = crate::html_parser::parse_html(&format!("<body>{}</body>", html));
    let frag_body = fragment_dom
        .get_elements_by_tag_name(NodeId::DOCUMENT, "body")
        .into_iter()
        .next();
    let frag_children: Vec<NodeId> = frag_body
        .map(|b| fragment_dom.children(b))
        .unwrap_or_default();
    if frag_children.is_empty() {
        return;
    }

    match position {
        "beforebegin" => {
            // Insert before this element (as previous sibling)
            if let Some(parent) = state.dom.get(id).and_then(|n| n.parent) {
                for &child_id in &frag_children {
                    let new_child = state.dom.merge_subtree(&fragment_dom, child_id);
                    state.dom.insert_before(parent, new_child, id);
                }
            }
        }
        "afterbegin" => {
            // Insert as first child
            let first = state.dom.get(id).and_then(|n| n.first_child);
            for child_id in frag_children.iter().rev() {
                let new_child = state.dom.merge_subtree(&fragment_dom, *child_id);
                if let Some(ref_child) = first {
                    state.dom.insert_before(id, new_child, ref_child);
                } else {
                    state.dom.append_child(id, new_child);
                }
            }
        }
        "beforeend" => {
            // Append as last child (same as appendChild)
            for &child_id in &frag_children {
                let new_child = state.dom.merge_subtree(&fragment_dom, child_id);
                state.dom.append_child(id, new_child);
            }
        }
        "afterend" => {
            // Insert after this element (as next sibling)
            if let Some(parent) = state.dom.get(id).and_then(|n| n.parent) {
                let next = state.dom.get(id).and_then(|n| n.next_sibling);
                for &child_id in &frag_children {
                    let new_child = state.dom.merge_subtree(&fragment_dom, child_id);
                    if let Some(ref_child) = next {
                        state.dom.insert_before(parent, new_child, ref_child);
                    } else {
                        state.dom.append_child(parent, new_child);
                    }
                }
            }
        }
        _ => {}
    }
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
}

#[op2]
#[serde]
pub fn op_dom_document_write(state: &mut OpState, #[string] html: &str) -> Vec<i32> {
    let state = state.borrow_mut::<DomState>();
    let body_id = state
        .dom
        .get_elements_by_tag_name(NodeId::DOCUMENT, "body")
        .into_iter()
        .next();
    let body_id = match body_id {
        Some(id) => id,
        None => return vec![],
    };
    let fragment_dom = crate::html_parser::parse_html(&format!("<body>{}</body>", html));
    let frag_body = fragment_dom
        .get_elements_by_tag_name(NodeId::DOCUMENT, "body")
        .into_iter()
        .next();
    let mut new_ids = Vec::new();
    if let Some(frag_body_id) = frag_body {
        for child_id in fragment_dom.children(frag_body_id) {
            let new_child = state.dom.merge_subtree(&fragment_dom, child_id);
            state.dom.append_child(body_id, new_child);
            new_ids.push(new_child.to_raw() as i32);
        }
    }
    state.layout_engine.mark_dirty();
    crate::js_runtime::readiness::bump_mutation();
    new_ids
}

#[op2(fast)]
pub fn op_dom_class_list_add(state: &mut OpState, #[smi] node_id: i32, #[string] class: &str) {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    if let Some(node) = state.dom.get_mut(id) {
        if let Some(elem) = node.as_element_mut() {
            let current = elem
                .attrs
                .iter()
                .find(|a| a.name.local == "class")
                .map(|a| a.value.clone())
                .unwrap_or_default();
            if !current.split_whitespace().any(|c| c == class) {
                let new_val = if current.is_empty() {
                    class.to_string()
                } else {
                    format!("{} {}", current, class)
                };
                if let Some(attr) = elem.attrs.iter_mut().find(|a| a.name.local == "class") {
                    attr.value = new_val;
                } else {
                    elem.attrs.push(crate::dom::node::Attribute {
                        name: crate::dom::node::QualName::new("class"),
                        value: new_val,
                    });
                }
            }
        }
    }
}

#[op2(fast)]
pub fn op_dom_class_list_remove(state: &mut OpState, #[smi] node_id: i32, #[string] class: &str) {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    if let Some(node) = state.dom.get_mut(id) {
        if let Some(elem) = node.as_element_mut() {
            if let Some(attr) = elem.attrs.iter_mut().find(|a| a.name.local == "class") {
                let new_val: String = attr
                    .value
                    .split_whitespace()
                    .filter(|c| *c != class)
                    .collect::<Vec<_>>()
                    .join(" ");
                attr.value = new_val;
            }
        }
    }
}

/// Get computed style for an element.
/// Checks: 1) inline style attribute, 2) `<style>` block rules, 3) CSS defaults.
/// Uses selector matching for style block rules. Higher specificity wins.
#[op2]
#[serde]
// explicit_counter_loop: `source_order` is a manual CSS source-order
// counter used inside the nested selector-match loop; .enumerate()
// would force a usize↔u32 cast against the stored specificity tuple.
#[allow(
    clippy::explicit_counter_loop,
    reason = "explicit CSS source-order counter"
)]
pub fn op_dom_get_all_computed_styles(
    state: &mut OpState,
    #[smi] node_id: i32,
) -> HashMap<String, String> {
    let state = state.borrow_mut::<DomState>();
    if state.cached_rules.is_empty() && !state.stylesheets.is_empty() {
        state.update_cached_rules();
    }
    let id = NodeId::from_raw(node_id as u32);
    let dom_el = if let Some(el) = DomElement::new(&state.dom, id) {
        el
    } else {
        return HashMap::new();
    };

    let mut declarations: HashMap<String, (u32, u32, String)> = HashMap::new();
    let mut source_order: u32 = 0;

    for rule in &state.cached_rules {
        for sel in &rule.selectors {
            if crate::css_selectors::matches_selector(&dom_el, sel) {
                let s = crate::css_selectors::compute_specificity(sel);
                let spec = s.a * 10000 + s.b * 100 + s.c;
                for (name, val) in &rule.declarations {
                    let entry = declarations
                        .entry(name.clone())
                        .or_insert((0, 0, String::new()));
                    if spec > entry.0 || (spec == entry.0 && source_order >= entry.1) {
                        *entry = (spec, source_order, val.clone());
                    }
                }
            }
        }
        source_order += 1;
    }

    // Add inline styles (highest specificity)
    if let Some(el) = state.dom.get(id).and_then(|n| n.as_element()) {
        if let Some(style) = el
            .attrs
            .iter()
            .find(|a| a.name.local.eq_ignore_ascii_case("style"))
        {
            for decl in style.value.split(';') {
                if let Some(colon) = decl.find(':') {
                    let name = decl[..colon].trim().to_string();
                    let val = decl[colon + 1..].trim().to_string();
                    declarations.insert(name, (999999, 999999, val));
                }
            }
        }
    }

    // Resolve calc() and CSS Values 4 math functions to their used
    // pixel value before returning — Chrome's getComputedStyle does
    // this. Otherwise scripts that compute via calc(... sin(pi) ...)
    // and read the result back would see the unresolved expression
    // text instead of the resolved value real Chrome returns.
    let ctx = calc_context_from(state);
    let res: HashMap<String, String> = declarations
        .into_iter()
        .map(|(k, v)| (k, resolve_computed_value(&v.2, &ctx)))
        .collect();
    res
}

#[op2]
#[string]
pub fn op_dom_get_computed_style(
    state: &mut OpState,
    #[smi] node_id: i32,
    #[string] property: &str,
) -> String {
    let state = state.borrow_mut::<DomState>();
    if state.cached_rules.is_empty() && !state.stylesheets.is_empty() {
        state.update_cached_rules();
    }
    let id = NodeId::from_raw(node_id as u32);
    let ctx = calc_context_from(state);

    // 1. Check inline style (highest specificity)
    let inline_val = get_inline_style_value(&state.dom, id, property);
    if let Some(val) = &inline_val {
        if !val.is_empty() {
            return resolve_computed_value(val, &ctx);
        }
    }

    // 2. Check <style> block rules (matched by selector)
    if let Some(val) = get_stylesheet_value(state, id, property) {
        return resolve_computed_value(&val, &ctx);
    }

    // 3. CSS inheritance — walk up the DOM for inherited properties
    const INHERITED: &[&str] = &[
        "color",
        "font-family",
        "font-size",
        "font-style",
        "font-weight",
        "font-variant",
        "line-height",
        "letter-spacing",
        "word-spacing",
        "text-align",
        "text-indent",
        "text-transform",
        "white-space",
        "direction",
        "visibility",
        "cursor",
        "list-style-type",
        "list-style-position",
        "list-style-image",
        "list-style",
        "border-collapse",
        "border-spacing",
        "caption-side",
        "empty-cells",
        "quotes",
        "orphans",
        "widows",
        "text-decoration-color",
    ];

    if INHERITED.contains(&property) {
        let mut current = id;
        while let Some(parent_id) = state.dom.get(current).and_then(|n| n.parent) {
            if let Some(val) = get_inline_style_value(&state.dom, parent_id, property) {
                if !val.is_empty() {
                    return resolve_computed_value(&val, &ctx);
                }
            }
            if let Some(val) = get_stylesheet_value(state, parent_id, property) {
                return resolve_computed_value(&val, &ctx);
            }
            current = parent_id;
        }
    }

    // 4. CSS default
    crate::js_runtime::extensions::layout_ext::css_default(property)
}

/// Extract a property value from an element's inline style attribute.
fn get_inline_style_value(dom: &crate::dom::Dom, id: NodeId, property: &str) -> Option<String> {
    let style_attr = dom.get(id).and_then(|n| n.as_element()).and_then(|e| {
        e.attrs
            .iter()
            .find(|a| a.name.local.eq_ignore_ascii_case("style"))
            .map(|a| a.value.clone())
    })?;

    for decl in style_attr.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim();
            let val = decl[colon + 1..].trim();
            if prop.eq_ignore_ascii_case(property) {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Search <style> block rules for a matching declaration.
/// Returns the value from the highest-specificity matching rule.
#[allow(clippy::explicit_counter_loop, reason = "CSS source-order counter")]
fn get_stylesheet_value(state: &DomState, id: NodeId, property: &str) -> Option<String> {
    let dom_el = DomElement::new(&state.dom, id)?;

    // Collect all matching declarations: (specificity, source_order, value)
    let mut matches: Vec<(u32, u32, String)> = Vec::new();
    let mut source_order: u32 = 0;

    for rule in &state.cached_rules {
        let mut matched = false;
        let mut best_spec: u32 = 0;
        for sel in &rule.selectors {
            if crate::css_selectors::matches_selector(&dom_el, sel) {
                matched = true;
                let s = crate::css_selectors::compute_specificity(sel);
                let spec = s.a * 10000 + s.b * 100 + s.c;
                if spec > best_spec {
                    best_spec = spec;
                }
            }
        }

        if matched {
            if let Some(val) = rule.declarations.get(property) {
                matches.push((best_spec, source_order, val.clone()));
            }
        }
        source_order += 1;
    }

    if matches.is_empty() {
        return None;
    }

    // Sort by specificity (ascending), then source order — last wins
    matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // Winner is the last entry (highest specificity, latest source order)
    matches.last().map(|(_, _, val)| val.clone())
}

// --- Shadow DOM ops ---

/// Attach a shadow root to an element. Returns the shadow root node ID.
#[op2(fast)]
#[smi]
pub fn op_dom_attach_shadow(state: &mut OpState, #[smi] node_id: i32, #[string] mode: &str) -> i32 {
    let state = state.borrow_mut::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    let shadow_mode = match mode {
        "closed" => crate::dom::node::ShadowRootMode::Closed,
        _ => crate::dom::node::ShadowRootMode::Open,
    };
    let shadow_id = state.dom.create_shadow_root(id, shadow_mode);
    shadow_id.to_raw() as i32
}

/// Get shadow root of an element (-1 if none).
#[op2(fast)]
#[smi]
pub fn op_dom_get_shadow_root(state: &mut OpState, #[smi] node_id: i32) -> i32 {
    let state = state.borrow::<DomState>();
    let id = NodeId::from_raw(node_id as u32);
    state
        .dom
        .get(id)
        .and_then(|n| n.as_element())
        .and_then(|e| e.shadow_root)
        .map(|sr| sr.to_raw() as i32)
        .unwrap_or(-1)
}

// --- CSSOM ops ---

#[op2(fast)]
pub fn op_dom_get_stylesheet_count(state: &mut OpState) -> i32 {
    let state = state.borrow::<DomState>();
    state.stylesheets.len() as i32
}

#[derive(serde::Serialize)]
pub struct CSSRuleJson {
    pub selector_text: String,
    pub css_text: String,
    pub rule_type: u8,
}

/// Get parsed rules for a stylesheet by index.
#[op2]
#[serde]
pub fn op_dom_get_stylesheet_rules(state: &mut OpState, #[smi] index: i32) -> Vec<CSSRuleJson> {
    let state = state.borrow::<DomState>();
    let idx = index as usize;
    if idx >= state.stylesheets.len() {
        return vec![];
    }
    let (stylesheet, _errors) = crate::css_parser::parse_stylesheet(&state.stylesheets[idx]);
    let mut rules = Vec::new();
    for rule in &stylesheet.rules {
        if let crate::css_parser::ast::Rule::Qualified(qr) = rule {
            let selector_text = tokens_to_string(&qr.prelude);
            if selector_text.is_empty() {
                continue;
            }
            let decl_parts: Vec<String> = qr
                .declarations
                .iter()
                .map(|d| {
                    let val = tokens_to_string(&d.value).trim().to_string();
                    if d.important {
                        format!("{}: {} !important", d.name, val)
                    } else {
                        format!("{}: {}", d.name, val)
                    }
                })
                .collect();
            let css_text = format!("{} {{ {} }}", selector_text, decl_parts.join("; "));
            rules.push(CSSRuleJson {
                selector_text: selector_text.trim().to_string(),
                css_text,
                rule_type: 1, // CSSStyleRule
            });
        }
    }
    rules
}

#[op2]
#[string]
pub fn op_dom_get_base_url(state: &mut OpState) -> String {
    let state = state.borrow::<DomState>();
    state
        .base_url
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_else(|| "about:blank".to_string())
}

#[op2]
#[string]
pub fn op_dom_storage_get(
    state: &mut OpState,
    #[string] area: String,
    #[string] key: String,
) -> Option<String> {
    let state = state.borrow::<DomState>();
    state.storage.get(&area).and_then(|m| m.get(&key)).cloned()
}

#[op2(fast)]
pub fn op_dom_storage_set(
    state: &mut OpState,
    #[string] area: String,
    #[string] key: String,
    #[string] value: String,
) {
    let state = state.borrow_mut::<DomState>();
    if let Some(m) = state.storage.get_mut(&area) {
        m.insert(key, value);
    }
}

#[op2(fast)]
pub fn op_dom_storage_remove(state: &mut OpState, #[string] area: String, #[string] key: String) {
    let state = state.borrow_mut::<DomState>();
    if let Some(m) = state.storage.get_mut(&area) {
        m.remove(&key);
    }
}

#[op2(fast)]
pub fn op_dom_storage_clear(state: &mut OpState, #[string] area: String) {
    let state = state.borrow_mut::<DomState>();
    if let Some(m) = state.storage.get_mut(&area) {
        m.clear();
    }
}

#[op2]
#[serde]
pub fn op_dom_storage_keys(state: &mut OpState, #[string] area: String) -> Vec<String> {
    let state = state.borrow::<DomState>();
    state
        .storage
        .get(&area)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

// ──────────────────────────────────────────────────────────────────
// Child-realm support
// ──────────────────────────────────────────────────────────────────

/// Window constructor callback — throws per the spec ("Illegal constructor").
/// Used only to create a real, named `Window` function whose `.name === "Window"`
/// and whose `.prototype.constructor === Window`. In practice nothing calls
/// `new Window()`, so the throw body is never reached; we keep it for spec correctness.
fn _window_ctor_cb(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut _rv: v8::ReturnValue,
) {
    if let Some(msg) = v8::String::new(scope, "Illegal constructor") {
        let e = v8::Exception::type_error(scope, msg);
        scope.throw_exception(e);
    }
}

/// Create (or return cached) a genuine `v8::Context` child realm for an iframe's
/// `contentWindow`.  Returns the child global as a live JS object — NOT a Proxy.
///
/// The child context gets:
/// - Real, realm-distinct native intrinsics (`Object`/`Function`/`Array`/… ≠ parent's)
///   — matching real Chrome, where contentWindow is a genuine realm, not a Proxy
///   or a parent alias.
/// - `[[Prototype]] === Window.prototype` → `cw.constructor.name === "Window"`.
/// - Genuine-native `Function.prototype.toString` (same API-fn recipe as the main window).
/// - Standard self-referential globals (`window`, `self`, `globalThis`, `frames`).
///
/// JS completes the setup by setting `document`, `location`, `navigator`, `fetch`,
/// `devicePixelRatio` (accessor), etc. on the returned object.
#[op2]
pub fn op_create_child_realm<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    #[smi] host_node_id: i32,
) -> v8::Local<'s, v8::Value> {
    let rid = realm_id as u32;
    let host_node_id = host_node_id as u32;

    // Access OpState via the isolate-level state (public, stable in 0.311).
    // op_state_from takes &Isolate; HandleScope auto-derefs there.
    let op_state_rc = JsRuntime::op_state_from(scope);

    // Fast path: cached realm — return the previously-created global.
    {
        let op_state = op_state_rc.borrow();
        if let Some(store) = op_state.try_borrow::<IframeRealmStore>() {
            if let Some(global) = store.globals.get(&rid) {
                return v8::Local::new(scope, global).into();
            }
        }
    }

    // Clone the `orig_fp_tostring` and `native_tag_sym` Globals into new
    // handles BEFORE entering the child ContextScope (requires parent scope).
    let orig_fpt: Option<v8::Global<v8::Function>>;
    let native_tag_sym: Option<v8::Global<v8::Symbol>>;
    let reusable_window_proxy: Option<v8::Global<v8::Object>>;
    {
        let op_state = op_state_rc.borrow();
        if let Some(store) = op_state.try_borrow::<IframeRealmStore>() {
            orig_fpt = store.orig_fp_tostring.as_ref().map(|g| {
                let local = v8::Local::new(scope, g);
                v8::Global::new(scope, local)
            });
            native_tag_sym = store.native_tag_sym.as_ref().map(|g| {
                let local = v8::Local::new(scope, g);
                v8::Global::new(scope, local)
            });
            reusable_window_proxy = store.window_proxies.get(&host_node_id).map(|g| {
                let local = v8::Local::new(scope, g);
                v8::Global::new(scope, local)
            });
        } else {
            orig_fpt = None;
            native_tag_sym = None;
            reusable_window_proxy = None;
        }
    }

    // Create the child context (vanilla v8::Context — full native intrinsics).
    // On navigation, V8 resets the reused global object's state while keeping
    // its identity, which is exactly WindowProxy's browser contract.
    let global_object = reusable_window_proxy
        .as_ref()
        .map(|global| v8::Local::new(scope, global).into());
    let parent_ctx = scope.get_current_context();
    let parent_microtask_queue =
        parent_ctx.get_microtask_queue() as *const v8::MicrotaskQueue as *mut v8::MicrotaskQueue;
    let child_ctx = v8::Context::new(
        scope,
        v8::ContextOptions {
            global_object,
            microtask_queue: Some(parent_microtask_queue),
            ..Default::default()
        },
    );

    // Copy parent's security token to child so V8 treats the contexts as
    // same-origin (about:blank inherits the parent origin in Chrome).
    // Without this, accessing child-realm objects from the parent scope
    // throws "TypeError: no access" via V8's cross-context security check.
    let parent_tok = parent_ctx.get_security_token(scope);
    child_ctx.set_security_token(parent_tok);

    // Child realm slots 1/2 (ContextState, ModuleMap) are unset; deno_core's
    // promise-reject callback reads them and would segfault, so borrow the parent's.
    // SAFETY: parent_ctx is a live deno_core context whose slots 1/2 hold valid
    // pointers; we copy them without taking ownership, so no double-free.
    unsafe {
        let cs_ptr = parent_ctx.get_aligned_pointer_from_embedder_data(1);
        let mm_ptr = parent_ctx.get_aligned_pointer_from_embedder_data(2);
        child_ctx.set_aligned_pointer_in_embedder_data(1, cs_ptr);
        child_ctx.set_aligned_pointer_in_embedder_data(2, mm_ptr);
    }

    // Set up the child context.  Returns None on any fatal V8 allocation
    // failure (extremely rare); the outer code falls back to undefined.
    let mut child_inner_global_g: Option<v8::Global<v8::Object>> = None;
    let property_bridge: Option<IframePropertyBridge>;
    let child_global_g: Option<v8::Global<v8::Object>> = {
        let cs = &mut v8::ContextScope::new(scope, child_ctx);

        // Build a real `Window` function (FunctionTemplate → native `[native code]`)
        // so the child global is typed: `constructor.name === "Window"`.
        let window_tmpl = v8::FunctionTemplate::new(cs, _window_ctor_cb);
        if let Some(n) = v8::String::new(cs, "Window") {
            window_tmpl.set_class_name(n);
        }
        let window_fn = match window_tmpl.get_function(cs) {
            Some(f) => f,
            None => return v8::undefined(cs).into(),
        };
        if let Some(n) = v8::String::new(cs, "Window") {
            window_fn.set_name(n);
        }

        // Preserve V8's native GlobalProxy -> inner-global link. The GlobalProxy
        // is the engine primitive we use as WindowProxy; replacing its
        // prototype directly disconnects the inner global and makes reflective
        // operations such as Object.isExtensible/DefineOwnProperty stop
        // forwarding correctly. Instead, type the *inner global* as Window.
        let child_proxy = child_ctx.global(cs);
        let inner_global = child_proxy
            .get_prototype(cs)
            .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok());
        if let Some(inner) = inner_global {
            child_inner_global_g = Some(v8::Global::new(cs, inner));
        }

        // inner_global.[[Prototype]] = Window.prototype
        // → GlobalProxy forwards into a Window-shaped ordinary global object.
        if let Some(pk) = v8::String::new(cs, "prototype") {
            if let Some(proto_val) = window_fn.get(cs, pk.into()) {
                if let Some(inner) = inner_global {
                    inner.set_prototype(cs, proto_val);
                }
            }
        }

        let child_global = child_proxy;

        // Expose Window on child global (scripts may read `contentWindow.Window`).
        if let Some(k) = v8::String::new(cs, "Window") {
            child_global.set(cs, k.into(), window_fn.into());
        }

        // Standard self-referential globals (all point to child_global).
        for key in &["window", "self", "globalThis", "frames"] {
            if let Some(k) = v8::String::new(cs, key) {
                child_global.set(cs, k.into(), child_global.into());
            }
        }
        // length = 0  (avoid borrow-twice by staging the value first)
        if let Some(k) = v8::String::new(cs, "length") {
            let zero = v8::Integer::new(cs, 0);
            child_global.set(cs, k.into(), zero.into());
        }
        // opener = null
        if let Some(k) = v8::String::new(cs, "opener") {
            let null = v8::null(cs);
            child_global.set(cs, k.into(), null.into());
        }

        // Install genuine-native Function.prototype.toString in child realm.
        // Closes the [[SourceText]] leak for child-realm functions too.
        // Pass native_tag_sym (JS global registry) so tagged host fns
        // in the child realm stringify correctly via the Array-data path.
        if let Some(ref orig) = orig_fpt {
            install_native_fp_tostring(cs, orig, native_tag_sym.as_ref());
        }

        property_bridge = (|| {
            Some(IframePropertyBridge {
                get: compile_realm_function(cs, "(function(k){return globalThis[k];})")?,
                set: compile_realm_function(
                    cs,
                    "(function(k,v){globalThis[k]=v;return true;})",
                )?,
                has: compile_realm_function(cs, "(function(k){return k in globalThis;})")?,
                delete: compile_realm_function(
                    cs,
                    "(function(k){return delete globalThis[k];})",
                )?,
                own_keys: compile_realm_function(
                    cs,
                    "(function(){return Reflect.ownKeys(globalThis).filter(function(k){return typeof k==='string';});})",
                )?,
                descriptor: compile_realm_function(
                    cs,
                    "(function(k){return Object.getOwnPropertyDescriptor(globalThis,k);})",
                )?,
                define: compile_realm_function(
                    cs,
                    "(function(k,d){return Reflect.defineProperty(globalThis,k,d);})",
                )?,
            })
        })();

        Some(v8::Global::new(cs, child_global))
    };

    let child_global_g = match child_global_g {
        Some(g) => g,
        None => return v8::undefined(scope).into(),
    };

    // Build Local and a second persistent handle BEFORE moving the realm-local
    // handle into the store.
    let local: v8::Local<'s, v8::Value> = v8::Local::new(scope, &child_global_g).into();
    let stable_window_proxy = {
        let local = v8::Local::new(scope, &child_global_g);
        v8::Global::new(scope, local)
    };

    // Persist context (keeps it alive) and cache global in OpState.
    {
        let mut op_state = op_state_rc.borrow_mut();
        if let Some(store) = op_state.try_borrow_mut::<IframeRealmStore>() {
            if let Some(old_rid) = store.node_to_realm.insert(host_node_id, rid) {
                if old_rid != rid {
                    store.contexts.remove(&old_rid);
                    store.globals.remove(&old_rid);
                    store.inner_globals.remove(&old_rid);
                    store.public_windows.remove(&old_rid);
                    store.property_bridges.remove(&old_rid);
                    store.realm_to_node.remove(&old_rid);
                }
            }
            store.realm_to_node.insert(rid, host_node_id);
            store
                .contexts
                .insert(rid, v8::Global::new(scope, child_ctx));
            store.globals.insert(rid, child_global_g);
            if let Some(inner) = child_inner_global_g {
                store.inner_globals.insert(rid, inner);
            }
            if let Some(bridge) = property_bridge {
                store.property_bridges.insert(rid, bridge);
            }
            store
                .window_proxies
                .insert(host_node_id, stable_window_proxy);
        }
    }

    local
}

/// Drop a same-isolate iframe realm and both host/realm index entries.
///
/// JS calls this when an iframe navigates or is detached. Rust also uses the
/// same store for public `FrameContext` lookup, so teardown must be atomic from
/// both directions or stale handles can accidentally enter a replaced realm.
#[op2(fast)]
pub fn op_dispose_child_realm(
    state: &mut OpState,
    #[smi] realm_id: i32,
    preserve_window_proxy: bool,
) {
    let rid = realm_id as u32;
    let Some(store) = state.try_borrow_mut::<IframeRealmStore>() else {
        return;
    };
    if let Some(node_id) = store.realm_to_node.remove(&rid) {
        if store.node_to_realm.get(&node_id) == Some(&rid) {
            store.node_to_realm.remove(&node_id);
        }
        if !preserve_window_proxy {
            store.window_proxies.remove(&node_id);
        }
    }
    store.contexts.remove(&rid);
    store.globals.remove(&rid);
    store.inner_globals.remove(&rid);
    store.public_windows.remove(&rid);
    store.property_bridges.remove(&rid);
}

/// Set a property on the INNER GLOBAL of a child realm.
///
/// The global proxy's own property dict is NOT visible to code running inside
/// the child realm (which reads from the inner global's scope chain). Setting
/// on the proxy via `proxy.set()` from Rust only writes to the proxy's own
/// Two-path write to guarantee visibility from both inside and outside the realm:
///
/// 1. `create_data_property` on the inner global (the JSGlobalObject behind the
///    GlobalProxy): makes the property an own property of the inner global, so
///    scope-chain lookups from scripts running INSIDE the realm find it.
///
/// The public HTML WindowProxy wrapper also forwards to this same inner global,
/// so the ordinary Window data object is the single property source of truth.
#[op2]
pub fn op_set_child_realm_prop<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    key: v8::Local<v8::Value>,
    value: v8::Local<v8::Value>,
) -> v8::Local<'s, v8::Value> {
    let rid = realm_id as u32;
    let op_state_rc = JsRuntime::op_state_from(scope);

    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return v8::undefined(scope).into();
        };
        (
            store.contexts.get(&rid).cloned(),
            store
                .property_bridges
                .get(&rid)
                .map(|bridge| bridge.set.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return v8::undefined(scope).into();
    };

    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    let _ = function.call(cs, receiver, &[key, value]);

    v8::undefined(cs).into()
}

/// Install a child-realm global only when the name is not already visible.
///
/// Window named-frame properties must not clobber native APIs or page-authored
/// globals. The existence check and both inner/proxy writes happen in one V8
/// context entry, so child mutation callbacks never need to re-enter the same
/// realm through JavaScript evaluation.
#[op2(fast)]
pub fn op_set_child_realm_prop_if_absent<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    key: v8::Local<v8::Value>,
    value: v8::Local<v8::Value>,
) -> bool {
    let rid = realm_id as u32;
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, has_g, set_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return false;
        };
        (
            store.contexts.get(&rid).cloned(),
            store
                .property_bridges
                .get(&rid)
                .map(|bridge| bridge.has.clone()),
            store
                .property_bridges
                .get(&rid)
                .map(|bridge| bridge.set.clone()),
        )
    };
    let (Some(child_ctx_g), Some(has_g), Some(set_g)) = (child_ctx_g, has_g, set_g) else {
        return false;
    };

    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    let has_fn = v8::Local::new(cs, &has_g);
    if has_fn
        .call(cs, receiver, &[key])
        .map(|value| value.boolean_value(cs))
        .unwrap_or(true)
    {
        return false;
    }
    let set_fn = v8::Local::new(cs, &set_g);
    set_fn
        .call(cs, receiver, &[key, value])
        .map(|value| value.boolean_value(cs))
        .unwrap_or(false)
}

/// Return the same-isolate iframe realm whose JavaScript is currently running.
/// `-1` denotes the top-level document / no child execution scope.
#[op2(fast)]
#[smi]
pub fn op_current_child_realm_id(state: &mut OpState) -> i32 {
    state
        .try_borrow::<IframeRealmStore>()
        .and_then(|store| store.execution_stack.last().copied())
        .map(|id| id as i32)
        .unwrap_or(-1)
}

/// Return the V8 GlobalProxy of the currently executing same-isolate iframe.
/// The top-level document is represented by `undefined` so callers can fall
/// back to their own `globalThis` without manufacturing another wrapper.
#[op2]
pub fn op_current_child_realm_window<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> v8::Local<'s, v8::Value> {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (public_window, child_ctx_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return v8::undefined(scope).into();
        };
        let Some(rid) = store.execution_stack.last().copied() else {
            return v8::undefined(scope).into();
        };
        (
            store.public_windows.get(&rid).cloned(),
            store.contexts.get(&rid).cloned(),
        )
    };
    if let Some(public_window) = public_window {
        return v8::Local::new(scope, &public_window).into();
    }
    let Some(child_ctx_g) = child_ctx_g else {
        return v8::undefined(scope).into();
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    child_ctx.global(scope).into()
}

/// Register the browser-visible WindowProxy wrapper for a same-isolate realm.
#[op2(fast)]
pub fn op_register_child_public_window<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    window: v8::Local<v8::Value>,
) {
    let Ok(window) = v8::Local::<v8::Object>::try_from(window) else {
        return;
    };
    let op_state_rc = JsRuntime::op_state_from(scope);
    let mut op_state = op_state_rc.borrow_mut();
    if let Some(store) = op_state.try_borrow_mut::<IframeRealmStore>() {
        store
            .public_windows
            .insert(realm_id as u32, v8::Global::new(scope, window));
    }
}

/// Return the ordinary inner global object behind a child realm's V8
/// GlobalProxy. The public HTML WindowProxy wrapper uses this object as its
/// data/reflection backend; the raw GlobalProxy remains the realm's execution
/// identity (`window === globalThis === this`).
#[op2]
pub fn op_child_realm_inner_global<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
) -> v8::Local<'s, v8::Value> {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let inner_global = {
        let op_state = op_state_rc.borrow();
        op_state
            .try_borrow::<IframeRealmStore>()
            .and_then(|store| store.inner_globals.get(&(realm_id as u32)).cloned())
    };
    let Some(inner_global) = inner_global else {
        return v8::undefined(scope).into();
    };
    v8::Local::new(scope, &inner_global).into()
}

/// Read a property from a child realm while entered in that realm's
/// ContextScope. This is the authoritative bridge used by the public HTML
/// WindowProxy wrapper; direct cross-context GlobalProxy reflection in V8 does
/// not implement the browser WindowProxy exotic contract.
#[op2(reentrant)]
pub fn op_child_realm_get_property<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    #[string] key: String,
) -> v8::Local<'s, v8::Value> {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return v8::undefined(scope).into();
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.get.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return v8::undefined(scope).into();
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return v8::undefined(cs).into();
    };
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[key.into()])
        .unwrap_or_else(|| v8::undefined(cs).into())
}

#[op2(fast)]
pub fn op_child_realm_set_property<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    #[string] key: String,
    value: v8::Local<v8::Value>,
) -> bool {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return false;
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.set.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return false;
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return false;
    };
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[key.into(), value])
        .map(|value| value.boolean_value(cs))
        .unwrap_or(false)
}

#[op2(fast)]
pub fn op_child_realm_has_property(
    scope: &mut v8::PinScope<'_, '_>,
    #[smi] realm_id: i32,
    #[string] key: String,
) -> bool {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return false;
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.has.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return false;
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return false;
    };
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[key.into()])
        .map(|value| value.boolean_value(cs))
        .unwrap_or(false)
}

#[op2(fast)]
pub fn op_child_realm_delete_property(
    scope: &mut v8::PinScope<'_, '_>,
    #[smi] realm_id: i32,
    #[string] key: String,
) -> bool {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return false;
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.delete.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return false;
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return false;
    };
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[key.into()])
        .map(|value| value.boolean_value(cs))
        .unwrap_or(false)
}

#[op2(reentrant)]
pub fn op_child_realm_own_property_names<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
) -> v8::Local<'s, v8::Array> {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return v8::Array::new(scope, 0);
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.own_keys.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return v8::Array::new(scope, 0);
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[])
        .and_then(|value| v8::Local::<v8::Array>::try_from(value).ok())
        .unwrap_or_else(|| v8::Array::new(cs, 0))
}

#[op2(reentrant)]
pub fn op_child_realm_get_own_property_descriptor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    #[string] key: String,
) -> v8::Local<'s, v8::Value> {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return v8::undefined(scope).into();
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.descriptor.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return v8::undefined(scope).into();
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return v8::undefined(cs).into();
    };
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[key.into()])
        .unwrap_or_else(|| v8::undefined(cs).into())
}

#[op2(fast)]
pub fn op_child_realm_define_property<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    #[string] key: String,
    descriptor: v8::Local<v8::Object>,
) -> bool {
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return false;
        };
        (
            store.contexts.get(&(realm_id as u32)).cloned(),
            store
                .property_bridges
                .get(&(realm_id as u32))
                .map(|bridge| bridge.define.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return false;
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return false;
    };
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    function
        .call(cs, receiver, &[key.into(), descriptor.into()])
        .map(|value| value.boolean_value(cs))
        .unwrap_or(false)
}

/// Delete a temporary property from both the inner global and the outer
/// WindowProxy of a child realm. Constructor installation uses this to pass
/// parent implementation references into a closure without leaving observable
/// bootstrap names behind on `iframe.contentWindow`.
#[op2(fast)]
pub fn op_delete_child_realm_prop(
    scope: &mut v8::PinScope,
    #[smi] realm_id: i32,
    #[string] key: String,
) {
    let rid = realm_id as u32;
    let op_state_rc = JsRuntime::op_state_from(scope);
    let (child_ctx_g, function_g) = {
        let op_state = op_state_rc.borrow();
        let Some(store) = op_state.try_borrow::<IframeRealmStore>() else {
            return;
        };
        (
            store.contexts.get(&rid).cloned(),
            store
                .property_bridges
                .get(&rid)
                .map(|bridge| bridge.delete.clone()),
        )
    };
    let (Some(child_ctx_g), Some(function_g)) = (child_ctx_g, function_g) else {
        return;
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let Some(key) = v8::String::new(cs, &key) else {
        return;
    };
    let key: v8::Local<v8::Value> = key.into();
    let function = v8::Local::new(cs, &function_g);
    let receiver: v8::Local<v8::Value> = child_ctx.global(cs).into();
    let _ = function.call(cs, receiver, &[key]);
}

/// Relation getter for child `parent` / `top`.
///
/// External callers observe the public HTML WindowProxy wrapper. If the
/// currently executing same-isolate realm is exactly the relation target, code
/// inside that realm must see its raw V8 GlobalProxy so strict identity such as
/// `inner.parent === window` remains true.
fn frame_relation_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _key: v8::Local<'s, v8::Name>,
    args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<v8::Value>,
) {
    let Ok(data) = v8::Local::<v8::Array>::try_from(args.data()) else {
        return;
    };
    let public = data.get_index(scope, 0);
    let raw = data.get_index(scope, 1);

    let executing_ctx = {
        let op_state_rc = JsRuntime::op_state_from(scope);
        let op_state = op_state_rc.borrow();
        op_state.try_borrow::<IframeRealmStore>().and_then(|store| {
            let rid = *store.execution_stack.last()?;
            store.contexts.get(&rid).cloned()
        })
    };
    if let (Some(raw), Some(ctx_g)) = (raw, executing_ctx) {
        let ctx = v8::Local::new(scope, &ctx_g);
        let executing_raw: v8::Local<v8::Value> = ctx.global(scope).into();
        if executing_raw.strict_equals(raw) {
            rv.set(raw);
            return;
        }
    }
    if let Some(public) = public {
        rv.set(public);
    }
}

/// Install browser-visible `parent`/`top` relations on a child realm.
/// Message routing is handled independently by the sender-realm stack.
#[op2]
pub fn op_install_frame_parent<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    parent_public_window: v8::Local<v8::Value>,
    parent_raw_window: v8::Local<v8::Value>,
    top_window: v8::Local<v8::Value>,
) -> v8::Local<'s, v8::Value> {
    let rid = realm_id as u32;
    let op_state_rc = JsRuntime::op_state_from(scope);
    let child_ctx_g = {
        let op_state = op_state_rc.borrow();
        op_state
            .try_borrow::<IframeRealmStore>()
            .and_then(|store| store.contexts.get(&rid).cloned())
    };
    let Some(child_ctx_g) = child_ctx_g else {
        return v8::undefined(scope).into();
    };
    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);
    let child_proxy = child_ctx.global(cs);

    for (key, public_window, raw_window) in [
        ("parent", parent_public_window, parent_raw_window),
        ("top", top_window, top_window),
    ] {
        let Some(k) = v8::String::new(cs, key) else {
            continue;
        };
        let data = v8::Array::new(cs, 2);
        data.set_index(cs, 0, public_window);
        data.set_index(cs, 1, raw_window);
        let name: v8::Local<v8::Name> = k.into();
        let cfg = v8::AccessorConfiguration::new(frame_relation_getter).data(data.into());
        child_proxy.set_accessor_with_configuration(cs, name, cfg);
    }
    v8::undefined(cs).into()
}

/// Execute a JavaScript string inside a child realm's context.
///
/// Compiles and runs `code` in the child context scope. Returns the result
/// (coerced to string) or `undefined` on compile/runtime error. Used for
/// cases where `op_set_child_realm_prop` cannot express the required
/// descriptor shape (e.g. accessor properties with a getter function).
/// Reentrant: child code runs synchronously and may call DOM ops.
#[op2(reentrant)]
#[string]
pub fn op_eval_in_child_realm<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    #[smi] realm_id: i32,
    #[string] code: String,
) -> Option<String> {
    let rid = realm_id as u32;
    let op_state_rc = JsRuntime::op_state_from(scope);

    let child_ctx_g: Option<v8::Global<v8::Context>> = {
        let op_state = op_state_rc.borrow();
        op_state.try_borrow::<IframeRealmStore>().and_then(|store| {
            store.contexts.get(&rid).map(|g| {
                let local = v8::Local::new(scope, g);
                v8::Global::new(scope, local)
            })
        })
    };
    let child_ctx_g = child_ctx_g?;

    let child_ctx = v8::Local::new(scope, &child_ctx_g);
    let cs = &mut v8::ContextScope::new(scope, child_ctx);

    let src = v8::String::new(cs, &code)?;
    // A swallowed compile/runtime error here means the child realm is
    // silently under-populated (a missing shim can make site scripts
    // bail or hit an undefined receiver). Surface it to an opt-in
    // diagnostic channel
    // (`BROWSER_OXIDE_DEBUG_CHILD_REALM`) WITHOUT changing behavior: still
    // best-effort runs the script, still returns `None`.
    {
        let mut op_state = op_state_rc.borrow_mut();
        if let Some(store) = op_state.try_borrow_mut::<IframeRealmStore>() {
            store.execution_stack.push(rid);
        }
    }
    v8::tc_scope!(let tc, cs);
    let ok = match v8::Script::compile(tc, src, None) {
        Some(script) => script.run(tc).is_some(),
        None => false,
    };
    // Parser/srcdoc/document.write scripts have the same end-of-task
    // microtask semantics as scripts entered through FrameContext. Keep the
    // realm id on the execution stack until the checkpoint has drained so
    // promise callbacks retain their correct browsing-context identity.
    let queue = child_ctx.get_microtask_queue();
    queue.perform_checkpoint(tc);
    {
        let mut op_state = op_state_rc.borrow_mut();
        if let Some(store) = op_state.try_borrow_mut::<IframeRealmStore>() {
            let popped = store.execution_stack.pop();
            debug_assert_eq!(popped, Some(rid));
        }
    }
    if !ok && std::env::var("BROWSER_OXIDE_DEBUG_CHILD_REALM").is_ok() {
        let msg = tc
            .exception()
            .and_then(|e| e.to_string(tc))
            .map(|s| s.to_rust_string_lossy(tc))
            .unwrap_or_else(|| "<no exception object>".to_string());
        let snippet: String = code.chars().take(160).collect();
        eprintln!("[child-realm:{rid}] eval error: {msg} | code[..160]={snippet:?}");
    }
    None
}

deno_core::extension!(
    dom_extension,
    ops = [
        op_dom_document_node,
        op_dom_get_tag_name,
        op_dom_get_node_type,
        op_dom_get_text_content,
        op_dom_get_inner_html,
        op_dom_get_outer_html,
        op_dom_get_attribute,
        op_dom_has_attribute,
        op_dom_get_attribute_names,
        op_dom_get_parent,
        op_dom_get_children,
        op_dom_get_children_with_types,
        op_dom_get_child_elements,
        op_dom_get_child_elements_with_types,
        op_dom_get_first_child,
        op_dom_get_last_child,
        op_dom_get_next_sibling,
        op_dom_get_prev_sibling,
        op_dom_query_selector,
        op_dom_query_selector_all,
        op_dom_get_element_by_id,
        op_dom_get_elements_by_tag_name,
        op_dom_get_elements_by_class_name,
        op_dom_collect_insert_targets,
        op_dom_matches,
        op_dom_closest,
        op_dom_contains,
        op_dom_is_connected,
        op_dom_get_first_element_child,
        op_dom_get_last_element_child,
        op_dom_get_next_element_sibling,
        op_dom_get_prev_element_sibling,
        op_dom_get_child_element_count,
        op_dom_create_element,
        op_dom_create_text_node,
        op_dom_create_document_fragment,
        op_dom_append_child,
        op_dom_insert_before,
        op_dom_remove_child,
        op_dom_set_attribute,
        op_dom_remove_attribute,
        op_dom_set_text_content,
        op_dom_set_inner_html,
        op_dom_document_write,
        op_dom_clone_node,
        op_dom_insert_adjacent_html,
        op_dom_class_list_add,
        op_dom_class_list_remove,
        op_dom_get_computed_style,
        op_dom_get_all_computed_styles,
        op_dom_get_stylesheet_count,
        op_dom_get_stylesheet_rules,
        op_dom_attach_shadow,
        op_dom_get_shadow_root,
        op_dom_get_base_url,
        op_dom_storage_get,
        op_dom_storage_set,
        op_dom_storage_remove,
        op_dom_storage_clear,
        op_dom_storage_keys,
        op_create_child_realm,
        op_dispose_child_realm,
        op_set_child_realm_prop,
        op_set_child_realm_prop_if_absent,
        op_current_child_realm_id,
        op_current_child_realm_window,
        op_register_child_public_window,
        op_child_realm_inner_global,
        op_child_realm_get_property,
        op_child_realm_set_property,
        op_child_realm_has_property,
        op_child_realm_delete_property,
        op_child_realm_own_property_names,
        op_child_realm_get_own_property_descriptor,
        op_child_realm_define_property,
        op_delete_child_realm_prop,
        op_install_frame_parent,
        op_eval_in_child_realm,
    ],
);
