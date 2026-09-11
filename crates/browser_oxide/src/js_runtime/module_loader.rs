//! ES-module loader for document scripts (P2 / thin-render fix).
//!
//! Without it, `<script type="module">` entries — how every modern Vite/React/
//! Vue SPA ships its app — throw `SyntaxError: Cannot use import statement
//! outside a module` under classic `v8::Script::compile`, so the whole bundle is
//! dropped and the engine serves only the server shell (a 1.8-13 KB "thin
//! render" of a site that should fully hydrate into an SPA).
//!
//! This loader resolves relative specifiers against the referrer/document URL
//! and fetches the import graph **on demand** through the same shared HTTP
//! session (cookies + stealth profile) the navigation uses, so a module's
//! `import "./chunk-[hash].js"` is fetched and evaluated. `ModuleSourceFuture`
//! has no `Send` bound (deno_core drives it on the per-thread LocalSet), so the
//! Rc-based `HttpClient` can be used directly.

use crate::net::HttpClient;
use crate::stealth::StealthProfile;
use deno_core::error::ModuleLoaderError;
use deno_core::{
    resolve_import, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader,
    ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, ResolutionKind,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// HTML JavaScript MIME type essence matching for module scripts. Unlike
/// classic scripts, module fetches must reject a missing or non-JavaScript
/// MIME type instead of executing the response body as JavaScript.
fn is_javascript_mime(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(
        essence.as_str(),
        "application/ecmascript"
            | "application/javascript"
            | "application/x-ecmascript"
            | "application/x-javascript"
            | "text/ecmascript"
            | "text/javascript"
            | "text/javascript1.0"
            | "text/javascript1.1"
            | "text/javascript1.2"
            | "text/javascript1.3"
            | "text/javascript1.4"
            | "text/javascript1.5"
            | "text/jscript"
            | "text/livescript"
            | "text/x-ecmascript"
            | "text/x-javascript"
    )
}

#[derive(Debug, Clone)]
enum ImportMapAddress {
    Url(String),
    Blocked,
}

type SpecifierMap = HashMap<String, ImportMapAddress>;

/// Parsed document import map used by the ES-module resolver.
#[derive(Debug, Clone, Default)]
pub struct ImportMap {
    imports: SpecifierMap,
    scopes: Vec<(String, SpecifierMap)>,
}

/// Per-runtime mutable import-map state. A warm-reused V8 isolate swaps this
/// when its DOM is replaced so mappings never leak between navigations.
#[derive(Debug, Clone, Default)]
pub struct ImportMapState(Rc<RefCell<ImportMap>>);

impl ImportMapState {
    pub fn new(map: ImportMap) -> Self {
        Self(Rc::new(RefCell::new(map)))
    }

    pub fn replace(&self, map: ImportMap) {
        *self.0.borrow_mut() = map;
    }

    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
    ) -> Result<Option<ModuleSpecifier>, ModuleLoaderError> {
        self.0.borrow().resolve(specifier, referrer)
    }
}

/// Per-runtime module-fetch context. The owning document's Origin stays
/// stable across a module graph even when an imported module has a different
/// origin; warm navigation replaces this state together with the import map.
#[derive(Debug, Default)]
struct ModuleRequestContext {
    origin: Option<String>,
    resolved_referrers: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleRequestState(Rc<RefCell<ModuleRequestContext>>);

impl ModuleRequestState {
    pub fn new(origin: Option<String>) -> Self {
        Self(Rc::new(RefCell::new(ModuleRequestContext {
            origin,
            resolved_referrers: HashMap::new(),
        })))
    }

    pub fn replace(&self, origin: Option<String>) {
        let mut state = self.0.borrow_mut();
        state.origin = origin;
        state.resolved_referrers.clear();
    }

    fn origin(&self) -> Option<String> {
        self.0.borrow().origin.clone()
    }

    fn record_referrer(&self, resolved: &ModuleSpecifier, referrer: &str) {
        self.0
            .borrow_mut()
            .resolved_referrers
            .entry(resolved.to_string())
            .or_insert_with(|| referrer.to_string());
    }

    fn referrer_for(&self, resolved: &ModuleSpecifier) -> Option<String> {
        self.0
            .borrow()
            .resolved_referrers
            .get(resolved.as_str())
            .cloned()
    }
}

impl ImportMap {
    /// Collect parser-inserted `<script type="importmap">` blocks in document
    /// order and merge them into the document's import map.
    pub fn from_dom(dom: &crate::dom::Dom, base_url: &str) -> Self {
        use crate::dom::node::{NodeData, NodeId};

        fn walk(dom: &crate::dom::Dom, id: NodeId, out: &mut Vec<String>) {
            for child in dom.children(id) {
                if let Some(node) = dom.get(child) {
                    if let NodeData::Element(elem) = &node.data {
                        let is_import_map = elem.name.local.eq_ignore_ascii_case("script")
                            && elem.attrs.iter().any(|attr| {
                                attr.name.local.eq_ignore_ascii_case("type")
                                    && attr.value.trim().eq_ignore_ascii_case("importmap")
                            });
                        if is_import_map {
                            let text = dom.text_content(child);
                            if !text.trim().is_empty() {
                                out.push(text);
                            }
                        }
                    }
                    walk(dom, child, out);
                }
            }
        }

        let mut blocks = Vec::new();
        walk(dom, NodeId::DOCUMENT, &mut blocks);
        Self::from_json_blocks(blocks.iter().map(String::as_str), base_url)
    }

    fn from_json_blocks<'a>(blocks: impl IntoIterator<Item = &'a str>, base_url: &str) -> Self {
        let Ok(base) = url::Url::parse(base_url) else {
            return Self::default();
        };
        let mut out = Self::default();
        for block in blocks {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(block) else {
                continue;
            };
            if let Some(imports) = value.get("imports").and_then(|v| v.as_object()) {
                merge_specifier_map(&mut out.imports, imports, &base);
            }
            if let Some(scopes) = value.get("scopes").and_then(|v| v.as_object()) {
                for (raw_scope, raw_map) in scopes {
                    let Some(entries) = raw_map.as_object() else {
                        continue;
                    };
                    let Some(scope_url) = resolve_url_like(raw_scope, &base) else {
                        continue;
                    };
                    if let Some((_, existing)) =
                        out.scopes.iter_mut().find(|(s, _)| s == &scope_url)
                    {
                        merge_specifier_map(existing, entries, &base);
                    } else {
                        let mut map = SpecifierMap::new();
                        merge_specifier_map(&mut map, entries, &base);
                        out.scopes.push((scope_url, map));
                    }
                }
            }
        }
        out.scopes
            .sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
        out
    }

    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
    ) -> Result<Option<ModuleSpecifier>, ModuleLoaderError> {
        let normalized = normalize_specifier_key(specifier, referrer);
        for (scope_prefix, map) in &self.scopes {
            if referrer.starts_with(scope_prefix) {
                if let Some(resolved) = resolve_in_map(map, &normalized)? {
                    return Ok(Some(resolved));
                }
            }
        }
        resolve_in_map(&self.imports, &normalized)
    }
}

fn normalize_specifier_key(specifier: &str, base: &str) -> String {
    if url::Url::parse(specifier).is_ok()
        || specifier.starts_with('/')
        || specifier.starts_with("./")
        || specifier.starts_with("../")
    {
        if let Ok(base) = url::Url::parse(base) {
            if let Ok(url) = base.join(specifier) {
                return url.to_string();
            }
        }
    }
    specifier.to_string()
}

fn resolve_url_like(value: &str, base: &url::Url) -> Option<String> {
    url::Url::parse(value)
        .or_else(|_| base.join(value))
        .ok()
        .map(|url| url.to_string())
}

fn normalize_map_key(key: &str, base: &url::Url) -> String {
    if url::Url::parse(key).is_ok()
        || key.starts_with('/')
        || key.starts_with("./")
        || key.starts_with("../")
    {
        return resolve_url_like(key, base).unwrap_or_else(|| key.to_string());
    }
    key.to_string()
}

fn merge_specifier_map(
    target: &mut SpecifierMap,
    entries: &serde_json::Map<String, serde_json::Value>,
    base: &url::Url,
) {
    for (raw_key, raw_value) in entries {
        let key = normalize_map_key(raw_key, base);
        if target.contains_key(&key) {
            continue;
        }
        let address = match raw_value {
            serde_json::Value::Null => ImportMapAddress::Blocked,
            serde_json::Value::String(value) => {
                let Some(url) = resolve_url_like(value, base) else {
                    continue;
                };
                if key.ends_with('/') && !url.ends_with('/') {
                    continue;
                }
                ImportMapAddress::Url(url)
            }
            _ => continue,
        };
        target.insert(key, address);
    }
}

fn resolve_in_map(
    map: &SpecifierMap,
    normalized: &str,
) -> Result<Option<ModuleSpecifier>, ModuleLoaderError> {
    let mut matched: Option<(&str, &ImportMapAddress)> = map
        .get_key_value(normalized)
        .map(|(key, value)| (key.as_str(), value));
    for (key, value) in map {
        if key.ends_with('/')
            && normalized.starts_with(key)
            && matched.is_none_or(|(best, _)| key.len() > best.len())
        {
            matched = Some((key.as_str(), value));
        }
    }

    let Some((key, address)) = matched else {
        return Ok(None);
    };
    let target = match address {
        ImportMapAddress::Blocked => {
            return Err(ModuleLoaderError::generic(format!(
                "import map blocked module specifier {normalized}"
            )))
        }
        ImportMapAddress::Url(url) if key == normalized => url.clone(),
        ImportMapAddress::Url(url) => {
            let suffix = &normalized[key.len()..];
            let base =
                url::Url::parse(url).map_err(|e| ModuleLoaderError::generic(e.to_string()))?;
            let joined = base
                .join(suffix)
                .map_err(|e| ModuleLoaderError::generic(e.to_string()))?
                .to_string();
            if !joined.starts_with(url) {
                return Err(ModuleLoaderError::generic(format!(
                    "import map target escapes mapped prefix for {normalized}"
                )));
            }
            joined
        }
    };
    ModuleSpecifier::parse(&target)
        .map(Some)
        .map_err(|e| ModuleLoaderError::generic(e.to_string()))
}

/// Minimal percent-decoder for non-base64 `data:` URL JS payloads.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Fetches ES-module sources through browser_oxide's shared HTTP session.
pub struct BrowserModuleLoader {
    /// Cached client on the shared session jar (cookie-consistent with the
    /// page nav). `None` only if the connector failed to build.
    client: Option<HttpClient>,
    import_map: ImportMapState,
    request_state: ModuleRequestState,
}

impl BrowserModuleLoader {
    pub fn new(
        profile: StealthProfile,
        import_map: ImportMapState,
        request_state: ModuleRequestState,
    ) -> Self {
        let client = HttpClient::shared(&profile).ok();
        Self {
            client,
            import_map,
            request_state,
        }
    }
}

impl ModuleLoader for BrowserModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, ModuleLoaderError> {
        let resolved = if let Some(mapped) = self.import_map.resolve(specifier, referrer)? {
            mapped
        } else {
            // Spec-compliant relative/absolute-specifier resolution against the referrer.
            resolve_import(specifier, referrer)
                .map_err(|e| ModuleLoaderError::generic(e.to_string()))?
        };
        if kind != ResolutionKind::MainModule {
            self.request_state.record_referrer(&resolved, referrer);
        }
        Ok(resolved)
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        maybe_referrer: Option<&ModuleLoadReferrer>,
        _options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        let spec = module_specifier.clone();
        let url = module_specifier.to_string();
        let client = self.client.clone();
        let request_origin = self.request_state.origin();
        let referer = maybe_referrer
            .map(|r| r.specifier.to_string())
            .or_else(|| self.request_state.referrer_for(&spec))
            .unwrap_or_else(|| url.clone());

        // data: modules. deno_core 0.311 routes `import('data:…')` THROUGH the
        // loader (it is NOT inlined by V8 for dynamic import), so we MUST resolve
        // it. Rejecting it (the previous behaviour) left an UNHANDLED promise
        // rejection that aborted the event-loop drain — duolingo's React app runs
        // a native-dynamic-import capability probe `import('data:text/javascript;
        // base64,Cg==')`, and that abort killed React's MessageChannel commit so
        // `#root` stayed an empty shell. Resolve data: like Chrome.
        if let Some(rest) = url.strip_prefix("data:") {
            use base64::Engine as _;
            let (meta, payload) = rest.split_once(',').unwrap_or(("", rest));
            if !is_javascript_mime(meta) {
                return ModuleLoadResponse::Sync(Err(ModuleLoaderError::generic(format!(
                    "module data URL has non-JavaScript MIME type: {meta}"
                ))));
            }
            let code = if meta.contains(";base64") {
                base64::engine::general_purpose::STANDARD
                    .decode(payload.trim())
                    .ok()
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default()
            } else {
                // Percent-decoded text payload.
                percent_decode(payload)
            };
            return ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::String(code.into()),
                &spec,
                None,
            )));
        }

        if url.starts_with("blob:") {
            let Some((bytes, content_type)) =
                crate::js_runtime::extensions::worker_ext::blob_module_entry(&url)
            else {
                return ModuleLoadResponse::Sync(Err(ModuleLoaderError::generic(format!(
                    "module fetch {url}: blob URL not found or revoked"
                ))));
            };
            if !is_javascript_mime(&content_type) {
                return ModuleLoadResponse::Sync(Err(ModuleLoaderError::generic(format!(
                    "module blob URL has non-JavaScript MIME type: {content_type}"
                ))));
            }
            let code = String::from_utf8_lossy(&bytes).into_owned();
            return ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::String(code.into()),
                &spec,
                None,
            )));
        }

        // Only http(s) modules are network-fetchable.
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return ModuleLoadResponse::Sync(Err(ModuleLoaderError::generic(format!(
                "unsupported module URL scheme: {url}"
            ))));
        }

        let fut = async move {
            // Count this dynamic-import chunk fetch as in-flight so the page
            // isn't reported settled while the chunk is still downloading.
            let _net = crate::js_runtime::readiness::RequestGuard::new();
            let client = client.ok_or_else(|| {
                ModuleLoaderError::generic("module loader: shared HTTP client unavailable")
            })?;
            let resp = client
                .get_script_resource(&url, &referer, request_origin.as_deref(), true, 5)
                .await
                .map_err(|e| ModuleLoaderError::generic(format!("module fetch {url}: {e}")))?;
            if !resp.ok() {
                return Err(ModuleLoaderError::generic(format!(
                    "module fetch {url} -> status {}",
                    resp.status
                )));
            }
            let content_type = resp
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value.as_str())
                .unwrap_or_default();
            if !is_javascript_mime(content_type) {
                return Err(ModuleLoaderError::generic(format!(
                    "module fetch {url} has non-JavaScript MIME type: {content_type}"
                )));
            }
            let code = resp.text();
            Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::String(code.into()),
                &spec,
                None,
            ))
        };
        ModuleLoadResponse::Async(Box::pin(fut))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_request_state_tracks_importer_and_clears_on_navigation() {
        let state = ModuleRequestState::new(Some("https://first.example".to_string()));
        let dep = ModuleSpecifier::parse("https://cdn.example/dep.js").unwrap();
        state.record_referrer(&dep, "https://first.example/entry.js");

        assert_eq!(state.origin().as_deref(), Some("https://first.example"));
        assert_eq!(
            state.referrer_for(&dep).as_deref(),
            Some("https://first.example/entry.js")
        );

        state.replace(Some("https://second.example".to_string()));
        assert_eq!(state.origin().as_deref(), Some("https://second.example"));
        assert_eq!(state.referrer_for(&dep), None);
    }
}
