//! Browser event loop wrapping deno_core's V8 event loop with
//! timer scheduling, requestAnimationFrame, and idle detection.

use crate::js_runtime::BrowserJsRuntime;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// PROFILER (env-gated, near-zero overhead when disabled)
// ---------------------------------------------------------------------------
//
// Enable with `BROWSER_OXIDE_EVENT_LOOP_PROFILE=1`. When active, every
// `run_until_idle` invocation captures per-tick wall-clock plus the
// deno_core RuntimeActivity snapshot (pending async ops, timers, intervals,
// resources) and dumps a histogram + per-tick CSV to stderr at exit.
//
// Used to root-cause SPA hydration timeouts where the page body remains
// near-empty after the nav budget elapses.

#[derive(Clone, Copy, Default, Debug)]
struct TickRow {
    tick: u32,
    wall_us: u64,           // tick wall-clock duration (microseconds)
    pending_async_ops: u32, // in-flight ops (op_fetch, op_sleep, ...)
    pending_timers: u32,    // setTimeout entries
    pending_intervals: u32, // setInterval entries
    pending_resources: u32, // open ResourceTable handles
    timed_out: bool,        // tick hit its 100ms slice ceiling without idling
}

#[inline(always)]
fn profile_enabled() -> bool {
    // Cached after first call — env var lookups are syscalls on every tick
    // when nested in run_until_idle, which would itself perturb the timing
    // we're measuring. Read once per process.
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BROWSER_OXIDE_EVENT_LOOP_PROFILE").as_deref(),
            Ok("1") | Ok("true") | Ok("yes")
        )
    })
}

/// Per-op-name aggregate so the profile dump can name the dominant op.
/// Stored in a thread-local because dump_profile takes only `rows` and we
/// don't want to bloat TickRow with a HashMap. Reset at start of each
/// `run_until_idle` invocation.
type OpNameMap = std::collections::HashMap<&'static str, u64>;

thread_local! {
    static OP_NAME_TOTALS: std::cell::RefCell<OpNameMap> =
        std::cell::RefCell::new(OpNameMap::new());
}

/// Capture the current pending-task counts from the underlying deno_core
/// runtime. Cheap-ish: walks 3-4 small Vecs and clones the activity
/// snapshot. Only called when profiling is enabled.
fn capture_pending(runtime: &mut BrowserJsRuntime) -> (u32, u32, u32, u32) {
    use deno_core::stats::{RuntimeActivity, RuntimeActivityStatsFilter};
    let factory = runtime.inner().runtime_activity_stats_factory();
    let stats = factory.capture(&RuntimeActivityStatsFilter::all());
    let snap = stats.dump();
    let mut ops = 0u32;
    let mut timers = 0u32;
    let mut intervals = 0u32;
    let mut resources = 0u32;
    OP_NAME_TOTALS.with(|m| {
        let mut m = m.borrow_mut();
        for a in snap.active.iter() {
            match a {
                RuntimeActivity::AsyncOp(_, _, name) => {
                    ops += 1;
                    *m.entry(*name).or_insert(0) += 1;
                }
                RuntimeActivity::Timer(..) => timers += 1,
                RuntimeActivity::Interval(..) => intervals += 1,
                RuntimeActivity::Resource(..) => resources += 1,
            }
        }
    });
    (ops, timers, intervals, resources)
}

/// Pretty-print a sequence of `TickRow`s to stderr as a profile dump.
/// Sections: header, top-N slowest ticks, growth check (quadratic
/// detector), and CSV tail for offline analysis.
fn dump_profile(label: &str, rows: &[TickRow], total: Duration, reason: IdleReason) {
    use std::io::Write;
    let stderr = std::io::stderr();
    let mut w = stderr.lock();

    let _ = writeln!(
        w,
        "\n========== BROWSER_OXIDE EVENT-LOOP PROFILE =========="
    );
    let _ = writeln!(w, "label              : {}", label);
    let _ = writeln!(w, "reason             : {:?}", reason);
    let _ = writeln!(w, "total wall (ms)    : {}", total.as_millis());
    let _ = writeln!(w, "ticks              : {}", rows.len());
    if rows.is_empty() {
        let _ = writeln!(w, "(no ticks recorded — instantaneous idle)");
        let _ = writeln!(w, "================================================\n");
        return;
    }

    let total_us: u64 = rows.iter().map(|r| r.wall_us).sum();
    let timed_out: usize = rows.iter().filter(|r| r.timed_out).count();
    let max_tick = rows.iter().max_by_key(|r| r.wall_us).copied().unwrap();
    let avg_us = total_us / rows.len() as u64;

    let _ = writeln!(w, "total tick us      : {}", total_us);
    let _ = writeln!(w, "avg tick us        : {}", avg_us);
    let _ = writeln!(w, "ticks-timed-out    : {}", timed_out);
    let _ = writeln!(
        w,
        "max tick           : #{} {}us pending(ops={}, timers={}, intervals={}, res={})",
        max_tick.tick,
        max_tick.wall_us,
        max_tick.pending_async_ops,
        max_tick.pending_timers,
        max_tick.pending_intervals,
        max_tick.pending_resources,
    );

    // Top-10 slowest ticks
    let mut sorted = rows.to_vec();
    sorted.sort_unstable_by_key(|r| std::cmp::Reverse(r.wall_us));
    let _ = writeln!(w, "\n--- top-10 slowest ticks ---");
    let _ = writeln!(
        w,
        "  tick   wall_us  ops  timers  intervals  res  timed_out"
    );
    for r in sorted.iter().take(10) {
        let _ = writeln!(
            w,
            "  {:5}  {:>7}  {:>3}  {:>6}  {:>9}  {:>3}  {}",
            r.tick,
            r.wall_us,
            r.pending_async_ops,
            r.pending_timers,
            r.pending_intervals,
            r.pending_resources,
            r.timed_out,
        );
    }

    // Pending-task histogram across all ticks
    let max_ops = rows.iter().map(|r| r.pending_async_ops).max().unwrap_or(0);
    let max_timers = rows.iter().map(|r| r.pending_timers).max().unwrap_or(0);
    let max_intervals = rows.iter().map(|r| r.pending_intervals).max().unwrap_or(0);
    let final_row = rows.last().copied().unwrap();
    let _ = writeln!(w, "\n--- pending-task envelope ---");
    let _ = writeln!(w, "  max async-ops   : {}", max_ops);
    let _ = writeln!(w, "  max timers      : {}", max_timers);
    let _ = writeln!(w, "  max intervals   : {}", max_intervals);
    let _ = writeln!(
        w,
        "  final pending   : ops={} timers={} intervals={} res={}",
        final_row.pending_async_ops,
        final_row.pending_timers,
        final_row.pending_intervals,
        final_row.pending_resources,
    );

    // Quadratic / monotonic-growth detector. We chunk into quartiles and
    // compare avg pending counts per quartile — if Q4 > 4 × Q1 it's a
    // strong sign of unbounded accumulation (Promise.then bombs,
    // MutationObserver flooding, runaway IntersectionObserver, ...).
    let n = rows.len();
    if n >= 8 {
        let q = n / 4;
        let avg_ops = |slice: &[TickRow]| -> f64 {
            slice
                .iter()
                .map(|r| r.pending_async_ops as u64)
                .sum::<u64>() as f64
                / slice.len() as f64
        };
        let avg_t = |slice: &[TickRow]| -> f64 {
            slice.iter().map(|r| r.pending_timers as u64).sum::<u64>() as f64 / slice.len() as f64
        };
        let q1_ops = avg_ops(&rows[..q]);
        let q4_ops = avg_ops(&rows[n - q..]);
        let q1_t = avg_t(&rows[..q]);
        let q4_t = avg_t(&rows[n - q..]);
        let _ = writeln!(w, "\n--- growth detector (quartile averages) ---");
        let _ = writeln!(
            w,
            "  ops    Q1={:.1}  Q4={:.1}  ratio={:.2}x",
            q1_ops,
            q4_ops,
            if q1_ops > 0.0 { q4_ops / q1_ops } else { 0.0 }
        );
        let _ = writeln!(
            w,
            "  timers Q1={:.1}  Q4={:.1}  ratio={:.2}x",
            q1_t,
            q4_t,
            if q1_t > 0.0 { q4_t / q1_t } else { 0.0 }
        );
        if (q1_ops > 0.0 && q4_ops / q1_ops > 4.0) || (q1_t > 0.0 && q4_t / q1_t > 4.0) {
            let _ = writeln!(
                w,
                "  WARNING: pending-task count > 4x growth Q1→Q4 — likely runaway scheduler"
            );
        }
    }

    // Top op names — names of the ops that were observed pending across
    // all ticks (counts are sum of "snapshot pending count" — i.e. an op
    // that stayed pending for N ticks contributes N). High-count names
    // identify the chain that's keeping is_pending=true.
    OP_NAME_TOTALS.with(|m| {
        let m = m.borrow();
        if !m.is_empty() {
            let mut v: Vec<(&&'static str, &u64)> = m.iter().collect();
            v.sort_unstable_by_key(|(_, c)| std::cmp::Reverse(**c));
            let _ = writeln!(
                w,
                "\n--- pending-op name breakdown (sum of per-tick pending counts) ---"
            );
            for (name, count) in v.iter().take(15) {
                let _ = writeln!(w, "  {:>8}  {}", count, name);
            }
        }
    });

    // CSV tail for offline crunching (paste into a spreadsheet / Pandas).
    let _ = writeln!(
        w,
        "\n--- per-tick CSV (tick,wall_us,ops,timers,intervals,res,timed_out) ---"
    );
    for r in rows.iter() {
        let _ = writeln!(
            w,
            "EL-CSV,{},{},{},{},{},{},{}",
            r.tick,
            r.wall_us,
            r.pending_async_ops,
            r.pending_timers,
            r.pending_intervals,
            r.pending_resources,
            if r.timed_out { 1 } else { 0 },
        );
    }
    let _ = writeln!(w, "================================================\n");
}

/// The browser event loop. Drives JS execution, timers, and async ops.
pub struct BrowserEventLoop {
    runtime: BrowserJsRuntime,
}

/// Why the event loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleReason {
    /// All pending work completed (no timers, no promises, no async ops).
    AllWorkDone,
    /// The page settled: load fired and the DOM stopped mutating, with
    /// background timers/rAF/network still pending (via `run_until_settled`).
    Settled,
    /// The timeout was reached.
    Timeout,
}

impl BrowserEventLoop {
    pub fn new(runtime: BrowserJsRuntime) -> Self {
        Self { runtime }
    }

    /// Drive the event loop, waker-driven with no timer. With `honor_settle`,
    /// exit on render-quiescence ([`IdleReason::Settled`]); else on full idle.
    async fn run_until_inner(
        &mut self,
        honor_settle: bool,
    ) -> Result<IdleReason, deno_core::error::AnyError> {
        // Profiling state — only allocates when the env var is set, so
        // production overhead is one OnceLock-cached load + a branch.
        let profiling = profile_enabled();
        if profiling {
            OP_NAME_TOTALS.with(|m| m.borrow_mut().clear());
        }
        let profile_start = if profiling {
            Some(Instant::now())
        } else {
            None
        };
        let mut rows: Vec<TickRow> = if profiling {
            Vec::with_capacity(2048)
        } else {
            Vec::new()
        };
        let mut tick_idx: u32 = 0;

        // Wakes only on real events (idle, frame message, navigation, readiness);
        // no poll cadence or timer.
        let nav_notify = self.runtime.nav_notify();

        // With `honor_settle`, its Notify wakes the select on each mutation/load
        // transition so a page parked on background work re-checks promptly.
        let ready_notify = crate::js_runtime::readiness::notify();

        let outcome: Result<IdleReason, deno_core::error::AnyError> = loop {
            // A frame posted a cross-frame message — hand back so the driver
            // delivers it now. Only ever set in frame-tree mode.
            if crate::js_runtime::extensions::frame_ext::frame_msg_pending() {
                break Ok(IdleReason::AllWorkDone);
            }

            // JS-triggered navigation: flush the microtask tail (a fetch `.then`
            // that wrote a cookie or set the nav) before handing off.
            if self.runtime.nav_pending() {
                self.runtime.drain_microtasks();
                break Ok(IdleReason::AllWorkDone);
            }

            let tick_t0 = if profiling {
                Some(Instant::now())
            } else {
                None
            };
            // `biased` keeps V8 progress first; the frame/nav/readiness wakes
            // let those transitions resume with no poll latency.
            let frame_notify = crate::js_runtime::extensions::frame_ext::frame_msg_notify();
            let ready_fut = ready_notify.notified();
            tokio::pin!(ready_fut);
            let result: Result<Result<(), deno_core::error::AnyError>, ()> = tokio::select! {
                biased;
                r = self.runtime.run_event_loop() => Ok(r),
                _ = frame_notify.notified() => Err(()),
                _ = nav_notify.notified() => Err(()),
                _ = &mut ready_fut, if honor_settle => Err(()),
            };

            // Microtasks drained this turn, so a quiet `settle_poll` means the
            // render stabilized. Call once per turn: it compares across turns.
            if honor_settle && crate::js_runtime::readiness::settle_poll() {
                break Ok(IdleReason::Settled);
            }

            // Capture pending-task snapshot AFTER the tick (so the row
            // reflects what's still in-flight). Skipped when profiling
            // disabled — capture_pending walks several Vecs and dumps the
            // activity snapshot, ~10-50us per call on x.com-class loads.
            if profiling {
                let elapsed = tick_t0.unwrap().elapsed().as_micros() as u64;
                let (ops, timers, intervals, resources) = capture_pending(&mut self.runtime);
                rows.push(TickRow {
                    tick: tick_idx,
                    wall_us: elapsed,
                    pending_async_ops: ops,
                    pending_timers: timers,
                    pending_intervals: intervals,
                    pending_resources: resources,
                    timed_out: result.is_err(),
                });
                tick_idx = tick_idx.wrapping_add(1);
            }

            match result {
                Ok(Ok(())) => {
                    // Event loop completed all work
                    break Ok(IdleReason::AllWorkDone);
                }
                Ok(Err(e)) => break Err(e),
                Err(_woke) => {
                    // A frame/nav/readiness notify woke us (not full idle);
                    // re-check the exits at the top and re-drive.
                    continue;
                }
            }
        };

        if profiling {
            let total = profile_start.map(|s| s.elapsed()).unwrap_or_default();
            let label = std::env::var("BROWSER_OXIDE_EVENT_LOOP_PROFILE_LABEL")
                .unwrap_or_else(|_| "run_until_idle".to_string());
            let reason = match &outcome {
                Ok(r) => *r,
                Err(_) => IdleReason::Timeout, // best-effort tag
            };
            dump_profile(&label, &rows, total, reason);
        }

        outcome
    }

    /// Drive until the event loop is idle (deno_core `AllWorkDone`), waker-driven
    /// with no timer; callers wanting a deadline wrap this in `tokio::time::timeout`.
    pub async fn run_until_idle(&mut self) -> Result<IdleReason, deno_core::error::AnyError> {
        self.run_until_inner(false).await
    }

    /// Drive until the page has settled (load fired + DOM stopped mutating),
    /// leaving background timers/rAF/network pending; returns [`IdleReason::Settled`].
    pub async fn run_until_settled(&mut self) -> Result<IdleReason, deno_core::error::AnyError> {
        self.run_until_inner(true).await
    }

    /// Execute a script in the runtime.
    pub fn execute_script(&mut self, code: &str) -> Result<String, deno_core::error::AnyError> {
        self.runtime.execute_script(code, None)
    }

    /// Flush the microtask queue. See [`BrowserJsRuntime::drain_microtasks`].
    pub fn drain_microtasks(&mut self) {
        self.runtime.drain_microtasks();
    }

    /// Execute a script in the runtime with a given source name.
    pub fn execute_script_with_name(
        &mut self,
        code: &str,
        name: &str,
    ) -> Result<String, deno_core::error::AnyError> {
        self.runtime.execute_script(code, Some(name))
    }

    /// P2 — execute an EXTERNAL ES module (`<script type="module" src>`) via the
    /// module loader (fetches the import graph) instead of classic compile.
    pub async fn eval_module_url(&mut self, url: &str) -> Result<(), deno_core::error::AnyError> {
        self.runtime.load_eval_module_url(url).await
    }

    /// P2 — execute an INLINE ES module. `specifier` is the document URL plus a
    /// unique fragment so its relative imports resolve against the document.
    pub async fn eval_module_code(
        &mut self,
        specifier: &str,
        code: String,
    ) -> Result<(), deno_core::error::AnyError> {
        self.runtime.load_eval_module_code(specifier, code).await
    }

    /// Run scripts then wait for idle.
    pub async fn execute_and_run(
        &mut self,
        code: &str,
        timeout: Duration,
    ) -> Result<IdleReason, deno_core::error::AnyError> {
        self.runtime.execute_script(code, None)?;
        // Caller-provided deadline lives at the boundary via `tokio::time::timeout`,
        // not inside the waker-driven drive.
        match tokio::time::timeout(timeout, self.run_until_idle()).await {
            Ok(r) => r,
            Err(_) => Ok(IdleReason::Timeout),
        }
    }

    /// Get the underlying runtime.
    pub fn runtime(&self) -> &BrowserJsRuntime {
        &self.runtime
    }

    /// Poll this frame's event loop once for the unified frame-tree driver.
    pub fn poll_once(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), deno_core::error::AnyError>> {
        self.runtime.poll_once(cx)
    }

    /// Deliver queued cross-frame messages into this frame (cached-fn call).
    pub fn deliver_frame_messages(&mut self) {
        self.runtime.deliver_frame_messages();
    }

    /// `--inspect-brk`: block until a CDP frontend attaches, then pause at the
    /// next statement. No-op unless the inspector is running. Debug-only.
    pub fn inspector_break_on_next_statement(&mut self) {
        self.runtime.inspector_break_on_next_statement();
    }

    /// This frame's queue of `<iframe src>` insertions awaiting materialization.
    pub fn iframe_signal(&mut self) -> crate::js_runtime::extensions::frame_ext::IframeSignal {
        self.runtime.iframe_signal()
    }

    /// Reset the runtime's pending-navigation signal. Called by callers
    /// that legitimately set `location.href` for URL-state setup (not as
    /// a real navigation trigger) — without this, subsequent
    /// `run_until_idle` calls would see nav_pending=true and short-circuit
    /// immediately, breaking timer-based tests.
    ///
    /// Also scrubs the JS-side `_browser_oxide.__pendingNavigation` value,
    /// which the `location.href = …` setter writes as a side-effect. Without
    /// the JS-side scrub the navigate loop's `PENDING_NAV_JS` reads back a
    /// spurious `{kind: "assign"}` after every initial URL setup and burns
    /// `max_iterations` round-trips re-fetching the same URL — observed as
    /// 6.5 s for an empty `example.com` because each iteration spins up a
    /// fresh V8 isolate before realising nothing actually requested a nav.
    pub fn reset_nav_pending(&mut self) {
        self.runtime.reset_nav_pending();
        let _ = self.runtime.execute_script(
            "globalThis._browser_oxide && (globalThis._browser_oxide.__pendingNavigation = null);",
            None,
        );
    }

    /// Get a mutable reference to the underlying runtime.
    pub fn runtime_mut(&mut self) -> &mut BrowserJsRuntime {
        &mut self.runtime
    }

    /// Consume the event loop and return the runtime.
    pub fn into_runtime(self) -> BrowserJsRuntime {
        self.runtime
    }

    /// Consume and return the DOM.
    pub fn take_dom(self) -> crate::dom::Dom {
        self.runtime.take_dom()
    }

    /// Snapshot current localStorage/sessionStorage for carrying across navigations.
    pub fn get_storage(
        &mut self,
    ) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
        self.runtime.get_storage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_loop() -> BrowserEventLoop {
        // Readiness is thread-local and tests share a thread (--test-threads=1),
        // so clear any load/counter state left by a previous test.
        crate::js_runtime::readiness::reset();
        let dom = crate::html_parser::parse_html(
            "<html><head></head><body><div id=\"output\"></div></body></html>",
        );
        BrowserEventLoop::new(BrowserJsRuntime::new(dom))
    }

    #[tokio::test]
    async fn idle_detection_no_work() {
        let mut evloop = create_loop();
        let reason = evloop.run_until_idle().await.unwrap();
        assert_eq!(reason, IdleReason::AllWorkDone);
    }

    #[tokio::test]
    async fn set_timeout_fires() {
        let mut evloop = create_loop();
        evloop
            .execute_script(
                r#"setTimeout(() => {
                    document.querySelector('#output').textContent = 'timer fired';
                }, 50);"#,
            )
            .unwrap();

        let reason = evloop.run_until_idle().await.unwrap();
        assert_eq!(reason, IdleReason::AllWorkDone);

        let result = evloop
            .execute_script("document.querySelector('#output').textContent")
            .unwrap();
        assert_eq!(result, "timer fired");
    }

    #[tokio::test]
    async fn promise_resolves() {
        let mut evloop = create_loop();
        evloop
            .execute_script(
                r#"Promise.resolve().then(() => {
                    document.querySelector('#output').textContent = 'promise resolved';
                });"#,
            )
            .unwrap();

        evloop.run_until_idle().await.unwrap();

        let result = evloop
            .execute_script("document.querySelector('#output').textContent")
            .unwrap();
        assert_eq!(result, "promise resolved");
    }

    // Readiness model: a page is settled once load has fired with no in-flight
    // fetch and no refed (short) render timer; background work stays pending.

    #[tokio::test]
    async fn settles_despite_running_short_interval() {
        let mut evloop = create_loop();
        // A short setInterval is refed (pins run_event_loop forever) but is
        // background work: run_until_settled must return once load fires.
        evloop
            .execute_script(
                r#"
                setInterval(() => {}, 1000);
                setTimeout(() => { globalThis[Symbol.for('__browser_oxide_mark_load__')](); }, 0);
            "#,
            )
            .unwrap();

        let start = std::time::Instant::now();
        let reason = evloop.run_until_settled().await.unwrap();
        assert_eq!(reason, IdleReason::Settled);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "a background interval must not pin the settle drain"
        );
    }

    #[tokio::test]
    async fn settles_despite_perpetual_short_timer() {
        let mut evloop = create_loop();
        // A self-rescheduling short timer (React's scheduler, telemetry) runs
        // forever; it must NOT block settle. Settle fires once load fires.
        evloop
            .execute_script(
                r#"
                (function spin(){ setTimeout(spin, 50); })();
                setTimeout(() => { globalThis[Symbol.for('__browser_oxide_mark_load__')](); }, 0);
            "#,
            )
            .unwrap();

        let start = std::time::Instant::now();
        let reason = evloop.run_until_settled().await.unwrap();
        assert_eq!(reason, IdleReason::Settled);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "a perpetual short timer must not pin the settle drain"
        );
    }

    #[tokio::test]
    async fn settles_after_render_burst() {
        let mut evloop = create_loop();
        // A microtask-burst render (React's initial mount): every node goes in
        // during one turn. Settle must capture the whole burst, never half-done.
        evloop
            .execute_script(
                r#"
                setTimeout(() => { globalThis[Symbol.for('__browser_oxide_mark_load__')](); }, 0);
                Promise.resolve().then(() => {
                    const out = document.querySelector('#output');
                    for (let i = 0; i < 5; i++) out.appendChild(document.createElement('div'));
                });
            "#,
            )
            .unwrap();

        let reason = evloop.run_until_settled().await.unwrap();
        let count = evloop
            .execute_script("String(document.querySelectorAll('#output div').length)")
            .unwrap();
        assert_eq!(count, "5", "settled before the render burst completed");
        assert!(matches!(
            reason,
            IdleReason::Settled | IdleReason::AllWorkDone
        ));
    }

    #[tokio::test]
    async fn caller_timeout_bounds_a_never_settling_page() {
        let mut evloop = create_loop();
        // load never fires and a refed timer keeps the loop busy, so the page
        // never settles; the CALLER bounds it with `tokio::time::timeout`.
        evloop.execute_script("setInterval(() => {}, 50);").unwrap();

        let start = std::time::Instant::now();
        let r = tokio::time::timeout(Duration::from_millis(400), evloop.run_until_settled()).await;
        assert!(
            r.is_err(),
            "a never-settling page must not resolve on its own"
        );
        assert!(start.elapsed() >= Duration::from_millis(400));
    }

    #[tokio::test]
    async fn chained_set_timeout() {
        let mut evloop = create_loop();
        evloop
            .execute_script(
                r#"
                setTimeout(() => {
                    document.querySelector('#output').textContent = '1';
                    setTimeout(() => {
                        document.querySelector('#output').textContent += '2';
                    }, 10);
                }, 10);
                "#,
            )
            .unwrap();

        evloop.run_until_idle().await.unwrap();

        let result = evloop
            .execute_script("document.querySelector('#output').textContent")
            .unwrap();
        assert_eq!(result, "12");
    }

    #[tokio::test]
    async fn request_animation_frame() {
        let mut evloop = create_loop();
        // rAF is a background timer, so it fires only while the loop is driven.
        // A refed 100ms timer keeps run_until_idle alive past the rAF tick.
        evloop
            .execute_script(
                r#"requestAnimationFrame((ts) => {
                    document.querySelector('#output').textContent = 'raf:' + (typeof ts);
                });
                setTimeout(() => {}, 100);"#,
            )
            .unwrap();

        evloop.run_until_idle().await.unwrap();

        let result = evloop
            .execute_script("document.querySelector('#output').textContent")
            .unwrap();
        assert_eq!(result, "raf:number");
    }
}
