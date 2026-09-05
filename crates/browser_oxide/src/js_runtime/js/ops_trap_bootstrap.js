((globalThis) => {
    // deno_core scrubs the `Deno` global right after bootstrap, so this is
    // the only window in which `Deno.core.ops` is reachable. Wrap every op
    // so a failed argument conversion (the classic `TypeError: expected
    // i32` from a `#[smi]` parameter receiving `undefined`) is recorded
    // with the op name and a trimmed page-side stack, instead of surfacing
    // as a bare uncaught error that neither `window.onerror` nor the
    // promise-rejection path lets the embedder observe.
    // Every other bootstrap reaches ops through property access on this
    // same object (`const ops = Deno.core.ops`), so the wrap covers them all.
    const ops = Deno.core.ops;
    Object.defineProperty(globalThis, "__browser_oxide_debug", {
        value: true,
        configurable: true,
    });
    // Survives the post-bootstrap `Deno` scrub so page code (and the
    // browser_oxide test probe) can still reach ops, e.g.
    // `__oxOps.op_worker_last_spawn()`.
    globalThis.__oxOps = ops;
    const ring = [];
    globalThis.__oxOpErrors = ring;
    globalThis.__oxOpsWrapped = true;
    globalThis.__oxOpCallCount = 0;
    for (const k of Object.keys(ops)) {
        const orig = ops[k];
        if (typeof orig !== "function" || orig.__oxWrapped) continue;
        const wrapped = function (...a) {
            globalThis.__oxOpCallCount++;
            try {
                return orig.apply(this, a);
            } catch (e) {
                if (ring.length < 24) {
                    let stack = "";
                    try {
                        stack = String((e && e.stack) || "")
                            .split("\n")
                            .slice(0, 4)
                            .join(" | ")
                            .slice(0, 300);
                    } catch (_) {}
                    let args;
                    try {
                        args = a.slice(0, 4).map((x) => String(x).slice(0, 40));
                    } catch (_) {}
                    ring.push({
                        op: k,
                        argc: a.length,
                        args,
                        msg: String((e && e.message) || e).slice(0, 120),
                        stack,
                    });
                }
                throw e;
            }
        };
        wrapped.__oxWrapped = true;
        wrapped.__oxOrig = orig;
        try {
            Object.defineProperty(wrapped, "name", { value: orig.name });
        } catch (_) {}
        ops[k] = wrapped;
    }
})(globalThis);
