//! Iframe support for browser_oxide.
//!
//! Each iframe with `srcdoc` gets its own DOM tree, V8 runtime, and event loop.
//! Communication between parent and child is via serialized postMessage.

use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;
use crate::event_loop::BrowserEventLoop;
use crate::js_runtime::runtime::BrowserRuntimeOptions;
use crate::js_runtime::BrowserJsRuntime;
use std::time::Duration;
use tracing;

const VM_CALL_TRACE_BOOTSTRAP: &str = r#"
globalThis.__oxVmInstrumentation = 'iframe-handler-call-v2';
globalThis.__oxVmEventN = 0;
globalThis.__oxVmEventCounts = Object.create(null);
globalThis.__oxVmEvents = [];
globalThis.__oxVmRuns = [];
globalThis.__oxVmStateMeta = [];
globalThis.__oxVmStates = [];
globalThis.__oxVmRunSnapshots = [];
globalThis.__oxVmCaughtErrors = [];
globalThis.__oxVmThrownCallErrors = [];
globalThis.__oxVmPromiseTrace = [];
globalThis.__oxVmPromiseAllTrace = [];
globalThis.__oxVmScheduleTrace = [];
const __oxVmIds = new WeakMap();
const __oxVmProgress = new WeakMap();
const __oxVmSeenErrors = new WeakSet();
const __oxVmSeenErrorSignatures = new Set();
let __oxVmNextId = 1;
let __oxVmPromiseNextId = 1;
const __oxVmNativePromiseThen = Promise.prototype.then;
const __oxVmTraceScheduler = function(name) {
    const nativeScheduler = globalThis[name];
    if (typeof nativeScheduler !== 'function') return;
    globalThis[name] = new Proxy(nativeScheduler, {
        apply(target, thisArg, args) {
            const callback = args[0];
            const record = {
                id: globalThis.__oxVmScheduleTrace.length + 1,
                phase: 'schedule', kind: name, at: performance.now(),
                delay: args.length > 1 ? Number(args[1]) : null,
                callback: typeof callback === 'function'
                    ? { name: String(callback.name || ''), source: String(callback).slice(0, 240) }
                    : { type: typeof callback, value: String(callback).slice(0, 240) },
                stack: String(new Error().stack || '').slice(0, 900)
            };
            const trace = globalThis.__oxVmScheduleTrace;
            trace.push(record);
            if (trace.length > 8192) trace.splice(0, trace.length - 8192);
            if (typeof callback === 'function') {
                const wrapped = function() {
                    trace.push({
                        id: record.id, phase: 'fire', kind: name,
                        at: performance.now(), argc: arguments.length
                    });
                    if (trace.length > 8192) trace.splice(0, trace.length - 8192);
                    try {
                        const result = Reflect.apply(callback, this, arguments);
                        trace.push({
                            id: record.id, phase: 'return', kind: name,
                            at: performance.now(), result: __oxVmDescribe(result)
                        });
                        return result;
                    } catch (error) {
                        trace.push({
                            id: record.id, phase: 'throw', kind: name,
                            at: performance.now(), error: __oxVmDescribe(error)
                        });
                        throw error;
                    }
                };
                args = Array.prototype.slice.call(args);
                args[0] = wrapped;
            }
            const result = Reflect.apply(target, thisArg, args);
            record.result = typeof result === 'number' ? result : __oxVmDescribe(result);
            return result;
        }
    });
};
for (const __oxVmSchedulerName of [
    'setTimeout', 'setInterval', 'requestAnimationFrame',
    'requestIdleCallback', 'queueMicrotask'
]) {
    try { __oxVmTraceScheduler(__oxVmSchedulerName); } catch (_) {}
}
const __oxVmPromiseCallback = function(callback) {
    if (typeof callback !== 'function') return typeof callback;
    return { name: String(callback.name || ''), source: String(callback).slice(0, 240) };
};
Promise.prototype.then = new Proxy(__oxVmNativePromiseThen, {
    apply(target, thisArg, args) {
        const record = {
            id: __oxVmPromiseNextId++, at: performance.now(),
            fulfilled: __oxVmPromiseCallback(args[0]),
            rejected: __oxVmPromiseCallback(args[1]),
            stack: String(new Error().stack || '').slice(0, 900)
        };
        const trace = globalThis.__oxVmPromiseTrace;
        trace.push(record);
        if (trace.length > 4096) trace.splice(0, trace.length - 4096);
        const wrap = function(callback, kind) {
            if (typeof callback !== 'function') return callback;
            return function(value) {
                record.settledAt = performance.now();
                record.settledKind = kind;
                record.value = __oxVmDescribe(value);
                try {
                    const result = Reflect.apply(callback, this, arguments);
                    record.result = __oxVmDescribe(result);
                    return result;
                } catch (error) {
                    record.callbackError = __oxVmDescribe(error);
                    throw error;
                }
            };
        };
        return Reflect.apply(target, thisArg, [
            wrap(args[0], 'fulfilled'), wrap(args[1], 'rejected')
        ]);
    }
});
const __oxVmShouldCaptureError = function(error) {
    const message = String(error && error.message || error || '');
    if (!message) return false;
    return !message.includes('Cyclic __proto__ value') &&
        !message.includes("'caller', 'callee', and 'arguments' properties") &&
        !message.includes("Function.prototype.toString requires that 'this' be a Function") &&
        !message.includes('Invalid code point -1');
};
const __oxVmErrorSignature = function(kind, error) {
    const stack = String(error && error.stack || '');
    return kind + '|' + String(error && error.name || '') + '|' +
        String(error && error.message || error || '') + '|' +
        stack.split('\n').slice(0, 3).join('\n');
};
const __oxVmDescribe = function(value, depth) {
    depth = Number(depth) || 0;
    const type = typeof value;
    if (value === undefined || value === null || type === 'boolean' || type === 'number') {
        return value;
    }
    if (type === 'string') return value.slice(0, 1200);
    if (type === 'function') return { type: 'function', name: String(value.name || '') };
    const tag = Object.prototype.toString.call(value);
    if (tag === '[object Window]') return { type: tag };
    if (depth >= 3) return { type: tag };
    if (tag === '[object MessageEvent]') {
        let data;
        try { data = __oxVmDescribe(value.data, depth + 1); } catch (error) { data = String(error); }
        return { type: tag, data: data };
    }
    if (value instanceof Error) {
        return { type: tag, name: value.name, message: value.message,
            stack: String(value.stack || '').slice(0, 1200) };
    }
    if (Array.isArray(value)) {
        return { type: tag, length: value.length,
            values: value.slice(0, 24).map(value => __oxVmDescribe(value, depth + 1)) };
    }
    const own = {};
    try {
        for (const key of Object.getOwnPropertyNames(value).slice(0, 40)) {
            const descriptor = Object.getOwnPropertyDescriptor(value, key);
            if (descriptor && 'value' in descriptor) {
                own[key] = __oxVmDescribe(descriptor.value, depth + 1);
            }
        }
    } catch (_) {}
    return { type: tag, own: own };
};
const __oxVmCounterStores = function(heap) {
    const found = [];
    const seen = new WeakSet();
    const walk = function(value, path, depth) {
        if (!value || typeof value !== 'object' || depth > 6 || seen.has(value)) return;
        seen.add(value);
        try {
            if (Object.prototype.hasOwnProperty.call(value, 'tQdUc5')) {
                const numbered = {};
                Object.getOwnPropertyNames(value)
                    .filter(key => /^\d+$/.test(key))
                    .forEach(key => { numbered[key] = __oxVmDescribe(value[key]); });
                found.push({
                    path, tQdUc5: value.tQdUc5, ZMSOw0: value.ZMSOw0,
                    twvE0: value.twvE0, TzZRB1: value.TzZRB1,
                    keys: Object.getOwnPropertyNames(value), numbered
                });
            }
            if (Array.isArray(value)) {
                for (let index = 0; index < value.length && index < 320; index++) {
                    walk(value[index], path + '[' + index + ']', depth + 1);
                }
            } else {
                for (const key of Object.getOwnPropertyNames(value).slice(0, 320)) {
                    const descriptor = Object.getOwnPropertyDescriptor(value, key);
                    if (descriptor && 'value' in descriptor) {
                        walk(descriptor.value, path + '.' + key, depth + 1);
                    }
                }
            }
        } catch (_) {}
    };
    for (let index = 0; index < heap.length; index++) {
        walk(heap[index], 'h[' + index + ']', 0);
    }
    return found;
};
const __oxVmNativePromiseAll = Promise.all;
Promise.all = new Proxy(__oxVmNativePromiseAll, {
    apply(target, thisArg, args) {
        const input = args[0];
        const record = {
            id: globalThis.__oxVmPromiseAllTrace.length + 1,
            at: performance.now(),
            length: Array.isArray(input) ? input.length : null,
            inputs: Array.isArray(input)
                ? input.slice(0, 48).map(value => __oxVmDescribe(value)) : [],
            stack: String(new Error().stack || '').slice(0, 900)
        };
        globalThis.__oxVmPromiseAllTrace.push(record);
        const promise = Reflect.apply(target, thisArg, args);
        Reflect.apply(__oxVmNativePromiseThen, promise, [
            function(value) {
                record.settledAt = performance.now();
                record.settledKind = 'fulfilled';
                record.value = __oxVmDescribe(value);
            },
            function(error) {
                record.settledAt = performance.now();
                record.settledKind = 'rejected';
                record.value = __oxVmDescribe(error);
            }
        ]);
        return promise;
    }
});
globalThis.__oxWrapVm = function(fn, name) {
    try {
        if (!fn || Object.prototype.hasOwnProperty.call(fn, 'call')) return;
        Object.defineProperty(fn, 'call', {
            configurable: true,
            writable: true,
            value: function(vm) {
                const args = Array.prototype.slice.call(arguments, 1);
                const heap = vm && vm.h;
                const before = heap && Number(heap[vm.l]);
                let vmId = __oxVmIds.get(vm);
                if (!vmId) {
                    vmId = __oxVmNextId++;
                    __oxVmIds.set(vm, vmId);
                    globalThis.__oxVmStateMeta.push([
                        vmId, vm && vm.i, vm && vm.j, vm && vm.m, vm && vm.l, vm && vm.o,
                        heap && heap.length
                    ]);
                    globalThis.__oxVmStates[vmId] = vm;
                }
                globalThis.__oxVmLast = vm;
                const at = performance.now();
                const previous = __oxVmProgress.get(vm);
                if (!previous || at - previous.at > 0.5) {
                    const runs = globalThis.__oxVmRuns;
                    runs.push([at, vmId, name, before, previous ? previous.after : null]);
                    if (runs.length > 2048) runs.splice(0, runs.length - 2048);
                    try {
                        const bytecode = heap && heap[vm.m];
                        if (bytecode && bytecode.length > 100000) {
                            const values = [];
                            for (let index = 0; index < heap.length; index++) {
                                const value = heap[index];
                                if (value !== undefined && value !== 0) {
                                    values.push([index, __oxVmDescribe(value)]);
                                }
                            }
                            const snapshots = globalThis.__oxVmRunSnapshots;
                            snapshots.push({ at: at, vmId: vmId, name: name, pc: before,
                                values: values, stores: __oxVmCounterStores(heap) });
                            if (snapshots.length > 128) snapshots.shift();
                        }
                    } catch (_) {}
                }
                const count = (globalThis.__oxVmEventCounts[name] || 0) + 1;
                globalThis.__oxVmEventCounts[name] = count;
                globalThis.__oxVmEventN++;
                let result;
                let error = null;
                try {
                    result = Reflect.apply(fn, vm, args);
                    return result;
                } catch (caught) {
                    error = caught;
                    try {
                        const bytecode = heap && heap[vm.m];
                        const signature = __oxVmErrorSignature('thrown', caught);
                        if (bytecode && bytecode.length > 100000 &&
                            __oxVmShouldCaptureError(caught) &&
                            !__oxVmSeenErrorSignatures.has(signature) &&
                            globalThis.__oxVmThrownCallErrors.length < 128) {
                            __oxVmSeenErrorSignatures.add(signature);
                            const values = [];
                            for (let index = 0; index < heap.length; index++) {
                                const value = heap[index];
                                if (value !== undefined && value !== 0) {
                                    values.push([index, __oxVmDescribe(value)]);
                                }
                            }
                            globalThis.__oxVmThrownCallErrors.push({
                                at: performance.now(), vmId: vmId, handler: name,
                                before: before, after: heap && Number(heap[vm.l]),
                                error: __oxVmDescribe(caught),
                                bytecode: Array.prototype.slice.call(
                                    bytecode, Math.max(0, Number(before) - 32), Number(before) + 64
                                ),
                                values: values
                            });
                        }
                    } catch (_) {}
                    throw caught;
                } finally {
                    try {
                        const after = heap && Number(heap[vm.l]);
                        const bytecode = heap && heap[vm.m];
                        if (bytecode && bytecode.length > 100000) {
                            for (let index = 0; index < heap.length; index++) {
                                const value = heap[index];
                                if (!(value instanceof Error) || __oxVmSeenErrors.has(value) ||
                                    !__oxVmShouldCaptureError(value)) continue;
                                __oxVmSeenErrors.add(value);
                                const signature = __oxVmErrorSignature('heap', value);
                                if (__oxVmSeenErrorSignatures.has(signature) ||
                                    globalThis.__oxVmCaughtErrors.length >= 128) continue;
                                __oxVmSeenErrorSignatures.add(signature);
                                const values = [];
                                for (let heapIndex = 0; heapIndex < heap.length; heapIndex++) {
                                    const heapValue = heap[heapIndex];
                                    if (heapValue !== undefined && heapValue !== 0) {
                                        values.push([heapIndex, __oxVmDescribe(heapValue)]);
                                    }
                                }
                                globalThis.__oxVmCaughtErrors.push({
                                    at: performance.now(), vmId: vmId, handler: name,
                                    before: before, after: after, heapIndex: index,
                                    error: __oxVmDescribe(value),
                                    bytecode: Array.prototype.slice.call(
                                        bytecode, Math.max(0, Number(before) - 32), Number(before) + 64
                                    ),
                                    values: values
                                });
                            }
                        }
                        __oxVmProgress.set(vm, { at: performance.now(), after });
                        const events = globalThis.__oxVmEvents;
                        events.push([
                            performance.now(), name, count, before, after,
                            error ? String(error && (error.stack || error)).slice(0, 600) : '',
                            vmId, vm && vm.i, vm && vm.j, vm && vm.m, vm && vm.l, vm && vm.o
                        ]);
                        if (events.length > 1024) events.splice(0, events.length - 1024);
                    } catch (_) {}
                }
            }
        });
    } catch (_) {}
};
"#;

fn instrument_vm_handler_calls(code: &str) -> String {
    let marker = "function ";
    let positions: Vec<usize> = code.match_indices(marker).map(|(at, _)| at).collect();
    let mut insertions = Vec::new();
    for (index, &at) in positions.iter().enumerate() {
        let name_start = at + marker.len();
        let Some(name_end_rel) = code[name_start..].find('(') else {
            continue;
        };
        let name_end = name_start + name_end_rel;
        let name = code[name_start..name_end].trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$')
        {
            continue;
        }
        let next = positions.get(index + 1).copied().unwrap_or(code.len());
        let function_region = &code[at..next];
        if function_region.contains("this.h") && function_region.contains("this.l") {
            insertions.push((at, name.to_string()));
        }
    }

    let mut traced =
        String::with_capacity(code.len() + insertions.len() * 40 + VM_CALL_TRACE_BOOTSTRAP.len());
    traced.push_str(VM_CALL_TRACE_BOOTSTRAP);
    let mut cursor = 0;
    for (at, name) in insertions {
        traced.push_str(&code[cursor..at]);
        traced.push_str("globalThis.__oxWrapVm(");
        traced.push_str(&name);
        traced.push_str(",'");
        traced.push_str(&name);
        traced.push_str("');");
        cursor = at;
    }
    traced.push_str(&code[cursor..]);
    traced
}

/// Info about an iframe found in the DOM.
pub struct IframeInfo {
    pub node_id: NodeId,
    pub srcdoc: Option<String>,
    pub src: Option<String>,
    pub name: Option<String>,
}

/// Internal runtime used only by network-backed frame-tree contexts. Public
/// callers use [`FrameContext`], so no API can create a second srcdoc isolate.
pub(crate) struct ChildIframe {
    pub(crate) node_id: NodeId,
    pub(crate) event_loop: BrowserEventLoop,
}

/// A borrowed handle to the single execution context backing an iframe.
///
/// `SameIsolateRealm` is the genuine V8 child context exposed by
/// `iframe.contentWindow`. `IsolatedRuntime` is used by the frame-tree network
/// backend. Both present one public API, so callers never create or observe a
/// second, duplicate iframe runtime.
pub struct FrameContext<'a> {
    node_id: NodeId,
    backend: FrameContextBackend<'a>,
}

enum FrameContextBackend<'a> {
    SameIsolateRealm {
        event_loop: &'a mut BrowserEventLoop,
        realm_id: u32,
    },
    IsolatedRuntime(&'a mut ChildIframe),
}

impl<'a> FrameContext<'a> {
    pub(crate) fn same_isolate(
        node_id: NodeId,
        event_loop: &'a mut BrowserEventLoop,
        realm_id: u32,
    ) -> Self {
        Self {
            node_id,
            backend: FrameContextBackend::SameIsolateRealm {
                event_loop,
                realm_id,
            },
        }
    }

    pub(crate) fn isolated(child: &'a mut ChildIframe) -> Self {
        Self {
            node_id: child.node_id,
            backend: FrameContextBackend::IsolatedRuntime(child),
        }
    }

    /// DOM host node for this browsing context.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Evaluate JavaScript in the exact context visible to page script.
    pub fn evaluate(&mut self, js: &str) -> Result<String, deno_core::error::AnyError> {
        match &mut self.backend {
            FrameContextBackend::SameIsolateRealm {
                event_loop,
                realm_id,
            } => event_loop.runtime_mut().execute_child_realm_script(
                *realm_id,
                js,
                Some("about:srcdoc"),
            ),
            FrameContextBackend::IsolatedRuntime(child) => child.evaluate(js),
        }
    }

    /// Drive pending work for this frame.
    pub async fn pump(&mut self, timeout: Duration) -> Result<(), deno_core::error::AnyError> {
        match &mut self.backend {
            FrameContextBackend::SameIsolateRealm { event_loop, .. } => {
                let _ = event_loop.run_until_idle(timeout).await;
                Ok(())
            }
            FrameContextBackend::IsolatedRuntime(child) => child.pump(timeout).await,
        }
    }

    /// Execute JavaScript and then drive pending work.
    pub async fn execute_and_run(
        &mut self,
        js: &str,
        timeout: Duration,
    ) -> Result<(), deno_core::error::AnyError> {
        self.evaluate(js)?;
        self.pump(timeout).await
    }

    /// Query text in this frame's document.
    pub fn query_text(&mut self, selector: &str) -> Option<String> {
        self.evaluate(&format!(
            r#"(() => {{ const el = document.querySelector("{}"); return el ? el.textContent : ""; }})()"#,
            selector.replace('"', "\\\"")
        ))
        .ok()
        .filter(|value| !value.is_empty())
    }
}

fn complete_document_lifecycle(event_loop: &mut BrowserEventLoop) {
    // Match the top-level Page lifecycle. Child frames previously executed
    // their scripts but never dispatched DOMContentLoaded/load or advanced
    // document.readyState, leaving real widgets stuck in the loading phase.
    event_loop.complete_document_lifecycle();
}

type FrameScriptFetchResult = Option<(String, crate::net::TimingStats, std::time::Instant, String)>;
type FrameScriptFetchHandle = tokio::task::JoinHandle<FrameScriptFetchResult>;

fn resolve_frame_subresource_url(base: &str, reference: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    let joined = base.join(reference).ok()?;
    match joined.scheme() {
        "http" | "https" => Some(joined.to_string()),
        _ => None,
    }
}

fn frame_document_origin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let origin = parsed.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

async fn take_frame_script_fetch(
    event_loop: &mut BrowserEventLoop,
    fetches: &mut std::collections::HashMap<usize, FrameScriptFetchHandle>,
    index: usize,
) -> Option<(String, std::time::Instant, String)> {
    let handle = fetches.remove(&index)?;
    match handle.await {
        Ok(Some((code, timing, completed_at, resolved_url))) => {
            event_loop.runtime_mut().record_resource_timing(timing);
            Some((code, completed_at, resolved_url))
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(script_index = index, error = %error, "iframe script fetch task failed");
            None
        }
    }
}

async fn execute_prepared_frame_script(
    event_loop: &mut BrowserEventLoop,
    script: &crate::script_runner::ScriptInfo,
    index: usize,
    code: String,
    document_base: &str,
    source_url: Option<String>,
) {
    if code.trim().is_empty() {
        return;
    }
    let code = if std::env::var_os("BROWSER_OXIDE_VM_CALL_TRACE").is_some() {
        instrument_vm_handler_calls(&code)
    } else {
        code
    };
    let name = source_url.unwrap_or_else(|| document_base.to_string());
    event_loop.note_executed_script(&name, &code);

    if script.is_module {
        // HTML module scripts are deferred by default and never become
        // document.currentScript. Each entry is an independent side module.
        event_loop.set_current_script(None);
        let specifier = if script.src.is_some() {
            name.clone()
        } else {
            format!("{document_base}#oxide-frame-mod-{index}")
        };
        match tokio::time::timeout(
            Duration::from_secs(10),
            event_loop.eval_module_code(&specifier, code),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(script = %name, error = %error, "iframe ES module eval error");
            }
            Err(_) => {
                tracing::warn!(script = %name, "iframe ES module eval timed out");
            }
        }
    } else {
        event_loop.set_current_script(Some(script.node_id));
        if let Err(error) = event_loop.execute_script_with_name(&code, &name) {
            tracing::warn!(script = %name, error = %error, "iframe script execution error");
        }
        event_loop.set_current_script(None);
    }
    event_loop.drain_microtasks();
}

async fn drain_ready_async_frame_scripts(
    event_loop: &mut BrowserEventLoop,
    scripts: &[crate::script_runner::ScriptInfo],
    document_base: &str,
    fetches: &mut std::collections::HashMap<usize, FrameScriptFetchHandle>,
    pending_async: &mut Vec<usize>,
) {
    let mut ready = Vec::new();
    let mut keep = Vec::new();
    for index in pending_async.drain(..) {
        let script = &scripts[index];
        if script.src.is_none() {
            ready.push((index, script.code.clone(), std::time::Instant::now(), None));
        } else if fetches
            .get(&index)
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            if let Some((code, completed_at, resolved_url)) =
                take_frame_script_fetch(event_loop, fetches, index).await
            {
                ready.push((index, code, completed_at, Some(resolved_url)));
            }
        } else {
            keep.push(index);
        }
    }
    *pending_async = keep;
    ready.sort_by_key(|(_, _, completed_at, _)| *completed_at);
    for (index, code, _, resolved_url) in ready {
        execute_prepared_frame_script(
            event_loop,
            &scripts[index],
            index,
            code,
            document_base,
            resolved_url,
        )
        .await;
    }
}

async fn finish_async_frame_scripts(
    event_loop: &mut BrowserEventLoop,
    scripts: &[crate::script_runner::ScriptInfo],
    document_base: &str,
    fetches: &mut std::collections::HashMap<usize, FrameScriptFetchHandle>,
    pending_async: &mut Vec<usize>,
) {
    use futures_util::stream::{FuturesUnordered, StreamExt};

    let mut inline = Vec::new();
    let mut external = FuturesUnordered::new();
    for index in pending_async.drain(..) {
        if scripts[index].src.is_none() {
            inline.push(index);
        } else if let Some(handle) = fetches.remove(&index) {
            external.push(async move { (index, handle.await) });
        }
    }
    for index in inline {
        execute_prepared_frame_script(
            event_loop,
            &scripts[index],
            index,
            scripts[index].code.clone(),
            document_base,
            None,
        )
        .await;
    }
    while let Some((index, result)) = external.next().await {
        match result {
            Ok(Some((code, timing, _, resolved_url))) => {
                event_loop.runtime_mut().record_resource_timing(timing);
                execute_prepared_frame_script(
                    event_loop,
                    &scripts[index],
                    index,
                    code,
                    document_base,
                    Some(resolved_url),
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(script_index = index, error = %error, "iframe async script fetch task failed");
            }
        }
    }
}

async fn run_frame_script_schedule(
    event_loop: &mut BrowserEventLoop,
    scripts: &[crate::script_runner::ScriptInfo],
    document_url: &str,
    document_base: &str,
    client: &crate::net::HttpClient,
) {
    use crate::script_runner::ScriptScheduling;

    let request_origin = frame_document_origin(document_url);
    let mut fetches: std::collections::HashMap<usize, FrameScriptFetchHandle> =
        std::collections::HashMap::new();
    for (index, script) in scripts.iter().enumerate() {
        let Some(src) = script.src.as_deref() else {
            continue;
        };
        let Some(resolved_url) = resolve_frame_subresource_url(document_base, src) else {
            continue;
        };
        let client = client.clone();
        let referer = document_url.to_string();
        let origin = request_origin.clone();
        let is_module = script.is_module;
        fetches.insert(
            index,
            tokio::spawn(async move {
                match client
                    .get_script_resource(&resolved_url, &referer, origin.as_deref(), is_module, 5)
                    .await
                {
                    Ok(response) if response.ok() => {
                        let text = response.text();
                        if text.trim_start().starts_with("<!")
                            || text.trim_start().starts_with("<html")
                        {
                            None
                        } else {
                            Some((
                                text,
                                response.timings.clone(),
                                std::time::Instant::now(),
                                resolved_url,
                            ))
                        }
                    }
                    Ok(_) | Err(_) => None,
                }
            }),
        );
    }

    let mut deferred = Vec::new();
    let mut pending_async = Vec::new();
    for (index, script) in scripts.iter().enumerate() {
        match script.scheduling() {
            ScriptScheduling::ParserBlocking => {
                let prepared = if script.src.is_some() {
                    take_frame_script_fetch(event_loop, &mut fetches, index)
                        .await
                        .map(|(code, _, resolved_url)| (code, Some(resolved_url)))
                } else {
                    Some((script.code.clone(), None))
                };
                if let Some((code, resolved_url)) = prepared {
                    execute_prepared_frame_script(
                        event_loop,
                        script,
                        index,
                        code,
                        document_base,
                        resolved_url,
                    )
                    .await;
                }
            }
            ScriptScheduling::Deferred => deferred.push(index),
            ScriptScheduling::Async => pending_async.push(index),
        }
        drain_ready_async_frame_scripts(
            event_loop,
            scripts,
            document_base,
            &mut fetches,
            &mut pending_async,
        )
        .await;
    }

    event_loop.mark_document_interactive();
    for index in deferred {
        drain_ready_async_frame_scripts(
            event_loop,
            scripts,
            document_base,
            &mut fetches,
            &mut pending_async,
        )
        .await;
        let script = &scripts[index];
        let prepared = if script.src.is_some() {
            take_frame_script_fetch(event_loop, &mut fetches, index)
                .await
                .map(|(code, _, resolved_url)| (code, Some(resolved_url)))
        } else {
            Some((script.code.clone(), None))
        };
        if let Some((code, resolved_url)) = prepared {
            execute_prepared_frame_script(
                event_loop,
                script,
                index,
                code,
                document_base,
                resolved_url,
            )
            .await;
        }
        drain_ready_async_frame_scripts(
            event_loop,
            scripts,
            document_base,
            &mut fetches,
            &mut pending_async,
        )
        .await;
    }

    drain_ready_async_frame_scripts(
        event_loop,
        scripts,
        document_base,
        &mut fetches,
        &mut pending_async,
    )
    .await;
    event_loop.dispatch_dom_content_loaded();
    finish_async_frame_scripts(
        event_loop,
        scripts,
        document_base,
        &mut fetches,
        &mut pending_async,
    )
    .await;
    event_loop.dispatch_load();
}

impl ChildIframe {
    /// Build isolated HTML for the internal frame-tree backend. Browser-visible
    /// srcdoc frames never use this path.
    async fn from_isolated_html(
        node_id: NodeId,
        html: &str,
        profile: &crate::stealth::StealthProfile,
    ) -> Result<Self, deno_core::error::AnyError> {
        let dom = crate::html_parser::parse_html(html);
        let scripts = crate::script_runner::find_scripts(&dom);
        let stylesheet_entries = crate::stylesheet_collector::find_stylesheets(&dom);
        let stylesheets = crate::stylesheet_collector::resolve_inline_only(&stylesheet_entries);

        let runtime = BrowserJsRuntime::with_options(
            dom,
            BrowserRuntimeOptions {
                stealth_profile: Some(profile.clone()),
                stylesheets,
                ..Default::default()
            },
        );
        let mut event_loop = BrowserEventLoop::new(runtime);

        // Execute scripts in the child's own V8 context. W2.7 — Chrome
        // reports `about:srcdoc` for srcdoc iframe stack frames.
        for (i, script) in scripts.iter().enumerate() {
            if script.src.is_some() {
                continue;
            } // Skip external scripts in srcdoc
            if script.code.trim().is_empty() {
                continue;
            }
            event_loop.note_executed_script("about:srcdoc", &script.code);
            if let Err(e) = event_loop.execute_script_with_name(&script.code, "about:srcdoc") {
                tracing::warn!(script_index = i, error = %e, "iframe script error");
            }
        }

        complete_document_lifecycle(&mut event_loop);
        // Mark the child realm's initial load as settled so deferred
        // parent->child messages (postMessage) wait for full initialization
        // instead of landing between this realm's per-script event-loop gaps.
        event_loop
            .execute_script(
                "try{Object.defineProperty(globalThis,'__oxFrameReady',{value:1,writable:true,configurable:true,enumerable:false})}catch(_){try{globalThis.__oxFrameReady=1}catch(_){}}",
            )
            .ok();

        // Bound child-frame startup by the same deployment-configurable
        // navigation deadline as top-level pages. An explicit "off" keeps the
        // unbounded behavior for embedders that deliberately opt into it.
        match crate::page::navigation_timeout() {
            Some(timeout) => event_loop.run_until_settled(timeout).await?,
            None => event_loop.run_until_settled_unbounded().await?,
        };

        Ok(Self {
            node_id,
            event_loop,
        })
    }

    /// Create a child iframe by fetching src URL via HTTP client.
    pub(crate) async fn from_url(
        node_id: NodeId,
        url: &str,
        client: &crate::net::HttpClient,
        stealth_profile: Option<&crate::stealth::StealthProfile>,
        frame_ids: Option<(u32, u32, u32)>,
        name: &str,
        referrer: &str,
        ancestor_origins: &[String],
    ) -> Result<(Self, crate::net::TimingStats), deno_core::error::AnyError> {
        // CSP `frame-src` enforcement (falls back to child-src then
        // default-src). Real Chrome refuses to navigate iframes whose
        // src violates the parent's CSP, surfacing the same network-
        // error shape we return on op_fetch blocks.
        // Skipped when `frame_ids` is set: check_csp reads the top page's CSP,
        // but a nested frame's frame-src belongs to its parent, which we don't track.
        if frame_ids.is_none() {
            if let Ok(parsed_url) = url::Url::parse(url) {
                if let Err(violated) = crate::js_runtime::extensions::fetch_ext::check_csp(
                    crate::net::csp::Directive::FrameSrc,
                    &parsed_url,
                    None,
                    false,
                ) {
                    eprintln!(
                    "[csp] Refused to frame '{}' because it violates the following Content Security Policy directive: \"{}\".",
                    url, violated
                );
                    return Err(deno_core::error::AnyError::msg(format!(
                        "iframe blocked by CSP: {}",
                        url
                    )));
                }
            }
        }

        let resp = client
            .get_frame_navigation(url, referrer)
            .await
            .map_err(|e| deno_core::error::AnyError::msg(format!("iframe fetch error: {}", e)))?;

        if !resp.ok() {
            return Err(deno_core::error::AnyError::msg(format!(
                "iframe fetch {} returned {}",
                url, resp.status
            )));
        }

        let html = resp.text();
        let navigation_timing = resp.timings.clone();
        // Skip if response looks like non-HTML (binary, error page)
        if html.trim().is_empty() {
            let child = Self::from_isolated_html(
                node_id,
                "<html><body></body></html>",
                stealth_profile.unwrap(),
            )
            .await?;
            return Ok((child, navigation_timing));
        }

        let dom = crate::html_parser::parse_html(&html);
        let scripts = crate::script_runner::find_scripts(&dom);
        let stylesheet_entries = crate::stylesheet_collector::find_stylesheets(&dom);
        let document_base = crate::page::document_base_url(&dom, &resp.url);
        let import_map =
            crate::js_runtime::module_loader::ImportMap::from_dom(&dom, &document_base);

        // Fetch external stylesheets
        let mut stylesheets = Vec::new();
        for entry in &stylesheet_entries {
            match entry {
                crate::stylesheet_collector::StylesheetEntry::Inline(css) => {
                    stylesheets.push(css.clone());
                }
                crate::stylesheet_collector::StylesheetEntry::External(href) => {
                    let Some(full_url) = resolve_frame_subresource_url(&document_base, href) else {
                        continue;
                    };
                    if let Ok(resp) = client.get(&full_url).await {
                        if resp.ok() {
                            let text = resp.text();
                            if !text.trim_start().starts_with("<!") {
                                stylesheets.push(text);
                            }
                        }
                    }
                }
            }
        }

        let mut options = BrowserRuntimeOptions {
            stylesheets,
            navigation_timing: Some(resp.timings.clone()),
            import_map,
            is_secure_context: crate::page::is_secure_url(&resp.url),
            cross_origin_isolated: crate::page::response_is_cross_origin_isolated(
                &resp.url,
                &resp.headers,
            ),
            module_request_origin: frame_document_origin(&resp.url),
            ..Default::default()
        };
        if let Some(profile) = stealth_profile {
            options.stealth_profile = Some(profile.clone());
        }

        let runtime = BrowserJsRuntime::with_options(dom, options);
        let mut event_loop = BrowserEventLoop::new(runtime);

        if std::env::var_os("BROWSER_OXIDE_CHL_TIMING_TRACE").is_some() {
            let object_url_delay_ms = std::env::var("BROWSER_OXIDE_OBJECT_URL_DELAY_MS")
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0)
                .clamp(0.0, 5_000.0);
            let timing_trace_script = r#"
globalThis.__oxChlTimingTrace = [];
Object.defineProperty(globalThis, '_cf_chl_opt', {
    configurable: true,
    set: function(value) {
        const trace = globalThis.__oxChlTimingTrace;
        const proxy = new Proxy(value, {
            set: function(target, key, next) {
                if (key === 'HPTB8' || key === 'USPmo4' || key === 'QWkZ6' ||
                    key === 'OuRz1' || key === 'GrXx7' || key === 'tLOO8' ||
                    key === 'AOzXq2' || key === 'lSKCv5' || key === 'xZLH1' ||
                    key === 'DausG2' || key === 'CXYBD4' || key === 'BHwY7') {
                    const record = {
                        key: String(key), value: next, date: Date.now(),
                        now: performance.now(),
                        stack: String(new Error().stack || '').slice(0, 1200)
                    };
                    if (key === 'HPTB8') {
                        record.vmEventN = globalThis.__oxVmEventN || 0;
                        record.vmEventCounts = Object.assign(
                            {}, globalThis.__oxVmEventCounts || {}
                        );
                        record.vmEvents = (globalThis.__oxVmEvents || []).slice(-1024);
                        record.opCallCount = globalThis.__oxOpCallCount || 0;
                    }
                    trace.push(record);
                }
                return Reflect.set(target, key, next);
            }
        });
        Object.defineProperty(globalThis, '_cf_chl_opt', {
            value: proxy,
            writable: true,
            configurable: true,
            enumerable: true
        });
    }
});
globalThis.__oxObjectUrlDiag = [];
if (globalThis.URL && typeof globalThis.URL.createObjectURL === 'function') {
    const nativeCreateObjectURL = globalThis.URL.createObjectURL;
    globalThis.URL.createObjectURL = new Proxy(nativeCreateObjectURL, {
        apply: function(target, thisArg, args) {
            const started = performance.now();
            const value = args[0];
            const record = {
                phase: 'call', at: started,
                size: Number(value && value.size) || 0,
                type: String(value && value.type || ''),
                tag: Object.prototype.toString.call(value),
                stack: String(new Error().stack || '').slice(0, 1600)
            };
            // Diagnostics-only: small JavaScript worker blobs are normally
            // loader shims. Capture their source so a stalled worker can be
            // distinguished from a parent that simply never posted work.
            // Keep the cap low so opaque proof programs are never dumped.
            try {
                if (record.size <= 1024
                    && record.type === 'text/javascript'
                    && value && value._data instanceof Uint8Array) {
                    record.smallJsSource = new TextDecoder('utf-8', {fatal:false})
                        .decode(value._data).slice(0, 1024);
                }
            } catch (_) {}
            globalThis.__oxObjectUrlDiag.push(record);
            try {
                const result = Reflect.apply(target, thisArg, args);
                const matchingDelay = globalThis.__oxObjectUrlDiag.length === 1
                    ? {object_url_delay_ms} : 0;
                const delayUntil = performance.now() + matchingDelay;
                while (performance.now() < delayUntil) {}
                record.phase = 'return';
                record.end = performance.now();
                record.duration = record.end - started;
                record.result = String(result).slice(0, 200);
                return result;
            } catch (error) {
                record.phase = 'throw';
                record.end = performance.now();
                record.duration = record.end - started;
                record.error = String(error && error.stack || error).slice(0, 1200);
                throw error;
            }
        }
    });
}
globalThis.__oxRunProgramDiag = [];
let __oxRunProgramValue;
const __oxShortHash = function(value) {
    if (typeof value !== 'string') return null;
    let hash = 2166136261 >>> 0;
    for (let i = 0; i < value.length; i++) {
        hash ^= value.charCodeAt(i);
        hash = Math.imul(hash, 16777619) >>> 0;
    }
    return hash.toString(16).padStart(8, '0');
};
Object.defineProperty(globalThis, 'runProgram', {
    configurable: true,
    enumerable: false,
    get: function() { return __oxRunProgramValue; },
    set: function(value) {
        const assigned = {
            phase: 'assign', at: performance.now(), type: typeof value,
            name: String(value && value.name || ''),
            length: Number(value && value.length) || 0,
            stack: String(new Error().stack || '').slice(0, 1600)
        };
        globalThis.__oxRunProgramDiag.push(assigned);
        if (typeof value !== 'function') {
            __oxRunProgramValue = value;
            return;
        }
        __oxRunProgramValue = new Proxy(value, {
            apply: function(target, thisArg, args) {
                const started = performance.now();
                const record = {
                    phase: 'factory-call', at: started, argc: args.length,
                    argTypes: args.map(function(arg) { return Object.prototype.toString.call(arg); }),
                    argLengths: args.map(function(arg) { return Number(arg && (arg.length === undefined ? arg.byteLength : arg.length)) || 0; }),
                    argHashes: args.map(__oxShortHash),
                    stack: String(new Error().stack || '').slice(0, 1600)
                };
                globalThis.__oxRunProgramDiag.push(record);
                const result = Reflect.apply(target, thisArg, args);
                record.end = performance.now();
                record.duration = record.end - started;
                record.resultType = typeof result;
                if (typeof result !== 'function') return result;
                return new Proxy(result, {
                    apply: function(innerTarget, innerThis, innerArgs) {
                        const innerStarted = performance.now();
                        const innerRecord = {
                            phase: 'program-call', at: innerStarted, argc: innerArgs.length,
                            argTypes: innerArgs.map(function(arg) { return Object.prototype.toString.call(arg); }),
                            stack: String(new Error().stack || '').slice(0, 1600)
                        };
                        globalThis.__oxRunProgramDiag.push(innerRecord);
                        try {
                            const innerResult = Reflect.apply(innerTarget, innerThis, innerArgs);
                            innerRecord.end = performance.now();
                            innerRecord.duration = innerRecord.end - innerStarted;
                            innerRecord.resultType = typeof innerResult;
                            return innerResult;
                        } catch (error) {
                            innerRecord.end = performance.now();
                            innerRecord.duration = innerRecord.end - innerStarted;
                            innerRecord.error = String(error && error.stack || error).slice(0, 1200);
                            throw error;
                        }
                    }
                });
            }
        });
    }
});
globalThis.__oxWorkerCtorEarlyDiag = [];
if (typeof globalThis.Worker === 'function') {
    const nativeWorker = globalThis.Worker;
    globalThis.Worker = new Proxy(nativeWorker, {
        construct: function(target, args, newTarget) {
            const record = {
                phase: 'call', at: performance.now(),
                url: String(args[0] || '').slice(0, 300),
                options: args.length > 1 ? String(args[1]) : '',
                stack: String(new Error().stack || '').slice(0, 1600)
            };
            globalThis.__oxWorkerCtorEarlyDiag.push(record);
            try {
                const result = Reflect.construct(target, args, newTarget);
                record.phase = 'return';
                record.end = performance.now();
                record.duration = record.end - record.at;
                return result;
            } catch (error) {
                record.phase = 'throw';
                record.end = performance.now();
                record.duration = record.end - record.at;
                record.error = String(error && error.stack || error).slice(0,1600);
                throw error;
            }
        }
    });
}
"#
            .replace("{object_url_delay_ms}", &object_url_delay_ms.to_string());
            event_loop.execute_script(&timing_trace_script)?;
        }

        // Set location
        let url_js = url.replace('\\', "\\\\").replace('\'', "\\'");
        event_loop
            .execute_script(&format!("location.href = '{}';", url_js))
            .ok();

        // The frame reads `window.name` on init, so set it before its scripts run.
        if !name.is_empty() {
            let name_js = name.replace('\\', "\\\\").replace('\'', "\\'");
            event_loop
                .execute_script(&format!(
                    "try{{globalThis.name='{}';}}catch(_){{}}",
                    name_js
                ))
                .ok();
        }

        // Wire id/parent/top before its scripts run, so a nested iframe it inserts
        // is queued for the frame tree instead of materialized as a child realm.
        if let Some((frame_id, parent_id, top_id)) = frame_ids {
            event_loop
                .execute_script(&format!(
                    "globalThis.__oxFrameSetup && globalThis.__oxFrameSetup({frame_id},{parent_id},{top_id});"
                ))
                .ok();
        }

        // A cross-origin iframe's `document.referrer` and `location.ancestorOrigins`
        // reflect the embedding chain; set them before its scripts run.
        let ref_js = referrer.replace('\\', "\\\\").replace('\'', "\\'");
        let ao_js: String = {
            let items: Vec<String> = ancestor_origins
                .iter()
                .map(|o| format!("'{}'", o.replace('\\', "\\\\").replace('\'', "\\'")))
                .collect();
            format!("[{}]", items.join(","))
        };
        event_loop
            .execute_script(&format!(
                "try{{Object.defineProperty(globalThis,'__frameReferrer',{{value:'{ref_js}',writable:true,configurable:true,enumerable:false}});Object.defineProperty(globalThis,'__frameAncestorOrigins',{{value:{ao_js},writable:true,configurable:true,enumerable:false}});}}catch(_){{}}"
            ))
            .ok();

        // Execute child-document scripts with the same browser scheduling
        // model as top-level documents: parser-blocking classic scripts run
        // during parsing, defer and non-async module entries wait until the
        // parser completes, and async entries run as soon as their fetch is
        // ready. External resources resolve against <base>/document URL with
        // URL::join so ports and ordinary relative paths are preserved.
        run_frame_script_schedule(&mut event_loop, &scripts, &resp.url, &document_base, client)
            .await;

        // See from_isolated_html: gate parent->child postMessage delivery on
        // completed initial load.
        event_loop
            .execute_script(
                "try{Object.defineProperty(globalThis,'__oxFrameReady',{value:1,writable:true,configurable:true,enumerable:false})}catch(_){try{globalThis.__oxFrameReady=1}catch(_){}}",
            )
            .ok();

        // `run_until_settled` returns once the frame has loaded with no in-flight
        // fetch, ignoring the perpetual short timers a live frame keeps running.
        match crate::page::navigation_timeout() {
            Some(timeout) => event_loop.run_until_settled(timeout).await?,
            None => event_loop.run_until_settled_unbounded().await?,
        };

        Ok((
            Self {
                node_id,
                event_loop,
            },
            navigation_timing,
        ))
    }

    /// Evaluate JS in the child's V8 context.
    pub(crate) fn evaluate(&mut self, js: &str) -> Result<String, deno_core::error::AnyError> {
        self.event_loop.execute_script(js)
    }

    /// Run the child's event loop until idle or the caller's deadline.
    pub(crate) async fn pump(
        &mut self,
        timeout: Duration,
    ) -> Result<(), deno_core::error::AnyError> {
        let _ = self.event_loop.run_until_idle(timeout).await;
        Ok(())
    }
}

/// Find all `<iframe>` elements in the DOM.
pub fn find_iframes(dom: &Dom) -> Vec<IframeInfo> {
    let mut iframes = Vec::new();
    collect_iframes(dom, NodeId::DOCUMENT, &mut iframes);
    iframes
}

fn collect_iframes(dom: &Dom, node_id: NodeId, iframes: &mut Vec<IframeInfo>) {
    let children = dom.children(node_id);
    for child_id in children {
        if let Some(node) = dom.get(child_id) {
            if let NodeData::Element(elem) = &node.data {
                if elem.name.local.eq_ignore_ascii_case("iframe") {
                    let srcdoc = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "srcdoc")
                        .map(|a| a.value.clone());
                    let src = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "src")
                        .map(|a| a.value.clone());
                    let name = elem
                        .attrs
                        .iter()
                        .find(|a| a.name.local == "name")
                        .map(|a| a.value.clone());
                    iframes.push(IframeInfo {
                        node_id: child_id,
                        srcdoc,
                        src,
                        name,
                    });
                }
            }
            collect_iframes(dom, child_id, iframes);
        }
    }
}
