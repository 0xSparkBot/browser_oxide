//! Server-Sent Events (SSE) support for EventSource API.
//!
//! Connects to SSE endpoints, parses the `text/event-stream` format,
//! and delivers events to JavaScript via async ops.

use deno_core::op2;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

lazy_static::lazy_static! {
    static ref SSE_CONNECTIONS: Arc<Mutex<SseStore>> = Arc::new(Mutex::new(SseStore::new()));
}

struct SseStore {
    receivers: HashMap<i32, Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<SseEvent>>>>,
    next_id: i32,
}

impl SseStore {
    fn new() -> Self {
        Self {
            receivers: HashMap::new(),
            next_id: 1,
        }
    }
}

/// SSE connection state (empty — connections stored globally).
pub struct SseState;

impl Default for SseState {
    fn default() -> Self {
        Self::new()
    }
}

impl SseState {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: String,
    /// Empty string = still open, "error" = connection error, "closed" = done
    pub status: String,
    pub retry_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct SseConnectResult {
    pub id: i32,
    pub ok: bool,
    pub error: String,
}

/// Connect to an SSE endpoint. Returns a connection ID.
#[op2(async(lazy), fast)]
#[serde]
pub async fn op_sse_connect(
    #[string] url: String,
    #[string] origin: String,
    with_credentials: bool,
    #[string] last_event_id: String,
) -> Result<SseConnectResult, deno_error::JsErrorBox> {
    let client = crate::js_runtime::extensions::fetch_ext::fetch_client().unwrap_or_else(|| {
        crate::net::HttpClient::new(&crate::stealth::chrome_148_linux())
            .expect("fallback SSE HTTP client")
    });
    let mut headers = vec![
        ("accept".to_string(), "text/event-stream".to_string()),
        ("cache-control".to_string(), "no-cache".to_string()),
        // Keep the stream byte-oriented. Incremental content decoding can be
        // added generically later; identity avoids buffering a compressed SSE
        // response before event dispatch.
        ("accept-encoding".to_string(), "identity".to_string()),
    ];
    if !last_event_id.is_empty() {
        headers.push(("last-event-id".to_string(), last_event_id));
    }
    let request_origin = (!origin.is_empty()).then_some(origin.as_str());
    let mut response = match client
        .fetch_get_stream(&url, &headers, request_origin, with_credentials)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return Ok(SseConnectResult {
                id: -1,
                ok: false,
                error: format!("SSE fetch failed: {error}"),
            });
        }
    };
    let content_type = response
        .headers
        .get("content-type")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if response.status != 200 || !content_type.starts_with("text/event-stream") {
        return Ok(SseConnectResult {
            id: -1,
            ok: false,
            error: format!(
                "EventSource response must be 200 text/event-stream (status={}, content-type={content_type:?})",
                response.status
            ),
        });
    }
    let is_cross_origin = match (url::Url::parse(&url), url::Url::parse(&origin)) {
        (Ok(target), Ok(source)) => target.origin() != source.origin(),
        _ => !origin.is_empty(),
    };
    if is_cross_origin {
        let allow_origin = response
            .headers
            .get("access-control-allow-origin")
            .map(String::as_str)
            .unwrap_or("");
        let origin_allowed = allow_origin == origin || (allow_origin == "*" && !with_credentials);
        let credentials_allowed = !with_credentials
            || response
                .headers
                .get("access-control-allow-credentials")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        if !origin_allowed || !credentials_allowed {
            return Ok(SseConnectResult {
                id: -1,
                ok: false,
                error: "EventSource response failed CORS validation".to_string(),
            });
        }
    }

    let (tx, rx) = mpsc::unbounded_channel::<SseEvent>();
    let id = {
        let mut store = SSE_CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        let id = store.next_id;
        store.next_id += 1;
        store
            .receivers
            .insert(id, Arc::new(tokio::sync::Mutex::new(rx)));
        id
    };

    tokio::spawn(async move {
        let mut parser = SseParser::default();
        while let Some(chunk) = response.body.recv().await {
            match chunk {
                Ok(bytes) => {
                    if !parser.feed(&bytes, &tx) {
                        return;
                    }
                }
                Err(error) => {
                    let _ = tx.send(SseEvent {
                        event: "error".to_string(),
                        data: error,
                        id: parser.last_id.clone(),
                        status: "error".to_string(),
                        retry_ms: None,
                    });
                    return;
                }
            }
        }
        parser.finish(&tx);
        let _ = tx.send(SseEvent {
            event: String::new(),
            data: String::new(),
            id: parser.last_id,
            status: "closed".to_string(),
            retry_ms: None,
        });
    });

    Ok(SseConnectResult {
        id,
        ok: true,
        error: String::new(),
    })
}

/// Read the next SSE event from a connection.
#[op2(async(lazy), fast)]
#[serde]
pub async fn op_sse_recv(#[smi] id: i32) -> Result<SseEvent, deno_error::JsErrorBox> {
    let rx = {
        let store = SSE_CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        store.receivers.get(&id).cloned()
    };
    match rx {
        Some(rx) => {
            let mut rx = rx.lock().await;
            match rx.recv().await {
                Some(event) => Ok(event),
                None => Ok(SseEvent {
                    event: String::new(),
                    data: String::new(),
                    id: String::new(),
                    status: "closed".to_string(),
                    retry_ms: None,
                }),
            }
        }
        None => Ok(SseEvent {
            event: String::new(),
            data: String::new(),
            id: String::new(),
            status: "closed".to_string(),
            retry_ms: None,
        }),
    }
}

/// Close an SSE connection.
#[op2(fast)]
pub fn op_sse_close(#[smi] id: i32) {
    let mut store = SSE_CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    store.receivers.remove(&id);
}

struct SseParser {
    line: Vec<u8>,
    skip_lf: bool,
    first_line: bool,
    event_type: String,
    data_buf: String,
    last_id: String,
}

impl Default for SseParser {
    fn default() -> Self {
        Self {
            line: Vec::new(),
            skip_lf: false,
            first_line: true,
            event_type: String::new(),
            data_buf: String::new(),
            last_id: String::new(),
        }
    }
}

impl SseParser {
    fn feed(&mut self, bytes: &[u8], tx: &mpsc::UnboundedSender<SseEvent>) -> bool {
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    if !self.process_line(tx) {
                        return false;
                    }
                    self.skip_lf = true;
                }
                b'\n' => {
                    if !self.process_line(tx) {
                        return false;
                    }
                }
                _ => self.line.push(byte),
            }
        }
        true
    }

    fn finish(&mut self, tx: &mpsc::UnboundedSender<SseEvent>) {
        if !self.line.is_empty() {
            let _ = self.process_line(tx);
        }
    }

    fn process_line(&mut self, tx: &mpsc::UnboundedSender<SseEvent>) -> bool {
        let bytes = std::mem::take(&mut self.line);
        let mut line = String::from_utf8_lossy(&bytes).into_owned();
        if self.first_line {
            line = line.trim_start_matches('\u{feff}').to_string();
            self.first_line = false;
        }
        if line.is_empty() {
            if self.data_buf.is_empty() {
                self.event_type.clear();
                return true;
            }
            if self.data_buf.ends_with('\n') {
                self.data_buf.pop();
            }
            let event = SseEvent {
                event: if self.event_type.is_empty() {
                    "message".to_string()
                } else {
                    std::mem::take(&mut self.event_type)
                },
                data: std::mem::take(&mut self.data_buf),
                id: self.last_id.clone(),
                status: String::new(),
                retry_ms: None,
            };
            return tx.send(event).is_ok();
        }
        if line.starts_with(':') {
            return true;
        }
        let (field, value) = if let Some(colon) = line.find(':') {
            let value = line[colon + 1..]
                .strip_prefix(' ')
                .unwrap_or(&line[colon + 1..]);
            (&line[..colon], value)
        } else {
            (line.as_str(), "")
        };
        match field {
            "event" => self.event_type = value.to_string(),
            "data" => {
                self.data_buf.push_str(value);
                self.data_buf.push('\n');
            }
            "id" if !value.contains('\0') => self.last_id = value.to_string(),
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                if let Ok(retry_ms) = value.parse::<u64>() {
                    return tx
                        .send(SseEvent {
                            event: String::new(),
                            data: String::new(),
                            id: self.last_id.clone(),
                            status: "retry".to_string(),
                            retry_ms: Some(retry_ms),
                        })
                        .is_ok();
                }
            }
            _ => {}
        }
        true
    }
}

deno_core::extension!(
    sse_extension,
    ops = [op_sse_connect, op_sse_recv, op_sse_close],
);
