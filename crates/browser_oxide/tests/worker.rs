//! Smoke tests for the real Web Worker implementation.
//!
//! Run: cargo test -p js_runtime --test worker -- --test-threads=1 --nocapture

use browser_oxide::js_runtime::BrowserJsRuntime;
use std::time::Duration;

fn drive_runtime(code: &str, wait_ms: u64) -> String {
    drive_runtime_with_secure_context(code, wait_ms, false)
}

fn drive_runtime_with_secure_context(code: &str, wait_ms: u64, secure: bool) -> String {
    let dom = browser_oxide::html_parser::parse_html(
        "<html><head></head><body><div id=\"out\"></div></body></html>",
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        let mut runtime = BrowserJsRuntime::with_options(
            dom,
            browser_oxide::js_runtime::runtime::BrowserRuntimeOptions {
                is_secure_context: secure,
                ..Default::default()
            },
        );
        runtime.execute_script(code, None).unwrap();
        // Drive the event loop with a bounded timeout, allowing setInterval
        // polling (Worker uses 5 ms poll) time to deliver the reply.
        let deadline = std::time::Instant::now() + Duration::from_millis(wait_ms);
        loop {
            if std::time::Instant::now() >= deadline {
                break;
            }
            let remaining = deadline - std::time::Instant::now();
            let tick = remaining.min(Duration::from_millis(50));
            let fut = Box::pin(runtime.run_event_loop());
            let _ = tokio::time::timeout(tick, fut).await;
            // Check if we got an answer yet.
            if let Ok(val) =
                runtime.execute_script("document.querySelector('#out').textContent || ''", None)
            {
                if !val.is_empty() {
                    return val;
                }
            }
        }
        runtime
            .execute_script("document.querySelector('#out').textContent || ''", None)
            .unwrap_or_default()
    })
}

#[test]
fn worker_echo_round_trip() {
    let code = r#"
        const src = `
            self.onmessage = function(e) {
                self.postMessage('echo:' + e.data);
            };
        `;
        const blob = new Blob([src], { type: 'text/javascript' });
        const url = URL.createObjectURL(blob);
        const w = new Worker(url);
        w.onmessage = function(e) {
            document.querySelector('#out').textContent = e.data;
            w.terminate();
        };
        setTimeout(() => w.postMessage('hello'), 20);
    "#;
    let out = drive_runtime(code, 2000);
    assert_eq!(out, "echo:hello", "worker should echo 'echo:hello'");
}

#[test]
fn worker_addeventlistener_roundtrip() {
    let code = r#"
        const src = `
            self.addEventListener('message', function(e) {
                self.postMessage({
                    type: 'reply',
                    n: e.data.n + 1,
                    incomingIsMessageEvent: e instanceof MessageEvent,
                    incomingIsTrusted: e.isTrusted,
                    incomingOrigin: e.origin,
                    incomingSourceIsNull: e.source === null,
                    incomingTargetIsSelf: e.target === self,
                });
            });
        `;
        const blob = new Blob([src], { type: 'text/javascript' });
        const url = URL.createObjectURL(blob);
        const w = new Worker(url);
        w.addEventListener('message', function(e) {
            document.querySelector('#out').textContent = JSON.stringify({
                data: e.data,
                isMessageEvent: e instanceof MessageEvent,
                isTrusted: e.isTrusted,
                targetIsWorker: e.target === w,
                currentTargetIsWorker: e.currentTarget === w,
                workerIsEventTarget: w instanceof EventTarget,
                workerPrototypeChain:
                    Object.getPrototypeOf(Worker.prototype) === EventTarget.prototype,
                workerConstructorChain: Object.getPrototypeOf(Worker) === EventTarget,
                workerOwnNames: Object.getOwnPropertyNames(w),
                workerPrototypeNames: Object.getOwnPropertyNames(Worker.prototype),
                workerTag: Object.prototype.toString.call(w),
            });
            w.terminate();
        });
        setTimeout(() => w.postMessage({ n: 41 }), 20);
    "#;
    let out = drive_runtime(code, 2000);
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}; raw={out}"));
    assert_eq!(v["data"]["type"], "reply", "{out}");
    assert_eq!(v["data"]["n"], 42, "{out}");
    assert_eq!(v["data"]["incomingIsMessageEvent"], true, "{out}");
    assert_eq!(v["data"]["incomingIsTrusted"], true, "{out}");
    assert_eq!(v["data"]["incomingOrigin"], "", "{out}");
    assert_eq!(v["data"]["incomingSourceIsNull"], true, "{out}");
    assert_eq!(v["data"]["incomingTargetIsSelf"], true, "{out}");
    assert_eq!(v["isMessageEvent"], true, "{out}");
    assert_eq!(v["isTrusted"], true, "{out}");
    assert_eq!(v["targetIsWorker"], true, "{out}");
    assert_eq!(v["currentTargetIsWorker"], true, "{out}");
    assert_eq!(v["workerIsEventTarget"], true, "{out}");
    assert_eq!(v["workerPrototypeChain"], true, "{out}");
    assert_eq!(v["workerConstructorChain"], true, "{out}");
    assert_eq!(v["workerOwnNames"], serde_json::json!([]), "{out}");
    assert_eq!(
        v["workerPrototypeNames"],
        serde_json::json!([
            "onmessage",
            "postMessage",
            "terminate",
            "constructor",
            "onerror"
        ]),
        "{out}"
    );
    assert_eq!(v["workerTag"], "[object Worker]", "{out}");
}

/// `self.location` must be populated from the URL the
/// worker was constructed with. Recaptcha enterprise's webworker reads
/// `self.location.origin` to verify it was loaded from a trusted
/// recaptcha.net URL; an undefined/missing location bails the token flow.
///
/// Uses a blob: URL so the worker source is deterministic across runs;
/// the `URL.createObjectURL` registers a real `blob:` scheme URL, which
/// `op_worker_self_url` echoes back, and `new URL(blob:…)` parses it
/// into origin/protocol/etc.
#[test]
fn worker_self_location_populated_from_construction_url() {
    let code = r#"
        const src = `
            self.onmessage = function(e) {
                self.postMessage(JSON.stringify({
                    has_location: typeof self.location === 'object' && self.location !== null,
                    href: self.location && self.location.href,
                    protocol: self.location && self.location.protocol,
                    origin: self.location && self.location.origin,
                    toString_works: self.location && (self.location + '') === self.location.href,
                }));
            };
        `;
        const blob = new Blob([src], { type: 'text/javascript' });
        const url = URL.createObjectURL(blob);
        const w = new Worker(url);
        w.onmessage = function(e) {
            document.querySelector('#out').textContent = e.data;
            w.terminate();
        };
        setTimeout(() => w.postMessage('go'), 20);
    "#;
    let out = drive_runtime(code, 2000);
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}; raw={out}"));
    assert_eq!(v["has_location"], true, "self.location must exist: {out}");
    assert!(
        v["href"].as_str().unwrap_or("").starts_with("blob:"),
        "href must echo the blob: URL: {out}"
    );
    // Load-bearing for recaptcha-class probes: location.toString() === href.
    assert_eq!(
        v["toString_works"], true,
        "location toString must equal href: {out}"
    );
    // vNext/10 URL polyfill blob: fix — real Chrome on a blob:null/uuid URL
    // returns `.protocol === "blob:"` and `.origin === "null"`. Pre-fix
    // the polyfill emitted "" for protocol; post-fix it matches Chrome.
    assert_eq!(
        v["protocol"], "blob:",
        "blob: URL must report protocol=\"blob:\": {out}"
    );
    assert_eq!(
        v["origin"], "null",
        "blob: URL must report origin=\"null\": {out}"
    );
}

/// MessageChannel and CacheStorage are both exposed in a secure dedicated
/// worker in Chrome. Turnstile-class challenge workers use MessageChannel as a
/// task scheduler and also include both names in their worker-realm capability
/// probe, so an absent implementation is observable even when no cache write
/// is attempted.
#[test]
fn worker_message_channel_and_cache_storage_are_functional() {
    let code = r#"
        const src = `
            self.onmessage = async function() {
              try {
                const channel = new MessageChannel();
                const channelResult = new Promise((resolve) => {
                    channel.port1.onmessage = (event) => resolve(event.data);
                });
                channel.port2.postMessage({ answer: 42 });

                const cache = await caches.open('probe');
                const result = {
                    workerScopeType: typeof WorkerGlobalScope,
                    dedicatedScopeType: typeof DedicatedWorkerGlobalScope,
                    channelType: typeof MessageChannel,
                    portType: typeof MessagePort,
                    channelTag: Object.prototype.toString.call(channel),
                    portTag: Object.prototype.toString.call(channel.port1),
                    channelValue: (await channelResult).answer,
                    cachesType: typeof caches,
                    cacheStorageTag: Object.prototype.toString.call(caches),
                    cacheTag: Object.prototype.toString.call(cache),
                    cacheNames: (await caches.keys()).length,
                    cacheMatch: await caches.match('/missing'),
                };
                channel.port1.close();
                channel.port2.close();
                self.postMessage(JSON.stringify(result));
              } catch (error) {
                self.postMessage(JSON.stringify({ error: String(error && error.stack || error) }));
              }
            };
        `;
        const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
        const w = new Worker(url);
        w.onmessage = function(e) {
            document.querySelector('#out').textContent = e.data;
            w.terminate();
        };
        setTimeout(() => w.postMessage('go'), 20);
    "#;
    let out = drive_runtime_with_secure_context(code, 2000, true);
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}; raw={out}"));
    assert_eq!(v["error"], serde_json::Value::Null, "{out}");
    assert_eq!(v["workerScopeType"], "function", "{out}");
    assert_eq!(v["dedicatedScopeType"], "function", "{out}");
    assert_eq!(v["channelType"], "function", "{out}");
    assert_eq!(v["portType"], "function", "{out}");
    assert_eq!(v["channelTag"], "[object MessageChannel]", "{out}");
    assert_eq!(v["portTag"], "[object MessagePort]", "{out}");
    assert_eq!(v["channelValue"], 42, "{out}");
    assert_eq!(v["cachesType"], "object", "{out}");
    assert_eq!(v["cacheStorageTag"], "[object CacheStorage]", "{out}");
    assert_eq!(v["cacheTag"], "[object Cache]", "{out}");
    assert_eq!(v["cacheNames"], 0, "{out}");
    assert_eq!(v["cacheMatch"], serde_json::Value::Null, "{out}");
}

/// Chrome's origin-private file-system surface is available in secure
/// dedicated workers. Managed Turnstile uses this exact chain as a timing
/// probe and waits for the worker reply before submitting its next `/fo`.
#[test]
fn worker_origin_private_file_system_sync_access_round_trip() {
    let code = r#"
        const src = `
            self.onmessage = async function() {
              try {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle('turnstile-probe', { create: true });
                const access = await file.createSyncAccessHandle();
                const bytes = new Uint8Array([7, 8, 9]);
                const wrote = access.write(bytes, { at: 0 });
                access.flush();
                const read = new Uint8Array(3);
                const readCount = access.read(read, { at: 0 });
                const result = {
                    getDirectoryType: typeof navigator.storage.getDirectory,
                    rootTag: Object.prototype.toString.call(root),
                    fileTag: Object.prototype.toString.call(file),
                    accessTag: Object.prototype.toString.call(access),
                    createSyncAccessHandleType: typeof file.createSyncAccessHandle,
                    wrote,
                    readCount,
                    read: Array.from(read),
                    size: access.getSize(),
                    mode: access.mode,
                    flush: access.flush(),
                    close: access.close(),
                };
                self.postMessage(JSON.stringify(result));
              } catch (error) {
                self.postMessage(JSON.stringify({ error: String(error && error.stack || error) }));
              }
            };
        `;
        const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
        const worker = new Worker(url);
        worker.onmessage = function(event) {
            document.querySelector('#out').textContent = event.data;
            worker.terminate();
        };
        setTimeout(() => worker.postMessage('go'), 20);
    "#;
    let out = drive_runtime_with_secure_context(code, 2000, true);
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}; raw={out}"));
    assert_eq!(v["error"], serde_json::Value::Null, "{out}");
    assert_eq!(v["getDirectoryType"], "function", "{out}");
    assert_eq!(v["rootTag"], "[object FileSystemDirectoryHandle]", "{out}");
    assert_eq!(v["fileTag"], "[object FileSystemFileHandle]", "{out}");
    assert_eq!(
        v["accessTag"], "[object FileSystemSyncAccessHandle]",
        "{out}"
    );
    assert_eq!(v["createSyncAccessHandleType"], "function", "{out}");
    assert_eq!(v["wrote"], 3, "{out}");
    assert_eq!(v["readCount"], 3, "{out}");
    assert_eq!(v["read"], serde_json::json!([7, 8, 9]), "{out}");
    assert_eq!(v["size"], 3, "{out}");
    assert_eq!(v["mode"], "readwrite", "{out}");
}

#[test]
fn worker_origin_private_file_system_flush_has_storage_latency() {
    let code = r#"
        const src = `
            self.onmessage = async function() {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle('flush-latency', { create: true });
                const handle = await file.createSyncAccessHandle();
                const started = performance.now();
                handle.flush();
                const elapsed = performance.now() - started;
                handle.close();
                self.postMessage(String(elapsed));
            };
        `;
        const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
        const worker = new Worker(url);
        worker.onmessage = function(event) {
            document.querySelector('#out').textContent = event.data;
            worker.terminate();
        };
        setTimeout(() => worker.postMessage('go'), 20);
    "#;
    let out = drive_runtime_with_secure_context(code, 2000, true);
    let elapsed: f64 = out
        .parse()
        .unwrap_or_else(|e| panic!("invalid latency: {e}; raw={out}"));
    assert!(elapsed >= 4.4, "flush latency was {elapsed}ms");
    assert!(elapsed < 20.0, "flush latency was {elapsed}ms");
}

/// Chrome exposes a deliberately smaller namespace inside a dedicated worker
/// than on Window. Loading the shared interface bootstrap without a final
/// worker-specific normalization leaks hundreds of DOM/HTML constructors and
/// is a high-signal cross-realm fingerprint.
#[test]
fn chrome_148_worker_namespace_and_prototype_shape() {
    let code = r#"
        const src = `
            const trustedPolicy = trustedTypes.createPolicy('worker-regression', {
                createScript(source) { return source; }
            });
            const trustedScript = trustedPolicy.createScript('21 * 2');
            const names = value => Object.getOwnPropertyNames(value).sort();
            self.postMessage(JSON.stringify({
                globalCount: names(globalThis).length,
                enumerableNames: Object.keys(globalThis).sort(),
                hasDocument: 'document' in globalThis,
                hasHtmlElement: 'HTMLElement' in globalThis,
                requiredTypes: Object.fromEntries([
                    'BroadcastChannel', 'PerformanceObserver', 'StorageManager',
                    'Worker', 'WorkerLocation', 'WorkerNavigator',
                    'requestAnimationFrame', 'cancelAnimationFrame'
                ].map(name => [name, typeof globalThis[name]])),
                globalProto: names(Object.getPrototypeOf(globalThis)),
                workerGlobalProto: names(WorkerGlobalScope.prototype),
                dedicatedProto: names(DedicatedWorkerGlobalScope.prototype),
                navigatorOwn: names(navigator),
                navigatorProto: names(WorkerNavigator.prototype),
                uaDataEnumerable: Object.keys(Object.getPrototypeOf(navigator.userAgentData)).sort(),
                workerScopeSetters: ['origin', 'performance', 'scheduler'].every((name) => {
                    const descriptor = Object.getOwnPropertyDescriptor(WorkerGlobalScope.prototype, name);
                    return !!descriptor && typeof descriptor.set === 'function';
                }),
                eventTargetWhen: typeof EventTarget.prototype.when === 'function',
                trustedTypes: {
                    createPolicy: typeof trustedTypes.createPolicy,
                    isScript: trustedTypes.isScript(trustedScript),
                    directEval: eval(trustedScript),
                },
                tag: Object.prototype.toString.call(self),
                scopeChecks: [
                    self === globalThis,
                    self instanceof WorkerGlobalScope,
                    self instanceof DedicatedWorkerGlobalScope,
                    Object.getPrototypeOf(self) === DedicatedWorkerGlobalScope.prototype,
                    Object.getPrototypeOf(WorkerGlobalScope.prototype) === EventTarget.prototype
                ]
            }));
        `;
        const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
        const w = new Worker(url);
        w.onmessage = function(e) {
            document.querySelector('#out').textContent = e.data;
            w.terminate();
        };
    "#;
    let out = drive_runtime_with_secure_context(code, 2000, true);
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}; raw={out}"));
    assert_eq!(v["globalCount"], 335, "{out}");
    assert_eq!(
        v["enumerableNames"],
        serde_json::json!([
            "cancelAnimationFrame",
            "close",
            "name",
            "onmessage",
            "onmessageerror",
            "onrtctransform",
            "postMessage",
            "requestAnimationFrame",
            "webkitRequestFileSystem",
            "webkitRequestFileSystemSync",
            "webkitResolveLocalFileSystemSyncURL",
            "webkitResolveLocalFileSystemURL"
        ]),
        "{out}"
    );
    assert_eq!(v["hasDocument"], false, "{out}");
    assert_eq!(v["hasHtmlElement"], false, "{out}");
    assert_eq!(v["tag"], "[object DedicatedWorkerGlobalScope]", "{out}");
    assert_eq!(
        v["scopeChecks"],
        serde_json::json!([true, true, true, true, true]),
        "{out}"
    );
    assert_eq!(
        v["globalProto"],
        serde_json::json!(["PERSISTENT", "TEMPORARY", "constructor"]),
        "{out}"
    );
    assert_eq!(
        v["dedicatedProto"],
        serde_json::json!(["PERSISTENT", "TEMPORARY", "constructor"]),
        "{out}"
    );
    assert_eq!(v["navigatorOwn"], serde_json::json!([]), "{out}");
    assert_eq!(
        v["uaDataEnumerable"],
        serde_json::json!([
            "brands",
            "getHighEntropyValues",
            "mobile",
            "platform",
            "toJSON"
        ]),
        "{out}"
    );
    assert_eq!(v["workerScopeSetters"], true, "{out}");
    assert_eq!(v["eventTargetWhen"], true, "{out}");
    assert_eq!(v["trustedTypes"]["createPolicy"], "function", "{out}");
    assert_eq!(v["trustedTypes"]["isScript"], true, "{out}");
    assert_eq!(v["trustedTypes"]["directEval"], 42, "{out}");
    assert_eq!(
        v["navigatorProto"],
        serde_json::json!([
            "appCodeName",
            "appName",
            "appVersion",
            "connection",
            "constructor",
            "deviceMemory",
            "gpu",
            "hardwareConcurrency",
            "hid",
            "language",
            "languages",
            "locks",
            "mediaCapabilities",
            "onLine",
            "permissions",
            "platform",
            "product",
            "serial",
            "storage",
            "storageBuckets",
            "usb",
            "userAgent",
            "userAgentData"
        ]),
        "{out}"
    );
    assert_eq!(
        v["workerGlobalProto"].as_array().map(Vec::len),
        Some(30),
        "{out}"
    );
    for name in [
        "BroadcastChannel",
        "PerformanceObserver",
        "StorageManager",
        "Worker",
        "WorkerLocation",
        "WorkerNavigator",
        "requestAnimationFrame",
        "cancelAnimationFrame",
    ] {
        assert_eq!(v["requiredTypes"][name], "function", "{name}: {out}");
    }
}
