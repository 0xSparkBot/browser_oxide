//! Navigation-pending signal — fast cross-thread flag the JS bootstrap
//! flips when `__pendingNavigation` (or any equivalent setter) is set.
//!
//! Why this exists: the navigation pipeline in `Page::navigate` runs an
//! iteration loop where each iteration does `event_loop.run_until_idle(30s)`.
//! Without this signal, when a script sets `location.href = ...` the
//! iteration still runs to its 30-second ceiling before the retry GET
//! fires — too late for sites with strict navigation-timing windows
//! (some expect the follow-up GET within a few seconds of a prior request).
//!
//! This op flips an `Arc<AtomicBool>` shared with `BrowserEventLoop`. The
//! event loop checks the flag each tick and exits early (after a brief
//! microtask tail to let in-flight `fetch().then(setCookie)` land in the
//! jar before the retry).

use deno_core::op2;
use deno_core::OpState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

/// State in OpState, cloned by `BrowserJsRuntime::nav_pending_signal`. The flag
/// lets the event loop poll without V8; the `Notify` wakes it on a raised nav.
#[derive(Clone, Default)]
pub struct NavSignal {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl NavSignal {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn raise(&self) {
        self.flag.store(true, Ordering::Relaxed);
        // The op runs inside a poll before any waiter exists; `notify_one`
        // stores a permit that the next `notified()` consumes.
        self.notify.notify_one();
    }

    pub fn pending(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.flag.store(false, Ordering::Relaxed);
    }

    pub fn notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }
}

#[op2(fast)]
pub fn op_set_pending_nav(s: &mut OpState) {
    let s = s.borrow::<NavSignal>();
    s.raise();
}

deno_core::extension!(nav_extension, ops = [op_set_pending_nav],);
