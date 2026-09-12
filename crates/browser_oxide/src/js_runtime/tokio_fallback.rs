//! Process-lifetime fallback tokio context for embedding threads that have
//! none (plain `fn main`, `#[test]`, sync script hosts).
//!
//! deno_core 0.408 hands every async-op future to tokio *at op-call time* via
//! `deno_unsync::spawn`, which requires an ambient **current-thread** runtime
//! context (debug-asserted) and aborts the process when the context is
//! missing — the op fn ptr is reached through C++ frames Rust cannot unwind
//! through. Page runs inside a caller-provided **current-thread** runtime, but
//! the synchronous `BrowserJsRuntime` API is also reachable from plain threads, and page
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

const MULTI_THREAD_RUNTIME_ERROR: &str = "BrowserOxide V8 runtimes require a current-thread Tokio runtime; direct use from a multi-thread Tokio runtime is unsupported because deno_core async ops are !Send. Use browser_oxide::js_runtime::block_on_v8_thread(...) or tokio::runtime::Builder::new_current_thread().";

#[cold]
#[track_caller]
fn reject_multithread_runtime() -> ! {
    panic!("{MULTI_THREAD_RUNTIME_ERROR}");
}

/// Enter the caller's current-thread Tokio context when there is one, otherwise
/// enter the shared fallback for plain synchronous embedding threads.
///
/// A Tokio multi-thread runtime is deliberately rejected. `deno_unsync` masks
/// local (`!Send`) op futures as `Send` under the invariant that the scheduler
/// is current-thread and co-located with the V8 isolate. Merely entering a
/// different current-thread runtime's handle while V8 remains on this thread
/// violates that invariant and can produce non-unwinding RefCell aborts.
/// Multi-thread applications should put the complete BrowserOxide future inside
/// [`crate::js_runtime::block_on_v8_thread`].
pub(crate) fn ensure_tokio_context() -> Option<tokio::runtime::EnterGuard<'static>> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread {
                None
            } else {
                reject_multithread_runtime()
            }
        }
        Err(_) => Some(fallback_handle().enter()),
    }
}

/// Reactor for spots that bind onto Tokio without entering (for example,
/// creating a `tokio::time::Sleep` inside an op). The same current-thread
/// contract as [`ensure_tokio_context`] applies.
pub(crate) fn reactor_handle() -> tokio::runtime::Handle {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread {
                handle
            } else {
                reject_multithread_runtime()
            }
        }
        Err(_) => fallback_handle().clone(),
    }
}
