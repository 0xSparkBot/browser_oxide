use deno_core::op2;
use deno_core::OpState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static SPEECH_VOICE_BACKEND_WARM: AtomicBool = AtomicBool::new(false);

/// Chromium services some renderer APIs on the browser main thread. Certain
/// cold initialisations (notably speech synthesis voice discovery on macOS)
/// occupy that lane for a while even though the renderer-side API returns and
/// keeps running. A later synchronous browser-service call must wait behind
/// that work. Keep a process-wide lane model so independent frame runtimes
/// observe the same browser-process serialization.
fn browser_main_busy_until() -> &'static Mutex<Option<Instant>> {
    static BUSY_UNTIL: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    BUSY_UNTIL.get_or_init(|| Mutex::new(None))
}

fn speech_voice_backend_ready_at() -> &'static Mutex<Option<Instant>> {
    static READY_AT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    READY_AT.get_or_init(|| Mutex::new(None))
}

fn occupy_browser_main_lane(duration: Duration) {
    if duration.is_zero() {
        return;
    }
    let target = Instant::now() + duration;
    let mut guard = browser_main_busy_until()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if guard.is_none_or(|current| current < target) {
        *guard = Some(target);
    }
}

/// Wait until browser-main work that started earlier has drained. This models
/// a synchronous Mojo/browser-service request from the renderer: the request
/// itself can be tiny, but it cannot be dispatched while the browser main
/// thread is occupied by an earlier task.
pub(crate) fn wait_for_browser_main_service_lane() {
    loop {
        let remaining = {
            let guard = browser_main_busy_until()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            guard.and_then(|until| until.checked_duration_since(Instant::now()))
        };
        let Some(remaining) = remaining else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining);
    }
}

fn refresh_speech_voice_backend_warm_state() -> bool {
    if SPEECH_VOICE_BACKEND_WARM.load(Ordering::Relaxed) {
        return true;
    }
    let ready = {
        let guard = speech_voice_backend_ready_at()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.is_some_and(|at| Instant::now() >= at)
    };
    if ready {
        SPEECH_VOICE_BACKEND_WARM.store(true, Ordering::Relaxed);
        let mut guard = speech_voice_backend_ready_at()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }
    ready
}

/// Timer state stored in OpState.
pub struct TimerState {
    next_id: i32,
    pub pending: HashMap<i32, TimerInfo>,
    pub cancelled: std::collections::HashSet<i32>,
}

#[derive(Debug, Clone)]
pub struct TimerInfo {
    pub delay_ms: u64,
    pub is_interval: bool,
}

impl Default for TimerState {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerState {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            pending: HashMap::new(),
            cancelled: std::collections::HashSet::new(),
        }
    }
}

#[op2(fast)]
#[smi]
pub fn op_set_timeout(state: &mut OpState, #[smi] delay_ms: i32) -> i32 {
    let state = state.borrow_mut::<TimerState>();
    let id = state.next_id;
    state.next_id += 1;
    state.pending.insert(
        id,
        TimerInfo {
            delay_ms: delay_ms.max(0) as u64,
            is_interval: false,
        },
    );
    id
}

#[op2(fast)]
#[smi]
pub fn op_set_interval(state: &mut OpState, #[smi] delay_ms: i32) -> i32 {
    let state = state.borrow_mut::<TimerState>();
    let id = state.next_id;
    state.next_id += 1;
    state.pending.insert(
        id,
        TimerInfo {
            delay_ms: delay_ms.max(4) as u64,
            is_interval: true,
        },
    );
    id
}

#[op2(fast)]
pub fn op_clear_timer(state: &mut OpState, #[smi] id: i32) {
    let state = state.borrow_mut::<TimerState>();
    state.cancelled.insert(id);
    state.pending.remove(&id);
}

/// Async sleep for `ms` milliseconds. Used by JS setTimeout/setInterval.
// Use `async(deferred), fast`, not `async(lazy), fast`: `lazy` never
// eager-polls, deferring every timer callback by an extra event-loop turn,
// which shifts setTimeout/setInterval timing away from real Chrome. (`fast`
// plus plain `async` is rejected by deno_ops 0.187, so `deferred` is the way
// to keep the fast path.) `deferred` eager-polls and stays `fast`-compatible,
// matching Chrome's timer cadence.
#[op2(async(deferred), fast)]
pub async fn op_timer_sleep(#[smi] ms: i32) {
    let sleep = {
        // `tokio::time::sleep` binds to a reactor at creation time; bootstrap
        // scripts schedule timers during `execute_script`, which is reachable
        // from plain threads with no ambient runtime. Enter a process-lifetime
        // fallback there — mirroring the V8 delayed-task fallback in
        // `runtime.rs` — so timer ops never panic (a panic here aborts: V8
        // calls the fn ptr through C++ frames that cannot unwind). The future
        // itself is polled by the caller's event loop and needs no context.
        let _guard = timer_reactor().enter();
        tokio::time::sleep(tokio::time::Duration::from_millis(ms.max(0) as u64))
    };
    sleep.await;
}

/// Private, engine-side timer diagnostics. This is intentionally exposed only
/// as an op consumed by the bootstrap: no page-visible global is created. The
/// JS side sends a pre-sanitized schema (numbers/bools/string lengths/object
/// keys only), never opaque string contents.
#[op2(fast)]
pub fn op_timer_private_trace_enabled() -> bool {
    std::env::var_os("BROWSER_OXIDE_TIMER_PRIVATE_TRACE").is_some()
}

#[op2(fast)]
pub fn op_timer_private_trace(#[string] summary: String) {
    if std::env::var_os("BROWSER_OXIDE_TIMER_PRIVATE_TRACE").is_some() {
        eprintln!("[timer-private] {summary}");
    }
}

#[op2(fast)]
pub fn op_timer_wire_trace_enabled() -> bool {
    std::env::var_os("BROWSER_OXIDE_TIMER_WIRE_TRACE").is_some()
}

#[op2(fast)]
pub fn op_timer_wire_trace(#[string] summary: String) {
    if std::env::var_os("BROWSER_OXIDE_TIMER_WIRE_TRACE").is_some() {
        eprintln!("[timer-wire] {summary}");
    }
}

#[op2(fast)]
pub fn op_event_private_trace_enabled() -> bool {
    std::env::var_os("BROWSER_OXIDE_EVENT_PRIVATE_TRACE").is_some()
}

#[op2(fast)]
pub fn op_event_private_trace(#[string] summary: String) {
    if std::env::var_os("BROWSER_OXIDE_EVENT_PRIVATE_TRACE").is_some() {
        eprintln!("[event-private] {summary}");
    }
}

#[op2(fast)]
pub fn op_speech_voice_backend_is_warm() -> bool {
    refresh_speech_voice_backend_warm_state()
}

/// Begin the process-wide speech voice backend cold initialisation. The first
/// caller owns the browser-main task; concurrent frames observe the same
/// in-flight initialisation and must not extend it.
#[op2(fast)]
pub fn op_speech_voice_backend_begin_init(#[smi] delay_ms: i32) -> bool {
    if refresh_speech_voice_backend_warm_state() {
        return false;
    }
    let duration = Duration::from_millis(delay_ms.max(0) as u64);
    let started = {
        let mut guard = speech_voice_backend_ready_at()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            false
        } else {
            *guard = Some(Instant::now() + duration);
            true
        }
    };
    if started {
        occupy_browser_main_lane(duration);
    }
    started
}

#[op2(fast)]
pub fn op_speech_voice_backend_mark_warm() {
    SPEECH_VOICE_BACKEND_WARM.store(true, Ordering::Relaxed);
    let mut guard = speech_voice_backend_ready_at()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

#[op2(fast)]
pub fn op_speech_private_trace_enabled() -> bool {
    std::env::var_os("BROWSER_OXIDE_SPEECH_PRIVATE_TRACE").is_some()
}

#[op2(fast)]
pub fn op_speech_private_trace(#[string] summary: String) {
    if std::env::var_os("BROWSER_OXIDE_SPEECH_PRIVATE_TRACE").is_some() {
        eprintln!("[speech-private] {summary}");
    }
}

#[op2(fast)]
pub fn op_keyboard_private_trace_enabled() -> bool {
    std::env::var_os("BROWSER_OXIDE_KEYBOARD_PRIVATE_TRACE").is_some()
}

#[op2(fast)]
pub fn op_keyboard_private_trace(#[string] summary: String) {
    if std::env::var_os("BROWSER_OXIDE_KEYBOARD_PRIVATE_TRACE").is_some() {
        eprintln!("[keyboard-private] {summary}");
    }
}

/// Reactor for timer ops: the caller's tokio context when there is one,
/// otherwise the shared process-lifetime fallback (see `tokio_fallback`).
fn timer_reactor() -> tokio::runtime::Handle {
    crate::js_runtime::tokio_fallback::reactor_handle()
}

deno_core::extension!(
    timer_extension,
    ops = [
        op_set_timeout,
        op_set_interval,
        op_clear_timer,
        op_timer_sleep,
        op_timer_private_trace_enabled,
        op_timer_private_trace,
        op_timer_wire_trace_enabled,
        op_timer_wire_trace,
        op_event_private_trace_enabled,
        op_event_private_trace,
        op_speech_voice_backend_is_warm,
        op_speech_voice_backend_begin_init,
        op_speech_voice_backend_mark_warm,
        op_speech_private_trace_enabled,
        op_speech_private_trace,
        op_keyboard_private_trace_enabled,
        op_keyboard_private_trace
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_main_service_lane_serializes_sync_requests() {
        // Keep the interval short so parallel tests can at worst experience a
        // small scheduling delay, while still proving that a synchronous
        // browser-service operation waits for earlier browser-main work.
        let started = Instant::now();
        occupy_browser_main_lane(Duration::from_millis(25));
        wait_for_browser_main_service_lane();
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(20),
            "service lane returned too early: {elapsed:?}"
        );

        let drained = Instant::now();
        wait_for_browser_main_service_lane();
        assert!(
            drained.elapsed() < Duration::from_millis(20),
            "drained service lane should not keep blocking"
        );
    }
}
