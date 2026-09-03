use crate::dom::Dom;
use crate::layout::{LayoutEngine, Viewport};
use std::collections::HashMap;
use std::sync::Arc;

/// Shared state stored in deno_core's OpState, accessible by all ops.
pub struct DomState {
    pub dom: Dom,
    pub layout_engine: LayoutEngine,
    pub base_url: Option<url::Url>,
    /// Console output capture
    pub console_output: Vec<ConsoleMessage>,
    /// localStorage / sessionStorage (in-memory)
    pub storage: HashMap<String, HashMap<String, String>>,
    /// CSS from `<style>` blocks, used by getComputedStyle
    pub stylesheets: Vec<String>,
    /// Parsed and simplified CSS rules for fast lookup
    pub cached_rules: Vec<CachedRule>,
    pub stealth_profile: Option<crate::stealth::StealthProfile>,
    /// Active Content Security Policy. Built from the response
    /// `Content-Security-Policy` header(s) plus any
    /// `<meta http-equiv="Content-Security-Policy">` tags found in the
    /// parsed HTML. None means no policy applies (e.g. about:blank,
    /// from_html with no header). The policy applies to ALL fetches —
    /// `<script src>`, `op_fetch`, `op_net_fetch_sync`, iframes — until
    /// the next top-level navigation.
    pub csp_policy: Option<Arc<crate::net::csp::PolicySet>>,
    /// Origin used to resolve `'self'` in CSP source matching. Equals
    /// the document's origin (scheme + host + port of the navigated
    /// URL). None for opaque/about:blank documents — those bypass CSP.
    pub csp_origin: Option<url::Url>,
    /// Resource timings for performance.getEntriesByType('resource')
    pub resource_timings: Vec<crate::net::TimingStats>,
    /// Diagnostics ring: the exact sources the engine executed as
    /// top-level `<script>` code, named by URL (or `<inline>#i`). Bounded
    /// (see `EXEC_SCRIPT_RING_CAP`); exists because stack frames in
    /// obfuscated bundles (`v6@1:50607`) can only be resolved against the
    /// text that actually ran — CDN bundles are regenerated per fetch, so
    /// a later re-fetch of the same URL does not match.
    pub executed_scripts: Vec<ExecutedScript>,
}

#[derive(Debug, Clone)]
pub struct ExecutedScript {
    pub name: String,
    pub code: String,
}

/// Keep the last N executed scripts per realm. Large enough to cover a
/// page's main bundle + a widget's challenge script; small enough that
/// `Page::executed_scripts` snapshots stay cheap.
pub const EXEC_SCRIPT_RING_CAP: usize = 8;

impl ExecutedScript {
    pub fn new(name: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            code: code.into(),
        }
    }
}

/// Record an executed top-level script into the realm's diagnostic ring.
/// No-op when no DomState is installed (bare runtimes in unit tests).
pub fn record_executed_script(state: &mut deno_core::OpState, name: &str, code: &str) {
    if let Some(dom_state) = state.try_borrow_mut::<DomState>() {
        // One entry per distinct source length is enough for stack
        // resolution and keeps repeated tiny bootstraps from
        // crowding out the big bundles we chase.
        if dom_state
            .executed_scripts
            .iter()
            .any(|s| s.code.len() == code.len())
        {
            return;
        }
        if dom_state.executed_scripts.len() >= EXEC_SCRIPT_RING_CAP {
            dom_state.executed_scripts.remove(0);
        }
        dom_state
            .executed_scripts
            .push(ExecutedScript::new(name, code));
    }
}

#[derive(Debug, Clone)]
pub struct CachedRule {
    pub selector_str: String,
    pub selectors: crate::css_selectors::SelectorList,
    pub declarations: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub level: ConsoleLevel,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
    Info,
    Debug,
}

impl DomState {
    pub fn new(dom: Dom) -> Self {
        let mut storage = HashMap::new();
        storage.insert("local".to_string(), HashMap::new());
        storage.insert("session".to_string(), HashMap::new());
        Self {
            dom,
            layout_engine: LayoutEngine::new(Viewport::new(1920.0, 1080.0)),
            base_url: None,
            console_output: Vec::new(),
            storage,
            stylesheets: Vec::new(),
            cached_rules: Vec::new(),
            stealth_profile: None,
            csp_policy: None,
            csp_origin: None,
            resource_timings: Vec::new(),
            executed_scripts: Vec::new(),
        }
    }

    pub fn update_cached_rules(&mut self) {
        use crate::js_runtime::utils::tokens_to_string;
        self.cached_rules.clear();
        for css_text in &self.stylesheets {
            let (stylesheet, _errors) = crate::css_parser::parse_stylesheet(css_text);
            for rule in &stylesheet.rules {
                if let crate::css_parser::ast::Rule::Qualified(qr) = rule {
                    let selector_str = tokens_to_string(&qr.prelude);
                    if selector_str.is_empty() {
                        continue;
                    }
                    let mut declarations = HashMap::new();
                    for d in &qr.declarations {
                        declarations.insert(
                            d.name.to_string(),
                            tokens_to_string(&d.value).trim().to_string(),
                        );
                    }
                    let selectors = crate::css_selectors::parse_selector_list(&selector_str)
                        .unwrap_or_default();
                    self.cached_rules.push(CachedRule {
                        selector_str,
                        selectors,
                        declarations,
                    });
                }
            }
        }
    }

    pub fn with_base_url(mut self, url: url::Url) -> Self {
        self.base_url = Some(url);
        self
    }
}
