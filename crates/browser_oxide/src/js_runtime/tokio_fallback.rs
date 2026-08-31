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
    if tokio::runtime::Handle::try_current().is_ok() {
        return None;
    }
    Some(fallback_handle().enter())
}

/// Reactor for spots that bind onto tokio without entering (e.g. creating a
/// `tokio::time::Sleep` inside an op): the caller's runtime when present,
/// otherwise the shared fallback.
pub(crate) fn reactor_handle() -> tokio::runtime::Handle {
    tokio::runtime::Handle::try_current().unwrap_or_else(|_| fallback_handle().clone())
}
