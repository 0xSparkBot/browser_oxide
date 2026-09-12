//! Process-lifetime fallback tokio context for embedding threads that have
//! none (plain `fn main`, `#[test]`, sync script hosts).
//!
//! deno_core 0.408 hands every async-op future to tokio *at op-call time* via
//! `deno_unsync::spawn`, which requires an ambient **current-thread** runtime
//! context (debug-asserted) and aborts the process when the context is
//! missing — the op fn ptr is reached through C++ frames Rust cannot unwind
//! through. Page runs inside a caller-provided runtime, but the synchronous
//! `BrowserJsRuntime` API is also reachable from plain threads, and page
//! scripts schedule timers (bootstrap code calls `setTimeout`) during plain
//! `execute_script` calls.
//!
//! The fallback is a live current-thread runtime owned by a dedicated driver
//! thread (`rt.block_on(pending)`), shared through its `Handle`. Embedder
//! threads only ever `enter()` the handle and await; the driver thread pumps
//! the scheduler and timer wheel, so spawned op futures and V8 delayed tasks
//! actually run. Verified against tokio 1.53: `Handle::block_on` does *not*
//! drive current-thread scheduler tasks from a foreign thread — the runtime
//! body must be `block_on`-held by the driver thread itself.

use std::sync::OnceLock;

static FALLBACK_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

fn fallback_handle() -> &'static tokio::runtime::Handle {
    FALLBACK_HANDLE.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build fallback tokio runtime for the JS runtime");
        let handle = runtime.handle().clone();
        std::thread::Builder::new()
            .name("browser-oxide-js-tokio".to_string())
            .spawn(move || {
                runtime.block_on(std::future::pending::<()>());
            })
            .expect("failed to spawn fallback tokio driver thread");
        handle
    })
}

/// Enter the caller's tokio context when there is one, otherwise the shared
/// fallback. Hold the returned guard for the duration of any JS execution or
/// event-loop poll that can reach an async op.
pub(crate) fn ensure_tokio_context() -> Option<tokio::runtime::EnterGuard<'static>> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread => {
            None
        }
        // deno_unsync async ops are !Send and may only be spawned on a
        // current-thread Tokio executor. A caller-provided multi-thread runtime
        // is therefore just as unsuitable as having no Tokio context at all:
        // enter the process-lifetime current-thread fallback for the duration
        // of the V8/deno_core call instead of letting deno_unsync abort.
        _ => Some(fallback_handle().enter()),
    }
}

/// Reactor for spots that bind onto tokio without entering (e.g. creating a
/// `tokio::time::Sleep` inside an op): reuse the caller only when it is a
/// current-thread runtime; otherwise use the shared current-thread fallback.
pub(crate) fn reactor_handle() -> tokio::runtime::Handle {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread => {
            handle
        }
        _ => fallback_handle().clone(),
    }
}
