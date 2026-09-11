// Browser-host Promise rejection lifecycle.
//
// deno_core treats an unhandled Promise rejection as fatal unless the
// embedder takes responsibility for it. Browsers instead surface
// `unhandledrejection` / `rejectionhandled` events and keep the browsing
// context alive.
((globalThis) => {
    let core;
    try { core = Deno.core; } catch (_) { return; }
    if (!core || typeof core.setUnhandledPromiseRejectionHandler !== 'function') return;

    const markTrusted = typeof globalThis.__bo_mark_trusted === 'function'
        ? globalThis.__bo_mark_trusted
        : event => event;
    const scheduleHostTask = typeof globalThis.setTimeout === 'function'
        ? globalThis.setTimeout.bind(globalThis)
        : null;
    const cancelHostTask = typeof globalThis.clearTimeout === 'function'
        ? globalThis.clearTimeout.bind(globalThis)
        : null;
    const pending = new WeakMap();
    const reported = new WeakSet();

    const dispatchPromiseRejection = (type, promise, reason, cancelable) => {
        if (typeof globalThis.PromiseRejectionEvent !== 'function'
            || typeof globalThis.dispatchEvent !== 'function') return null;
        try {
            const event = new globalThis.PromiseRejectionEvent(type, {
                promise,
                reason,
                bubbles: false,
                cancelable,
            });
            markTrusted(event);
            globalThis.dispatchEvent(event);
            return event;
        } catch (_) {
            return null;
        }
    };

    core.setUnhandledPromiseRejectionHandler((promise, reason) => {
        // Browser default handling may report to DevTools, but it does not
        // terminate the Window/Worker runtime. Returning true tells deno_core
        // that the host accepted responsibility for this rejection.
        //
        // V8/deno_core surfaces the host callback before Chromium queues the
        // browser's `unhandledrejection` task. Defer one host task so an
        // already-queued task can attach a handler first; the corresponding
        // handled callback below then cancels this pending report. This is why
        // `Promise.reject(); setTimeout(() => p.catch(...), 0)` does not fire
        // `unhandledrejection` in Chrome.
        if (promise && (typeof promise === 'object' || typeof promise === 'function')
            && scheduleHostTask) {
            const record = { reason, timer: 0 };
            record.timer = scheduleHostTask(() => {
                if (pending.get(promise) !== record) return;
                pending.delete(promise);
                reported.add(promise);
                dispatchPromiseRejection('unhandledrejection', promise, reason, true);
            // BrowserOxide's zero-delay timers are independent async sleeps,
            // so two 0ms timers do not have a stable FIFO tie-break. Chrome's
            // rejection task runs after an already-queued setTimeout(0) but
            // before the next animation frame; 1ms gives this host task the
            // same observable slot without racing the page's 0ms timer.
            }, 1);
            pending.set(promise, record);
        } else {
            dispatchPromiseRejection('unhandledrejection', promise, reason, true);
        }
        return true;
    });

    if (typeof core.setHandledPromiseRejectionHandler === 'function') {
        core.setHandledPromiseRejectionHandler((promise, reason) => {
            const record = promise && pending.get(promise);
            if (record) {
                pending.delete(promise);
                if (cancelHostTask) cancelHostTask(record.timer);
                return;
            }
            if (promise && reported.has(promise)) reported.delete(promise);
            dispatchPromiseRejection('rejectionhandled', promise, reason, false);
        });
    }
})(globalThis);
