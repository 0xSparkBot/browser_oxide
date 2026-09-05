// worker_bootstrap.js — runs inside a dedicated Worker V8 isolate.
//
// Sets up the worker-side surface: `self`, postMessage, onmessage dispatch,
// close, and a navigator stub. A setInterval-driven poll loop drains
// parent→worker messages via op_worker_self_recv and fires onmessage events.

((globalThis) => {
    const ops = Deno.core.ops;
    const _browser_oxide = globalThis.__browser_oxide;
    const _markTrustedEvent =
        (_browser_oxide && _browser_oxide._markTrustedEvent)
        || globalThis.__bo_mark_trusted
        || ((event) => event);
    const _diagnosticsEnabled = globalThis.__oxideDiagnostics === true;
    try { delete globalThis.__oxideDiagnostics; } catch (_) {}

    // Helper: read from stealth profile or use default
    const _p = (key, fallback) => {
        if (ops.op_has_stealth_profile && ops.op_has_stealth_profile()) {
            const v = ops.op_get_profile_value(key);
            return v !== "" ? v : fallback;
        }
        return fallback;
    };
    const _pInt = (key, fallback) => {
        const v = _p(key, "");
        return v !== "" ? parseInt(v, 10) : fallback;
    };
    const _pJson = (key, fallback) => {
        const v = _p(key, "");
        if (v !== "") try { return JSON.parse(v); } catch {}
        return fallback;
    };
    // Firefox/Gecko coherence (mirrors window_bootstrap). The worker realm must
    // not leak Chrome-only APIs under a Firefox UA — bot-detection sensors also
    // run inside workers, so a Chrome-shaped WorkerNavigator there is the same
    // impersonation tell as on the main thread.
    const _isFirefox = () => /Firefox\//.test(_p("user_agent", ""));

    // The global object doubles as WorkerGlobalScope / DedicatedWorkerGlobalScope / self.
    const self = globalThis;
    self.self = self;
    {
        const WorkerGlobalScope = function WorkerGlobalScope() {
            throw new TypeError('Illegal constructor');
        };
        const DedicatedWorkerGlobalScope = function DedicatedWorkerGlobalScope() {
            throw new TypeError('Illegal constructor');
        };
        DedicatedWorkerGlobalScope.prototype = Object.create(WorkerGlobalScope.prototype);
        Object.defineProperty(DedicatedWorkerGlobalScope.prototype, 'constructor', {
            value: DedicatedWorkerGlobalScope,
            configurable: true,
            writable: true,
        });
        Object.defineProperty(WorkerGlobalScope, Symbol.hasInstance, {
            value(candidate) { return candidate === globalThis; },
            configurable: true,
        });
        Object.defineProperty(DedicatedWorkerGlobalScope, Symbol.hasInstance, {
            value(candidate) { return candidate === globalThis; },
            configurable: true,
        });
        Object.defineProperty(WorkerGlobalScope.prototype, Symbol.toStringTag, {
            value: 'WorkerGlobalScope', configurable: true,
        });
        Object.defineProperty(DedicatedWorkerGlobalScope.prototype, Symbol.toStringTag, {
            value: 'DedicatedWorkerGlobalScope', configurable: true,
        });
        globalThis.WorkerGlobalScope = WorkerGlobalScope;
        globalThis.DedicatedWorkerGlobalScope = DedicatedWorkerGlobalScope;
        if (typeof _maskFunction === 'function') {
            _maskFunction(WorkerGlobalScope, 'WorkerGlobalScope');
            _maskFunction(DedicatedWorkerGlobalScope, 'DedicatedWorkerGlobalScope');
        }
    }

    // --- WorkerLocation ---
    // Real Chrome workers expose `self.location` as a WorkerLocation
    // object reporting the script's URL. Some workers read
    // `self.location.origin` to verify they were loaded from an
    // expected URL; absence can silently break their flow.
    if (!self.location) {
        try {
            const _workerUrl = (ops && typeof ops.op_worker_self_url === 'function')
                ? ops.op_worker_self_url()
                : '';
            if (_workerUrl) {
                const _u = new URL(_workerUrl);
                const _locationState = new WeakMap();
                const WorkerLocation = function WorkerLocation() {
                    throw new TypeError('Illegal constructor');
                };
                const _locationGetters = {
                    href: () => _u.href,
                    origin: () => _u.origin,
                    protocol: () => _u.protocol,
                    host: () => _u.host,
                    hostname: () => _u.hostname,
                    port: () => _u.port,
                    pathname: () => _u.pathname,
                    search: () => _u.search,
                    hash: () => _u.hash,
                };
                for (const [name, read] of Object.entries(_locationGetters)) {
                    Object.defineProperty(WorkerLocation.prototype, name, {
                        configurable: true,
                        enumerable: true,
                        get() {
                            const state = _locationState.get(this);
                            if (!state) throw new TypeError('Illegal invocation');
                            return read(state);
                        },
                    });
                }
                Object.defineProperty(WorkerLocation.prototype, 'toString', {
                    configurable: true,
                    enumerable: true,
                    writable: true,
                    value: function toString() {
                        if (!_locationState.has(this)) throw new TypeError('Illegal invocation');
                        return _u.href;
                    },
                });
                Object.defineProperty(WorkerLocation.prototype, Symbol.toStringTag, {
                    value: 'WorkerLocation', configurable: true,
                });
                const location = Object.create(WorkerLocation.prototype);
                _locationState.set(location, _u);
                self.WorkerLocation = WorkerLocation;
                self.location = location;
                if (typeof _maskFunction === 'function') {
                    _maskFunction(WorkerLocation, 'WorkerLocation');
                    _maskFunction(WorkerLocation.prototype.toString, 'toString');
                }
                if (typeof _maskAsNative === 'function') {
                    _maskAsNative(WorkerLocation.prototype,
                        'href', 'origin', 'protocol', 'host', 'hostname',
                        'port', 'pathname', 'search', 'hash');
                }
            }
        } catch (_e) {
            // URL parse failure (rare) — leave location undefined.
        }
    }

    // --- Intl Sync (matches window_bootstrap) ---
    if (ops.op_has_stealth_profile && ops.op_has_stealth_profile()) {
        const profileTz = ops.op_get_profile_value("timezone") || "Europe/Moscow";
        const profileLocale = ops.op_get_profile_value("language") || "ru-RU";
        if (globalThis.Intl) {
            const _intlClasses = ['DateTimeFormat', 'NumberFormat', 'Collator', 'PluralRules', 'RelativeTimeFormat'];
            for (const klass of _intlClasses) {
                if (globalThis.Intl[klass]) {
                    const proto = globalThis.Intl[klass].prototype;
                    const origResolved = proto.resolvedOptions;
                    proto.resolvedOptions = function() {
                        const res = origResolved.call(this);
                        res.timeZone = profileTz || res.timeZone;
                        res.locale = profileLocale || res.locale;
                        return res;
                    };
                }
            }
        }
    }

    // --- WorkerNavigator (matches StealthProfile + Chrome's prototype shape) ---
    if (!self.navigator) {
        // WorkerNavigator is a distinct WebIDL interface. A plain object with
        // eighteen own properties is immediately distinguishable from Chrome,
        // whose instance has no own string properties and exposes the worker-
        // legal subset through enumerable prototype accessors.
        const _osName = _p("os_name", "Windows");
        const _browserMajor = _p("browser_version", "148.0.0.0").split(".")[0];
        const _browserFull = _p("browser_version", "148.0.0.0");
        const _brands = [
            { brand: "Google Chrome", version: _browserMajor },
            { brand: "Not.A/Brand", version: "8" },
            { brand: "Chromium", version: _browserMajor },
        ];
        const _fullVersionList = [
            { brand: "Google Chrome", version: _browserFull },
            { brand: "Not.A/Brand", version: "8.0.0.0" },
            { brand: "Chromium", version: _browserFull },
        ];
        class WorkerNavigatorUAData {
            get brands() { return _brands.slice(); }
            get mobile() { return false; }
            get platform() { return _osName; }
            getHighEntropyValues(hints) {
                if (!Array.isArray(hints)) {
                    return Promise.reject(new TypeError(
                        "Failed to execute 'getHighEntropyValues' on 'NavigatorUAData': The provided value cannot be converted to a sequence."
                    ));
                }
                const out = { brands: _brands.slice(), mobile: false, platform: _osName };
                for (const h of hints) {
                    switch (h) {
                        case "architecture": out.architecture = _p("cpu_architecture", "x86"); break;
                        case "bitness": out.bitness = _p("cpu_bitness", "64"); break;
                        case "model": out.model = _p("ua_model", ""); break;
                        case "platformVersion": out.platformVersion = _p("platform_version", ""); break;
                        case "uaFullVersion": out.uaFullVersion = _browserFull; break;
                        case "fullVersionList": out.fullVersionList = _fullVersionList.slice(); break;
                        case "wow64": out.wow64 = _p("ua_wow64", "false") === "true"; break;
                        case "formFactors": out.formFactors = ["Desktop"]; break;
                        default: break;
                    }
                }
                return Promise.resolve(out);
            }
            toJSON() { return { brands: _brands.slice(), mobile: false, platform: _osName }; }
        }
        Object.defineProperty(WorkerNavigatorUAData.prototype, Symbol.toStringTag, {
            value: "NavigatorUAData", configurable: true,
        });
        // WebIDL members are enumerable prototype properties. Class syntax
        // creates non-enumerable methods/getters, so normalize this bit to
        // Chromium's NavigatorUAData descriptor shape.
        for (const name of [
            'brands', 'mobile', 'platform', 'getHighEntropyValues', 'toJSON',
        ]) {
            const descriptor = Object.getOwnPropertyDescriptor(
                WorkerNavigatorUAData.prototype, name
            );
            if (descriptor) {
                Object.defineProperty(WorkerNavigatorUAData.prototype, name, {
                    ...descriptor,
                    enumerable: true,
                });
            }
        }

        const _singletons = {
            connection: globalThis.NetworkInformation
                ? Object.create(globalThis.NetworkInformation.prototype) : {},
            gpu: globalThis.GPU ? Object.create(globalThis.GPU.prototype) : {},
            hid: globalThis.HID ? Object.create(globalThis.HID.prototype) : {},
            locks: globalThis.LockManager ? Object.create(globalThis.LockManager.prototype) : {},
            mediaCapabilities: globalThis.MediaCapabilities
                ? Object.create(globalThis.MediaCapabilities.prototype) : {},
            permissions: globalThis.Permissions
                ? Object.create(globalThis.Permissions.prototype) : {},
            serial: globalThis.Serial ? Object.create(globalThis.Serial.prototype) : {},
            storage: globalThis.StorageManager
                ? Object.create(globalThis.StorageManager.prototype) : {},
            storageBuckets: globalThis.StorageBucketManager
                ? Object.create(globalThis.StorageBucketManager.prototype) : {},
            usb: globalThis.USB ? Object.create(globalThis.USB.prototype) : {},
        };
        const _values = {
            appCodeName: "Mozilla",
            appName: "Netscape",
            appVersion: _p("app_version", "5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36"),
            connection: _singletons.connection,
            deviceMemory: _pInt("device_memory", 8),
            gpu: _singletons.gpu,
            hardwareConcurrency: _pInt("hardware_concurrency", 8),
            hid: _singletons.hid,
            language: _p("language", "en-US"),
            languages: Object.freeze(_pJson("languages", ["en-US", "en"])),
            locks: _singletons.locks,
            mediaCapabilities: _singletons.mediaCapabilities,
            onLine: true,
            permissions: _singletons.permissions,
            platform: _p("platform", "Win32"),
            product: "Gecko",
            serial: _singletons.serial,
            storage: _singletons.storage,
            storageBuckets: _singletons.storageBuckets,
            usb: _singletons.usb,
            userAgent: _p("user_agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36"),
            userAgentData: new WorkerNavigatorUAData(),
        };
        const WorkerNavigator = function WorkerNavigator() {
            throw new TypeError('Illegal constructor');
        };
        for (const name of Object.keys(_values)) {
            Object.defineProperty(WorkerNavigator.prototype, name, {
                configurable: true,
                enumerable: true,
                get() {
                    if (this !== workerNavigator) throw new TypeError('Illegal invocation');
                    return _values[name];
                },
            });
        }
        Object.defineProperty(WorkerNavigator.prototype, Symbol.toStringTag, {
            value: "WorkerNavigator", configurable: true,
        });
        const workerNavigator = Object.create(WorkerNavigator.prototype);
        self.WorkerNavigator = WorkerNavigator;
        self.NavigatorUAData = WorkerNavigatorUAData;
        self.navigator = workerNavigator;
        if (typeof _maskFunction === 'function') {
            _maskFunction(WorkerNavigator, 'WorkerNavigator');
            _maskFunction(WorkerNavigatorUAData, 'NavigatorUAData');
        }
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(WorkerNavigator.prototype, ...Object.keys(_values));
            _maskAsNative(WorkerNavigatorUAData.prototype,
                'brands', 'mobile', 'platform', 'getHighEntropyValues', 'toJSON');
        }
    }

    // --- performance.now() humanization (matches window_bootstrap) ---
    if (!globalThis.performance) {
        globalThis.performance = {
            now() { return ops.op_perf_now_humanized(); },
        };
    } else {
        globalThis.performance.now = () => ops.op_perf_now_humanized();
    }

    // --- performance.memory jitter (matches window_bootstrap) ---
    if (globalThis.performance) {
        Object.defineProperty(globalThis.performance, 'memory', {
            get() {
                const jsHeapSizeLimit = 4294705152;
                const base = 10485760; // 10 MB
                const jitter = ((Date.now() * 0x9e3779b9) >>> 0) % 5000000;
                const totalJSHeapSize = base + jitter;
                const usedJSHeapSize = Math.floor(totalJSHeapSize * 0.85);
                return { jsHeapSizeLimit, totalJSHeapSize, usedJSHeapSize };
            },
            configurable: true,
            enumerable: true
        });
    }

    // --- MessageChannel / MessagePort ---
    // Dedicated workers expose the same channel primitives as Window.  Apart
    // from being a commonly fingerprinted surface, MessageChannel is also a
    // standard zero-delay task scheduler used by challenge/WASM runtimes.
    // interfaces_bootstrap deliberately leaves these names undefined so that
    // the realm-specific implementation can install the functional classes.
    {
        const _pairedPorts = new WeakMap();
        const _queuedMessages = new WeakMap();
        const _enabledPorts = new WeakMap();
        const _closedPorts = new WeakMap();

        const _cloneMessage = (data) => {
            if (typeof globalThis.structuredClone !== 'function') return data;
            return globalThis.structuredClone(data);
        };

        const _deliverMessage = (port, data) => {
            if (_closedPorts.get(port)) return;
            if (!_enabledPorts.get(port)) {
                const queue = _queuedMessages.get(port) || [];
                queue.push(data);
                _queuedMessages.set(port, queue);
                return;
            }
            const fire = () => {
                if (_closedPorts.get(port)) return;
                port.dispatchEvent(new MessageEvent('message', {
                    data,
                    bubbles: false,
                    cancelable: false,
                }));
            };
            // Message ports enqueue a task. A timer preserves that ordering and
            // avoids synchronous re-entry in schedulers built on a channel.
            try { globalThis.setTimeout(fire, 0); } catch (_e) { fire(); }
        };

        const _enablePort = (port) => {
            if (_enabledPorts.get(port)) return;
            _enabledPorts.set(port, true);
            const queue = _queuedMessages.get(port) || [];
            _queuedMessages.set(port, []);
            for (const message of queue) _deliverMessage(port, message);
        };

        class MessagePort extends EventTarget {
            constructor() {
                super();
                this._onmessage = null;
                this.onmessageerror = null;
            }
            get onmessage() { return this._onmessage; }
            set onmessage(listener) {
                this._onmessage = typeof listener === 'function' ? listener : null;
                if (this._onmessage) _enablePort(this);
            }
            postMessage(data /*, transfer */) {
                if (_closedPorts.get(this)) return;
                const paired = _pairedPorts.get(this);
                if (!paired) return;
                _deliverMessage(paired, _cloneMessage(data));
            }
            start() { _enablePort(this); }
            close() {
                _closedPorts.set(this, true);
                _pairedPorts.delete(this);
                _queuedMessages.set(this, []);
            }
            addEventListener(type, listener, options) {
                super.addEventListener(type, listener, options);
                if (type === 'message') _enablePort(this);
            }
        }
        Object.defineProperty(MessagePort.prototype, Symbol.toStringTag, {
            value: 'MessagePort', configurable: true,
        });

        class MessageChannel {
            constructor() {
                this.port1 = new MessagePort();
                this.port2 = new MessagePort();
                _pairedPorts.set(this.port1, this.port2);
                _pairedPorts.set(this.port2, this.port1);
            }
        }
        Object.defineProperty(MessageChannel.prototype, Symbol.toStringTag, {
            value: 'MessageChannel', configurable: true,
        });

        globalThis.MessagePort = MessagePort;
        globalThis.MessageChannel = MessageChannel;
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(MessagePort.prototype,
                'postMessage', 'start', 'close', 'addEventListener', 'onmessage');
        }
        if (typeof _maskFunction === 'function') {
            _maskFunction(MessagePort, 'MessagePort');
            _maskFunction(MessageChannel, 'MessageChannel');
        }
    }

    // --- CacheStorage ---
    // CacheStorage is exposed in secure dedicated workers. Persistence is not
    // implemented yet, but a standards-shaped asynchronous empty store is much
    // closer to Chrome than an absent global and is sufficient for capability
    // probes and cache-miss paths.
    if (ops.op_is_secure_context && ops.op_is_secure_context()) {
        class Cache {
            match(_request, _options) { return Promise.resolve(undefined); }
            matchAll(_request, _options) { return Promise.resolve([]); }
            add(_request) { return Promise.reject(new TypeError('Cache.add not supported')); }
            addAll(_requests) { return Promise.reject(new TypeError('Cache.addAll not supported')); }
            put(_request, _response) { return Promise.reject(new TypeError('Cache.put not supported')); }
            delete(_request, _options) { return Promise.resolve(false); }
            keys(_request, _options) { return Promise.resolve([]); }
        }
        Object.defineProperty(Cache.prototype, Symbol.toStringTag, {
            value: 'Cache', configurable: true,
        });

        class CacheStorage {
            match(_request, _options) { return Promise.resolve(undefined); }
            has(_cacheName) { return Promise.resolve(false); }
            open(_cacheName) { return Promise.resolve(new Cache()); }
            delete(_cacheName) { return Promise.resolve(false); }
            keys() { return Promise.resolve([]); }
        }
        Object.defineProperty(CacheStorage.prototype, Symbol.toStringTag, {
            value: 'CacheStorage', configurable: true,
        });

        globalThis.Cache = Cache;
        globalThis.CacheStorage = CacheStorage;
        Object.defineProperty(globalThis, 'caches', {
            value: new CacheStorage(),
            configurable: true,
            enumerable: true,
            writable: false,
        });
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(Cache.prototype,
                'match', 'matchAll', 'add', 'addAll', 'put', 'delete', 'keys');
            _maskAsNative(CacheStorage.prototype,
                'match', 'has', 'open', 'delete', 'keys');
        }
        if (typeof _maskFunction === 'function') {
            _maskFunction(Cache, 'Cache');
            _maskFunction(CacheStorage, 'CacheStorage');
        }
    }

    // --- postMessage: send a message to the parent thread ---
    self.postMessage = function (message, transfer) {
        // Validate transferables (same shape as main thread).
        const transferList = Array.isArray(transfer) ? transfer : [];
        for (const t of transferList) {
            if (
                t !== null &&
                !(t instanceof ArrayBuffer) &&
                !(ArrayBuffer.isView && ArrayBuffer.isView(t))
            ) {
                throw new TypeError(
                    "postMessage: transferable must be an ArrayBuffer or view"
                );
            }
        }
        let wire;
        try {
            wire =
                (_browser_oxide &&
                    _browser_oxide.serializeForWire &&
                    _browser_oxide.serializeForWire(message)) ||
                message;
        } catch (e) {
            // DataCloneError — propagate.
            throw e;
        }
        let payload;
        try {
            payload = JSON.stringify({ data: wire });
        } catch (_e) {
            payload = JSON.stringify({ data: null });
        }
        if (_diagnosticsEnabled) {
            try { ops.op_worker_diag_note("tx len=" + payload.length + " @" + Math.round(performance.now())); } catch (_) {}
        }
        ops.op_worker_self_post(payload);
    };

    // --- close: stop this worker's message pump ---
    // Terminating a worker from inside is rare; the parent handles cleanup.
    let _closed = false;
    self.close = function () {
        _closed = true;
        // parent.terminate() drives real shutdown via the terminate flag +
        // notify_worker, which resolves the awaited recv and ends the pump.
    };

    function _dispatchWorkerMessage(s) {
        let payload;
        try {
            payload = JSON.parse(s);
        } catch (_e) {
            if (_diagnosticsEnabled) {
                try { ops.op_worker_diag_note("rx-parse-fail len=" + s.length); } catch (_) {}
            }
            return;
        }
        const deserializer =
            _browser_oxide && _browser_oxide.deserializeFromWire;
        const data = deserializer
            ? deserializer(payload && payload.data)
            : payload && payload.data;
        let shape = typeof data;
        try {
            if (data && typeof data === "object") {
                shape = "object:" + Object.keys(data).slice(0, 5).join(",");
            }
        } catch (_) {}
        if (_diagnosticsEnabled) {
            try {
                ops.op_worker_diag_note(
                    "rx " + shape
                    + (typeof data === 'string' ? " len=" + data.length : "")
                    + " @" + Math.round(performance.now())
                );
            } catch (_) {}
        }
        // This message crossed the browser-managed worker channel, so it is a
        // trusted MessageEvent. Challenge workers commonly reject synthetic
        // delivery with `if (!event.isTrusted) return` before evaluating their
        // sensor payload.
        self.dispatchEvent(_markTrustedEvent(new MessageEvent("message", {
            data,
            origin: "",
            source: null,
            ports: [],
        })));
    }

    // --- Pump: event-driven parent→worker message delivery ---
    // Parks on the worker's Notify via `op_worker_self_await_message`, which
    // resolves with "" once the worker is terminated to end the loop.
    (async function _workerPump() {
        while (!_closed) {
            let s;
            try {
                s = await ops.op_worker_self_await_message();
            } catch (_e) {
                break;
            }
            if (!s) break; // "" ⇒ terminated
            _dispatchWorkerMessage(s);
        }
    })();

    // --- importScripts: classic-worker synchronous script loader ---
    self.importScripts = function importScripts(...urls) {
        for (const raw of urls) {
            const url = String(raw);
            let source;
            if (url.startsWith("blob:")) {
                source = ops.op_blob_fetch_text(url);
                if (!source) throw new Error("importScripts failed to load blob URL " + url);
            } else if (url.startsWith("data:")) {
                const comma = url.indexOf(",");
                if (comma < 0) throw new Error("importScripts: malformed data URL");
                const meta = url.slice(5, comma);
                const body = url.slice(comma + 1);
                if (meta.endsWith(";base64")) {
                    source = atob(decodeURIComponent(body));
                } else {
                    source = decodeURIComponent(body);
                }
            } else if (url.startsWith("http://") || url.startsWith("https://")) {
                source = ops.op_worker_sync_fetch(url);
                if (!source) throw new Error("importScripts failed to load " + url);
            } else {
                throw new Error("importScripts: unsupported URL scheme: " + url);
            }
            (0, eval)(source);
        }
    };

    // MediaSource + MediaRecorder.isTypeSupported in Worker realm.
    // Some scripts read .isTypeSupported in a Worker context; without
    // this it would be an undefined receiver — real Chrome has
    // MediaSource available in DedicatedWorker since Chrome 108.
    const _mediaTypes = new Set([
        "video/mp4", 'video/mp4;codecs="avc1.42E01E,mp4a.40.2"',
        'video/mp4;codecs="avc1.640028"', "video/webm",
        'video/webm;codecs="vp8,vorbis"', 'video/webm;codecs="vp9"',
        'video/webm;codecs="vp9,opus"', "audio/mp4",
        'audio/mp4;codecs="mp4a.40.2"', "audio/webm",
        'audio/webm;codecs=opus', 'audio/webm;codecs=vorbis',
    ]);
    if (!globalThis.MediaSource) {
        globalThis.MediaSource = class MediaSource {
            static isTypeSupported(type) {
                if (typeof type !== 'string') return false;
                if (_mediaTypes.has(type)) return true;
                const base = type.split(';')[0].trim();
                return _mediaTypes.has(base);
            }
        };
    }
    if (!globalThis.MediaRecorder) {
        globalThis.MediaRecorder = class MediaRecorder {
            static isTypeSupported(type) {
                if (typeof type !== 'string') return false;
                if (_mediaTypes.has(type)) return true;
                const base = type.split(';')[0].trim();
                return _mediaTypes.has(base);
            }
        };
    }

    // crypto.subtle for Workers (digest only, rest are stubs)
    if (!globalThis.crypto || !globalThis.crypto.subtle) {
        const _toBytes = (src) => {
            if (src == null) return new Uint8Array(0);
            if (src instanceof Uint8Array) return src;
            if (src instanceof ArrayBuffer) return new Uint8Array(src);
            if (ArrayBuffer.isView(src)) return new Uint8Array(src.buffer, src.byteOffset, src.byteLength);
            return new Uint8Array(src);
        };
        const _subtleStub = (name) => function() {
            return Promise.reject(new DOMException(name + " not implemented", "NotSupportedError"));
        };
        const subtle = {
            digest: function(algorithm, data) {
                try {
                    const algName = typeof algorithm === 'string' ? algorithm : (algorithm && algorithm.name) || "";
                    const bytes = _toBytes(data);
                    const out = ops.op_crypto_digest(String(algName), bytes);
                    return Promise.resolve(out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength));
                } catch (e) {
                    return Promise.reject(e);
                }
            },
        };
        for (const m of ['sign','verify','encrypt','decrypt','generateKey','importKey','exportKey','deriveKey','deriveBits','wrapKey','unwrapKey']) {
            subtle[m] = _subtleStub(m);
        }
        if (!globalThis.crypto) {
            globalThis.crypto = {
                subtle: subtle,
                getRandomValues: function(arr) {
                    ops.op_crypto_random_fill(arr);
                    return arr;
                },
                randomUUID: function() {
                    const b = new Uint8Array(16);
                    ops.op_crypto_random_fill(b);
                    b[6] = (b[6] & 0x0f) | 0x40;
                    b[8] = (b[8] & 0x3f) | 0x80;
                    const h = Array.from(b).map(x => x.toString(16).padStart(2, '0')).join('');
                    return h.slice(0,8)+'-'+h.slice(8,12)+'-'+h.slice(12,16)+'-'+h.slice(16,20)+'-'+h.slice(20);
                }
            };
        } else {
            globalThis.crypto.subtle = subtle;
        }
    }

    // --- Chrome 148 worker-only surface ---------------------------------
    // interfaces_bootstrap is shared with Window, so install the handful of
    // worker-only interfaces here. The final cleanup removes the Window-only
    // half of that shared bootstrap.
    const _workerStub = (name, methods = []) => {
        if (typeof globalThis[name] === 'function') return globalThis[name];
        const ctor = { [name]: function () {
            throw new TypeError('Illegal constructor');
        } }[name];
        for (const method of methods) {
            Object.defineProperty(ctor.prototype, method, {
                configurable: true,
                enumerable: true,
                writable: true,
                value: { [method]: function () {} }[method],
            });
        }
        Object.defineProperty(ctor.prototype, Symbol.toStringTag, {
            value: name, configurable: true,
        });
        globalThis[name] = ctor;
        if (typeof _maskFunction === 'function') _maskFunction(ctor, name);
        if (methods.length && typeof _maskAsNative === 'function') {
            _maskAsNative(ctor.prototype, ...methods);
        }
        return ctor;
    };

    _workerStub('CloseEvent');
    _workerStub('CompressionStream');
    _workerStub('DecompressionStream');
    _workerStub('EventSource', ['close']);
    _workerStub('FileReaderSync', ['readAsArrayBuffer', 'readAsBinaryString', 'readAsDataURL', 'readAsText']);
    const FileSystemSyncAccessHandle = _workerStub(
        'FileSystemSyncAccessHandle',
        ['close', 'flush', 'getSize', 'read', 'truncate', 'write']
    );
    _workerStub('PerformanceEntry', ['toJSON']);
    const PerformanceObserver = _workerStub('PerformanceObserver', ['disconnect', 'observe', 'takeRecords']);
    if (!Object.prototype.hasOwnProperty.call(PerformanceObserver, 'supportedEntryTypes')) {
        Object.defineProperty(PerformanceObserver, 'supportedEntryTypes', {
            value: Object.freeze(['element', 'event', 'first-input', 'largest-contentful-paint', 'layout-shift', 'long-animation-frame', 'longtask', 'mark', 'measure', 'navigation', 'paint', 'resource', 'visibility-state']),
            configurable: true,
        });
    }
    _workerStub('PressureObserver', ['disconnect', 'observe', 'takeRecords', 'unobserve']);
    _workerStub('RTCRtpScriptTransformer', ['generateKeyFrame', 'sendKeyFrameRequest']);
    _workerStub('RTCTransformEvent');
    _workerStub('ReportingObserver', ['disconnect', 'observe', 'takeRecords']);
    // Origin-private file system (OPFS). Turnstile runs a synchronous-access
    // timing probe in a dedicated worker:
    // navigator.storage.getDirectory() -> getFileHandle() ->
    // createSyncAccessHandle() -> write()/flush()/close(). Returning a plain
    // object from getDirectory() leaves that promise chain stuck before its
    // worker can answer, which in turn prevents the next challenge POST.
    const FileSystemHandle = globalThis.FileSystemHandle;
    const FileSystemDirectoryHandle = globalThis.FileSystemDirectoryHandle;
    const FileSystemFileHandle = globalThis.FileSystemFileHandle;
    const _opfsHandleState = new WeakMap();
    const _opfsAccessState = new WeakMap();

    const _opfsDefineMethod = (prototype, name, implementation) => {
        Object.defineProperty(prototype, name, {
            value: implementation,
            configurable: true,
            enumerable: true,
            writable: true,
        });
        if (typeof _maskFunction === 'function') _maskFunction(implementation, name);
    };
    const _opfsDefineGetter = (prototype, name, getter) => {
        Object.defineProperty(prototype, name, {
            get: getter,
            configurable: true,
            enumerable: true,
        });
        if (typeof _maskFunction === 'function') _maskFunction(getter, 'get ' + name);
    };
    const _opfsDirectory = (name = '') => {
        const handle = Object.create(FileSystemDirectoryHandle.prototype);
        _opfsHandleState.set(handle, { kind: 'directory', name, entries: new Map() });
        return handle;
    };
    const _opfsFile = (name) => {
        const handle = Object.create(FileSystemFileHandle.prototype);
        _opfsHandleState.set(handle, { kind: 'file', name, bytes: new Uint8Array(0) });
        return handle;
    };

    Object.setPrototypeOf(FileSystemDirectoryHandle.prototype, FileSystemHandle.prototype);
    Object.setPrototypeOf(FileSystemFileHandle.prototype, FileSystemHandle.prototype);
    _opfsDefineGetter(FileSystemHandle.prototype, 'kind', function kind() {
        return _opfsHandleState.get(this)?.kind || '';
    });
    _opfsDefineGetter(FileSystemHandle.prototype, 'name', function name() {
        return _opfsHandleState.get(this)?.name || '';
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'isSameEntry', function isSameEntry(other) {
        return Promise.resolve(this === other);
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'queryPermission', function queryPermission() {
        return Promise.resolve('granted');
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'remove', function remove() {
        return Promise.resolve();
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'requestPermission', function requestPermission() {
        return Promise.resolve('granted');
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'getDirectoryHandle', function getDirectoryHandle(name, options = {}) {
        const state = _opfsHandleState.get(this);
        const key = String(name);
        let handle = state?.entries.get(key);
        if (!handle && options.create) {
            handle = _opfsDirectory(key);
            state.entries.set(key, handle);
        }
        return handle
            ? Promise.resolve(handle)
            : Promise.reject(new DOMException('A requested file or directory could not be found', 'NotFoundError'));
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'getFileHandle', function getFileHandle(name, options = {}) {
        const state = _opfsHandleState.get(this);
        const key = String(name);
        let handle = state?.entries.get(key);
        if (!handle && options.create) {
            handle = _opfsFile(key);
            state.entries.set(key, handle);
        }
        return handle
            ? Promise.resolve(handle)
            : Promise.reject(new DOMException('A requested file or directory could not be found', 'NotFoundError'));
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'removeEntry', function removeEntry(name) {
        _opfsHandleState.get(this)?.entries.delete(String(name));
        return Promise.resolve();
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'resolve', function resolve(handle) {
        const state = _opfsHandleState.get(this);
        for (const [name, candidate] of state?.entries || []) {
            if (candidate === handle) return Promise.resolve([name]);
        }
        return Promise.resolve(null);
    });
    const _opfsIterator = (kind) => async function* iterator() {
        const entries = _opfsHandleState.get(this)?.entries || new Map();
        if (kind === 'keys') yield* entries.keys();
        else if (kind === 'values') yield* entries.values();
        else yield* entries.entries();
    };
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'entries', _opfsIterator('entries'));
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'keys', _opfsIterator('keys'));
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'values', _opfsIterator('values'));
    Object.defineProperty(FileSystemDirectoryHandle.prototype, Symbol.asyncIterator, {
        value: FileSystemDirectoryHandle.prototype.entries,
        configurable: true,
        writable: true,
    });

    _opfsDefineMethod(FileSystemFileHandle.prototype, 'getFile', function getFile() {
        const state = _opfsHandleState.get(this);
        return Promise.resolve(new File([state?.bytes || new Uint8Array(0)], state?.name || ''));
    });
    _opfsDefineMethod(FileSystemFileHandle.prototype, 'createWritable', function createWritable() {
        return Promise.resolve(Object.create(globalThis.FileSystemWritableFileStream.prototype));
    });
    _opfsDefineMethod(FileSystemFileHandle.prototype, 'move', function move(name) {
        const state = _opfsHandleState.get(this);
        if (state) state.name = String(name);
        return Promise.resolve();
    });
    _opfsDefineMethod(FileSystemFileHandle.prototype, 'createSyncAccessHandle', function createSyncAccessHandle() {
        const access = Object.create(FileSystemSyncAccessHandle.prototype);
        _opfsAccessState.set(access, { file: _opfsHandleState.get(this), closed: false, mode: 'readwrite' });
        return Promise.resolve(access);
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'close', function close() {
        const state = _opfsAccessState.get(this);
        if (state) state.closed = true;
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'flush', function flush() {
        const state = _opfsAccessState.get(this);
        if (!state || state.closed) {
            throw new DOMException('The access handle is closed', 'InvalidStateError');
        }
        // A real synchronous OPFS flush blocks on storage. Chrome 148 on the
        // macOS profile reports roughly 4.6 ms for Turnstile's one-byte probe;
        // an instantaneous no-op is a strong automation fingerprint.
        const started = performance.now();
        while (performance.now() - started < 4.5) {}
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'getSize', function getSize() {
        return _opfsAccessState.get(this)?.file?.bytes.length || 0;
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'read', function read(buffer, options = {}) {
        const state = _opfsAccessState.get(this);
        const target = ArrayBuffer.isView(buffer)
            ? new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
            : new Uint8Array(buffer);
        const at = Math.max(0, Number(options.at) || 0);
        const source = state?.file?.bytes || new Uint8Array(0);
        const length = Math.min(target.length, Math.max(0, source.length - at));
        target.set(source.subarray(at, at + length));
        return length;
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'truncate', function truncate(size) {
        const state = _opfsAccessState.get(this);
        if (!state?.file) return;
        const length = Math.max(0, Number(size) || 0);
        const bytes = new Uint8Array(length);
        bytes.set(state.file.bytes.subarray(0, length));
        state.file.bytes = bytes;
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'write', function write(buffer, options = {}) {
        const state = _opfsAccessState.get(this);
        if (!state?.file) return 0;
        const source = ArrayBuffer.isView(buffer)
            ? new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
            : new Uint8Array(buffer);
        const at = Math.max(0, Number(options.at) || 0);
        const length = Math.max(state.file.bytes.length, at + source.length);
        const bytes = new Uint8Array(length);
        bytes.set(state.file.bytes);
        bytes.set(source, at);
        state.file.bytes = bytes;
        return source.length;
    });
    _opfsDefineGetter(FileSystemSyncAccessHandle.prototype, 'mode', function mode() {
        return _opfsAccessState.get(this)?.mode || 'readwrite';
    });

    const _opfsRoot = _opfsDirectory('');
    const StorageManager = _workerStub('StorageManager', ['estimate', 'getDirectory', 'persisted']);
    // WorkerNavigator is assembled before worker-only interfaces are
    // installed. Upgrade its eagerly-created storage singleton now that the
    // real StorageManager prototype exists.
    Object.setPrototypeOf(globalThis.navigator.storage, StorageManager.prototype);
    StorageManager.prototype.estimate = function estimate() {
        return Promise.resolve({ quota: 10 * 1024 * 1024 * 1024, usage: 0, usageDetails: {} });
    };
    StorageManager.prototype.persisted = function persisted() { return Promise.resolve(false); };
    StorageManager.prototype.getDirectory = function getDirectory() { return Promise.resolve(_opfsRoot); };
    if (typeof _maskAsNative === 'function') {
        _maskAsNative(StorageManager.prototype, 'estimate', 'getDirectory', 'persisted');
    }

    if (typeof globalThis.BroadcastChannel !== 'function') {
        const _broadcasts = new Map();
        globalThis.BroadcastChannel = class BroadcastChannel extends EventTarget {
            constructor(name) {
                super();
                this.name = String(name);
                this.onmessage = null;
                this.onmessageerror = null;
                const peers = _broadcasts.get(this.name) || new Set();
                peers.add(this);
                _broadcasts.set(this.name, peers);
            }
            postMessage(data) {
                for (const peer of _broadcasts.get(this.name) || []) {
                    if (peer === this) continue;
                    setTimeout(() => peer.dispatchEvent(new MessageEvent('message', { data })), 0);
                }
            }
            close() {
                const peers = _broadcasts.get(this.name);
                if (peers) peers.delete(this);
            }
        };
        if (typeof _maskFunction === 'function') _maskFunction(globalThis.BroadcastChannel, 'BroadcastChannel');
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(globalThis.BroadcastChannel.prototype, 'postMessage', 'close');
        }
    }

    // Chrome 148 exposes EventTarget.when() in both Window and worker realms.
    // It resolves with the next matching event and accepts an AbortSignal.
    if (globalThis.EventTarget
        && !Object.prototype.hasOwnProperty.call(EventTarget.prototype, 'when')) {
        Object.defineProperty(EventTarget.prototype, 'when', {
            configurable: true,
            enumerable: true,
            writable: true,
            value: function when(type, options = {}) {
                const target = this;
                return new Promise((resolve, reject) => {
                    const signal = options && options.signal;
                    const done = (event) => {
                        target.removeEventListener(type, done);
                        resolve(event);
                    };
                    if (signal && signal.aborted) {
                        reject(signal.reason || new DOMException('The operation was aborted', 'AbortError'));
                        return;
                    }
                    target.addEventListener(type, done, { once: true });
                    if (signal && typeof signal.addEventListener === 'function') {
                        signal.addEventListener('abort', () => {
                            target.removeEventListener(type, done);
                            reject(signal.reason || new DOMException('The operation was aborted', 'AbortError'));
                        }, { once: true });
                    }
                });
            },
        });
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(EventTarget.prototype, 'when');
        }
    }

    if (typeof globalThis.Worker !== 'function') {
        // Nested workers are exposed by Chromium. Keep the WebIDL surface
        // coherent even though recursive worker spawning is not wired yet.
        globalThis.Worker = class Worker extends EventTarget {
            constructor() {
                super();
                this.onerror = null;
                this.onmessage = null;
                throw new DOMException('Nested worker spawning is unavailable', 'NetworkError');
            }
            postMessage() {}
            terminate() {}
        };
        if (typeof _maskFunction === 'function') _maskFunction(globalThis.Worker, 'Worker');
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(globalThis.Worker.prototype, 'onerror', 'onmessage', 'postMessage', 'terminate');
        }
    }

    if (typeof globalThis.requestAnimationFrame !== 'function') {
        let _workerRafId = 1;
        const _workerRafs = new Map();
        globalThis.requestAnimationFrame = function requestAnimationFrame(callback) {
            const id = _workerRafId++;
            const timer = setTimeout(() => {
                _workerRafs.delete(id);
                callback(performance.now());
            }, 16);
            _workerRafs.set(id, timer);
            return id;
        };
        globalThis.cancelAnimationFrame = function cancelAnimationFrame(id) {
            const timer = _workerRafs.get(Number(id));
            if (timer !== undefined) clearTimeout(timer);
            _workerRafs.delete(Number(id));
        };
    }
    if (typeof globalThis.name !== 'string') globalThis.name = '';
    if (!Object.prototype.hasOwnProperty.call(globalThis, 'onrtctransform')) {
        globalThis.onrtctransform = null;
    }
    for (const name of [
        'webkitRequestFileSystem', 'webkitRequestFileSystemSync',
        'webkitResolveLocalFileSystemSyncURL', 'webkitResolveLocalFileSystemURL',
    ]) {
        if (typeof globalThis[name] !== 'function') {
            globalThis[name] = { [name]: function () {} }[name];
        }
    }

    for (const [name, value] of Object.entries({
        GPUBufferUsage: {
            MAP_READ: 1, MAP_WRITE: 2, COPY_SRC: 4, COPY_DST: 8,
            INDEX: 16, VERTEX: 32, UNIFORM: 64, STORAGE: 128,
            INDIRECT: 256, QUERY_RESOLVE: 512,
        },
        GPUColorWrite: { RED: 1, GREEN: 2, BLUE: 4, ALPHA: 8, ALL: 15 },
        GPUMapMode: { READ: 1, WRITE: 2 },
        GPUShaderStage: { VERTEX: 1, FRAGMENT: 2, COMPUTE: 4 },
        GPUTextureUsage: {
            COPY_SRC: 1, COPY_DST: 2, TEXTURE_BINDING: 4,
            STORAGE_BINDING: 8, RENDER_ATTACHMENT: 16,
            TRANSIENT_ATTACHMENT: 32,
        },
    })) {
        if (typeof globalThis[name] !== 'object') {
            Object.defineProperty(globalThis, name, {
                value: Object.freeze(value),
                writable: false, enumerable: true, configurable: true,
            });
        }
    }

    // Build the real worker global prototype chain. WebIDL global members
    // also exist as own properties on the exotic global object; these
    // prototype descriptors provide the cross-check Chrome exposes.
    try {
        const workerGlobalProto = globalThis.WorkerGlobalScope.prototype;
        const dedicatedProto = globalThis.DedicatedWorkerGlobalScope.prototype;
        if (globalThis.EventTarget && globalThis.EventTarget.prototype) {
            Object.setPrototypeOf(workerGlobalProto, globalThis.EventTarget.prototype);
        }
        const methodNames = [
            'atob', 'btoa', 'clearInterval', 'clearTimeout', 'createImageBitmap',
            'fetch', 'importScripts', 'queueMicrotask', 'reportError',
            'setInterval', 'setTimeout', 'structuredClone',
        ];
        for (const name of methodNames) {
            const implementation = typeof globalThis[name] === 'function'
                ? globalThis[name]
                : { [name]: function () {} }[name];
            Object.defineProperty(workerGlobalProto, name, {
                value: implementation,
                writable: true, enumerable: true, configurable: true,
            });
        }
        const valueNames = [
            'caches', 'crossOriginIsolated', 'crypto', 'fonts', 'indexedDB',
            'isSecureContext', 'location', 'navigator', 'onerror',
            'onlanguagechange', 'onrejectionhandled', 'onunhandledrejection',
            'origin', 'performance', 'scheduler', 'self', 'trustedTypes',
        ];
        const prototypeValues = new Map();
        for (const name of valueNames) {
            if (!Object.prototype.hasOwnProperty.call(globalThis, name)) {
                globalThis[name] = name.startsWith('on') ? null
                    : (name === 'origin' ? String(globalThis.location && globalThis.location.origin || '')
                        : (name === 'self' ? globalThis : {}));
            }
            prototypeValues.set(name, globalThis[name]);
            const descriptor = {
                enumerable: true,
                configurable: true,
                get() { return prototypeValues.get(name); },
            };
            if (name.startsWith('on')
                || name === 'origin'
                || name === 'performance'
                || name === 'scheduler') {
                descriptor.set = function (value) { prototypeValues.set(name, value); };
            }
            Object.defineProperty(workerGlobalProto, name, descriptor);
        }
        Object.defineProperties(dedicatedProto, {
            TEMPORARY: { value: 0, writable: false, enumerable: true, configurable: false },
            PERSISTENT: { value: 1, writable: false, enumerable: true, configurable: false },
        });
        Object.setPrototypeOf(globalThis, dedicatedProto);
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(workerGlobalProto, ...methodNames, ...valueNames);
        }
    } catch (_) {}

    // Diagnostics: snapshot the worker-realm surface right after bootstrap,
    // before the worker script runs. If the worker script dies during its
    // own synchronous init (challenge PoW workers do), this is the record of
    // what the realm looked like when it started.
    // Eval-source tap (single source of truth lives in dom_bootstrap.js as
    // __oxInstallEvalTap; duplicated here because workers are a separate
    // isolate that never loads the document bootstrap). Dedicated workers
    // host the Turnstile challenge VM — its runtime-assembled `new Function`
    // programs are where the 'call' crashes unwrap. Mirror each capture to
    // the owner window; the Worker pump in window_bootstrap.js relays
    // `__oxEvalSrc` payloads into globalThis.__oxParentEvalSrc.
    if (_diagnosticsEnabled) try {
        if (!globalThis.__oxEvalTapReady) {
            globalThis.__oxEvalTapReady = true;
            const log = globalThis.__oxEvalSrcLog
                || (globalThis.__oxEvalSrcLog = []);
            const record = (src) => {
                try {
                    globalThis.__oxTapN = (globalThis.__oxTapN | 0) + 1;
                    if (typeof src !== "string" || src.length < 512) return;
                    if (log.length >= 8) return;
                    let dup = false;
                    for (let i = 0; i < log.length; i++) {
                        if (log[i] && log[i].length === src.length) {
                            dup = true;
                            break;
                        }
                    }
                    if (dup) return;
                    log.push(src);
                    try {
                        self.postMessage({
                            __oxEvalSrc: {
                                href: String(
                                    (self.location && self.location.href) || "",
                                ).slice(0, 80),
                                code: src,
                            },
                        });
                    } catch (_) {}
                } catch (_) {}
            };
            const NativeFunction = globalThis.Function;
            globalThis.Function = new Proxy(NativeFunction, {
                apply(target, thisArg, args) {
                    record(typeof args[0] === "string"
                        ? args[0]
                        : (args[0] != null ? String(args[0]) : ""));
                    return Reflect.apply(target, thisArg, args);
                },
                construct(target, args, newTarget) {
                    record(typeof args[0] === "string"
                        ? args[0]
                        : (args[0] != null ? String(args[0]) : ""));
                    return Reflect.construct(target, args, newTarget);
                },
            });
        }
    } catch (_) {}

    if (_diagnosticsEnabled) try {
        const _dk = [
            'location', 'navigator', 'crypto', 'performance', 'fetch', 'Request',
            'Response', 'Headers', 'URL', 'URLSearchParams', 'Blob', 'FileReader',
            'TextEncoder', 'TextDecoder', 'atob', 'btoa', 'setTimeout', 'setInterval',
            'clearTimeout', 'queueMicrotask', 'structuredClone', 'MessageChannel',
            'WebSocket', 'XMLHttpRequest', 'localStorage', 'indexedDB', 'caches',
            'AbortController', 'Event', 'EventTarget', 'onmessage', 'postMessage',
            'close', 'importScripts', 'Promise', 'Reflect', 'Proxy',
        ];
        const _typeofs = _dk.map(k => k + ':' + typeof self[k]).join(',');
        ops.op_worker_diag_note(JSON.stringify({
            phase: 'bootstrap-end',
            g: _typeofs,
            subtle: typeof (self.crypto && self.crypto.subtle),
            loc: self.location ? String(self.location.href).slice(0, 120) : 'none',
            ua: self.navigator ? String(self.navigator.userAgent).slice(0, 48) : 'none',
        }));
    } catch (e) {
        try { ops.op_worker_diag_note('diag-fail ' + e); } catch (_) {}
    }
})(globalThis);
