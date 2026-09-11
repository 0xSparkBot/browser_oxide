use crate::protocol::types::*;
use crate::Page;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::SystemTime;

const MAX_CACHED_RESPONSE_BODIES: usize = 32;
const MAX_CACHED_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct PendingNavigation {
    pub url: String,
    pub loader_id: String,
    pub request_id: String,
}

struct CachedResponseBody {
    body: Vec<u8>,
    mime_type: String,
}

/// Per-connection CDP session state.
pub struct CdpSession {
    pub enabled_domains: HashSet<String>,
    pub scripts_on_new_document: Vec<String>,
    pub frame_id: String,
    loader_counter: u64,
    request_counter: u64,
    extra_headers: std::collections::HashMap<String, String>,
    request_interception_enabled: bool,
    /// Set by Page.navigate — the server handles the actual page replacement
    /// because V8 isolates must be dropped in LIFO order.
    pub(crate) pending_navigate: Option<PendingNavigation>,
    response_bodies: HashMap<String, CachedResponseBody>,
    response_body_order: VecDeque<String>,
    response_body_bytes: usize,
    /// Last known mouse coordinates for trajectory generation.
    last_mouse_x: f32,
    last_mouse_y: f32,
    /// Behavioral profile for input humanization.
    behavior: crate::stealth::behavior::BehaviorProfile,
}

impl Default for CdpSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CdpSession {
    pub fn new() -> Self {
        Self {
            enabled_domains: HashSet::new(),
            scripts_on_new_document: Vec::new(),
            frame_id: "main".to_string(),
            loader_counter: 0,
            request_counter: 0,
            extra_headers: std::collections::HashMap::new(),
            request_interception_enabled: false,
            pending_navigate: None,
            response_bodies: HashMap::new(),
            response_body_order: VecDeque::new(),
            response_body_bytes: 0,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            behavior: crate::stealth::behavior::BehaviorProfile::default(),
        }
    }

    pub fn next_request_id(&mut self) -> String {
        self.request_counter += 1;
        format!("{}.1", self.request_counter)
    }

    pub fn next_loader_id(&mut self) -> String {
        self.loader_counter += 1;
        format!("loader-{}", self.loader_counter)
    }

    pub fn is_domain_enabled(&self, domain: &str) -> bool {
        self.enabled_domains.contains(domain)
    }

    pub fn enable_domain(&mut self, domain: &str) {
        self.enabled_domains.insert(domain.to_string());
    }

    pub(crate) fn navigation_extra_headers(&self) -> Vec<(String, String)> {
        self.extra_headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    fn cache_response_body(&mut self, request_id: &str, response: &crate::net::Response) {
        if let Some(previous) = self.response_bodies.remove(request_id) {
            self.response_body_bytes = self.response_body_bytes.saturating_sub(previous.body.len());
            self.response_body_order.retain(|id| id != request_id);
        }

        // Keep the CDP session bounded. Chrome may retain a much larger
        // inspector cache, but silently retaining arbitrary response bodies in
        // this lightweight server would make long-lived automation sessions
        // unbounded. Oversized single bodies remain unavailable by request id.
        if response.body.len() > MAX_CACHED_RESPONSE_BODY_BYTES {
            return;
        }
        while self.response_bodies.len() >= MAX_CACHED_RESPONSE_BODIES
            || self.response_body_bytes + response.body.len() > MAX_CACHED_RESPONSE_BODY_BYTES
        {
            let Some(oldest) = self.response_body_order.pop_front() else {
                break;
            };
            if let Some(old) = self.response_bodies.remove(&oldest) {
                self.response_body_bytes = self.response_body_bytes.saturating_sub(old.body.len());
            }
        }

        let mime_type = response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.split(';').next().unwrap_or(value).trim().to_string())
            .unwrap_or_default();
        self.response_body_bytes += response.body.len();
        self.response_body_order.push_back(request_id.to_string());
        self.response_bodies.insert(
            request_id.to_string(),
            CachedResponseBody {
                body: response.body.clone(),
                mime_type,
            },
        );
    }

    pub(crate) fn record_navigation_response(
        &mut self,
        navigation: &PendingNavigation,
        response: &crate::net::Response,
    ) -> Vec<CdpEvent> {
        self.cache_response_body(&navigation.request_id, response);
        if !self.is_domain_enabled("Network") {
            return Vec::new();
        }

        let mime_type = response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.split(';').next().unwrap_or(value).trim())
            .unwrap_or("");
        vec![
            CdpEvent::new(
                "Network.responseReceived",
                serde_json::json!({
                    "requestId": navigation.request_id,
                    "loaderId": navigation.loader_id,
                    "timestamp": timestamp(),
                    "type": "Document",
                    "frameId": self.frame_id,
                    "hasExtraInfo": false,
                    "response": {
                        "url": response.url,
                        "status": response.status,
                        "statusText": response.status_text,
                        "headers": response.headers,
                        "mimeType": mime_type,
                        "connectionReused": false,
                        "connectionId": 0,
                        "encodedDataLength": response.body.len(),
                        "securityState": if response.url.starts_with("https://") { "secure" } else { "neutral" },
                    }
                }),
            ),
            CdpEvent::new(
                "Network.loadingFinished",
                serde_json::json!({
                    "requestId": navigation.request_id,
                    "timestamp": timestamp(),
                    "encodedDataLength": response.body.len(),
                }),
            ),
        ]
    }

    pub(crate) fn record_navigation_failure(
        &self,
        navigation: &PendingNavigation,
        error_text: &str,
    ) -> Vec<CdpEvent> {
        if !self.is_domain_enabled("Network") {
            return Vec::new();
        }
        vec![CdpEvent::new(
            "Network.loadingFailed",
            serde_json::json!({
                "requestId": navigation.request_id,
                "timestamp": timestamp(),
                "type": "Document",
                "errorText": error_text,
                "canceled": false,
            }),
        )]
    }

    /// Handle a CDP request and return response + events to emit.
    pub async fn handle_request(
        &mut self,
        page: &mut Page,
        req: &CdpRequest,
        http_client: Option<&crate::net::HttpClient>,
    ) -> (String, Vec<CdpEvent>) {
        // Fast path: Runtime.evaluate is the hot path — handle before allocating events Vec
        if req.method == "Runtime.evaluate" {
            let expression = req
                .params
                .get("expression")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            return match page.evaluate(expression) {
                Ok(result_str) => {
                    let ty = js_type(&result_str);
                    let return_by_value = req
                        .params
                        .get("returnByValue")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let resp_json = if return_by_value {
                        let raw_value = match ty {
                            "number" | "boolean" => result_str.clone(),
                            "undefined" => "null".to_string(),
                            _ => {
                                if serde_json::from_str::<serde_json::Value>(&result_str).is_ok() {
                                    result_str.clone()
                                } else {
                                    json_escape_string(&result_str)
                                }
                            }
                        };
                        format!(
                            r#"{{"id":{},"result":{{"result":{{"type":"{}","value":{}}}}}}}"#,
                            req.id, ty, raw_value
                        )
                    } else {
                        format!(
                            r#"{{"id":{},"result":{{"type":"{}","value":{}}}}}"#,
                            req.id,
                            ty,
                            json_escape_string(&result_str)
                        )
                    };
                    (resp_json, Vec::new())
                }
                Err(e) => {
                    let resp = to_json(&serde_json::json!({
                        "id": req.id,
                        "result": {
                            "exceptionDetails": {
                                "text": e.to_string(),
                                "lineNumber": 0,
                                "columnNumber": 0,
                            }
                        }
                    }));
                    (resp, Vec::new())
                }
            };
        }

        let mut events = Vec::new();

        let result: Result<serde_json::Value, String> = match req.method.as_str() {
            // --- Target domain ---
            "Target.getTargets" => Ok(serde_json::json!({
                "targetInfos": [{
                    "targetId": "page-1",
                    "type": "page",
                    "title": page.title(),
                    "url": page.url(),
                    "attached": true,
                }]
            })),

            // --- Page domain ---
            "Page.enable" => {
                self.enable_domain("Page");
                Ok(serde_json::json!({}))
            }
            "Page.navigate" => {
                let url = req
                    .params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("about:blank");
                let loader_id = self.next_loader_id();
                let request_id = if url != "about:blank" && http_client.is_some() {
                    Some(self.next_request_id())
                } else {
                    None
                };

                // Signal the server to handle navigation after we release the page borrow.
                // V8 requires isolates to be dropped in LIFO order, so page replacement
                // must happen at the server level where we control the RefCell.
                if let Some(request_id) = request_id.as_ref() {
                    self.pending_navigate = Some(PendingNavigation {
                        url: url.to_string(),
                        loader_id: loader_id.clone(),
                        request_id: request_id.clone(),
                    });

                    if self.is_domain_enabled("Network") {
                        let now = timestamp();
                        events.push(CdpEvent::new(
                            "Network.requestWillBeSent",
                            serde_json::json!({
                                "requestId": request_id,
                                "loaderId": loader_id,
                                "documentURL": url,
                                "request": {
                                    "url": url,
                                    "method": "GET",
                                    "headers": self.extra_headers,
                                    "initialPriority": "VeryHigh",
                                    "referrerPolicy": "strict-origin-when-cross-origin",
                                },
                                "timestamp": now,
                                "wallTime": now,
                                "initiator": { "type": "other" },
                                "type": "Document",
                                "frameId": self.frame_id,
                                "hasUserGesture": false,
                            }),
                        ));
                    }
                }

                // Emit lifecycle events
                if self.is_domain_enabled("Page") {
                    events.push(CdpEvent::new(
                        "Page.frameNavigated",
                        serde_json::json!({
                            "frame": {
                                "id": self.frame_id,
                                "loaderId": loader_id,
                                "url": url,
                                "securityOrigin": url,
                                "mimeType": "text/html",
                            }
                        }),
                    ));
                    events.push(CdpEvent::new(
                        "Page.domContentEventFired",
                        serde_json::json!({
                            "timestamp": timestamp()
                        }),
                    ));
                    events.push(CdpEvent::new(
                        "Page.loadEventFired",
                        serde_json::json!({
                            "timestamp": timestamp()
                        }),
                    ));
                }

                Ok(serde_json::json!({
                    "frameId": self.frame_id,
                    "loaderId": loader_id,
                }))
            }
            "Page.getFrameTree" => Ok(serde_json::json!({
                "frameTree": {
                    "frame": {
                        "id": self.frame_id,
                        "loaderId": format!("loader-{}", self.loader_counter),
                        "url": page.url(),
                        "securityOrigin": page.url(),
                        "mimeType": "text/html",
                    },
                    "childFrames": []
                }
            })),
            "Page.addScriptToEvaluateOnNewDocument" => {
                let source = req
                    .params
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.scripts_on_new_document.push(source.to_string());
                Ok(serde_json::json!({
                    "identifier": format!("script-{}", self.scripts_on_new_document.len())
                }))
            }
            "Page.setLifecycleEventsEnabled" => Ok(serde_json::json!({})),
            "Page.createIsolatedWorld" => Ok(serde_json::json!({ "executionContextId": 2 })),

            // --- Runtime domain ---
            "Runtime.enable" => {
                self.enable_domain("Runtime");
                events.push(CdpEvent::new("Runtime.executionContextCreated", serde_json::json!({
                    "context": {
                        "id": 1,
                        "origin": page.url(),
                        "name": "",
                        "auxData": { "isDefault": true, "type": "default", "frameId": self.frame_id }
                    }
                })));
                Ok(serde_json::json!({}))
            }
            // Runtime.evaluate handled in fast path above
            "Runtime.callFunctionOn" => {
                let decl = req
                    .params
                    .get("functionDeclaration")
                    .and_then(|v| v.as_str())
                    .unwrap_or("() => undefined");
                let code = format!("({})()", decl);
                match page.evaluate(&code) {
                    Ok(result_str) => Ok(serde_json::json!({
                        "result": { "type": js_type(&result_str), "value": result_str }
                    })),
                    Err(e) => Ok(serde_json::json!({
                        "exceptionDetails": { "text": e.to_string() }
                    })),
                }
            }
            "Runtime.disable" => Ok(serde_json::json!({})),
            "Runtime.runIfWaitingForDebugger" => Ok(serde_json::json!({})),

            // --- DOM domain ---
            "DOM.enable" => {
                self.enable_domain("DOM");
                Ok(serde_json::json!({}))
            }
            "DOM.getDocument" => {
                // `depth` parameter is honored by real Chrome to bound the
                // returned subtree; we currently return a minimal fixed-
                // depth document, so the value is read for spec
                // compatibility but not yet acted on.
                let _depth = req
                    .params
                    .get("depth")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                // Return minimal document structure
                Ok(serde_json::json!({
                    "root": {
                        "nodeId": 1,
                        "backendNodeId": 1,
                        "nodeType": 9,
                        "nodeName": "#document",
                        "localName": "",
                        "nodeValue": "",
                        "childNodeCount": 1,
                        "documentURL": page.url(),
                        "baseURL": page.url(),
                    }
                }))
            }
            "DOM.querySelector" => {
                let selector = req
                    .params
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let has = page.has_element(selector);
                Ok(serde_json::json!({
                    "nodeId": if has { 2 } else { 0 }
                }))
            }
            "DOM.querySelectorAll" => {
                let selector = req
                    .params
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // Count matches via JS
                let count_js = format!(
                    "document.querySelectorAll(\"{}\").length",
                    selector.replace('\\', "\\\\").replace('"', "\\\"")
                );
                let count: usize = page
                    .evaluate(&count_js)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let node_ids: Vec<u32> = (0..count).map(|i| (i + 2) as u32).collect();
                Ok(serde_json::json!({ "nodeIds": node_ids }))
            }
            "DOM.getOuterHTML" => {
                let result = page
                    .evaluate("document.documentElement.outerHTML")
                    .unwrap_or_default();
                Ok(serde_json::json!({ "outerHTML": result }))
            }
            "DOM.disable" => Ok(serde_json::json!({})),

            // --- Network domain ---
            "Network.enable" => {
                self.enable_domain("Network");
                Ok(serde_json::json!({}))
            }
            "Network.getCookies" => Ok(serde_json::json!({ "cookies": [] })),
            "Network.setCookies" => Ok(serde_json::json!({})),
            "Network.disable" => {
                self.enabled_domains.remove("Network");
                Ok(serde_json::json!({}))
            }
            "Network.setExtraHTTPHeaders" => {
                // CDP treats this as a setter for the complete extra-header
                // map, not an incremental merge. Headers omitted from a later
                // call must stop being sent on subsequent requests.
                self.extra_headers.clear();
                if let Some(headers) = req.params.get("headers").and_then(|h| h.as_object()) {
                    for (k, v) in headers {
                        if let Some(val) = v.as_str() {
                            self.extra_headers.insert(k.clone(), val.to_string());
                        }
                    }
                }
                Ok(serde_json::json!({}))
            }
            "Network.setRequestInterception" => {
                self.request_interception_enabled = req
                    .params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(serde_json::json!({}))
            }
            "Network.getResponseBody" => {
                let Some(request_id) = req.params.get("requestId").and_then(|v| v.as_str()) else {
                    let err = CdpError::server(req.id, "Request id is required");
                    return (to_json(&err), events);
                };
                let Some(cached) = self.response_bodies.get(request_id) else {
                    let err = CdpError::server(
                        req.id,
                        &format!("No resource with given identifier found: {request_id}"),
                    );
                    return (to_json(&err), events);
                };

                if is_textual_mime_type(&cached.mime_type) {
                    if let Ok(body) = std::str::from_utf8(&cached.body) {
                        Ok(serde_json::json!({
                            "body": body,
                            "base64Encoded": false,
                        }))
                    } else {
                        use base64::Engine as _;
                        Ok(serde_json::json!({
                            "body": base64::engine::general_purpose::STANDARD.encode(&cached.body),
                            "base64Encoded": true,
                        }))
                    }
                } else {
                    use base64::Engine as _;
                    Ok(serde_json::json!({
                        "body": base64::engine::general_purpose::STANDARD.encode(&cached.body),
                        "base64Encoded": true,
                    }))
                }
            }
            "Network.clearBrowserCache" => Ok(serde_json::json!({})),
            "Network.clearBrowserCookies" => Ok(serde_json::json!({})),
            "Network.setCacheDisabled" => Ok(serde_json::json!({})),
            "Network.emulateNetworkConditions" => Ok(serde_json::json!({})),

            // --- Emulation domain ---
            "Emulation.setDeviceMetricsOverride" => Ok(serde_json::json!({})),
            "Emulation.setUserAgentOverride" => Ok(serde_json::json!({})),
            "Emulation.setTouchEmulationEnabled" => Ok(serde_json::json!({})),

            // --- Browser domain ---
            "Browser.getVersion" => Ok(serde_json::json!({
                "protocolVersion": "1.3",
                "product": "browser_oxide/0.1.0",
                "revision": "0",
                "userAgent": "Mozilla/5.0 browser_oxide/0.1.0",
                "jsVersion": "V8",
            })),

            // --- Log domain ---
            "Log.enable" => Ok(serde_json::json!({})),
            "Log.disable" => Ok(serde_json::json!({})),

            // --- Inspector domain ---
            "Inspector.enable" => Ok(serde_json::json!({})),

            // --- Performance domain ---
            "Performance.enable" => Ok(serde_json::json!({})),

            // --- Security domain ---
            "Security.enable" => Ok(serde_json::json!({})),

            // --- Input domain ---
            //
            // Puppeteer/Playwright drive the page via Input.dispatch* CDP
            // methods. Without these handlers, every user interaction script
            // gets "method not found" — full incompatibility. We translate
            // each CDP call into a JS-side event dispatch via page.evaluate.
            //
            // Mouse-path humanization is offered separately via the JS
            // helper `__browserOxide.humanMousePath` (wired to
            // crate::stealth::behavior::mouse_trajectory). Callers that want
            // humanized input call multiple Input.dispatchMouseEvent
            // moves between actions; CDP itself stays event-faithful.
            "Input.dispatchMouseEvent" => {
                let event_type = req
                    .params
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let x = req.params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = req.params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let button = req
                    .params
                    .get("button")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                let buttons = req
                    .params
                    .get("buttons")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let click_count = req
                    .params
                    .get("clickCount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                let modifiers = req
                    .params
                    .get("modifiers")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                if event_type == "mouseWheel" {
                    let delta_x = req
                        .params
                        .get("deltaX")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let delta_y = req
                        .params
                        .get("deltaY")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let script = format!(
                        "globalThis._browser_oxide.__dispatchTrustedWheelEvent(\
                         {x},{y},{delta_x},{delta_y},{{\
                         ctrlKey:{ctrl},shiftKey:{shift},altKey:{alt},metaKey:{meta}}})",
                        ctrl = (modifiers & 2) != 0,
                        shift = (modifiers & 8) != 0,
                        alt = (modifiers & 1) != 0,
                        meta = (modifiers & 4) != 0,
                    );
                    let _ = page.evaluate(&script);
                    self.last_mouse_x = x;
                    self.last_mouse_y = y;
                }

                let js_event = match event_type {
                    "mousePressed" => "mousedown",
                    "mouseReleased" => "mouseup",
                    "mouseMoved" => "mousemove",
                    _ => "",
                };

                if !js_event.is_empty() {
                    let button_n = match button {
                        "left" => 0,
                        "middle" => 1,
                        "right" => 2,
                        "back" => 3,
                        "forward" => 4,
                        _ => 0,
                    };

                    let dx = x - self.last_mouse_x;
                    let dy = y - self.last_mouse_y;
                    let dist = (dx * dx + dy * dy).sqrt();

                    // For 'mouseMoved' with significant distance, generate human trajectory.
                    if event_type == "mouseMoved" && dist > 10.0 {
                        let pts = crate::stealth::behavior::mouse_trajectory(
                            (self.last_mouse_x, self.last_mouse_y),
                            (x, y),
                            30.0, // assumption: target width is 30px if not specified
                            &self.behavior,
                        );

                        for (i, p) in pts.iter().enumerate() {
                            let script = format!(
                                "(() => {{ \
                                  const props = {{ \
                                    bubbles: true, cancelable: true, composed: true, \
                                    clientX: {x}, clientY: {y}, screenX: {x}, screenY: {y}, \
                                    button: {button_n}, buttons: {buttons}, detail: {click_count}, \
                                    ctrlKey: {ctrl}, shiftKey: {shift}, altKey: {alt}, metaKey: {meta}, \
                                    pointerId: 1, width: 1, height: 1, pressure: {pressure}, \
                                    pointerType: 'mouse', isPrimary: true \
                                  }}; \
                                  const e = new PointerEvent('pointermove', props); \
                                  const m = new MouseEvent('mousemove', props); \
                                  const t = (document.elementFromPoint && document.elementFromPoint({x},{y})) || document.body || document; \
                                  t.dispatchEvent(e); \
                                  t.dispatchEvent(m); \
                                }})()",
                                x = p.x,
                                y = p.y,
                                pressure = if buttons != 0 { 0.5 } else { 0.0 },
                                ctrl = (modifiers & 2) != 0,
                                shift = (modifiers & 8) != 0,
                                alt = (modifiers & 1) != 0,
                                meta = (modifiers & 4) != 0,
                            );
                            let _ = page.evaluate(&script);

                            // Real-time delay between points (8ms sample rate in model)
                            if i < pts.len() - 1 {
                                crate::stealth::stealth_delay(std::time::Duration::from_millis(8))
                                    .await;
                            }
                        }
                    } else {
                        // Single jump for clicks or small moves
                        let pointer_type = match event_type {
                            "mousePressed" => "pointerdown",
                            "mouseReleased" => "pointerup",
                            "mouseMoved" => "pointermove",
                            _ => "pointermove",
                        };
                        let script = format!(
                            "(() => {{ \
                              const props = {{ \
                                bubbles: true, cancelable: true, composed: true, \
                                clientX: {x}, clientY: {y}, screenX: {x}, screenY: {y}, \
                                button: {button_n}, buttons: {buttons}, detail: {click_count}, \
                                ctrlKey: {ctrl}, shiftKey: {shift}, altKey: {alt}, metaKey: {meta}, \
                                pointerId: 1, width: 1, height: 1, pressure: {pressure}, \
                                pointerType: 'mouse', isPrimary: true \
                              }}; \
                              const e = new PointerEvent({pointer_type:?}, props); \
                              const m = new MouseEvent({js_event:?}, props); \
                              const t = (document.elementFromPoint && document.elementFromPoint({x},{y})) || document.body || document; \
                              t.dispatchEvent(e); \
                              t.dispatchEvent(m); \
                            }})()",
                            pressure = if event_type == "mousePressed" || buttons != 0 { 0.5 } else { 0.0 },
                            ctrl = (modifiers & 2) != 0,
                            shift = (modifiers & 8) != 0,
                            alt = (modifiers & 1) != 0,
                            meta = (modifiers & 4) != 0,
                        );
                        let _ = page.evaluate(&script);
                    }

                    self.last_mouse_x = x;
                    self.last_mouse_y = y;
                }
                Ok(serde_json::json!({}))
            }
            "Input.dispatchKeyEvent" => {
                let event_type = req
                    .params
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let key = req.params.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let code = req
                    .params
                    .get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or(key);
                let text = req
                    .params
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let modifiers = req
                    .params
                    .get("modifiers")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let location = req
                    .params
                    .get("location")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let auto_repeat = req
                    .params
                    .get("autoRepeat")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let is_composing = req
                    .params
                    .get("isComposing")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let key_code = req
                    .params
                    .get("windowsVirtualKeyCode")
                    .or_else(|| req.params.get("nativeVirtualKeyCode"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let js_event = match event_type {
                    "keyDown" | "rawKeyDown" => "keydown",
                    "keyUp" => "keyup",
                    "char" => "keypress",
                    _ => "",
                };
                if !js_event.is_empty() {
                    let script = format!(
                        "globalThis._browser_oxide.__dispatchTrustedKeyEvent(\
                         {js_event:?},{{key:{key:?},code:{code:?},text:{text:?},\
                         location:{location},repeat:{auto_repeat},isComposing:{is_composing},\
                         keyCode:{key_code},charCode:{char_code},\
                         ctrlKey:{ctrl},shiftKey:{shift},altKey:{alt},metaKey:{meta}}})",
                        char_code = if event_type == "char" { key_code } else { 0 },
                        ctrl = (modifiers & 2) != 0,
                        shift = (modifiers & 8) != 0,
                        alt = (modifiers & 1) != 0,
                        meta = (modifiers & 4) != 0,
                    );
                    let _ = page.evaluate(&script);
                }
                Ok(serde_json::json!({}))
            }
            "Input.dispatchTouchEvent" => {
                let event_type = req
                    .params
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let dom_type = match event_type {
                    "touchStart" => "touchstart",
                    "touchMove" => "touchmove",
                    "touchEnd" => "touchend",
                    "touchCancel" => "touchcancel",
                    _ => "",
                };
                if !dom_type.is_empty() {
                    let points = req
                        .params
                        .get("touchPoints")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    let points_json =
                        serde_json::to_string(&points).unwrap_or_else(|_| "[]".into());
                    let modifiers = req
                        .params
                        .get("modifiers")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let script = format!(
                        "globalThis._browser_oxide.__dispatchTrustedTouchEvent(\
                         {dom_type:?},{points_json},{{\
                         ctrlKey:{ctrl},shiftKey:{shift},altKey:{alt},metaKey:{meta}}})",
                        ctrl = (modifiers & 2) != 0,
                        shift = (modifiers & 8) != 0,
                        alt = (modifiers & 1) != 0,
                        meta = (modifiers & 4) != 0,
                    );
                    let _ = page.evaluate(&script);
                }
                Ok(serde_json::json!({}))
            }
            "Input.insertText" => {
                let text = req
                    .params
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !text.is_empty() {
                    let script = format!("globalThis._browser_oxide.__insertTrustedText({text:?})");
                    let _ = page.evaluate(&script);
                }
                Ok(serde_json::json!({}))
            }
            "Input.setIgnoreInputEvents" => Ok(serde_json::json!({})),

            // Unknown method
            _ => {
                let err = CdpError::method_not_found(req.id, &req.method);
                return (to_json(&err), events);
            }
        };

        match result {
            Ok(value) => (to_json(&CdpResponse::ok(req.id, value)), events),
            Err(msg) => (to_json(&CdpError::internal(req.id, &msg)), events),
        }
    }
}

/// Fast JSON string escaping — avoids serde_json::to_string overhead.
pub fn json_escape_string(s: &str) -> String {
    // Fast path: if no special chars, just quote it
    if s.bytes().all(|b| b > 31 && b != b'"' && b != b'\\') {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        out.push_str(s);
        out.push('"');
        return out;
    }
    // Slow path: escape special characters
    let mut out = String::with_capacity(s.len() + 8);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn timestamp() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn is_textual_mime_type(mime_type: &str) -> bool {
    let mime = mime_type.to_ascii_lowercase();
    mime.starts_with("text/")
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/javascript"
                | "application/x-javascript"
                | "application/xml"
                | "application/xhtml+xml"
                | "application/wasm-text"
                | "image/svg+xml"
        )
}

fn js_type(value: &str) -> &'static str {
    match value {
        "undefined" => "undefined",
        "null" => "object",
        "true" | "false" => "boolean",
        s if s.parse::<f64>().is_ok() => "number",
        _ => "string",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_body(body: Vec<u8>, content_type: &str) -> crate::net::Response {
        let mut headers = std::collections::HashMap::new();
        headers.insert("content-type".to_string(), content_type.to_string());
        crate::net::Response {
            status: 200,
            status_text: "OK".to_string(),
            headers,
            set_cookies: Vec::new(),
            body,
            url: "https://example.test/resource".to_string(),
            accept_ch_upgrade: false,
            timings: crate::net::TimingStats::default(),
        }
    }

    #[tokio::test]
    async fn handle_page_enable() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 1,
            method: "Page.enable".to_string(),
            params: serde_json::Value::Null,
        };
        let (resp, _events) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("\"id\":1"));
        assert!(session.is_domain_enabled("Page"));
    }

    #[tokio::test]
    async fn handle_runtime_evaluate() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 2,
            method: "Runtime.evaluate".to_string(),
            params: serde_json::json!({"expression": "1 + 2"}),
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("3"), "response: {}", resp);
    }

    #[tokio::test]
    async fn handle_runtime_enable_emits_context() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 3,
            method: "Runtime.enable".to_string(),
            params: serde_json::Value::Null,
        };
        let (_, events) = session.handle_request(&mut page, &req, None).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].method, "Runtime.executionContextCreated");
    }

    #[tokio::test]
    async fn input_mouse_wheel_dispatches_trusted_wheel_and_scrolls() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body style="margin:0;width:5000px;height:5000px">
              <div id="target" style="width:200px;height:200px"></div>
              <script>
                globalThis.__wheel = null;
                globalThis.__scroll = null;
                document.addEventListener('wheel', event => {
                  globalThis.__wheel = {
                    dx: event.deltaX,
                    dy: event.deltaY,
                    mode: event.deltaMode,
                    x: event.clientX,
                    y: event.clientY,
                    shift: event.shiftKey,
                    trusted: event.isTrusted,
                  };
                });
                document.addEventListener('scroll', event => {
                  globalThis.__scroll = { trusted: event.isTrusted };
                });
              </script>
            </body></html>"#,
            None,
        )
        .await
        .unwrap();
        let req = CdpRequest {
            id: 30,
            method: "Input.dispatchMouseEvent".to_string(),
            params: serde_json::json!({
                "type": "mouseWheel",
                "x": 12,
                "y": 34,
                "deltaX": 5,
                "deltaY": 120,
                "modifiers": 8,
            }),
        };

        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("\"id\":30"), "response: {resp}");
        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__wheel)").unwrap(),
            r#"{"dx":5,"dy":120,"mode":0,"x":12,"y":34,"shift":true,"trusted":true}"#
        );
        assert_eq!(page.evaluate("String(window.scrollX)").unwrap(), "5");
        assert_eq!(page.evaluate("String(window.scrollY)").unwrap(), "120");
        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__scroll)")
                .unwrap(),
            r#"{"trusted":true}"#
        );
    }

    #[tokio::test]
    async fn input_mouse_wheel_prevent_default_blocks_scroll() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body><script>
              globalThis.__wheelTrusted = null;
              document.addEventListener('wheel', event => {
                globalThis.__wheelTrusted = event.isTrusted;
                event.preventDefault();
              });
            </script></body></html>"#,
            None,
        )
        .await
        .unwrap();
        let req = CdpRequest {
            id: 31,
            method: "Input.dispatchMouseEvent".to_string(),
            params: serde_json::json!({
                "type": "mouseWheel",
                "x": 1,
                "y": 1,
                "deltaX": 0,
                "deltaY": 80,
            }),
        };

        let _ = session.handle_request(&mut page, &req, None).await;
        assert_eq!(
            page.evaluate("String(globalThis.__wheelTrusted)").unwrap(),
            "true"
        );
        assert_eq!(page.evaluate("String(window.scrollY)").unwrap(), "0");
    }

    #[tokio::test]
    async fn input_touch_events_preserve_points_and_trust() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body style="margin:0">
              <div id="target" style="width:300px;height:300px"></div>
              <script>
                globalThis.__touchEvents = [];
                for (const type of ['touchstart', 'touchmove', 'touchend']) {
                  document.addEventListener(type, event => {
                    const point = list => list.length ? {
                      id: list[0].identifier,
                      x: list[0].clientX,
                      y: list[0].clientY,
                      force: list[0].force,
                      target: list[0].target && list[0].target.id,
                    } : null;
                    globalThis.__touchEvents.push({
                      type: event.type,
                      trusted: event.isTrusted,
                      shift: event.shiftKey,
                      touches: event.touches.length,
                      changed: event.changedTouches.length,
                      touch: point(event.touches),
                      changedTouch: point(event.changedTouches),
                    });
                  });
                }
              </script>
            </body></html>"#,
            None,
        )
        .await
        .unwrap();

        for (id, event_type, points) in [
            (
                32,
                "touchStart",
                serde_json::json!([{"id":7,"x":10,"y":20,"radiusX":4,"radiusY":5,"force":0.5}]),
            ),
            (
                33,
                "touchMove",
                serde_json::json!([{"id":7,"x":15,"y":25,"radiusX":4,"radiusY":5,"force":0.75}]),
            ),
            (34, "touchEnd", serde_json::json!([])),
        ] {
            let req = CdpRequest {
                id,
                method: "Input.dispatchTouchEvent".to_string(),
                params: serde_json::json!({
                    "type": event_type,
                    "touchPoints": points,
                    "modifiers": 8,
                }),
            };
            let (resp, _) = session.handle_request(&mut page, &req, None).await;
            assert!(resp.contains(&format!("\"id\":{id}")), "response: {resp}");
        }

        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__touchEvents)")
                .unwrap(),
            r#"[{"type":"touchstart","trusted":true,"shift":true,"touches":1,"changed":1,"touch":{"id":7,"x":10,"y":20,"force":0.5,"target":"target"},"changedTouch":{"id":7,"x":10,"y":20,"force":0.5,"target":"target"}},{"type":"touchmove","trusted":true,"shift":true,"touches":1,"changed":1,"touch":{"id":7,"x":15,"y":25,"force":0.75,"target":"target"},"changedTouch":{"id":7,"x":15,"y":25,"force":0.75,"target":"target"}},{"type":"touchend","trusted":true,"shift":true,"touches":0,"changed":1,"touch":null,"changedTouch":{"id":7,"x":15,"y":25,"force":0.75,"target":"target"}}]"#
        );
    }

    #[tokio::test]
    async fn input_touch_cancel_uses_last_active_touch_as_changed_touch() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body><script>
              globalThis.__cancel = null;
              document.addEventListener('touchcancel', event => {
                globalThis.__cancel = {
                  trusted: event.isTrusted,
                  touches: event.touches.length,
                  changed: event.changedTouches.length,
                  id: event.changedTouches.length ? event.changedTouches[0].identifier : -1,
                };
              });
            </script></body></html>"#,
            None,
        )
        .await
        .unwrap();
        let start = CdpRequest {
            id: 35,
            method: "Input.dispatchTouchEvent".to_string(),
            params: serde_json::json!({
                "type": "touchStart",
                "touchPoints": [{"id":11,"x":1,"y":1}],
            }),
        };
        let cancel = CdpRequest {
            id: 36,
            method: "Input.dispatchTouchEvent".to_string(),
            params: serde_json::json!({"type": "touchCancel", "touchPoints": []}),
        };
        let _ = session.handle_request(&mut page, &start, None).await;
        let _ = session.handle_request(&mut page, &cancel, None).await;

        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__cancel)")
                .unwrap(),
            r#"{"trusted":true,"touches":0,"changed":1,"id":11}"#
        );
    }

    #[tokio::test]
    async fn input_key_events_target_focused_element_and_are_trusted() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body>
              <input id="target" value="abcd">
              <script>
                globalThis.__keys = [];
                const target = document.getElementById('target');
                target.focus();
                for (const type of ['keydown', 'keyup']) {
                  target.addEventListener(type, event => {
                    globalThis.__keys.push({
                      type: event.type,
                      trusted: event.isTrusted,
                      key: event.key,
                      code: event.code,
                      location: event.location,
                      repeat: event.repeat,
                      shift: event.shiftKey,
                    });
                  });
                }
              </script>
            </body></html>"#,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            page.evaluate("document.activeElement.id").unwrap(),
            "target"
        );
        for (id, event_type) in [(37, "keyDown"), (38, "keyUp")] {
            let req = CdpRequest {
                id,
                method: "Input.dispatchKeyEvent".to_string(),
                params: serde_json::json!({
                    "type": event_type,
                    "key": "A",
                    "code": "KeyA",
                    "location": 1,
                    "autoRepeat": event_type == "keyDown",
                    "windowsVirtualKeyCode": 65,
                    "modifiers": 8,
                }),
            };
            let _ = session.handle_request(&mut page, &req, None).await;
        }

        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__keys)").unwrap(),
            r#"[{"type":"keydown","trusted":true,"key":"A","code":"KeyA","location":1,"repeat":true,"shift":true},{"type":"keyup","trusted":true,"key":"A","code":"KeyA","location":1,"repeat":false,"shift":true}]"#
        );
    }

    #[tokio::test]
    async fn input_char_replaces_selection_and_emits_trusted_input_events() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body>
              <input id="target" value="abcd">
              <script>
                globalThis.__charEvents = [];
                const target = document.getElementById('target');
                target.focus();
                target.setSelectionRange(1, 3);
                for (const type of ['keypress', 'beforeinput', 'input']) {
                  target.addEventListener(type, event => {
                    globalThis.__charEvents.push({
                      type: event.type,
                      trusted: event.isTrusted,
                      data: event.data === undefined ? null : event.data,
                      inputType: event.inputType === undefined ? null : event.inputType,
                    });
                  });
                }
              </script>
            </body></html>"#,
            None,
        )
        .await
        .unwrap();
        let req = CdpRequest {
            id: 39,
            method: "Input.dispatchKeyEvent".to_string(),
            params: serde_json::json!({
                "type": "char",
                "key": "X",
                "code": "KeyX",
                "text": "X",
                "windowsVirtualKeyCode": 88,
            }),
        };

        let _ = session.handle_request(&mut page, &req, None).await;
        assert_eq!(
            page.evaluate("document.getElementById('target').value")
                .unwrap(),
            "aXd"
        );
        assert_eq!(
            page.evaluate("String(document.getElementById('target').selectionStart)")
                .unwrap(),
            "2"
        );
        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__charEvents)")
                .unwrap(),
            r#"[{"type":"keypress","trusted":true,"data":null,"inputType":null},{"type":"beforeinput","trusted":true,"data":"X","inputType":"insertText"},{"type":"input","trusted":true,"data":"X","inputType":"insertText"}]"#
        );
    }

    #[tokio::test]
    async fn input_insert_text_edits_selection_without_synthetic_key_events() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body>
              <input id="target" value="abcd">
              <script>
                globalThis.__insertEvents = [];
                const target = document.getElementById('target');
                target.focus();
                target.setSelectionRange(1, 3);
                for (const type of ['keydown', 'keyup', 'keypress', 'beforeinput', 'input']) {
                  target.addEventListener(type, event => {
                    globalThis.__insertEvents.push({
                      type: event.type,
                      trusted: event.isTrusted,
                      data: event.data === undefined ? null : event.data,
                      inputType: event.inputType === undefined ? null : event.inputType,
                    });
                  });
                }
              </script>
            </body></html>"#,
            None,
        )
        .await
        .unwrap();
        let req = CdpRequest {
            id: 40,
            method: "Input.insertText".to_string(),
            params: serde_json::json!({"text": "XY"}),
        };

        let _ = session.handle_request(&mut page, &req, None).await;
        assert_eq!(
            page.evaluate("document.getElementById('target').value")
                .unwrap(),
            "aXYd"
        );
        assert_eq!(
            page.evaluate("String(document.getElementById('target').selectionStart)")
                .unwrap(),
            "3"
        );
        assert_eq!(
            page.evaluate("JSON.stringify(globalThis.__insertEvents)")
                .unwrap(),
            r#"[{"type":"beforeinput","trusted":true,"data":"XY","inputType":"insertText"},{"type":"input","trusted":true,"data":"XY","inputType":"insertText"}]"#
        );
    }

    #[tokio::test]
    async fn input_insert_text_respects_beforeinput_prevent_default() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            r#"<html><body><input id="target" value="abcd"><script>
              const target = document.getElementById('target');
              target.focus();
              target.setSelectionRange(1, 3);
              target.addEventListener('beforeinput', event => event.preventDefault());
            </script></body></html>"#,
            None,
        )
        .await
        .unwrap();
        let req = CdpRequest {
            id: 41,
            method: "Input.insertText".to_string(),
            params: serde_json::json!({"text": "XY"}),
        };

        let _ = session.handle_request(&mut page, &req, None).await;
        assert_eq!(
            page.evaluate("document.getElementById('target').value")
                .unwrap(),
            "abcd"
        );
        assert_eq!(
            page.evaluate("String(document.getElementById('target').selectionStart)")
                .unwrap(),
            "1"
        );
    }

    #[tokio::test]
    async fn handle_page_navigate() {
        let mut session = CdpSession::new();
        session.enable_domain("Page");
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 4,
            method: "Page.navigate".to_string(),
            params: serde_json::json!({"url": "about:blank"}),
        };
        let (resp, events) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("frameId"));
        assert!(resp.contains("loaderId"));
        // Should emit frameNavigated + domContentEventFired + loadEventFired
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].method, "Page.frameNavigated");
        assert_eq!(events[1].method, "Page.domContentEventFired");
        assert_eq!(events[2].method, "Page.loadEventFired");
    }

    #[tokio::test]
    async fn network_page_navigate_emits_request_id_for_real_navigation() {
        let mut session = CdpSession::new();
        session.enable_domain("Page");
        session.enable_domain("Network");
        let mut page = Page::from_html("<html><body></body></html>", None)
            .await
            .unwrap();
        let profile = crate::stealth::presets::chrome_148_macos();
        let client = crate::net::HttpClient::new(&profile).unwrap();
        let req = CdpRequest {
            id: 40,
            method: "Page.navigate".to_string(),
            params: serde_json::json!({"url": "https://example.test/document"}),
        };

        let (_, events) = session.handle_request(&mut page, &req, Some(&client)).await;
        assert_eq!(events[0].method, "Network.requestWillBeSent");
        assert_eq!(events[0].params["type"], "Document");
        assert_eq!(events[0].params["request"]["method"], "GET");
        let request_id = events[0].params["requestId"].as_str().unwrap();
        let pending = session.pending_navigate.as_ref().unwrap();
        assert_eq!(pending.request_id, request_id);
        assert_eq!(pending.loader_id, events[0].params["loaderId"]);
        assert_eq!(pending.url, "https://example.test/document");
        assert_eq!(events[1].method, "Page.frameNavigated");
    }

    #[tokio::test]
    async fn network_get_response_body_returns_text_or_base64() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><body></body></html>", None)
            .await
            .unwrap();

        session.cache_response_body(
            "text.1",
            &response_with_body(b"hello response".to_vec(), "text/plain; charset=utf-8"),
        );
        let text_req = CdpRequest {
            id: 41,
            method: "Network.getResponseBody".to_string(),
            params: serde_json::json!({"requestId": "text.1"}),
        };
        let (text_resp, _) = session.handle_request(&mut page, &text_req, None).await;
        let text_json: serde_json::Value = serde_json::from_str(&text_resp).unwrap();
        assert_eq!(text_json["result"]["body"], "hello response");
        assert_eq!(text_json["result"]["base64Encoded"], false);

        session.cache_response_body(
            "binary.1",
            &response_with_body(vec![0, 159, 146, 150, 255], "application/octet-stream"),
        );
        let binary_req = CdpRequest {
            id: 42,
            method: "Network.getResponseBody".to_string(),
            params: serde_json::json!({"requestId": "binary.1"}),
        };
        let (binary_resp, _) = session.handle_request(&mut page, &binary_req, None).await;
        let binary_json: serde_json::Value = serde_json::from_str(&binary_resp).unwrap();
        assert_eq!(binary_json["result"]["body"], "AJ+Slv8=");
        assert_eq!(binary_json["result"]["base64Encoded"], true);
    }

    #[tokio::test]
    async fn network_get_response_body_unknown_request_matches_cdp_server_error() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 43,
            method: "Network.getResponseBody".to_string(),
            params: serde_json::json!({"requestId": "missing.1"}),
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        let json: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32000);
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("No resource with given identifier found"));
    }

    #[tokio::test]
    async fn handle_dom_get_document() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 5,
            method: "DOM.getDocument".to_string(),
            params: serde_json::json!({}),
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("\"nodeType\":9"));
        assert!(resp.contains("#document"));
    }

    #[tokio::test]
    async fn handle_dom_query_selector() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html(
            "<html><head></head><body><div id='test'></div></body></html>",
            None,
        )
        .await
        .unwrap();
        let req = CdpRequest {
            id: 6,
            method: "DOM.querySelector".to_string(),
            params: serde_json::json!({"nodeId": 1, "selector": "#test"}),
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("\"nodeId\":2"), "response: {}", resp); // found
    }

    #[tokio::test]
    async fn handle_unknown_method() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 99,
            method: "Unknown.method".to_string(),
            params: serde_json::Value::Null,
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("-32601"), "response: {}", resp);
        assert!(resp.contains("Unknown.method"));
    }

    #[tokio::test]
    async fn handle_browser_get_version() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 7,
            method: "Browser.getVersion".to_string(),
            params: serde_json::Value::Null,
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("browser_oxide"));
    }

    #[tokio::test]
    async fn handle_add_script_on_new_document() {
        let mut session = CdpSession::new();
        let mut page = Page::from_html("<html><head></head><body></body></html>", None)
            .await
            .unwrap();
        let req = CdpRequest {
            id: 8,
            method: "Page.addScriptToEvaluateOnNewDocument".to_string(),
            params: serde_json::json!({"source": "window.__test = true;"}),
        };
        let (resp, _) = session.handle_request(&mut page, &req, None).await;
        assert!(resp.contains("identifier"));
        assert_eq!(session.scripts_on_new_document.len(), 1);
    }
}
