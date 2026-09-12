use crate::css_selectors::Element;
use crate::dom::arena::Dom;
use crate::dom::node::{NodeData, NodeId};

/// A DOM element wrapper that implements `crate::css_selectors::Element`.
///
/// This is a lightweight handle: it borrows the `Dom` and holds a `NodeId`.
/// Created on-the-fly for selector matching.
#[derive(Clone)]
pub struct DomElement<'a> {
    pub dom: &'a Dom,
    pub id: NodeId,
}

impl<'a> DomElement<'a> {
    pub fn new(dom: &'a Dom, id: NodeId) -> Option<Self> {
        let node = dom.get(id)?;
        if node.is_element() {
            Some(Self { dom, id })
        } else {
            None
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.id
    }

    fn node(&self) -> &crate::dom::node::Node {
        self.dom.get(self.id).unwrap()
    }

    fn element_data(&self) -> &crate::dom::node::ElementData {
        self.node().as_element().unwrap()
    }

    fn tag_is(&self, names: &[&str]) -> bool {
        names
            .iter()
            .any(|name| self.local_name().eq_ignore_ascii_case(name))
    }

    fn input_type(&self) -> &str {
        self.attribute_value("type")
            .filter(|value| !value.is_empty())
            .unwrap_or("text")
    }

    fn is_descendant_of_or_self(&self, ancestor: NodeId) -> bool {
        let mut current = Some(self.id);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.dom.get(id).and_then(|node| node.parent);
        }
        false
    }

    fn disabled_by_fieldset(&self) -> bool {
        // HTML disabled-fieldset inheritance has one important exception:
        // descendants of the fieldset's first legend element remain enabled.
        // Walk every disabled fieldset ancestor because nested fieldsets may
        // independently disable the element.
        let mut current = self.node().parent;
        while let Some(id) = current {
            let Some(node) = self.dom.get(id) else {
                break;
            };
            if let Some(data) = node.as_element() {
                if data.name.local.eq_ignore_ascii_case("fieldset")
                    && data
                        .attrs
                        .iter()
                        .any(|a| a.name.local.eq_ignore_ascii_case("disabled"))
                {
                    let first_legend = self.dom.child_elements(id).into_iter().find(|child| {
                        self.dom
                            .get(*child)
                            .and_then(|n| n.as_element())
                            .is_some_and(|e| e.name.local.eq_ignore_ascii_case("legend"))
                    });
                    let exempt =
                        first_legend.is_some_and(|legend| self.is_descendant_of_or_self(legend));
                    if !exempt {
                        return true;
                    }
                }
            }
            current = node.parent;
        }
        false
    }

    fn content_editable_state(&self) -> bool {
        // contenteditable is inherited. Invalid values behave like the
        // missing-value state, so keep walking until a valid token is found.
        let mut current = Some(self.id);
        while let Some(id) = current {
            let Some(node) = self.dom.get(id) else {
                break;
            };
            if let Some(data) = node.as_element() {
                if let Some(attr) = data
                    .attrs
                    .iter()
                    .find(|a| a.name.local.eq_ignore_ascii_case("contenteditable"))
                {
                    let value = attr.value.trim();
                    if value.is_empty()
                        || value.eq_ignore_ascii_case("true")
                        || value.eq_ignore_ascii_case("plaintext-only")
                    {
                        return true;
                    }
                    if value.eq_ignore_ascii_case("false") {
                        return false;
                    }
                }
            }
            current = node.parent;
        }
        false
    }

    fn input_supports_required(&self) -> bool {
        matches!(
            self.input_type().to_ascii_lowercase().as_str(),
            "text"
                | "search"
                | "url"
                | "tel"
                | "email"
                | "password"
                | "date"
                | "month"
                | "week"
                | "time"
                | "datetime-local"
                | "number"
                | "checkbox"
                | "radio"
                | "file"
        )
    }

    fn input_is_text_editable(&self) -> bool {
        matches!(
            self.input_type().to_ascii_lowercase().as_str(),
            "text"
                | "search"
                | "url"
                | "tel"
                | "email"
                | "password"
                | "date"
                | "month"
                | "week"
                | "time"
                | "datetime-local"
                | "number"
        )
    }

    fn input_supports_placeholder(&self) -> bool {
        matches!(
            self.input_type().to_ascii_lowercase().as_str(),
            "text" | "search" | "url" | "tel" | "email" | "password" | "number"
        )
    }
}

impl<'a> std::fmt::Debug for DomElement<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = self.element_data();
        write!(f, "<{}", data.name.local)?;
        for attr in &data.attrs {
            write!(f, " {}=\"{}\"", attr.name.local, attr.value)?;
        }
        write!(f, ">")
    }
}

impl<'a> Element for DomElement<'a> {
    fn same_element(&self, other: &Self) -> bool {
        std::ptr::eq(self.dom, other.dom) && self.id == other.id
    }

    fn local_name(&self) -> &str {
        &self.element_data().name.local
    }

    fn namespace(&self) -> Option<&str> {
        self.element_data().name.ns.as_deref()
    }

    fn is_defined(&self) -> bool {
        // HTML parser nodes use `None` for the HTML namespace while elements
        // created through the DOM ops may retain the canonical XHTML URI.
        // Only potential autonomous custom-element names can be undefined.
        // SVG (including hyphenated SVG names) is always outside the custom
        // element definition state used by HTML's `:defined` pseudo-class.
        let namespace = self.namespace();
        let is_html = namespace.is_none() || namespace == Some("http://www.w3.org/1999/xhtml");
        if !is_html {
            return true;
        }

        let name = self.local_name().to_ascii_lowercase();
        if !name.contains('-') {
            return true;
        }

        // HTML reserves these historical hyphenated names from the custom
        // element name grammar. Chromium still considers them defined even
        // though they are exposed as HTMLUnknownElement instances.
        if matches!(
            name.as_str(),
            "annotation-xml"
                | "color-profile"
                | "font-face"
                | "font-face-src"
                | "font-face-uri"
                | "font-face-format"
                | "font-face-name"
                | "missing-glyph"
        ) {
            return true;
        }

        self.dom.is_custom_element_defined(&name)
    }

    fn id(&self) -> Option<&str> {
        self.element_data()
            .attrs
            .iter()
            .find(|a| a.name.local == "id")
            .map(|a| a.value.as_str())
    }

    fn has_class(&self, name: &str) -> bool {
        self.element_data()
            .attrs
            .iter()
            .find(|a| a.name.local == "class")
            .is_some_and(|a| a.value.split_whitespace().any(|c| c == name))
    }

    fn has_attribute(&self, name: &str) -> bool {
        self.element_data()
            .attrs
            .iter()
            .any(|a| a.name.local.eq_ignore_ascii_case(name))
    }

    fn attribute_value(&self, name: &str) -> Option<&str> {
        self.element_data()
            .attrs
            .iter()
            .find(|a| a.name.local.eq_ignore_ascii_case(name))
            .map(|a| a.value.as_str())
    }

    fn text_content(&self) -> String {
        self.dom.text_content(self.id)
    }

    fn parent_element(&self) -> Option<Self> {
        let mut parent_id = self.node().parent?;
        loop {
            let parent = self.dom.get(parent_id)?;
            if parent.is_element() {
                return Some(DomElement {
                    dom: self.dom,
                    id: parent_id,
                });
            }
            parent_id = parent.parent?;
        }
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let mut sib_id = self.node().prev_sibling?;
        loop {
            let sib = self.dom.get(sib_id)?;
            if sib.is_element() {
                return Some(DomElement {
                    dom: self.dom,
                    id: sib_id,
                });
            }
            sib_id = sib.prev_sibling?;
        }
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let mut sib_id = self.node().next_sibling?;
        loop {
            let sib = self.dom.get(sib_id)?;
            if sib.is_element() {
                return Some(DomElement {
                    dom: self.dom,
                    id: sib_id,
                });
            }
            sib_id = sib.next_sibling?;
        }
    }

    fn first_child_element(&self) -> Option<Self> {
        let mut child_id = self.node().first_child?;
        loop {
            let child = self.dom.get(child_id)?;
            if child.is_element() {
                return Some(DomElement {
                    dom: self.dom,
                    id: child_id,
                });
            }
            child_id = child.next_sibling?;
        }
    }

    fn last_child_element(&self) -> Option<Self> {
        let mut child_id = self.node().last_child?;
        loop {
            let child = self.dom.get(child_id)?;
            if child.is_element() {
                return Some(DomElement {
                    dom: self.dom,
                    id: child_id,
                });
            }
            child_id = child.prev_sibling?;
        }
    }

    fn is_root(&self) -> bool {
        // Root element is an element whose parent is the Document node
        match self.node().parent {
            Some(parent_id) => self
                .dom
                .get(parent_id)
                .is_some_and(|n| matches!(n.data, NodeData::Document)),
            None => false,
        }
    }

    fn is_empty(&self) -> bool {
        // Empty = no child elements and no non-empty text nodes
        let mut child_id = self.node().first_child;
        while let Some(id) = child_id {
            if let Some(child) = self.dom.get(id) {
                match &child.data {
                    NodeData::Element(_) => return false,
                    NodeData::Text(t) if !t.is_empty() => return false,
                    _ => {}
                }
                child_id = child.next_sibling;
            } else {
                break;
            }
        }
        true
    }

    fn lang(&self) -> Option<&str> {
        // HTML language is inherited from the nearest ancestor carrying a
        // `lang` attribute. An explicitly empty value is significant: it
        // declares the language unknown and therefore stops inheritance.
        let mut current = Some(self.id);
        while let Some(id) = current {
            let node = self.dom.get(id)?;
            if let Some(data) = node.as_element() {
                if let Some(attr) = data
                    .attrs
                    .iter()
                    .find(|a| a.name.local.eq_ignore_ascii_case("lang"))
                {
                    return Some(attr.value.as_str());
                }
            }
            current = node.parent;
        }
        None
    }

    fn is_link(&self) -> bool {
        let name = self.local_name();
        (name == "a" || name == "area") && self.has_attribute("href")
    }

    fn is_enabled(&self) -> bool {
        // Chromium exposes :enabled/:disabled only on controls with an HTML
        // disabled state. Generic elements (including legend/form/output) are
        // neither enabled nor disabled.
        if !self.tag_is(&[
            "button", "fieldset", "input", "optgroup", "option", "select", "textarea",
        ]) {
            return false;
        }
        !self.is_disabled()
    }

    fn is_disabled(&self) -> bool {
        if !self.tag_is(&[
            "button", "fieldset", "input", "optgroup", "option", "select", "textarea",
        ]) {
            return false;
        }

        if self.has_attribute("disabled") {
            return true;
        }

        // An option inherits :disabled from a disabled optgroup, but notably
        // not from a disabled select (Chrome 148 behavior).
        if self.local_name().eq_ignore_ascii_case("option") {
            if let Some(parent) = self.parent_element() {
                if parent.local_name().eq_ignore_ascii_case("optgroup") && parent.is_disabled() {
                    return true;
                }
            }
            return false;
        }

        // Form controls inside a disabled fieldset inherit disabled state,
        // except when they are descendants of the fieldset's first legend.
        self.disabled_by_fieldset()
    }

    fn is_required(&self) -> bool {
        if !self.has_attribute("required") {
            return false;
        }
        if self.local_name().eq_ignore_ascii_case("input") {
            return self.input_supports_required();
        }
        self.tag_is(&["select", "textarea"])
    }

    fn is_optional(&self) -> bool {
        // Chromium applies :optional to input/select/textarea and button.
        // It does not apply it to option/fieldset/generic elements.
        self.tag_is(&["button", "input", "select", "textarea"]) && !self.is_required()
    }

    fn is_read_write(&self) -> bool {
        if self.is_disabled() {
            return false;
        }

        if self.local_name().eq_ignore_ascii_case("textarea") {
            return !self.has_attribute("readonly");
        }

        if self.local_name().eq_ignore_ascii_case("input") {
            return self.input_is_text_editable() && !self.has_attribute("readonly");
        }

        self.content_editable_state()
    }

    fn is_placeholder_shown(&self) -> bool {
        if !self.has_attribute("placeholder") {
            return false;
        }

        if self.local_name().eq_ignore_ascii_case("input") {
            let value = self
                .dom
                .form_control_value(self.id)
                .unwrap_or_else(|| self.attribute_value("value").unwrap_or_default());
            return self.input_supports_placeholder() && value.is_empty();
        }

        if self.local_name().eq_ignore_ascii_case("textarea") {
            let value = self
                .dom
                .form_control_value(self.id)
                .map(str::to_owned)
                .unwrap_or_else(|| self.text_content());
            return value.is_empty();
        }

        false
    }

    fn is_modal(&self) -> bool {
        self.dom.is_dialog_modal(self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::arena::Dom;
    use crate::dom::node::{Attribute, QualName};

    fn build_test_dom() -> Dom {
        let mut dom = Dom::new();
        let html = dom.create_element(QualName::new("html"), vec![]);
        dom.append_child(NodeId::DOCUMENT, html);

        let body = dom.create_element(QualName::new("body"), vec![]);
        dom.append_child(html, body);

        let div = dom.create_element(
            QualName::new("div"),
            vec![
                Attribute {
                    name: QualName::new("id"),
                    value: "main".to_string(),
                },
                Attribute {
                    name: QualName::new("class"),
                    value: "container active".to_string(),
                },
            ],
        );
        dom.append_child(body, div);

        let p = dom.create_element(QualName::new("p"), vec![]);
        dom.append_child(div, p);

        let text = dom.create_text("Hello world".to_string());
        dom.append_child(p, text);

        dom
    }

    #[test]
    fn element_local_name() {
        let dom = build_test_dom();
        let _div_id = dom.child_elements(dom.children(NodeId::DOCUMENT)[0])[0]; // body's first child
        let _body_id = dom.child_elements(NodeId::DOCUMENT)[0]; // html
        let html_el = DomElement::new(&dom, dom.children(NodeId::DOCUMENT)[0]).unwrap();
        assert_eq!(html_el.local_name(), "html");
    }

    #[test]
    fn element_id_and_class() {
        let dom = build_test_dom();
        let html = dom.children(NodeId::DOCUMENT)[0];
        let body = dom.children(html)[0];
        let div = dom.children(body)[0];
        let el = DomElement::new(&dom, div).unwrap();

        assert_eq!(el.id(), Some("main"));
        assert!(el.has_class("container"));
        assert!(el.has_class("active"));
        assert!(!el.has_class("inactive"));
    }

    #[test]
    fn parent_and_children() {
        let dom = build_test_dom();
        let html = dom.children(NodeId::DOCUMENT)[0];
        let body = dom.children(html)[0];
        let div = dom.children(body)[0];
        let p = dom.children(div)[0];

        let p_el = DomElement::new(&dom, p).unwrap();
        let parent = p_el.parent_element().unwrap();
        assert_eq!(parent.local_name(), "div");

        let div_el = DomElement::new(&dom, div).unwrap();
        let first_child = div_el.first_child_element().unwrap();
        assert_eq!(first_child.local_name(), "p");
    }

    #[test]
    fn is_root() {
        let dom = build_test_dom();
        let html = dom.children(NodeId::DOCUMENT)[0];
        let body = dom.children(html)[0];

        let html_el = DomElement::new(&dom, html).unwrap();
        assert!(html_el.is_root());

        let body_el = DomElement::new(&dom, body).unwrap();
        assert!(!body_el.is_root());
    }

    #[test]
    fn is_empty() {
        let dom = build_test_dom();
        let html = dom.children(NodeId::DOCUMENT)[0];
        let body = dom.children(html)[0];
        let div = dom.children(body)[0];
        let p = dom.children(div)[0];

        let div_el = DomElement::new(&dom, div).unwrap();
        assert!(!div_el.is_empty()); // has child <p>

        // p has text "Hello world" → not empty
        let p_el = DomElement::new(&dom, p).unwrap();
        assert!(!p_el.is_empty());
    }

    #[test]
    fn selector_matching_integration() {
        let dom = build_test_dom();
        let html = dom.children(NodeId::DOCUMENT)[0];
        let body = dom.children(html)[0];
        let div = dom.children(body)[0];

        let div_el = DomElement::new(&dom, div).unwrap();

        let selectors = crate::css_selectors::parse_selector_list("div#main.container").unwrap();
        assert!(crate::css_selectors::matches_selector(
            &div_el,
            &selectors[0]
        ));

        let selectors2 = crate::css_selectors::parse_selector_list("body > div").unwrap();
        assert!(crate::css_selectors::matches_selector(
            &div_el,
            &selectors2[0]
        ));

        let selectors3 = crate::css_selectors::parse_selector_list(".nonexistent").unwrap();
        assert!(!crate::css_selectors::matches_selector(
            &div_el,
            &selectors3[0]
        ));
    }
}
