//! Frame-tree signal. The JS bootstrap pushes an entry here when a script
//! inserts an `<iframe src=…>`, so the frame-tree driver can materialize it as
//! a real child context (own isolate + DOM + event loop). One `IframeSignal`
//! lives in every runtime's `OpState` because every frame runs the same
//! bootstrap; the driver drains each frame's queue in turn, so an iframe
//! inserted inside a child frame materializes as a grandchild.

use deno_core::op2;
use deno_core::OpState;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct PendingFrame {
    /// Assigned at insert time so the parent's `contentWindow` returns a routing
    /// handle immediately, before the frame materializes.
    pub frame_id: u32,
    pub host_node_id: u32,
    pub src: String,
    /// The `<iframe name>` at insert time → the frame's `window.name`.
    pub name: String,
}

#[derive(Clone, Default)]
pub struct IframeSignal(pub Arc<Mutex<Vec<PendingFrame>>>);

impl IframeSignal {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    pub fn push(&self, frame_id: u32, host_node_id: u32, src: String, name: String) {
        if let Ok(mut q) = self.0.lock() {
            q.push(PendingFrame {
                frame_id,
                host_node_id,
                src,
                name,
            });
        }
    }

    pub fn drain(&self) -> Vec<PendingFrame> {
        self.0
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }

    pub fn has_pending(&self) -> bool {
        self.0.lock().map(|q| !q.is_empty()).unwrap_or(false)
    }
}

/// Queue an iframe for materialization and return its frame id. `src` must
/// already be absolute; the bootstrap resolves it against the frame's location.
#[op2(fast)]
#[smi]
pub fn op_frame_pending(
    state: &mut OpState,
    #[smi] host_node_id: u32,
    #[string] src: &str,
    #[string] name: &str,
) -> u32 {
    let fid = next_frame_id();
    let sig = state.borrow::<IframeSignal>().clone();
    sig.push(fid, host_node_id, src.to_string(), name.to_string());
    fid
}

// Frames are separate isolates on one thread, so cross-frame postMessage routes
// through a process-global mailbox registry keyed by frame id.

/// One queued cross-frame message. `data` is the structured-clone wire JSON
/// (`serializeForWire` output).
#[derive(Clone)]
pub struct FrameMessage {
    pub source_id: u32,
    pub data: String,
    pub origin: String,
}

fn frame_mailboxes() -> &'static Mutex<HashMap<u32, VecDeque<FrameMessage>>> {
    static INST: OnceLock<Mutex<HashMap<u32, VecDeque<FrameMessage>>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

fn frame_origins() -> &'static Mutex<HashMap<u32, String>> {
    static INST: OnceLock<Mutex<HashMap<u32, String>>> = OnceLock::new();
    INST.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_FRAME_ID: AtomicU32 = AtomicU32::new(1);

/// Set when a frame posts a cross-frame message so the event loop returns early
/// and the driver delivers it at once — a multi-step frame RPC times out otherwise.
static FRAME_MSG_PENDING: AtomicBool = AtomicBool::new(false);

pub fn frame_msg_pending() -> bool {
    FRAME_MSG_PENDING.load(Ordering::Relaxed)
}

pub fn clear_frame_msg_pending() {
    // Keep the process-wide wake flag set while *any* frame tree still has
    // queued work. Clearing it unconditionally lets one Page consume another
    // Page's wake when multiple frame trees share the same LocalSet.
    //
    // Hold the mailbox lock through the store: a concurrent post cannot insert
    // a message and set the flag until after this check completes, so no wake is
    // lost between observing "all empty" and publishing false.
    if let Ok(mailboxes) = frame_mailboxes().lock() {
        if mailboxes.values().all(VecDeque::is_empty) {
            FRAME_MSG_PENDING.store(false, Ordering::Relaxed);
        }
    }
}

/// Woken when a frame posts a message so `run_until_idle` returns at once. A
/// permit-storing `Notify`: a post with no waiter still wakes the next `notified()`.
pub fn frame_msg_notify() -> &'static tokio::sync::Notify {
    static INST: OnceLock<tokio::sync::Notify> = OnceLock::new();
    INST.get_or_init(tokio::sync::Notify::new)
}

pub fn next_frame_id() -> u32 {
    NEXT_FRAME_ID.fetch_add(1, Ordering::Relaxed)
}

/// Drop a frame's mailbox so the global registry doesn't leak across navigations.
pub fn dispose_frame(frame_id: u32) {
    if let Ok(mut m) = frame_mailboxes().lock() {
        m.remove(&frame_id);
    }
    if let Ok(mut origins) = frame_origins().lock() {
        origins.remove(&frame_id);
    }
}

#[op2(fast)]
pub fn op_frame_register_origin(#[smi] frame_id: u32, #[string] origin: &str) {
    if let Ok(mut origins) = frame_origins().lock() {
        origins.insert(frame_id, origin.to_string());
    }
}

#[op2(fast)]
pub fn op_frame_post_message(
    #[smi] target_id: u32,
    #[smi] source_id: u32,
    #[string] data: &str,
    #[string] origin: &str,
    #[string] target_origin: &str,
) {
    if target_origin != "*" {
        let allowed = frame_origins()
            .lock()
            .ok()
            .and_then(|origins| origins.get(&target_id).cloned())
            .map(|actual| actual == target_origin)
            .unwrap_or(false);
        if !allowed {
            return;
        }
    }
    if std::env::var_os("BROWSER_OXIDE_FT_DEBUG").is_some() {
        eprintln!(
            "[FT-msg] {source_id}->{target_id} origin={origin} target={target_origin} data={}",
            &data[..data.len().min(2000)]
        );
    }
    if let Ok(mut m) = frame_mailboxes().lock() {
        m.entry(target_id).or_default().push_back(FrameMessage {
            source_id,
            data: data.to_string(),
            origin: origin.to_string(),
        });
    }
    FRAME_MSG_PENDING.store(true, Ordering::Relaxed);
    frame_msg_notify().notify_one();
}

/// Drain all messages queued for `frame_id` as a JSON array of
/// `{s: sourceId, d: wireJson, o: origin}`, parsed by `__pumpFrameMessages`.
#[op2]
#[string]
pub fn op_frame_take_messages(#[smi] frame_id: u32) -> String {
    let msgs: Vec<FrameMessage> = {
        match frame_mailboxes().lock() {
            Ok(mut m) => m
                .get_mut(&frame_id)
                .map(|q| q.drain(..).collect())
                .unwrap_or_default(),
            Err(_) => return "[]".to_string(),
        }
    };
    if msgs.is_empty() {
        return "[]".to_string();
    }
    let mut out = String::from("[");
    for (i, msg) in msgs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // `data` is already JSON; origin needs escaping.
        out.push_str(&format!(
            "{{\"s\":{},\"d\":{},\"o\":{}}}",
            msg.source_id,
            msg.data,
            serde_json::to_string(&msg.origin).unwrap_or_else(|_| "\"\"".to_string()),
        ));
    }
    out.push(']');
    out
}

pub fn frame_has_messages(frame_id: u32) -> bool {
    frame_mailboxes()
        .lock()
        .map(|m| m.get(&frame_id).map(|q| !q.is_empty()).unwrap_or(false))
        .unwrap_or(false)
}

deno_core::extension!(
    frame_extension,
    ops = [
        op_frame_pending,
        op_frame_register_origin,
        op_frame_post_message,
        op_frame_take_messages
    ],
);
