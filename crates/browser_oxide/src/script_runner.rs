use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;

/// Information about a <script> element found in the DOM.
pub struct ScriptInfo {
    pub code: String,
    pub src: Option<String>,
    /// Value of the `nonce` attribute, if any. Required by CSP3
    /// `'nonce-...'` source matching — when the active policy uses
    /// `'strict-dynamic'`, only nonce-tagged parser-inserted scripts
    /// are authorized to load. Captured here at HTML-walk time so the
    /// fetch path (`page.rs::navigate_with_init`) can pass it to
    /// `crate::net::csp::CheckCtx`.
    pub nonce: Option<String>,
    /// `<script type="module">` — must be executed via the ES-module path
    /// (`load_side_es_module` + `mod_evaluate`) NOT classic `execute_script`,
    /// which throws `SyntaxError: Cannot use import statement outside a module`
    /// and silently drops modern Vite/React/Vue bundles. (P2 / thin-render fix.)
    pub is_module: bool,
    /// Raw `NodeId` of the `<script>` element in the arena DOM. Used to set
    /// `document.currentScript` to this element's wrapper for the duration of
    /// the script's execution (the standard web-API contract). Scripts that
    /// locate their own `<script>` element via `document.currentScript` (e.g.
    /// to read a `data-*` attribute or resolve a relative path) depend on it;
    /// without it set, `currentScript` is `null` and such scripts stall.
    pub node_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptKind {
    Classic,
    Module,
    Data,
}

/// Classify an HTML `<script type>` value using browser-style semantics.
///
/// Missing/empty `type` is classic JavaScript. `module` is an ES-module
/// entry. Otherwise only JavaScript MIME types execute; every other value is
/// a data block and must not be handed to V8 as source code.
fn classify_script_type(raw: Option<&str>) -> ScriptKind {
    let Some(raw) = raw else {
        return ScriptKind::Classic;
    };
    let value = raw.trim();
    if value.is_empty() {
        return ScriptKind::Classic;
    }
    if value.eq_ignore_ascii_case("module") {
        return ScriptKind::Module;
    }
    if value.eq_ignore_ascii_case("importmap") || value.eq_ignore_ascii_case("speculationrules") {
        return ScriptKind::Data;
    }

    // MIME parameters do not change the JavaScript MIME essence, e.g.
    // `text/javascript; charset=utf-8` is still executable JavaScript.
    let essence = value
        .split_once(';')
        .map_or(value, |(essence, _)| essence)
        .trim();
    let is_javascript_mime = [
        "application/ecmascript",
        "application/javascript",
        "application/x-ecmascript",
        "application/x-javascript",
        "text/ecmascript",
        "text/javascript",
        "text/javascript1.0",
        "text/javascript1.1",
        "text/javascript1.2",
        "text/javascript1.3",
        "text/javascript1.4",
        "text/javascript1.5",
        "text/jscript",
        "text/livescript",
        "text/x-ecmascript",
        "text/x-javascript",
    ]
    .iter()
    .any(|candidate| essence.eq_ignore_ascii_case(candidate));

    if is_javascript_mime {
        ScriptKind::Classic
    } else {
        ScriptKind::Data
    }
}

/// Find all <script> elements in the DOM and extract their content.
/// Returns both inline scripts (code) and external scripts (src URL).
pub fn find_scripts(dom: &Dom) -> Vec<ScriptInfo> {
    let mut scripts = Vec::new();
    collect_scripts(dom, NodeId::DOCUMENT, &mut scripts);
    for (i, s) in scripts.iter().enumerate() {
        if let Some(src) = &s.src {
            tracing::debug!(index = i, src = %src, "Found external script");
        } else {
            tracing::debug!(index = i, code_len = s.code.len(), "Found inline script");
        }
    }
    scripts
}

fn collect_scripts(dom: &Dom, node_id: NodeId, scripts: &mut Vec<ScriptInfo>) {
    let children = dom.children(node_id);
    for child_id in children {
        if let Some(node) = dom.get(child_id) {
            if let NodeData::Element(elem) = &node.data {
                if elem.name.local.eq_ignore_ascii_case("script") {
                    let script_type = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "type")
                        .map(|a| a.value.as_str());
                    let script_kind = classify_script_type(script_type);
                    if script_kind == ScriptKind::Data {
                        collect_scripts(dom, child_id, scripts);
                        continue;
                    }

                    // `nomodule` suppresses classic scripts in browsers that
                    // support ES modules. BrowserOxide has a module loader, so
                    // executing this legacy fallback would double-run app
                    // bootstraps on modern pages.
                    let no_module = elem
                        .attrs
                        .iter()
                        .any(|a| a.name.local.eq_ignore_ascii_case("nomodule"));
                    if script_kind == ScriptKind::Classic && no_module {
                        collect_scripts(dom, child_id, scripts);
                        continue;
                    }

                    let src = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "src")
                        .map(|a| decode_html_entities(a.value.as_str()));

                    let nonce = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "nonce")
                        .map(|a| a.value.to_string())
                        .filter(|n| !n.is_empty());

                    let is_module = script_kind == ScriptKind::Module;

                    if src.is_some() {
                        // External script — store the URL for fetching
                        scripts.push(ScriptInfo {
                            code: String::new(),
                            src,
                            nonce,
                            is_module,
                            node_id: child_id.to_raw(),
                        });
                    } else {
                        // Inline script
                        let code = dom.text_content(child_id);
                        if !code.trim().is_empty() {
                            scripts.push(ScriptInfo {
                                code,
                                src: None,
                                nonce,
                                is_module,
                                node_id: child_id.to_raw(),
                            });
                        }
                    }
                }
            }
            collect_scripts(dom, child_id, scripts);
        }
    }
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::{classify_script_type, find_scripts, ScriptKind};

    #[test]
    fn script_type_classification_matches_browser_execution_rules() {
        assert_eq!(classify_script_type(None), ScriptKind::Classic);
        assert_eq!(classify_script_type(Some("")), ScriptKind::Classic);
        assert_eq!(classify_script_type(Some("  \t")), ScriptKind::Classic);
        assert_eq!(classify_script_type(Some("module")), ScriptKind::Module);
        assert_eq!(classify_script_type(Some("MODULE")), ScriptKind::Module);
        assert_eq!(
            classify_script_type(Some("text/javascript; charset=utf-8")),
            ScriptKind::Classic
        );
        assert_eq!(
            classify_script_type(Some("application/javascript")),
            ScriptKind::Classic
        );
        assert_eq!(classify_script_type(Some("a-state")), ScriptKind::Data);
        assert_eq!(
            classify_script_type(Some("application/ld+json")),
            ScriptKind::Data
        );
        assert_eq!(classify_script_type(Some("importmap")), ScriptKind::Data);
        assert_eq!(
            classify_script_type(Some("speculationrules")),
            ScriptKind::Data
        );
        assert_eq!(
            classify_script_type(Some("application/x-custom-data")),
            ScriptKind::Data
        );
    }

    #[test]
    fn find_scripts_skips_unknown_data_blocks_but_keeps_js_and_modules() {
        let dom = crate::html_parser::parse_html(
            r#"<html><body>
                <script>globalThis.a = 1;</script>
                <script type="a-state">{"not":"javascript"}</script>
                <script type="application/x-custom-data">value: still-data</script>
                <script type="text/javascript; charset=utf-8">globalThis.b = 2;</script>
                <script nomodule>globalThis.legacy = true;</script>
                <script type="module">globalThis.c = 3;</script>
                <script type="importmap">{"imports":{}}</script>
                <script type="speculationrules">{"prefetch":[]}</script>
            </body></html>"#,
        );

        let scripts = find_scripts(&dom);
        assert_eq!(scripts.len(), 3);
        assert!(!scripts[0].is_module);
        assert!(scripts[0].code.contains("globalThis.a"));
        assert!(!scripts[1].is_module);
        assert!(scripts[1].code.contains("globalThis.b"));
        assert!(scripts[2].is_module);
        assert!(scripts[2].code.contains("globalThis.c"));
    }
}
