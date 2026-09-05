//! Iframe support for browser_oxide.
//!
//! Each iframe with `srcdoc` gets its own DOM tree, V8 runtime, and event loop.
//! Communication between parent and child is via serialized postMessage.

use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;
use crate::event_loop::BrowserEventLoop;
use crate::js_runtime::runtime::BrowserRuntimeOptions;
use crate::js_runtime::BrowserJsRuntime;
use std::time::Duration;
use tracing;

/// Info about an iframe found in the DOM.
pub struct IframeInfo {
    pub node_id: NodeId,
    pub srcdoc: Option<String>,
    pub src: Option<String>,
    pub name: Option<String>,
}

/// Internal runtime used only by network-backed frame-tree contexts. Public
/// callers use [`FrameContext`], so no API can create a second srcdoc isolate.
pub(crate) struct ChildIframe {
    pub(crate) node_id: NodeId,
    pub(crate) event_loop: BrowserEventLoop,
}

/// A borrowed handle to the single execution context backing an iframe.
///
/// `SameIsolateRealm` is the genuine V8 child context exposed by
/// `iframe.contentWindow`. `IsolatedRuntime` is used by the frame-tree network
/// backend. Both present one public API, so callers never create or observe a
/// second, duplicate iframe runtime.
pub struct FrameContext<'a> {
    node_id: NodeId,
    backend: FrameContextBackend<'a>,
}

enum FrameContextBackend<'a> {
    SameIsolateRealm {
        event_loop: &'a mut BrowserEventLoop,
        realm_id: u32,
    },
    IsolatedRuntime(&'a mut ChildIframe),
}

impl<'a> FrameContext<'a> {
    pub(crate) fn same_isolate(
        node_id: NodeId,
        event_loop: &'a mut BrowserEventLoop,
        realm_id: u32,
    ) -> Self {
        Self {
            node_id,
            backend: FrameContextBackend::SameIsolateRealm {
                event_loop,
                realm_id,
            },
        }
    }

    pub(crate) fn isolated(child: &'a mut ChildIframe) -> Self {
        Self {
            node_id: child.node_id,
            backend: FrameContextBackend::IsolatedRuntime(child),
        }
    }

    /// DOM host node for this browsing context.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Evaluate JavaScript in the exact context visible to page script.
    pub fn evaluate(&mut self, js: &str) -> Result<String, deno_core::error::AnyError> {
        match &mut self.backend {
            FrameContextBackend::SameIsolateRealm {
                event_loop,
                realm_id,
            } => event_loop.runtime_mut().execute_child_realm_script(
                *realm_id,
                js,
                Some("about:srcdoc"),
            ),
            FrameContextBackend::IsolatedRuntime(child) => child.evaluate(js),
        }
    }

    /// Drive pending work for this frame.
    pub async fn pump(&mut self, timeout: Duration) -> Result<(), deno_core::error::AnyError> {
        match &mut self.backend {
            FrameContextBackend::SameIsolateRealm { event_loop, .. } => {
                let _ = event_loop.run_until_idle(timeout).await;
                Ok(())
            }
            FrameContextBackend::IsolatedRuntime(child) => child.pump(timeout).await,
        }
    }

    /// Execute JavaScript and then drive pending work.
    pub async fn execute_and_run(
        &mut self,
        js: &str,
        timeout: Duration,
    ) -> Result<(), deno_core::error::AnyError> {
        self.evaluate(js)?;
        self.pump(timeout).await
    }

    /// Query text in this frame's document.
    pub fn query_text(&mut self, selector: &str) -> Option<String> {
        self.evaluate(&format!(
            r#"(() => {{ const el = document.querySelector("{}"); return el ? el.textContent : ""; }})()"#,
            selector.replace('"', "\\\"")
        ))
        .ok()
        .filter(|value| !value.is_empty())
    }
}

fn complete_document_lifecycle(event_loop: &mut BrowserEventLoop) {
    // Match the top-level Page lifecycle. Child frames previously executed
    // their scripts but never dispatched DOMContentLoaded/load or advanced
    // document.readyState, leaving real widgets stuck in the loading phase.
    event_loop.complete_document_lifecycle();
}

impl ChildIframe {
    /// Build isolated HTML for the internal frame-tree backend. Browser-visible
    /// srcdoc frames never use this path.
    async fn from_isolated_html(
        node_id: NodeId,
        html: &str,
        profile: &crate::stealth::StealthProfile,
    ) -> Result<Self, deno_core::error::AnyError> {
        let dom = crate::html_parser::parse_html(html);
        let scripts = crate::script_runner::find_scripts(&dom);
        let stylesheet_entries = crate::stylesheet_collector::find_stylesheets(&dom);
        let stylesheets = crate::stylesheet_collector::resolve_inline_only(&stylesheet_entries);

        let runtime = BrowserJsRuntime::with_options(
            dom,
            BrowserRuntimeOptions {
                stealth_profile: Some(profile.clone()),
                stylesheets,
                ..Default::default()
            },
        );
        let mut event_loop = BrowserEventLoop::new(runtime);

        // Execute scripts in the child's own V8 context. W2.7 — Chrome
        // reports `about:srcdoc` for srcdoc iframe stack frames.
        for (i, script) in scripts.iter().enumerate() {
            if script.src.is_some() {
                continue;
            } // Skip external scripts in srcdoc
            if script.code.trim().is_empty() {
                continue;
            }
            event_loop.note_executed_script("about:srcdoc", &script.code);
            if let Err(e) = event_loop.execute_script_with_name(&script.code, "about:srcdoc") {
                tracing::warn!(script_index = i, error = %e, "iframe script error");
            }
        }

        complete_document_lifecycle(&mut event_loop);
        // Mark the child realm's initial load as settled so deferred
        // parent->child messages (postMessage) wait for full initialization
        // instead of landing between this realm's per-script event-loop gaps.
        event_loop
            .execute_script(
                "try{Object.defineProperty(globalThis,'__oxFrameReady',{value:1,writable:true,configurable:true,enumerable:false})}catch(_){try{globalThis.__oxFrameReady=1}catch(_){}}",
            )
            .ok();

        // Bound child-frame startup by the same deployment-configurable
        // navigation deadline as top-level pages. An explicit "off" keeps the
        // unbounded behavior for embedders that deliberately opt into it.
        match crate::page::navigation_timeout() {
            Some(timeout) => event_loop.run_until_settled(timeout).await?,
            None => event_loop.run_until_settled_unbounded().await?,
        };

        Ok(Self {
            node_id,
            event_loop,
        })
    }

    /// Create a child iframe by fetching src URL via HTTP client.
    pub(crate) async fn from_url(
        node_id: NodeId,
        url: &str,
        client: &crate::net::HttpClient,
        stealth_profile: Option<&crate::stealth::StealthProfile>,
        frame_ids: Option<(u32, u32, u32)>,
        name: &str,
        referrer: &str,
        ancestor_origins: &[String],
    ) -> Result<Self, deno_core::error::AnyError> {
        // CSP `frame-src` enforcement (falls back to child-src then
        // default-src). Real Chrome refuses to navigate iframes whose
        // src violates the parent's CSP, surfacing the same network-
        // error shape we return on op_fetch blocks.
        // Skipped when `frame_ids` is set: check_csp reads the top page's CSP,
        // but a nested frame's frame-src belongs to its parent, which we don't track.
        if frame_ids.is_none() {
            if let Ok(parsed_url) = url::Url::parse(url) {
                if let Err(violated) = crate::js_runtime::extensions::fetch_ext::check_csp(
                    crate::net::csp::Directive::FrameSrc,
                    &parsed_url,
                    None,
                    false,
                ) {
                    eprintln!(
                    "[csp] Refused to frame '{}' because it violates the following Content Security Policy directive: \"{}\".",
                    url, violated
                );
                    return Err(deno_core::error::AnyError::msg(format!(
                        "iframe blocked by CSP: {}",
                        url
                    )));
                }
            }
        }

        let resp = client
            .get(url)
            .await
            .map_err(|e| deno_core::error::AnyError::msg(format!("iframe fetch error: {}", e)))?;

        if !resp.ok() {
            return Err(deno_core::error::AnyError::msg(format!(
                "iframe fetch {} returned {}",
                url, resp.status
            )));
        }

        let html = resp.text();
        // Skip if response looks like non-HTML (binary, error page)
        if html.trim().is_empty() {
            return Self::from_isolated_html(
                node_id,
                "<html><body></body></html>",
                stealth_profile.unwrap(),
            )
            .await;
        }

        let dom = crate::html_parser::parse_html(&html);
        let scripts = crate::script_runner::find_scripts(&dom);
        let stylesheet_entries = crate::stylesheet_collector::find_stylesheets(&dom);

        // Fetch external stylesheets
        let mut stylesheets = Vec::new();
        for entry in &stylesheet_entries {
            match entry {
                crate::stylesheet_collector::StylesheetEntry::Inline(css) => {
                    stylesheets.push(css.clone());
                }
                crate::stylesheet_collector::StylesheetEntry::External(href) => {
                    let full_url = if href.starts_with("http") {
                        href.clone()
                    } else if href.starts_with('/') {
                        if let Ok(base) = url::Url::parse(url) {
                            format!(
                                "{}://{}{}",
                                base.scheme(),
                                base.host_str().unwrap_or(""),
                                href
                            )
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };
                    if let Ok(resp) = client.get(&full_url).await {
                        if resp.ok() {
                            let text = resp.text();
                            if !text.trim_start().starts_with("<!") {
                                stylesheets.push(text);
                            }
                        }
                    }
                }
            }
        }

        let mut options = BrowserRuntimeOptions {
            stylesheets,
            is_secure_context: crate::page::is_secure_url(url),
            ..Default::default()
        };
        if let Some(profile) = stealth_profile {
            options.stealth_profile = Some(profile.clone());
        }

        let runtime = BrowserJsRuntime::with_options(dom, options);
        let mut event_loop = BrowserEventLoop::new(runtime);

        // Set location
        let url_js = url.replace('\\', "\\\\").replace('\'', "\\'");
        event_loop
            .execute_script(&format!("location.href = '{}';", url_js))
            .ok();

        // The frame reads `window.name` on init, so set it before its scripts run.
        if !name.is_empty() {
            let name_js = name.replace('\\', "\\\\").replace('\'', "\\'");
            event_loop
                .execute_script(&format!(
                    "try{{globalThis.name='{}';}}catch(_){{}}",
                    name_js
                ))
                .ok();
        }

        // Wire id/parent/top before its scripts run, so a nested iframe it inserts
        // is queued for the frame tree instead of materialized as a child realm.
        if let Some((frame_id, parent_id, top_id)) = frame_ids {
            event_loop
                .execute_script(&format!(
                    "globalThis.__oxFrameSetup && globalThis.__oxFrameSetup({frame_id},{parent_id},{top_id});"
                ))
                .ok();
        }

        // A cross-origin iframe's `document.referrer` and `location.ancestorOrigins`
        // reflect the embedding chain; set them before its scripts run.
        let ref_js = referrer.replace('\\', "\\\\").replace('\'', "\\'");
        let ao_js: String = {
            let items: Vec<String> = ancestor_origins
                .iter()
                .map(|o| format!("'{}'", o.replace('\\', "\\\\").replace('\'', "\\'")))
                .collect();
            format!("[{}]", items.join(","))
        };
        event_loop
            .execute_script(&format!(
                "try{{Object.defineProperty(globalThis,'__frameReferrer',{{value:'{ref_js}',writable:true,configurable:true,enumerable:false}});Object.defineProperty(globalThis,'__frameAncestorOrigins',{{value:{ao_js},writable:true,configurable:true,enumerable:false}});}}catch(_){{}}"
            ))
            .ok();

        // Execute scripts, fetching external ones
        for (i, script) in scripts.iter().enumerate() {
            let code = if let Some(src) = &script.src {
                let full_url = if src.starts_with("http") {
                    src.clone()
                } else if src.starts_with('/') {
                    if let Ok(base) = url::Url::parse(url) {
                        format!(
                            "{}://{}{}",
                            base.scheme(),
                            base.host_str().unwrap_or(""),
                            src
                        )
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };
                match client.get(&full_url).await {
                    Ok(resp) if resp.ok() => {
                        let text = resp.text();
                        if text.trim_start().starts_with("<!") {
                            continue;
                        }
                        text
                    }
                    _ => continue,
                }
            } else {
                script.code.clone()
            };

            if code.trim().is_empty() {
                continue;
            }
            // W2.7 — name scripts by their actual URL (external src or
            // the iframe document URL for inline). Chrome stack frames
            // are URL-tagged, not anonymous.
            let name = if let Some(src) = &script.src {
                src.clone()
            } else {
                url.to_string()
            };
            event_loop.note_executed_script(&name, &code);
            if let Err(e) = event_loop.execute_script_with_name(&code, &name) {
                tracing::warn!(script_index = i, error = %e, "iframe script error");
            }
        }

        complete_document_lifecycle(&mut event_loop);
        // See from_isolated_html: gate parent->child postMessage delivery on
        // completed initial load.
        event_loop
            .execute_script(
                "try{Object.defineProperty(globalThis,'__oxFrameReady',{value:1,writable:true,configurable:true,enumerable:false})}catch(_){try{globalThis.__oxFrameReady=1}catch(_){}}",
            )
            .ok();

        // `run_until_settled` returns once the frame has loaded with no in-flight
        // fetch, ignoring the perpetual short timers a live frame keeps running.
        match crate::page::navigation_timeout() {
            Some(timeout) => event_loop.run_until_settled(timeout).await?,
            None => event_loop.run_until_settled_unbounded().await?,
        };

        Ok(Self {
            node_id,
            event_loop,
        })
    }

    /// Evaluate JS in the child's V8 context.
    pub(crate) fn evaluate(&mut self, js: &str) -> Result<String, deno_core::error::AnyError> {
        self.event_loop.execute_script(js)
    }

    /// Run the child's event loop until idle or the caller's deadline.
    pub(crate) async fn pump(
        &mut self,
        timeout: Duration,
    ) -> Result<(), deno_core::error::AnyError> {
        let _ = self.event_loop.run_until_idle(timeout).await;
        Ok(())
    }
}

/// Find all `<iframe>` elements in the DOM.
pub fn find_iframes(dom: &Dom) -> Vec<IframeInfo> {
    let mut iframes = Vec::new();
    collect_iframes(dom, NodeId::DOCUMENT, &mut iframes);
    iframes
}

fn collect_iframes(dom: &Dom, node_id: NodeId, iframes: &mut Vec<IframeInfo>) {
    let children = dom.children(node_id);
    for child_id in children {
        if let Some(node) = dom.get(child_id) {
            if let NodeData::Element(elem) = &node.data {
                if elem.name.local.eq_ignore_ascii_case("iframe") {
                    let srcdoc = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "srcdoc")
                        .map(|a| a.value.clone());
                    let src = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "src")
                        .map(|a| a.value.clone());
                    let name = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "name")
                        .map(|a| a.value.clone());
                    iframes.push(IframeInfo {
                        node_id: child_id,
                        srcdoc,
                        src,
                        name,
                    });
                }
            }
            collect_iframes(dom, child_id, iframes);
        }
    }
}
