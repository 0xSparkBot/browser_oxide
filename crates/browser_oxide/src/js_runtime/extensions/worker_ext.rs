//! Web Worker implementation with real thread-based V8 isolates.
//!
//! Each `new Worker(url)` in JS spawns an OS thread that owns its own
//! `JsRuntime` built via `create_worker_runtime`. Messages cross the thread
//! boundary through `std::sync::mpsc` channels: parent↔worker uses two
//! unidirectional channels, one each way.
//!
//! Also hosts a process-global BlobRegistry so that `URL.createObjectURL(blob)`
//! produces a blob: URL whose source text can be resolved when a worker is
//! spawned from it (a common pattern: scripts build an inline worker from a
//! blob: URL via `URL.createObjectURL`).

use crate::js_runtime::extensions::stealth_ext::StealthState;
use crate::js_runtime::state::DomState;
use deno_core::{op2, v8, OpState};
use futures_util::task::AtomicWaker;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::task::Waker;
use tokio::sync::Notify;

fn trace_worker_wire(direction: &str, worker_id: u32, data: &str) {
    if std::env::var_os("BROWSER_OXIDE_WORKER_WIRE_TRACE").is_none() {
        return;
    }
    let parsed = serde_json::from_str::<serde_json::Value>(data).ok();
    let keys = parsed
        .as_ref()
        .and_then(|value| value.as_object())
        .map(|map| map.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let (inner_len, inner_keys) = parsed
        .as_ref()
        .and_then(|value| value.get("data"))
        .map(|inner| {
            let len = serde_json::to_string(inner)
                .map(|text| text.len())
                .unwrap_or(0);
            let keys = inner
                .as_object()
                .map(|map| map.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            (len, keys)
        })
        .unwrap_or_default();
    eprintln!(
        "[worker-wire] dir={direction} id={worker_id} len={} keys={} inner_len={inner_len} inner_keys={}",
        data.len(),
        keys.join(","),
        inner_keys.join(",")
    );
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerContextState {
    pub storage_directory_allowed: bool,
}

/// Per-owner-runtime wake channel for worker -> owner messages.
///
/// The JS-side receive promise is deliberately unref'ed after worker startup
/// so an idle Worker does not pin navigation forever. Chromium still queues a
/// MessageEvent immediately when that worker posts, so an actual message needs
/// a wake path that is independent of the ref-state of the receive op.
#[derive(Clone)]
pub struct WorkerOwnerWake {
    notify: Arc<Notify>,
    task_waker: Arc<AtomicWaker>,
    generation: Arc<AtomicU64>,
}

impl Default for WorkerOwnerWake {
    fn default() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            task_waker: Arc::new(AtomicWaker::new()),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl WorkerOwnerWake {
    pub fn register(&self, waker: &Waker) {
        self.task_waker.register(waker);
    }

    pub fn notify_handle(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    /// Monotonic owner-wake generation. `BrowserEventLoop` snapshots this
    /// before driving V8 and re-checks it before accepting an idle result, so
    /// a worker/MessagePort wake that races with deno_core's idle transition
    /// cannot be lost merely because both futures became ready in one poll.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn wake(&self) {
        // `Notify` stores a permit for BrowserEventLoop even when it is between
        // waits. AtomicWaker targets the persistent FrameWaker registered by
        // the frame-tree driver's BrowserJsRuntime::poll_once.
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.notify.notify_one();
        self.task_waker.wake();
    }
}

// ============================================================================
// Worker diagnostics — page-readable record of every worker spawn and
// worker-realm notes. Challenge scripts crash inside their blob workers with
// stacks like `<anonymous>:1:19417` whose source exists nowhere in the
// page's DOM; these records are the only way to reconstruct the failure.
// ============================================================================

const WORKER_SOURCE_CAP: usize = 512 * 1024;
const WORKER_DIAG_CAP: usize = 48;
const WORKER_DIAG_NOTE_CAP: usize = 4096;

fn worker_spawn_log() -> &'static Mutex<Vec<(String, String)>> {
    static INST: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(Vec::new()))
}

fn worker_diag_log() -> &'static Mutex<Vec<String>> {
    static INST: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(Vec::new()))
}

fn worker_diag_note(note: String) {
    if std::env::var_os("BROWSER_OXIDE_WORKER_WIRE_TRACE").is_some() {
        eprintln!("[worker-diag] {note}");
    }
    let mut log = worker_diag_log().lock().unwrap_or_else(|e| e.into_inner());
    if log.len() >= WORKER_DIAG_CAP {
        log.remove(0);
    }
    log.push(note.chars().take(WORKER_DIAG_NOTE_CAP).collect());
}

/// Record one worker spawn (url + resolved script, script capped). Keeps the
/// last 3 spawns.
fn record_worker_spawn(url: &str, script: &str) {
    let mut log = worker_spawn_log().lock().unwrap_or_else(|e| e.into_inner());
    if log.len() >= 3 {
        log.remove(0);
    }
    let capped: String = script.chars().take(WORKER_SOURCE_CAP).collect();
    log.push((url.to_string(), capped));
}

/// Last worker spawns as `[url, script]` pairs (script capped at 512 KB).
#[op2]
#[serde]
pub fn op_worker_last_spawn() -> Vec<(String, String)> {
    worker_spawn_log()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Worker-realm note (diagnostics). Callable from the worker isolate.
#[op2(fast)]
pub fn op_worker_diag_note(#[string] note: String) {
    worker_diag_note(note);
}

/// Drain-clone of worker-realm notes; oldest first. Notes are kept (not
/// cleared) so repeated polls see a stable history.
#[op2]
#[serde]
pub fn op_worker_diag_read() -> Vec<String> {
    worker_diag_log()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

// ============================================================================
// BlobRegistry — backs URL.createObjectURL / .revokeObjectURL / blob: loader.
// ============================================================================

struct BlobEntry {
    data: Vec<u8>,
    content_type: String,
}

struct BlobRegistry {
    blobs: HashMap<String, BlobEntry>,
}

fn blob_registry() -> &'static Mutex<BlobRegistry> {
    static INST: OnceLock<Mutex<BlobRegistry>> = OnceLock::new();
    INST.get_or_init(|| {
        Mutex::new(BlobRegistry {
            blobs: HashMap::new(),
        })
    })
}

/// Read a registered Blob URL for Rust-side consumers such as the ES module
/// loader. Module import, fetch(), Worker(), and importScripts() all observe
/// the same browser-owned registry and revoke/lifetime state.
pub(crate) fn blob_module_entry(url: &str) -> Option<(Vec<u8>, String)> {
    let reg = blob_registry().lock().unwrap_or_else(|e| e.into_inner());
    reg.blobs
        .get(url)
        .map(|entry| (entry.data.clone(), entry.content_type.clone()))
}

/// Register a blob's bytes + MIME type under a blob: URL. Called from
/// `URL.createObjectURL`. `content_type` comes from the `Blob.type`
/// field; may be empty string for unspecified blobs.
#[op2(fast)]
pub fn op_blob_register(
    #[string] url: String,
    #[buffer] data: &[u8],
    #[string] content_type: String,
) {
    // URL.createObjectURL is a synchronous renderer -> browser-service
    // operation in Chromium. If an earlier browser-main service task (for
    // example cold SpeechSynthesis voice discovery) is still occupying the
    // browser process, BlobURLStore::Register waits in that queue before the
    // tiny registration itself runs. Model the shared service lane rather
    // than baking a vendor/site-specific delay into ObjectURL.
    crate::js_runtime::extensions::timer_ext::wait_for_browser_main_service_lane();
    // Trace blob registration so the blob-worker path
    // (URL.createObjectURL(blob) -> new Worker(blobUrl)) is observable
    // just before the spawn.
    tracing::debug!(
        url = %url,
        bytes = data.len(),
        content_type = %content_type,
        "op_blob_register"
    );
    let mut reg = blob_registry().lock().unwrap_or_else(|e| e.into_inner());
    reg.blobs.insert(
        url,
        BlobEntry {
            data: data.to_vec(),
            content_type,
        },
    );
}

/// Fetch a blob's text content (UTF-8 lossy) by blob: URL. Used by
/// worker spawning when the script is loaded from a blob: URL, and by
/// the classic-script `importScripts` path.
#[op2]
#[string]
pub fn op_blob_fetch_text(#[string] url: String) -> String {
    let reg = blob_registry().lock().unwrap_or_else(|e| e.into_inner());
    match reg.blobs.get(&url) {
        Some(entry) => String::from_utf8_lossy(&entry.data).to_string(),
        None => String::new(),
    }
}

/// Full response shape for `fetch(blob:...)`: raw bytes + MIME. The JS
/// side constructs a synthetic `Response` from this, so the fetch
/// flow doesn't have to reach into the HTTP client for blob: URLs.
#[derive(serde::Serialize)]
pub struct JsBlobResponse {
    /// Raw bytes of the blob. Transported as a `Vec<u8>` so binary data
    /// survives round-trip (a base64 detour would be lossy for some
    /// encodings and needlessly slow for big buffers).
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub found: bool,
}

/// Binary blob fetch — returns both the bytes and the `Blob.type`
/// string that was passed at registration time. Returns `found=false`
/// for unknown / revoked URLs so the JS side can synthesise a 404.
#[op2]
#[serde]
pub fn op_blob_fetch_bytes(#[string] url: String) -> JsBlobResponse {
    let reg = blob_registry().lock().unwrap_or_else(|e| e.into_inner());
    match reg.blobs.get(&url) {
        Some(entry) => JsBlobResponse {
            bytes: entry.data.clone(),
            content_type: entry.content_type.clone(),
            found: true,
        },
        None => JsBlobResponse {
            bytes: Vec::new(),
            content_type: String::new(),
            found: false,
        },
    }
}

#[op2(fast)]
pub fn op_blob_revoke(#[string] url: String) {
    let mut reg = blob_registry().lock().unwrap_or_else(|e| e.into_inner());
    reg.blobs.remove(&url);
}

/// Synchronous HTTP(S) fetch for worker `importScripts(url)`. Classic
/// workers spec the call as blocking: JS stays on-thread until the
/// response arrives. Because the worker thread is already inside its
/// own tokio `block_on`, we can't reuse that runtime — spinning up a
/// fresh single-threaded runtime on a short-lived helper thread
/// avoids the nested-block_on panic.
///
/// Returns the response body as UTF-8 (lossy on invalid sequences).
/// Empty string means "not fetched" — the JS side interprets that as
/// a network error and throws.
#[op2]
#[string]
pub fn op_worker_sync_fetch(#[string] url: String) -> String {
    // Clone this worker thread's fetch client (seeded by op_worker_spawn
    // from the page's profile + shared cookie jar — F3) so the helper
    // thread inherits the correct identity + cookies. The chrome_148_linux
    // fallback is now only reached if the worker was spawned with no
    // profile at all (it used to be the common case, leaking a Linux UA).
    let client = match crate::js_runtime::extensions::fetch_ext::fetch_client() {
        Some(c) => c,
        None => match crate::net::HttpClient::new(&crate::stealth::chrome_148_linux()) {
            Ok(c) => c,
            Err(_) => return String::new(),
        },
    };

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => {
                let _ = tx.send(String::new());
                return;
            }
        };
        let body = rt.block_on(async move {
            match client.get(&url).await {
                Ok(resp) if resp.ok() => resp.text(),
                _ => String::new(),
            }
        });
        let _ = tx.send(body);
    });

    // Block the worker thread until the helper returns. Max wait
    // 30 seconds to match the page event-loop timeout.
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap_or_default()
}

// ============================================================================
// Worker registry (parent side).
// ============================================================================

struct WorkerSlot {
    to_worker: Sender<String>,
    from_worker: Receiver<String>,
    terminate: Arc<AtomicBool>,
    /// Notified by the worker thread after sending each message AND on
    /// terminate. Used by `op_worker_await_message` to wake without
    /// polling. Drives the W5b-deep fix: SPA pages stop pinning the
    /// V8 event loop with a 5ms setInterval.
    notify_parent: Arc<Notify>,
    /// Wakes the worker's event-loop pump and `op_worker_self_await_message`
    /// on a parent post or terminate, so an idle worker parks instead of polling.
    notify_worker: Arc<Notify>,
}

fn worker_registry() -> &'static Mutex<HashMap<u32, WorkerSlot>> {
    static INST: OnceLock<Mutex<HashMap<u32, WorkerSlot>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_WORKER_ID: AtomicU32 = AtomicU32::new(1);

// ============================================================================
// SharedWorker registry — process-global worker identity keyed by
// (origin, resolved script URL, name). Module/classic type and credentials
// are compatibility attributes of the selected global, not identity fields.
//
// A SharedWorker is not owned by the first Page that creates it. Each Page
// that successfully constructs the worker owns that global until the Page is
// dropped/reused, independently of MessagePort.close(). The worker is reaped
// when the last Page owner disappears or the worker calls self.close().
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SharedWorkerKey {
    origin: String,
    url: String,
    name: String,
}

struct SharedWorkerSlot {
    worker_id: u32,
    connections: usize,
    owners: usize,
    is_module: bool,
    credentials: String,
}

#[derive(Debug, Clone)]
struct SharedWorkerConnection {
    key: SharedWorkerKey,
    worker_id: u32,
}

#[derive(Debug, Clone)]
struct SharedWorkerOwner {
    key: SharedWorkerKey,
    worker_id: u32,
}

fn shared_worker_registry() -> &'static Mutex<HashMap<SharedWorkerKey, SharedWorkerSlot>> {
    static INST: OnceLock<Mutex<HashMap<SharedWorkerKey, SharedWorkerSlot>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

fn shared_worker_connections() -> &'static Mutex<HashMap<u32, SharedWorkerConnection>> {
    static INST: OnceLock<Mutex<HashMap<u32, SharedWorkerConnection>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_SHARED_WORKER_CONNECTION_ID: AtomicU32 = AtomicU32::new(1);

// ============================================================================
// MessagePort registry — process-global entangled endpoints.
//
// Normal same-realm MessageChannel traffic stays on the JS fast path. A pair
// is promoted into this registry only when one of its ports is transferred,
// at which point the peer may live in another V8 isolate / OS thread.
// ============================================================================

#[derive(serde::Serialize)]
struct JsMessagePortHandle {
    id: u32,
    generation: u32,
}

struct MessagePortEndpoint {
    peer: u32,
    queue: VecDeque<String>,
    owner_wake: Option<WorkerOwnerWake>,
    worker_notify: Option<Arc<Notify>>,
    generation: u32,
    closed: bool,
}

fn message_port_registry() -> &'static Mutex<HashMap<u32, MessagePortEndpoint>> {
    static INST: OnceLock<Mutex<HashMap<u32, MessagePortEndpoint>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_MESSAGE_PORT_ID: AtomicU32 = AtomicU32::new(1);

fn runtime_owner_wake(op_state: &OpState) -> WorkerOwnerWake {
    op_state
        .try_borrow::<WorkerOwnerWake>()
        .cloned()
        .unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_message_port_create_pair(op_state: &mut OpState) -> Vec<JsMessagePortHandle> {
    let first = NEXT_MESSAGE_PORT_ID.fetch_add(2, Ordering::Relaxed);
    let second = first.saturating_add(1);
    let owner_wake = runtime_owner_wake(op_state);
    let worker_notify = WORKER_SELF.with(|worker| {
        worker
            .borrow()
            .as_ref()
            .map(|worker| worker.notify_message_port.clone())
    });
    let mut registry = message_port_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    registry.insert(
        first,
        MessagePortEndpoint {
            peer: second,
            queue: VecDeque::new(),
            owner_wake: Some(owner_wake.clone()),
            worker_notify: worker_notify.clone(),
            generation: 1,
            closed: false,
        },
    );
    registry.insert(
        second,
        MessagePortEndpoint {
            peer: first,
            queue: VecDeque::new(),
            owner_wake: Some(owner_wake),
            worker_notify,
            generation: 1,
            closed: false,
        },
    );
    vec![
        JsMessagePortHandle {
            id: first,
            generation: 1,
        },
        JsMessagePortHandle {
            id: second,
            generation: 1,
        },
    ]
}

#[op2(fast)]
#[smi]
pub fn op_message_port_adopt(op_state: &mut OpState, #[smi] endpoint_id: i32) -> i32 {
    let owner_wake = runtime_owner_wake(op_state);
    let worker_notify = WORKER_SELF.with(|worker| {
        worker
            .borrow()
            .as_ref()
            .map(|worker| worker.notify_message_port.clone())
    });
    let mut registry = message_port_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(endpoint) = registry.get_mut(&(endpoint_id as u32)) else {
        return 0;
    };
    if endpoint.closed {
        return 0;
    }
    endpoint.owner_wake = Some(owner_wake);
    endpoint.worker_notify = worker_notify;
    endpoint.generation as i32
}

#[op2(fast)]
#[smi]
pub fn op_message_port_transfer_out(#[smi] endpoint_id: i32, #[smi] generation: i32) -> i32 {
    let mut registry = message_port_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(endpoint) = registry.get_mut(&(endpoint_id as u32)) else {
        return 0;
    };
    if endpoint.closed || endpoint.generation != generation as u32 {
        return 0;
    }
    endpoint.generation = endpoint.generation.wrapping_add(1).max(1);
    endpoint.owner_wake = None;
    endpoint.worker_notify = None;
    endpoint.generation as i32
}

#[op2(fast)]
pub fn op_message_port_post(
    #[smi] endpoint_id: i32,
    #[smi] generation: i32,
    #[string] data: String,
) {
    let wake = {
        let mut registry = message_port_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(sender) = registry.get(&(endpoint_id as u32)) else {
            return;
        };
        if sender.closed || sender.generation != generation as u32 {
            return;
        }
        let peer_id = sender.peer;
        let Some(peer) = registry.get_mut(&peer_id) else {
            return;
        };
        if peer.closed {
            return;
        }
        peer.queue.push_back(data);
        let owner_wake = peer.owner_wake.clone();
        let worker_notify = peer.worker_notify.clone();
        (owner_wake, worker_notify)
    };
    if let Some(wake) = wake.0 {
        wake.wake();
    }
    if let Some(notify) = wake.1 {
        notify.notify_one();
    }
}

#[op2]
#[string]
pub fn op_message_port_try_recv(#[smi] endpoint_id: i32, #[smi] generation: i32) -> String {
    let endpoint_id = endpoint_id as u32;
    let generation = generation as u32;
    let message = {
        let mut registry = message_port_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match registry.get_mut(&endpoint_id) {
            Some(endpoint) if !endpoint.closed && endpoint.generation == generation => {
                endpoint.queue.pop_front()
            }
            _ => None,
        }
    };
    message.unwrap_or_default()
}

#[op2(fast)]
pub fn op_message_port_close(#[smi] endpoint_id: i32, #[smi] generation: i32) {
    let mut registry = message_port_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(endpoint) = registry.get_mut(&(endpoint_id as u32)) else {
        return;
    };
    if endpoint.generation != generation as u32 {
        return;
    }
    endpoint.closed = true;
    endpoint.owner_wake = None;
    endpoint.worker_notify = None;
}

// ============================================================================
// BroadcastChannel registry — process-global same-origin named channels.
//
// Unlike MessagePort, BroadcastChannel fan-outs to every other live endpoint
// with the same (origin, name) storage key. Endpoints retain the owning
// runtime's wake handle so cross-thread sends wake an otherwise-idle Window;
// worker endpoints additionally retain the worker's existing message-port
// Notify so the worker-side parked receive wakes without polling.
// ============================================================================

struct BroadcastChannelEndpoint {
    origin: String,
    name: String,
    queue: VecDeque<String>,
    owner_wake: WorkerOwnerWake,
    worker_notify: Option<Arc<Notify>>,
}

fn broadcast_channel_registry() -> &'static Mutex<HashMap<u32, BroadcastChannelEndpoint>> {
    static INST: OnceLock<Mutex<HashMap<u32, BroadcastChannelEndpoint>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_BROADCAST_CHANNEL_ID: AtomicU32 = AtomicU32::new(1);

// ============================================================================
// Web Locks registry — process-global, origin-scoped lock coordination.
//
// The Web Locks API coordinates same-origin windows and workers, so a realm-local
// JS queue is observably wrong: a worker and its owner would both believe they
// held the same exclusive lock. Keep the scheduler in Rust beside the existing
// cross-runtime BroadcastChannel registry and let each realm run only its own
// granted callback.
// ============================================================================

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WebLockMode {
    Exclusive,
    Shared,
}

impl WebLockMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "exclusive",
            Self::Shared => "shared",
        }
    }
}

#[derive(Clone)]
struct WebLockHeld {
    lock_id: u32,
    client_id: String,
    mode: WebLockMode,
}

struct WebLockBreakState {
    broken: bool,
    released: bool,
    notify: Arc<Notify>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WebLockRequestState {
    Pending,
    Granted(u32),
    Unavailable,
    Canceled,
}

struct WebLockRequest {
    origin: String,
    name: String,
    client_id: String,
    mode: WebLockMode,
    state: WebLockRequestState,
    notify: Arc<Notify>,
}

#[derive(Default)]
struct WebLockQueue {
    held: Vec<WebLockHeld>,
    pending: VecDeque<u32>,
}

#[derive(Default)]
struct WebLockRegistry {
    queues: HashMap<(String, String), WebLockQueue>,
    requests: HashMap<u32, WebLockRequest>,
    lock_index: HashMap<u32, (String, String)>,
    break_states: HashMap<u32, WebLockBreakState>,
}

fn web_lock_registry() -> &'static Mutex<WebLockRegistry> {
    static INST: OnceLock<Mutex<WebLockRegistry>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(WebLockRegistry::default()))
}

static NEXT_WEB_LOCK_REQUEST_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_WEB_LOCK_ID: AtomicU32 = AtomicU32::new(1);

fn web_lock_grant(registry: &mut WebLockRegistry, key: &(String, String), request_id: u32) {
    let Some(request) = registry.requests.get(&request_id) else {
        return;
    };
    if request.state != WebLockRequestState::Pending {
        return;
    }
    let client_id = request.client_id.clone();
    let mode = request.mode;
    let notify = request.notify.clone();
    let lock_id = NEXT_WEB_LOCK_ID.fetch_add(1, Ordering::Relaxed);

    if let Some(request) = registry.requests.get_mut(&request_id) {
        request.state = WebLockRequestState::Granted(lock_id);
    }
    registry
        .queues
        .entry(key.clone())
        .or_default()
        .held
        .push(WebLockHeld {
            lock_id,
            client_id,
            mode,
        });
    registry.lock_index.insert(lock_id, key.clone());
    registry.break_states.insert(
        lock_id,
        WebLockBreakState {
            broken: false,
            released: false,
            notify: Arc::new(Notify::new()),
        },
    );
    notify.notify_waiters();
}

fn web_lock_process_queue(registry: &mut WebLockRegistry, key: &(String, String)) {
    loop {
        // Drop stale/canceled request ids from the front before making the
        // fairness decision. Requests are otherwise FIFO per (origin, name).
        loop {
            let front = registry
                .queues
                .get(key)
                .and_then(|queue| queue.pending.front().copied());
            let Some(front) = front else {
                return;
            };
            let is_pending = registry
                .requests
                .get(&front)
                .map(|request| request.state == WebLockRequestState::Pending)
                .unwrap_or(false);
            if is_pending {
                break;
            }
            if let Some(queue) = registry.queues.get_mut(key) {
                queue.pending.pop_front();
            }
        }

        let (has_held, has_exclusive) = registry
            .queues
            .get(key)
            .map(|queue| {
                (
                    !queue.held.is_empty(),
                    queue
                        .held
                        .iter()
                        .any(|held| held.mode == WebLockMode::Exclusive),
                )
            })
            .unwrap_or((false, false));
        if has_exclusive {
            return;
        }

        let request_id = registry
            .queues
            .get(key)
            .and_then(|queue| queue.pending.front().copied())
            .unwrap();
        let mode = registry
            .requests
            .get(&request_id)
            .map(|request| request.mode)
            .unwrap_or(WebLockMode::Exclusive);

        // Shared locks may join existing shared holders only while they remain
        // at the head of the queue. Once an exclusive request reaches the head,
        // later shared requests cannot bypass it.
        if has_held && mode == WebLockMode::Exclusive {
            return;
        }

        if let Some(queue) = registry.queues.get_mut(key) {
            queue.pending.pop_front();
        }
        web_lock_grant(registry, key, request_id);
        if mode == WebLockMode::Exclusive {
            return;
        }
    }
}

#[op2(fast)]
#[smi]
pub fn op_web_lock_enqueue(
    #[string] origin: String,
    #[string] name: String,
    #[string] client_id: String,
    shared: bool,
    if_available: bool,
    steal: bool,
) -> i32 {
    let request_id = NEXT_WEB_LOCK_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let mode = if shared {
        WebLockMode::Shared
    } else {
        WebLockMode::Exclusive
    };
    let key = (origin.clone(), name.clone());
    let notify = Arc::new(Notify::new());
    let mut registry = web_lock_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    registry.requests.insert(
        request_id,
        WebLockRequest {
            origin,
            name,
            client_id,
            mode,
            state: WebLockRequestState::Pending,
            notify: notify.clone(),
        },
    );

    let immediately_available = registry
        .queues
        .get(&key)
        .map(|queue| {
            queue.pending.is_empty()
                && !queue
                    .held
                    .iter()
                    .any(|held| held.mode == WebLockMode::Exclusive)
                && (queue.held.is_empty() || mode == WebLockMode::Shared)
        })
        .unwrap_or(true);

    if if_available && !immediately_available {
        if let Some(request) = registry.requests.get_mut(&request_id) {
            request.state = WebLockRequestState::Unavailable;
        }
        notify.notify_waiters();
        return request_id as i32;
    }

    if steal {
        // A stealing exclusive request is placed ahead of queued requests and
        // immediately releases the resource from current holders. Existing
        // callbacks continue running, but their later release becomes a no-op.
        let stolen_ids = registry
            .queues
            .entry(key.clone())
            .or_default()
            .held
            .drain(..)
            .map(|held| held.lock_id)
            .collect::<Vec<_>>();
        for lock_id in stolen_ids {
            registry.lock_index.remove(&lock_id);
            if let Some(state) = registry.break_states.get_mut(&lock_id) {
                state.broken = true;
                state.notify.notify_waiters();
            }
        }
        registry
            .queues
            .entry(key.clone())
            .or_default()
            .pending
            .push_front(request_id);
    } else {
        registry
            .queues
            .entry(key.clone())
            .or_default()
            .pending
            .push_back(request_id);
    }
    web_lock_process_queue(&mut registry, &key);
    request_id as i32
}

#[op2(async(lazy), fast)]
#[string]
pub async fn op_web_lock_wait(#[smi] request_id: i32) -> String {
    let request_id = request_id as u32;
    loop {
        let (state, notified) = {
            let registry = web_lock_registry()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let Some(request) = registry.requests.get(&request_id) else {
                return "c".to_string();
            };
            // Construct the notification future while the registry mutex still
            // protects the state check. `notified_owned()` snapshots the
            // notify_waiters generation here, so a grant that happens after we
            // drop the mutex but before the future is first polled cannot be
            // lost.
            (request.state, request.notify.clone().notified_owned())
        };

        match state {
            WebLockRequestState::Pending => notified.await,
            WebLockRequestState::Granted(lock_id) => {
                web_lock_registry()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .requests
                    .remove(&request_id);
                return format!("g:{lock_id}");
            }
            WebLockRequestState::Unavailable => {
                web_lock_registry()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .requests
                    .remove(&request_id);
                return "u".to_string();
            }
            WebLockRequestState::Canceled => {
                web_lock_registry()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .requests
                    .remove(&request_id);
                return "c".to_string();
            }
        }
    }
}

#[op2(fast)]
pub fn op_web_lock_cancel(#[smi] request_id: i32) -> bool {
    let request_id = request_id as u32;
    let mut registry = web_lock_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(request) = registry.requests.get(&request_id) else {
        return false;
    };
    if request.state != WebLockRequestState::Pending {
        return false;
    }
    let key = (request.origin.clone(), request.name.clone());
    let notify = request.notify.clone();
    if let Some(request) = registry.requests.get_mut(&request_id) {
        request.state = WebLockRequestState::Canceled;
    }
    if let Some(queue) = registry.queues.get_mut(&key) {
        queue.pending.retain(|id| *id != request_id);
    }
    notify.notify_waiters();
    web_lock_process_queue(&mut registry, &key);
    true
}

#[op2(fast)]
pub fn op_web_lock_release(#[smi] lock_id: i32) {
    let lock_id = lock_id as u32;
    let mut registry = web_lock_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(key) = registry.lock_index.remove(&lock_id) else {
        return;
    };
    if let Some(queue) = registry.queues.get_mut(&key) {
        queue.held.retain(|held| held.lock_id != lock_id);
    }
    if let Some(state) = registry.break_states.get_mut(&lock_id) {
        state.released = true;
        state.notify.notify_waiters();
    }
    web_lock_process_queue(&mut registry, &key);
}

#[op2(async(lazy), fast)]
pub async fn op_web_lock_wait_break(#[smi] lock_id: i32) -> bool {
    let lock_id = lock_id as u32;
    loop {
        let (broken, released, notified) = {
            let registry = web_lock_registry()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let Some(state) = registry.break_states.get(&lock_id) else {
                return false;
            };
            (
                state.broken,
                state.released,
                state.notify.clone().notified_owned(),
            )
        };

        if broken || released {
            web_lock_registry()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .break_states
                .remove(&lock_id);
            return broken;
        }
        notified.await;
    }
}

#[op2]
#[string]
pub fn op_web_lock_query(#[string] origin: String) -> String {
    let registry = web_lock_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut held = Vec::new();
    let mut pending = Vec::new();

    for ((queue_origin, name), queue) in &registry.queues {
        if *queue_origin != origin {
            continue;
        }
        for lock in &queue.held {
            held.push((
                lock.lock_id,
                serde_json::json!({
                    "clientId": lock.client_id,
                    "mode": lock.mode.as_str(),
                    "name": name,
                }),
            ));
        }
        for request_id in &queue.pending {
            if let Some(request) = registry.requests.get(request_id) {
                if request.state == WebLockRequestState::Pending {
                    pending.push((
                        *request_id,
                        serde_json::json!({
                            "clientId": request.client_id,
                            "mode": request.mode.as_str(),
                            "name": name,
                        }),
                    ));
                }
            }
        }
    }
    held.sort_by_key(|(id, _)| *id);
    pending.sort_by_key(|(id, _)| *id);
    serde_json::json!({
        "held": held.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
        "pending": pending.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
    })
    .to_string()
}

#[op2(fast)]
#[smi]
pub fn op_broadcast_channel_register(
    op_state: &mut OpState,
    #[string] origin: String,
    #[string] name: String,
) -> i32 {
    let id = NEXT_BROADCAST_CHANNEL_ID.fetch_add(1, Ordering::Relaxed);
    let owner_wake = runtime_owner_wake(op_state);
    let worker_notify = WORKER_SELF.with(|worker| {
        worker
            .borrow()
            .as_ref()
            .map(|worker| worker.notify_message_port.clone())
    });
    broadcast_channel_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            id,
            BroadcastChannelEndpoint {
                origin,
                name,
                queue: VecDeque::new(),
                owner_wake,
                worker_notify,
            },
        );
    id as i32
}

#[op2(fast)]
pub fn op_broadcast_channel_post(#[smi] endpoint_id: i32, #[string] data: String) {
    let endpoint_id = endpoint_id as u32;
    let wakes = {
        let mut registry = broadcast_channel_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(sender) = registry.get(&endpoint_id) else {
            return;
        };
        let origin = sender.origin.clone();
        let name = sender.name.clone();
        let mut wakes = Vec::new();
        for (peer_id, peer) in registry.iter_mut() {
            if *peer_id == endpoint_id || peer.origin != origin || peer.name != name {
                continue;
            }
            peer.queue.push_back(data.clone());
            wakes.push((peer.owner_wake.clone(), peer.worker_notify.clone()));
        }
        wakes
    };
    for (owner_wake, worker_notify) in wakes {
        owner_wake.wake();
        if let Some(notify) = worker_notify {
            notify.notify_one();
        }
    }
}

#[op2]
#[string]
pub fn op_broadcast_channel_try_recv(#[smi] endpoint_id: i32) -> String {
    broadcast_channel_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&(endpoint_id as u32))
        .and_then(|endpoint| endpoint.queue.pop_front())
        .unwrap_or_default()
}

#[op2(fast)]
pub fn op_broadcast_channel_close(#[smi] endpoint_id: i32) {
    broadcast_channel_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(endpoint_id as u32));
}

// ============================================================================
// Per-thread worker "self" state — populated when a worker thread starts.
// ============================================================================

struct WorkerSelf {
    /// Process-global worker registry id. Used by `self.close()` to terminate
    /// the current worker without routing through its parent Page.
    worker_id: u32,
    to_parent: Sender<String>,
    from_parent: Receiver<String>,
    /// Same Arc as the parent's `WorkerSlot.notify_parent`. Worker
    /// signals after every send so the parent's awaiting promise wakes
    /// up without polling.
    notify_parent: Arc<Notify>,
    /// Same Arc as the parent's `WorkerSlot.notify_worker`; the worker's
    /// `op_worker_self_await_message` parks on it, the parent signals on post/terminate.
    notify_worker: Arc<Notify>,
    /// Dedicated wake for transferred MessagePort endpoints owned by this
    /// worker realm. It resolves the same ref'ed receive op with an internal
    /// sentinel so JS can synchronously drain the endpoint registry.
    notify_message_port: Arc<Notify>,
    /// Out-of-band wake for the browser runtime that owns this worker.
    owner_wake: WorkerOwnerWake,
    /// Same Arc as the parent's `WorkerSlot.terminate`. Lets the worker's
    /// `op_worker_self_await_message` return "" (stop) once terminated.
    terminate: Arc<AtomicBool>,
    /// URL the worker was constructed with (`new Worker(url)`). Drives
    /// `self.location.href` in the worker realm via `op_worker_self_url`.
    /// Some workers read `self.location.origin` to verify they were
    /// loaded from an expected URL; an empty / missing `self.location`
    /// can leave a worker-dependent app stuck on a thin shell.
    url: String,
    /// Shared workers reuse the dedicated-worker transport/thread machinery,
    /// but expose SharedWorkerGlobalScope semantics and receive `connect`
    /// events carrying transferred MessagePorts instead of parent `message`
    /// events.
    is_shared: bool,
    /// `SharedWorkerGlobalScope.name` (empty for dedicated workers).
    name: String,
}

thread_local! {
    static WORKER_SELF: RefCell<Option<WorkerSelf>> = const { RefCell::new(None) };
}

// ============================================================================
// Ops — parent side.
// ============================================================================

fn spawn_worker_inner(
    op_state: &mut OpState,
    script: String,
    name: String,
    is_module: bool,
    url: String,
    storage_directory_allowed: bool,
    is_shared: bool,
    track_owner: bool,
) -> i32 {
    record_worker_spawn(&url, &script);
    // 0.403: #[state] removed — borrow the three (immutable) states from OpState.
    let state = op_state.borrow::<DomState>();
    let stealth = op_state.borrow::<StealthState>();
    let owned = op_state.borrow::<WorkerOwnership>();
    let owner_wake = op_state
        .try_borrow::<WorkerOwnerWake>()
        .cloned()
        .unwrap_or_default();
    // Prefer StealthState.profile (always set from BrowserRuntimeOptions) over
    // DomState.stealth_profile (historically always None in the main runtime).
    let profile = stealth
        .profile
        .clone()
        .or_else(|| state.stealth_profile.clone());
    // A Worker inherits its owner's secure-context (HTML spec). Captured here
    // (Copy bool) and moved into the worker thread so the worker realm keeps
    // crypto.subtle / crypto.randomUUID when spawned from an https/blob:https
    // page — required by SHA-256 proof-of-work workers (a common pattern in
    // challenge scripts that run in workers).
    let is_secure_context = stealth.is_secure_context;
    // Dedicated workers inherit the owner's cross-origin-isolated state too.
    // This controls both self.crossOriginIsolated and SharedArrayBuffer
    // exposure in the worker cleanup phase.
    let cross_origin_isolated = stealth.cross_origin_isolated && is_secure_context;
    // Capture the page's fetch client (correct profile + shared cookie
    // jar) on the MAIN thread so the worker thread
    // can seed its own thread-local FETCH_CLIENT with it. Without this,
    // `op_worker_sync_fetch` runs on the worker thread where the
    // thread-local is None and falls back to `chrome_148_linux()` — a Linux
    // UA leak on a macOS/Windows page, and a window<->worker fetch-identity
    // mismatch — real Chrome's worker shares the document's network identity.
    let parent_fetch_client = crate::js_runtime::extensions::fetch_ext::fetch_client();
    let (to_worker_tx, to_worker_rx) = std::sync::mpsc::channel::<String>();
    let (to_parent_tx, to_parent_rx) = std::sync::mpsc::channel::<String>();
    let terminate = Arc::new(AtomicBool::new(false));
    let notify_parent = Arc::new(Notify::new());
    let notify_worker = Arc::new(Notify::new());
    let worker_id = NEXT_WORKER_ID.fetch_add(1, Ordering::Relaxed);
    // op_worker_spawn previously logged only on failure, so a missing
    // spawn and a silent spawn were indistinguishable when diagnosing a
    // proof-of-work worker path. Trace every spawn (module/classic,
    // secure-context, url) so the worker flow is observable under
    // RUST_LOG=js_runtime::extensions::worker_ext=debug.
    tracing::debug!(
        worker_id,
        is_module,
        is_shared,
        is_secure_context,
        url = %url,
        script_len = script.len(),
        "op_worker_spawn"
    );

    {
        let mut reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
        reg.insert(
            worker_id,
            WorkerSlot {
                to_worker: to_worker_tx,
                from_worker: to_parent_rx,
                terminate: terminate.clone(),
                notify_parent: notify_parent.clone(),
                notify_worker: notify_worker.clone(),
            },
        );
    }
    // Track this worker as owned by the current isolate so
    // `drain_owned_workers` (called from `Page::drop`) can reap it
    // if the page never explicitly called `worker.terminate()`.
    if track_owner {
        owned.spawned_ids.borrow_mut().push(worker_id);
    }

    // 64 MB stack: V8's default stack guard isn't large enough for some
    // scripts that recurse deeply through wrapped natives.
    // Chrome's renderer threads also run with ~16 MB stacks; we go larger
    // because our shim adds more JS frames per native call.
    let thread_result = std::thread::Builder::new()
        .name(format!("worker-{worker_id}"))
        .stack_size(crate::js_runtime::V8_THREAD_STACK)
        .spawn(move || {
            // Install per-thread worker state BEFORE any ops run.
            let notify_message_port = Arc::new(Notify::new());
            WORKER_SELF.with(|w| {
                *w.borrow_mut() = Some(WorkerSelf {
                    worker_id,
                    to_parent: to_parent_tx,
                    from_parent: to_worker_rx,
                    notify_parent: notify_parent.clone(),
                    notify_worker: notify_worker.clone(),
                    notify_message_port: notify_message_port.clone(),
                    owner_wake: owner_wake.clone(),
                    terminate: terminate.clone(),
                    url,
                    is_shared,
                    name,
                });
            });

            // F3: seed THIS worker thread's fetch client so
            // `op_worker_sync_fetch` inherits the page's profile + shared
            // cookie jar. Falls back to building from the worker's own
            // profile, and only to chrome_148_linux() if no profile exists
            // at all (matching the historic last-resort, but now reached
            // far less often).
            let seed_client = parent_fetch_client
                .or_else(|| profile.as_ref().and_then(|p| crate::net::HttpClient::new(p).ok()));
            if let Some(c) = seed_client {
                crate::js_runtime::extensions::fetch_ext::set_fetch_client(c);
            }

            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!(worker_id = worker_id, error = %e, "worker tokio build error");
                    return;
                }
            };

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let mut runtime = crate::js_runtime::runtime::create_worker_runtime(
                    profile,
                    is_secure_context,
                    cross_origin_isolated,
                    storage_directory_allowed,
                );
                // Nested DedicatedWorkers use this runtime as their owner. Their
                // receive op is unref'ed after startup so an idle child does not
                // pin this worker's V8 event loop, but an actual child->owner
                // message still has to wake the parked outer worker loop.
                let nested_worker_wake = runtime
                    .op_state()
                    .borrow()
                    .borrow::<WorkerOwnerWake>()
                    .notify_handle();

                // Capture-and-delete the privileged message-pump starter
                // before page-authored worker code runs. worker_bootstrap has
                // to register the closure during runtime construction, but it
                // must not remain reachable through Symbol.for() once the
                // user's worker body starts executing.
                let worker_pump_start = {
                    let context = runtime.main_context();
                    v8::scope_with_context!(scope, runtime.v8_isolate(), context);
                    let global = scope.get_current_context().global(scope);
                    let key = v8::String::new(scope, "__browser_oxide_worker_start_pump__")
                        .expect("worker pump symbol key");
                    let symbol = v8::Symbol::for_key(scope, key);
                    let value = global.get(scope, symbol.into());
                    let function = value
                        .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
                        .map(|function| v8::Global::new(scope, function));
                    let _ = global.delete(scope, symbol.into());
                    function
                };
                // Execute the worker script inside the worker's isolate.
                // Module workers go through `load_main_es_module_from_code`
                // so top-level `import.meta` and module-scoped evaluation
                // work the way sites expect. Classic workers stick with
                // the direct `execute_script` path.
                if is_module {
                    let specifier = deno_core::ModuleSpecifier::parse(&format!(
                        "worker-oxide://{worker_id}/main.mjs"
                    ))
                    .expect("worker-oxide URL parses");
                    match runtime
                        .load_main_es_module_from_code(&specifier, script)
                        .await
                    {
                        Ok(mod_id) => {
                            let eval_fut = runtime.mod_evaluate(mod_id);
                            // Drive the event loop alongside evaluation so
                            // async top-level work in the module body
                            // resolves. Ignore the eval result here — we
                            // want to continue even if the module throws
                            // so the worker stays alive for onmessage.
                            if let Err(e) = eval_fut.await {
                                tracing::warn!(
                                    worker_id = worker_id, error = %e, "worker module eval error"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(worker_id = worker_id, error = %e, "worker module load error");
                        }
                    }
                } else if let Err(e) = runtime.execute_script("<anonymous>", script) {
                    worker_diag_note(format!(
                        "eval-fail err={}",
                        format!("{e}").chars().take(600).collect::<String>()
                    ));
                    tracing::warn!(worker_id = worker_id, error = %e, "worker script error");
                } else {
                    worker_diag_note("eval-ok".to_string());
                }

                // Browser worker message tasks do not run until the initial
                // worker script has evaluated. Invoke the closure captured
                // above only now, after the user's onmessage/listener setup is
                // complete, without re-exposing an internal hook to page JS.
                if let Some(worker_pump_start) = worker_pump_start {
                    let context = runtime.main_context();
                    v8::scope_with_context!(scope, runtime.v8_isolate(), context);
                    let function = v8::Local::new(scope, &worker_pump_start);
                    let receiver = v8::undefined(scope).into();
                    let _ = function.call(scope, receiver, &[]);
                }

                // Release the owner's initially-ref'ed receive after the
                // worker body has run once. This preserves unsolicited
                // startup messages while allowing a genuinely idle Worker to
                // stop pinning page-idle detection for its whole lifetime.
                WORKER_SELF.with(|worker| {
                    if let Some(worker) = worker.borrow().as_ref() {
                        let _ = worker
                            .to_parent
                            .send("__browser_oxide_worker_ready__".to_string());
                        worker.notify_parent.notify_one();
                    }
                });

                // When the event loop drains (worker idle), park on `notify_worker`
                // instead of polling; a parent post or terminate wakes it.
                while !terminate.load(Ordering::Acquire) {
                    match runtime
                        .run_event_loop(deno_core::PollEventLoopOptions::default())
                        .await
                    {
                        Ok(()) => {
                            if terminate.load(Ordering::Acquire) {
                                break;
                            }
                            tokio::select! {
                                _ = notify_worker.notified() => {}
                                _ = nested_worker_wake.notified() => {}
                            }
                        }
                        Err(e) => {
                            tracing::warn!(worker_id = worker_id, error = %e, "worker event loop error");
                            break;
                        }
                    }
                }

                // Reap workers spawned by this worker realm before dropping its
                // JsRuntime. Page::drop only sees top-level worker ownership;
                // without this, terminating an outer worker could orphan its
                // nested worker threads.
                {
                    let op_state = runtime.op_state();
                    let mut op_state = op_state.borrow_mut();
                    drain_owned_workers(&mut op_state);
                }

                // Clear thread-local worker state.
                WORKER_SELF.with(|w| *w.borrow_mut() = None);
            });
        });

    if let Err(e) = thread_result {
        tracing::error!(worker_id = worker_id, error = %e, "worker thread spawn failed");
        worker_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&worker_id);
        return 0;
    }

    worker_id as i32
}

#[op2(fast)]
#[smi]
pub fn op_worker_spawn(
    op_state: &mut OpState,
    #[string] script: String,
    #[string] name: String,
    is_module: bool,
    #[string] url: String,
    storage_directory_allowed: bool,
) -> i32 {
    spawn_worker_inner(
        op_state,
        script,
        name,
        is_module,
        url,
        storage_directory_allowed,
        false,
        true,
    )
}

fn shared_worker_is_live(worker_id: u32) -> bool {
    worker_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(&worker_id)
}

fn disconnect_shared_worker_connection_inner(connection_id: u32) {
    let connection = shared_worker_connections()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&connection_id);
    let Some(connection) = connection else {
        return;
    };

    let mut registry = shared_worker_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = registry.get_mut(&connection.key) {
        if slot.worker_id == connection.worker_id {
            // Closing a MessagePort does not release the Document's ownership
            // of its SharedWorker. Chromium keeps the global alive while the
            // creating realm itself is alive, so a later constructor call in
            // the same page reconnects to the existing global. The owner is
            // released by `drain_owned_shared_worker_connections` when that
            // page/runtime is dropped or warm-reused.
            slot.connections = slot.connections.saturating_sub(1);
        }
    }
}

#[op2(fast)]
#[smi]
pub fn op_shared_worker_connect(
    op_state: &mut OpState,
    #[string] origin: String,
    #[string] url: String,
    #[string] name: String,
    is_module: bool,
    #[string] credentials: String,
    #[string] script: String,
    storage_directory_allowed: bool,
    #[smi] endpoint_id: i32,
) -> i32 {
    let key = SharedWorkerKey {
        origin,
        url: url.clone(),
        name: name.clone(),
    };

    let existing = shared_worker_registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .map(|slot| (slot.worker_id, slot.is_module, slot.credentials.clone()));
    let existing = existing.filter(|(worker_id, _, _)| shared_worker_is_live(*worker_id));

    // Chromium uses (storage key/origin, resolved URL, name) as the shared
    // worker identity. `type` and `credentials` are compatibility checks on
    // an existing named worker, not independent identity dimensions. A
    // mismatch returns a SharedWorker object whose `error` event fires
    // asynchronously instead of starting a second global.
    if let Some((_worker_id, existing_module, existing_credentials)) = &existing {
        if *existing_module != is_module || existing_credentials != &credentials {
            return -1;
        }
    }

    let worker_id = if let Some((worker_id, _, _)) = existing {
        worker_id
    } else {
        // Remove a stale registry entry before creating the replacement.
        shared_worker_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
        let spawned = spawn_worker_inner(
            op_state,
            script,
            name,
            is_module,
            url,
            storage_directory_allowed,
            true,
            false,
        );
        if spawned <= 0 {
            return 0;
        }
        let spawned = spawned as u32;

        // Another runtime can race the first connection. Keep one canonical
        // worker for the storage key and terminate the redundant spawn.
        let canonical = {
            let mut registry = shared_worker_registry()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match registry.get(&key) {
                Some(slot) if shared_worker_is_live(slot.worker_id) => {
                    if slot.is_module != is_module || slot.credentials != credentials {
                        0
                    } else {
                        slot.worker_id
                    }
                }
                _ => {
                    registry.insert(
                        key.clone(),
                        SharedWorkerSlot {
                            worker_id: spawned,
                            connections: 0,
                            owners: 0,
                            is_module,
                            credentials: credentials.clone(),
                        },
                    );
                    spawned
                }
            }
        };
        if canonical == 0 {
            terminate_worker_inner(spawned);
            return -1;
        }
        if canonical != spawned {
            terminate_worker_inner(spawned);
        }
        canonical
    };

    let is_new_owner = {
        let owned = op_state.borrow::<SharedWorkerOwnership>();
        let mut owners = owned.owners.borrow_mut();
        if owners
            .iter()
            .any(|owner| owner.worker_id == worker_id && owner.key == key)
        {
            false
        } else {
            owners.push(SharedWorkerOwner {
                key: key.clone(),
                worker_id,
            });
            true
        }
    };

    let connection_id = NEXT_SHARED_WORKER_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
    {
        let mut registry = shared_worker_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(slot) = registry.get_mut(&key) else {
            return 0;
        };
        if is_new_owner {
            slot.owners += 1;
        }
        slot.connections += 1;
    }
    shared_worker_connections()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(connection_id, SharedWorkerConnection { key, worker_id });
    op_state
        .borrow::<SharedWorkerOwnership>()
        .connection_ids
        .borrow_mut()
        .push(connection_id);

    // The worker-side endpoint has already been transferred out of the page
    // MessageChannel. Deliver it through the existing worker control channel;
    // worker_bootstrap recognizes the marker and dispatches a trusted
    // `connect` MessageEvent with ports[0].
    let payload = serde_json::json!({
        "data": { "__browser_oxide_shared_connect": true },
        "ports": [endpoint_id],
    })
    .to_string();
    post_to_worker_inner(worker_id, payload);

    connection_id as i32
}

#[op2(fast)]
pub fn op_shared_worker_disconnect(op_state: &mut OpState, #[smi] connection_id: i32) {
    let connection_id = connection_id as u32;
    if let Some(owned) = op_state.try_borrow::<SharedWorkerOwnership>() {
        let mut ids = owned.connection_ids.borrow_mut();
        if let Some(index) = ids.iter().position(|id| *id == connection_id) {
            ids.swap_remove(index);
        }
    }
    disconnect_shared_worker_connection_inner(connection_id);
}

fn post_to_worker_inner(worker_id: u32, data: String) {
    trace_worker_wire("parent->worker", worker_id, &data);
    let reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = reg.get(&worker_id) {
        let _ = slot.to_worker.send(data);
        // Wake the worker's parked pump / `op_worker_self_await_message`.
        slot.notify_worker.notify_one();
    }
}

#[op2(fast)]
pub fn op_worker_post_to_worker(#[smi] worker_id: i32, #[string] data: String) {
    post_to_worker_inner(worker_id as u32, data);
}

/// Return the next pending message from a worker, or the empty string if none.
/// Empty string is safe as a sentinel because our JS wrapper JSON-encodes
/// every payload — an empty JSON encoding of a real message is never "".
#[op2]
#[string]
pub fn op_worker_poll_from_worker(#[smi] worker_id: i32) -> String {
    let reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = reg.get(&(worker_id as u32)) {
        match slot.from_worker.try_recv() {
            Ok(msg) => {
                trace_worker_wire("worker->parent", worker_id as u32, &msg);
                return msg;
            }
            Err(_) => return String::new(),
        }
    }
    String::new()
}

#[op2(fast)]
pub fn op_worker_terminate(#[smi] worker_id: i32) {
    terminate_worker_inner(worker_id as u32);
}

/// Synchronous terminate, callable from non-V8 contexts (notably
/// `Page::drop` reaping orphan workers a page never explicitly
/// terminated). Signals the worker's terminate flag and removes the
/// registry slot — the worker thread polls the flag and exits its
/// tokio runtime on the next tick, which releases its 64 MB stack
/// + child `JsRuntime` heap. Idempotent: missing slot is a no-op.
pub fn terminate_worker_inner(worker_id: u32) {
    let mut reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = reg.get(&worker_id) {
        slot.terminate.store(true, Ordering::Release);
        // Wake any in-flight `op_worker_await_message` so it can return
        // empty and the JS-side pump can stop chaining.
        slot.notify_parent.notify_waiters();
        // notify_one stores a permit, so a terminate raised while the worker
        // isn't parked isn't lost — the next `notified()` consumes it and exits.
        slot.notify_worker.notify_one();
    }
    reg.remove(&worker_id);
}

/// Terminate a worker that may also be registered as a SharedWorker.
///
/// SharedWorker owners/connections carry the worker id as a generation token,
/// so removing this generation here is safe even if a later constructor
/// creates a replacement for the same (origin, URL, name) key. Stale Page
/// ownership records will no-op when they are eventually drained.
fn terminate_shared_worker_instance_inner(worker_id: u32) {
    {
        let mut registry = shared_worker_registry()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        registry.retain(|_, slot| slot.worker_id != worker_id);
    }
    {
        let mut connections = shared_worker_connections()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        connections.retain(|_, connection| connection.worker_id != worker_id);
    }
    terminate_worker_inner(worker_id);
}

/// Reaper for `Page::drop` — terminates every worker spawned by a
/// page's V8 isolate. Without this, workers created via `new Worker(blob)`
/// keep their OS thread + child `JsRuntime` alive for the lifetime of
/// the process, leaking ~30 MB per worker. Pages that explicitly call
/// `worker.terminate()` from JS pre-drained the list, so this is a no-op
/// for well-behaved pages and a critical reap for sites that don't
/// (cnn / bloomberg / youtube / discord / udemy — the 13 sites driving
/// the +15 MB step-ups in the cold-sweep RSS curve).
pub fn drain_owned_workers(state: &mut OpState) {
    let ids: Vec<u32> = state
        .try_borrow::<WorkerOwnership>()
        .map(|o| std::mem::take(&mut *o.spawned_ids.borrow_mut()))
        .unwrap_or_default();
    for id in ids {
        terminate_worker_inner(id);
    }
}

/// Release SharedWorker connection references owned by one Page/runtime.
/// The shared worker thread itself survives while another runtime still owns
/// the same (origin, URL, name) worker, even if all of its ports are closed.
pub fn drain_owned_shared_worker_connections(state: &mut OpState) {
    let (ids, owners): (Vec<u32>, Vec<SharedWorkerOwner>) = state
        .try_borrow::<SharedWorkerOwnership>()
        .map(|o| {
            (
                std::mem::take(&mut *o.connection_ids.borrow_mut()),
                std::mem::take(&mut *o.owners.borrow_mut()),
            )
        })
        .unwrap_or_default();
    for id in ids {
        disconnect_shared_worker_connection_inner(id);
    }

    for owner in owners {
        let terminate_id = {
            let mut registry = shared_worker_registry()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match registry.get_mut(&owner.key) {
                Some(slot) if slot.worker_id == owner.worker_id => {
                    slot.owners = slot.owners.saturating_sub(1);
                    if slot.owners == 0 {
                        let worker_id = slot.worker_id;
                        registry.remove(&owner.key);
                        Some(worker_id)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
        if let Some(worker_id) = terminate_id {
            terminate_worker_inner(worker_id);
        }
    }
}

/// Per-`JsRuntime` set of worker IDs spawned by this isolate. Populated
/// by `op_worker_spawn`; drained by `drain_owned_workers` at `Page::drop`.
/// `RefCell` because deno_core's `#[op2]` macro requires all `#[state]`
/// parameters on a single op be either all `&` or all `&mut` — we need
/// `&DomState` + `&StealthState` (immutable) so the only way to mutate
/// this third state from inside the op is via interior mutability.
#[derive(Default)]
pub struct WorkerOwnership {
    pub spawned_ids: RefCell<Vec<u32>>,
}

#[derive(Default)]
pub struct SharedWorkerOwnership {
    connection_ids: RefCell<Vec<u32>>,
    owners: RefCell<Vec<SharedWorkerOwner>>,
}

/// Async op that returns the next worker→parent message,
/// awaiting on a tokio Notify rather than polling. Returns "" when the
/// worker has terminated. Replaces the JS-level `setInterval(5)` pump
/// at `window_bootstrap.js:1633` that previously pinned `is_pending=true`
/// for the lifetime of every Worker, blocking SPA hydration completion
/// detection (twitter, x.com, etc.).
#[op2(async(lazy), fast)]
#[string]
pub async fn op_worker_await_message(#[smi] worker_id: i32) -> String {
    let id = worker_id as u32;
    // Acquire the notify Arc + drain any messages already queued.
    // Drop the registry lock BEFORE awaiting so other ops on this worker
    // (terminate, post_to_worker) aren't blocked.
    let (notify, terminate, fast_msg) = {
        let reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
        match reg.get(&id) {
            Some(slot) => {
                // Try to drain a message synchronously first — if one is
                // already buffered we don't even need to await.
                let already = slot.from_worker.try_recv().ok();
                (slot.notify_parent.clone(), slot.terminate.clone(), already)
            }
            None => return String::new(), // worker is gone
        }
    };
    if let Some(msg) = fast_msg {
        trace_worker_wire("worker->parent", id, &msg);
        return msg;
    }
    // Loop on notify until we get a message OR the worker terminates.
    // Notified is edge-triggered so we have to re-check the queue after
    // each wake.
    loop {
        if terminate.load(Ordering::Acquire) {
            return String::new();
        }
        notify.notified().await;
        // Re-acquire and drain.
        let reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
        match reg.get(&id) {
            Some(slot) => {
                if let Ok(msg) = slot.from_worker.try_recv() {
                    trace_worker_wire("worker->parent", id, &msg);
                    return msg;
                }
                // Spurious wake — re-loop.
            }
            None => return String::new(),
        }
    }
}

// ============================================================================
// Ops — worker side (read from thread-local WORKER_SELF).
// ============================================================================

#[op2(fast)]
pub fn op_worker_self_post(#[string] data: String) {
    WORKER_SELF.with(|w| {
        if let Some(s) = w.borrow().as_ref() {
            if s.to_parent.send(data).is_ok() {
                // Settle a referenced receive if one exists and independently
                // wake the owner runtime when that receive has been unref'ed.
                s.notify_parent.notify_one();
                s.owner_wake.wake();
            }
        }
    });
}

#[op2(fast)]
pub fn op_worker_self_close() {
    let worker_id = WORKER_SELF.with(|w| w.borrow().as_ref().map(|state| state.worker_id));
    if let Some(worker_id) = worker_id {
        terminate_shared_worker_instance_inner(worker_id);
    }
}

#[op2]
#[string]
pub fn op_worker_self_recv() -> String {
    WORKER_SELF.with(|w| {
        if let Some(s) = w.borrow().as_ref() {
            s.from_parent.try_recv().unwrap_or_default()
        } else {
            String::new()
        }
    })
}

/// Worker-side recv that parks on the worker's Notify instead of polling.
/// Returns "" once terminated so the JS pump can stop chaining.
#[op2(async(lazy), fast)]
#[string]
pub async fn op_worker_self_await_message() -> String {
    // Clone the Notify + terminate flag and try a fast drain, dropping the
    // WORKER_SELF borrow before any await.
    let (notify, port_notify, terminate, fast) = WORKER_SELF.with(|w| match w.borrow().as_ref() {
        Some(s) => (
            Some(s.notify_worker.clone()),
            Some(s.notify_message_port.clone()),
            Some(s.terminate.clone()),
            s.from_parent.try_recv().ok(),
        ),
        None => (None, None, None, None),
    });
    if let Some(msg) = fast {
        return msg;
    }
    let (Some(notify), Some(port_notify), Some(terminate)) = (notify, port_notify, terminate)
    else {
        return String::new();
    };
    // Notified is edge-triggered, so re-check the queue after each wake.
    loop {
        if terminate.load(Ordering::Acquire) {
            return String::new();
        }
        tokio::select! {
            _ = notify.notified() => {
                let msg = WORKER_SELF.with(|w| {
                    w.borrow()
                        .as_ref()
                        .and_then(|s| s.from_parent.try_recv().ok())
                });
                if let Some(msg) = msg {
                    return msg;
                }
            }
            _ = port_notify.notified() => {
                return "__browser_oxide_message_port_wake__".to_string();
            }
        }
    }
}

/// Return the URL the current worker was constructed
/// with (`new Worker(url)`). Used by `worker_bootstrap.js` to install
/// `self.location` — real Chrome's `WorkerLocation` reports the
/// worker script's URL, and some workers read
/// `self.location.origin` to verify they were loaded from an expected
/// URL. Empty `self.location` bails such a worker silently.
#[op2]
#[string]
pub fn op_worker_self_url() -> String {
    WORKER_SELF.with(|w| {
        if let Some(s) = w.borrow().as_ref() {
            s.url.clone()
        } else {
            String::new()
        }
    })
}

#[op2(fast)]
pub fn op_worker_self_is_shared() -> bool {
    WORKER_SELF.with(|worker| {
        worker
            .borrow()
            .as_ref()
            .map(|state| state.is_shared)
            .unwrap_or(false)
    })
}

#[op2]
#[string]
pub fn op_worker_self_name() -> String {
    WORKER_SELF.with(|worker| {
        worker
            .borrow()
            .as_ref()
            .map(|state| state.name.clone())
            .unwrap_or_default()
    })
}

/// Whether the worker owner may use an origin-private filesystem. Chromium
/// denies `navigator.storage.getDirectory()` in cross-site embedded frames
/// even though the same origin can use it when loaded as the top-level page.
#[op2(fast)]
pub fn op_worker_storage_directory_allowed(op_state: &mut OpState) -> bool {
    op_state
        .try_borrow::<WorkerContextState>()
        .map(|state| state.storage_directory_allowed)
        .unwrap_or(true)
}

deno_core::extension!(
    worker_extension,
    ops = [
        op_blob_register,
        op_blob_fetch_text,
        op_blob_fetch_bytes,
        op_blob_revoke,
        op_worker_sync_fetch,
        op_worker_spawn,
        op_shared_worker_connect,
        op_shared_worker_disconnect,
        op_worker_post_to_worker,
        op_worker_poll_from_worker,
        op_worker_await_message,
        op_worker_terminate,
        op_worker_self_post,
        op_worker_self_close,
        op_worker_self_url,
        op_worker_self_is_shared,
        op_worker_self_name,
        op_worker_storage_directory_allowed,
        op_worker_self_recv,
        op_worker_self_await_message,
        op_worker_last_spawn,
        op_worker_diag_note,
        op_worker_diag_read,
        op_message_port_create_pair,
        op_message_port_adopt,
        op_message_port_transfer_out,
        op_message_port_post,
        op_message_port_try_recv,
        op_message_port_close,
        op_broadcast_channel_register,
        op_broadcast_channel_post,
        op_broadcast_channel_try_recv,
        op_broadcast_channel_close,
        op_web_lock_enqueue,
        op_web_lock_wait,
        op_web_lock_cancel,
        op_web_lock_release,
        op_web_lock_wait_break,
        op_web_lock_query,
    ],
);
