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
// Per-thread worker "self" state — populated when a worker thread starts.
// ============================================================================

struct WorkerSelf {
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
}

thread_local! {
    static WORKER_SELF: RefCell<Option<WorkerSelf>> = const { RefCell::new(None) };
}

// ============================================================================
// Ops — parent side.
// ============================================================================

#[op2(fast)]
#[smi]
pub fn op_worker_spawn(
    op_state: &mut OpState,
    #[string] script: String,
    #[string] _name: String,
    is_module: bool,
    #[string] url: String,
    storage_directory_allowed: bool,
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
    owned.spawned_ids.borrow_mut().push(worker_id);

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
                    to_parent: to_parent_tx,
                    from_parent: to_worker_rx,
                    notify_parent: notify_parent.clone(),
                    notify_worker: notify_worker.clone(),
                    notify_message_port: notify_message_port.clone(),
                    owner_wake: owner_wake.clone(),
                    terminate: terminate.clone(),
                    url,
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
                            notify_worker.notified().await;
                        }
                        Err(e) => {
                            tracing::warn!(worker_id = worker_id, error = %e, "worker event loop error");
                            break;
                        }
                    }
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
pub fn op_worker_post_to_worker(#[smi] worker_id: i32, #[string] data: String) {
    trace_worker_wire("parent->worker", worker_id as u32, &data);
    let reg = worker_registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = reg.get(&(worker_id as u32)) {
        let _ = slot.to_worker.send(data);
        // Wake the worker's parked pump / `op_worker_self_await_message`.
        slot.notify_worker.notify_one();
    }
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
        op_worker_post_to_worker,
        op_worker_poll_from_worker,
        op_worker_await_message,
        op_worker_terminate,
        op_worker_self_post,
        op_worker_self_url,
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
    ],
);
