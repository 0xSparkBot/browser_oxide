//! "Page settled" readiness — render-quiescence, not network-idle.
//!
//! After a turn drains microtasks the page is settled once `load` has fired, the
//! turn mutated no DOM structurally, and either something rendered since load or
//! no content fetch is in flight. Live pages never reach network-idle (perpetual
//! scheduler / telemetry / long-poll), and none of that touches the DOM, so only
//! in-flight content fetches hold readiness open. Structural mutations are counted
//! via `bump_mutation` (attribute/style writes are not). Counters are thread-local,
//! aggregating the whole frame tree into one signal.

use std::cell::Cell;
use std::rc::Rc;
use tokio::sync::Notify;

struct Readiness {
    epoch: Cell<u64>,
    load_fired: Cell<bool>,
    in_flight: Cell<u32>,
    /// Monotonic count of structural DOM mutations this navigation.
    mutations: Cell<u64>,
    /// `mutations` as of the last [`settle_poll`], to detect a quiet turn.
    last_poll: Cell<u64>,
    /// Set once the page has rendered (a structural mutation has happened).
    rendered: Cell<bool>,
    notify: Rc<Notify>,
}

thread_local! {
    static R: Readiness = Readiness {
        epoch: Cell::new(0),
        load_fired: Cell::new(false),
        in_flight: Cell::new(0),
        mutations: Cell::new(0),
        last_poll: Cell::new(0),
        rendered: Cell::new(false),
        notify: Rc::new(Notify::new()),
    };
}

/// Fired on every transition (load / in-flight / DOM mutation) so a driver
/// parked on background work re-polls and re-checks settledness.
pub fn notify() -> Rc<Notify> {
    R.with(|r| r.notify.clone())
}

pub fn epoch() -> u64 {
    R.with(|r| r.epoch.get())
}

/// One structural DOM mutation (node inserted/removed, text changed). Called by
/// the DOM ops. Marks the page as having rendered and wakes any settle waiter.
pub fn bump_mutation() {
    R.with(|r| {
        r.mutations.set(r.mutations.get().wrapping_add(1));
        r.rendered.set(true);
        r.notify.notify_one();
    });
}

/// Poll settledness for one event-loop turn. Must be called exactly once per
/// turn, *after* `poll_event_loop` has drained the turn's microtasks: it treats
/// "no structural mutation since the previous call" as this turn being quiet.
/// Returns true when the page has loaded, this turn was quiet, and the page has
/// rendered (or has no content fetch outstanding).
pub fn settle_poll() -> bool {
    R.with(|r| {
        let m = r.mutations.get();
        let quiet = m == r.last_poll.get();
        r.last_poll.set(m);
        r.load_fired.get() && quiet && (r.rendered.get() || r.in_flight.get() == 0)
    })
}

/// New navigation: clear per-nav state and bump the epoch so a stale waiter
/// from the previous nav bails.
pub fn reset() {
    R.with(|r| {
        r.epoch.set(r.epoch.get().wrapping_add(1));
        r.load_fired.set(false);
        r.in_flight.set(0);
        r.mutations.set(0);
        r.last_poll.set(0);
        r.rendered.set(false);
        r.notify.notify_one();
    });
}

/// Called from `op_lifecycle_load`, inside the load-dispatch callback, after
/// `window.dispatchEvent(new Event('load'))`.
pub fn mark_load() {
    R.with(|r| {
        r.load_fired.set(true);
        r.notify.notify_one();
    });
}

/// RAII guard for one in-flight async content fetch (`fetch()` / module import).
/// Only gates readiness before the first render (an SSR/prefetch page that never
/// mutates the DOM still settles once its fetches finish); once the page has
/// rendered, background fetches no longer hold it open.
pub struct RequestGuard(());

impl RequestGuard {
    pub fn new() -> Self {
        R.with(|r| {
            r.in_flight.set(r.in_flight.get() + 1);
            r.notify.notify_one();
        });
        RequestGuard(())
    }
}

impl Default for RequestGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        R.with(|r| {
            r.in_flight.set(r.in_flight.get().saturating_sub(1));
            r.notify.notify_one();
        });
    }
}

#[deno_core::op2(fast)]
pub fn op_lifecycle_load() {
    mark_load();
}

deno_core::extension!(readiness_extension, ops = [op_lifecycle_load]);
