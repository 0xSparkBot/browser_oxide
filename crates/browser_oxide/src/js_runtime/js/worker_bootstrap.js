// worker_bootstrap.js — runs inside a dedicated Worker V8 isolate.
//
// Sets up the worker-side surface: `self`, postMessage, onmessage dispatch,
// close, and a navigator stub. A setInterval-driven poll loop drains
// parent→worker messages via op_worker_self_recv and fires onmessage events.

((globalThis) => {
    const ops = Deno.core.ops;
    const _isSharedWorker = !!(ops.op_worker_self_is_shared && ops.op_worker_self_is_shared());
    const _workerName = ops.op_worker_self_name
        ? String(ops.op_worker_self_name())
        : '';
    let _sharedWorkerOnConnect = null;
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

    // The global object doubles as WorkerGlobalScope and the concrete worker
    // global (DedicatedWorkerGlobalScope or SharedWorkerGlobalScope).
    const self = globalThis;
    self.self = self;
    {
        const WorkerGlobalScope = function WorkerGlobalScope() {
            throw new TypeError('Illegal constructor');
        };
        Object.defineProperty(WorkerGlobalScope, Symbol.hasInstance, {
            value(candidate) { return candidate === globalThis; },
            configurable: true,
        });
        Object.defineProperty(WorkerGlobalScope.prototype, Symbol.toStringTag, {
            value: 'WorkerGlobalScope', configurable: true,
        });
        globalThis.WorkerGlobalScope = WorkerGlobalScope;
        if (typeof _maskFunction === 'function') {
            _maskFunction(WorkerGlobalScope, 'WorkerGlobalScope');
        }
        if (_isSharedWorker) {
            const SharedWorkerGlobalScope = function SharedWorkerGlobalScope() {
                throw new TypeError('Illegal constructor');
            };
            SharedWorkerGlobalScope.prototype = Object.create(WorkerGlobalScope.prototype);
            Object.defineProperty(SharedWorkerGlobalScope.prototype, 'constructor', {
                value: SharedWorkerGlobalScope,
                configurable: true,
                writable: true,
            });
            Object.defineProperty(SharedWorkerGlobalScope, Symbol.hasInstance, {
                value(candidate) { return candidate === globalThis; },
                configurable: true,
            });
            Object.defineProperty(SharedWorkerGlobalScope.prototype, Symbol.toStringTag, {
                value: 'SharedWorkerGlobalScope', configurable: true,
            });
            globalThis.SharedWorkerGlobalScope = SharedWorkerGlobalScope;
            if (typeof _maskFunction === 'function') {
                _maskFunction(SharedWorkerGlobalScope, 'SharedWorkerGlobalScope');
            }
        } else {
            const DedicatedWorkerGlobalScope = function DedicatedWorkerGlobalScope() {
                throw new TypeError('Illegal constructor');
            };
            DedicatedWorkerGlobalScope.prototype = Object.create(WorkerGlobalScope.prototype);
            Object.defineProperty(DedicatedWorkerGlobalScope.prototype, 'constructor', {
                value: DedicatedWorkerGlobalScope,
                configurable: true,
                writable: true,
            });
            Object.defineProperty(DedicatedWorkerGlobalScope, Symbol.hasInstance, {
                value(candidate) { return candidate === globalThis; },
                configurable: true,
            });
            Object.defineProperty(DedicatedWorkerGlobalScope.prototype, Symbol.toStringTag, {
                value: 'DedicatedWorkerGlobalScope', configurable: true,
            });
            globalThis.DedicatedWorkerGlobalScope = DedicatedWorkerGlobalScope;
            if (typeof _maskFunction === 'function') {
                _maskFunction(DedicatedWorkerGlobalScope, 'DedicatedWorkerGlobalScope');
            }
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
        const _brandSpec = _browserMajor === "145" ? [
            ["Not:A-Brand", "99", "99.0.0.0"],
            ["Google Chrome", _browserMajor, _browserFull],
            ["Chromium", _browserMajor, _browserFull],
        ] : _browserMajor === "148" ? [
            ["Chromium", _browserMajor, _browserFull],
            ["Not(A:Brand", "24", "24.0.0.0"],
            ["Google Chrome", _browserMajor, _browserFull],
        ] : _browserMajor === "152" ? [
            ["Chromium", _browserMajor, _browserFull],
            ["Not?A_Brand", "24", "24.0.0.0"],
            ["Google Chrome", _browserMajor, _browserFull],
        ] : [
            ["Google Chrome", _browserMajor, _browserFull],
            ["Not.A/Brand", "8", "8.0.0.0"],
            ["Chromium", _browserMajor, _browserFull],
        ];
        const _brands = _brandSpec.map((entry) => ({ brand: entry[0], version: entry[1] }));
        const _fullVersionList = _brandSpec.map((entry) => ({ brand: entry[0], version: entry[2] }));
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
        const _remotePorts = new WeakMap();
        const _transferredPorts = new WeakSet();
        const _remoteActivePorts = new Set();

        const _cloneMessage = (data) => {
            if (typeof globalThis.structuredClone !== 'function') return data;
            return globalThis.structuredClone(data);
        };

        const _dispatchPortEvent = (port, data, ports = []) => {
            if (_closedPorts.get(port)) return;
            try {
                port.dispatchEvent(new MessageEvent('message', {
                    data,
                    ports,
                    bubbles: false,
                    cancelable: false,
                }));
            } catch (_) {}
        };

        const _deliverMessage = (port, data, ports = []) => {
            if (_closedPorts.get(port)) return;
            if (!_enabledPorts.get(port)) {
                const queue = _queuedMessages.get(port) || [];
                queue.push({ data, ports });
                _queuedMessages.set(port, queue);
                return;
            }
            const fire = () => _dispatchPortEvent(port, data, ports);
            // Message ports enqueue a task. A timer preserves that ordering and
            // avoids synchronous re-entry in schedulers built on a channel.
            try { globalThis.setTimeout(fire, 0); } catch (_e) { fire(); }
        };

        const _enablePort = (port) => {
            if (_enabledPorts.get(port)) return;
            _enabledPorts.set(port, true);
            const queue = _queuedMessages.get(port) || [];
            _queuedMessages.set(port, []);
            for (const message of queue) {
                _deliverMessage(port, message.data, message.ports || []);
            }
            if (_remotePorts.has(port)) {
                _remoteActivePorts.add(port);
                // A transferred endpoint may already contain messages that
                // arrived before this worker adopted it. No owner notify was
                // attached at enqueue time, so start/adopt must drain once.
                try { _pumpRemotePorts(); } catch (_) {}
            }
        };

        function _pumpRemotePorts() {
            for (const port of Array.from(_remoteActivePorts)) {
                if (_transferredPorts.has(port)
                    || _closedPorts.get(port)
                    || !_enabledPorts.get(port)) {
                    _remoteActivePorts.delete(port);
                    continue;
                }
                const state = _remotePorts.get(port);
                if (!state || state.pending || !state.generation) continue;
                for (;;) {
                    let raw = '';
                    try {
                        raw = ops.op_message_port_try_recv(state.id, state.generation);
                    } catch (_) {
                        break;
                    }
                    if (!raw) break;
                    let payload;
                    try { payload = JSON.parse(raw); }
                    catch (_) { continue; }
                    const cache = new Map();
                    const data = _browser_oxide && _browser_oxide.deserializeFromWire
                        ? _browser_oxide.deserializeFromWire(payload && payload.data, cache)
                        : payload && payload.data;
                    const ports = _browser_oxide && _browser_oxide.adoptTransferredPorts
                        ? _browser_oxide.adoptTransferredPorts(
                            (payload && payload.ports) || [],
                            cache,
                        )
                        : [];
                    _dispatchPortEvent(port, data, ports);
                }
            }
        }

        class MessagePort extends EventTarget {
            constructor() {
                super();
                this._onmessage = null;
                this.onmessageerror = null;
                _queuedMessages.set(this, []);
                _enabledPorts.set(this, false);
                _closedPorts.set(this, false);
            }
            get onmessage() { return this._onmessage; }
            set onmessage(listener) {
                this._onmessage = typeof listener === 'function' ? listener : null;
                if (this._onmessage) _enablePort(this);
            }
            postMessage(data, transfer) {
                if (_closedPorts.get(this)) return;
                if (_transferredPorts.has(this)) return;
                const remote = _remotePorts.get(this);
                if (remote && !remote.pending && remote.generation
                    && _browser_oxide && _browser_oxide.prepareWireMessage) {
                    const prepared = _browser_oxide.prepareWireMessage(
                        data,
                        transfer,
                        'postMessage',
                        'MessagePort',
                    );
                    const payload = JSON.stringify({
                        data: prepared.data,
                        ports: prepared.ports,
                    });
                    _browser_oxide.commitPreparedTransfers(prepared.transferState);
                    ops.op_message_port_post(remote.id, remote.generation, payload);
                    return;
                }
                const paired = _pairedPorts.get(this);
                if (!paired) return;
                _deliverMessage(
                    paired,
                    transfer === undefined
                        ? _cloneMessage(data)
                        : structuredClone(data, { transfer }),
                );
            }
            start() { _enablePort(this); }
            close() {
                _closedPorts.set(this, true);
                _remoteActivePorts.delete(this);
                const remote = _remotePorts.get(this);
                if (remote && !remote.pending && remote.generation) {
                    try { ops.op_message_port_close(remote.id, remote.generation); } catch (_) {}
                }
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
        Object.defineProperty(MessagePort.prototype.postMessage, 'length', {
            value: 1,
            configurable: true,
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

        function _ensureRemotePair(port) {
            const existing = _remotePorts.get(port);
            if (existing) return existing;
            const paired = _pairedPorts.get(port);
            if (!paired) return null;
            const handles = ops.op_message_port_create_pair();
            if (!Array.isArray(handles) || handles.length !== 2) return null;
            const first = {
                id: handles[0].id,
                generation: handles[0].generation,
                pending: false,
            };
            const second = {
                id: handles[1].id,
                generation: handles[1].generation,
                pending: false,
            };
            _remotePorts.set(port, first);
            _remotePorts.set(paired, second);
            if (_enabledPorts.get(port)) _remoteActivePorts.add(port);
            if (_enabledPorts.get(paired)) _remoteActivePorts.add(paired);
            return first;
        }

        const _messagePortHooks = {
            isMessagePort(value) {
                return !!value && _queuedMessages.has(value);
            },
            prepareTransfer(port) {
                if (!this.isMessagePort(port)
                    || _closedPorts.get(port)
                    || _transferredPorts.has(port)) {
                    return 0;
                }
                const state = _ensureRemotePair(port);
                return state ? state.id : 0;
            },
            commitTransfer(port) {
                const state = _remotePorts.get(port);
                if (!state || state.pending || !state.generation) return false;
                const nextGeneration = ops.op_message_port_transfer_out(
                    state.id,
                    state.generation,
                );
                if (!nextGeneration) return false;
                state.generation = nextGeneration;
                _transferredPorts.add(port);
                _remoteActivePorts.delete(port);
                return true;
            },
            createPlaceholder(endpointId) {
                const port = new MessagePort();
                _remotePorts.set(port, {
                    id: endpointId,
                    generation: 0,
                    pending: true,
                });
                return port;
            },
            finalizePlaceholder(port) {
                const state = _remotePorts.get(port);
                if (!state || !state.pending) return port;
                const generation = ops.op_message_port_adopt(state.id);
                if (!generation) return port;
                state.generation = generation;
                state.pending = false;
                if (_enabledPorts.get(port)) _remoteActivePorts.add(port);
                return port;
            },
            adopt(endpointId) {
                const port = this.createPlaceholder(endpointId);
                return this.finalizePlaceholder(port);
            },
        };
        if (_browser_oxide) {
            Object.defineProperty(_browser_oxide, 'messagePortHooks', {
                value: _messagePortHooks,
                configurable: true,
                enumerable: false,
                writable: false,
            });
        }
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
    if (ops.op_is_secure_context && ops.op_is_secure_context()
        && typeof globalThis.caches === 'undefined') {
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
        const prepared = _browser_oxide && _browser_oxide.prepareWireMessage
            ? _browser_oxide.prepareWireMessage(
                message,
                transfer,
                'postMessage',
                'DedicatedWorkerGlobalScope',
            )
            : { data: message, ports: [], transferState: null };
        if (_browser_oxide && _browser_oxide.commitPreparedTransfers) {
            _browser_oxide.commitPreparedTransfers(prepared.transferState);
        }
        let payload;
        try {
            payload = JSON.stringify({
                data: prepared.data,
                ports: prepared.ports,
            });
        } catch (_e) {
            payload = JSON.stringify({ data: null });
        }
        if (_diagnosticsEnabled) {
            try { ops.op_worker_diag_note("tx len=" + payload.length + " @" + Math.round(performance.now())); } catch (_) {}
        }
        ops.op_worker_self_post(payload);
    };

    // --- close: terminate this worker from inside its own global scope ---
    let _closed = false;
    self.close = function () {
        if (_closed) return;
        _closed = true;
        try { ops.op_worker_self_close(); } catch (_) {}
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
        const portCache = new Map();
        const data = deserializer
            ? deserializer(payload && payload.data, portCache)
            : payload && payload.data;
        const ports = _browser_oxide && _browser_oxide.adoptTransferredPorts
            ? _browser_oxide.adoptTransferredPorts(
                (payload && payload.ports) || [],
                portCache,
            )
            : [];
        if (_isSharedWorker
            && data
            && typeof data === 'object'
            && data.__browser_oxide_shared_connect === true) {
            if (_diagnosticsEnabled) {
                try {
                    ops.op_worker_diag_note(
                        'shared-connect ports=' + ports.length
                        + ' onconnect=' + typeof self.onconnect
                    );
                } catch (_) {}
            }
            const connectEvent = _markTrustedEvent(new MessageEvent('connect', {
                data: undefined,
                origin: '',
                source: null,
                ports,
            }));
            self.dispatchEvent(connectEvent);
            if (_diagnosticsEnabled) {
                try { ops.op_worker_diag_note('shared-connect-dispatched'); } catch (_) {}
            }
            return;
        }
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
            ports,
        })));
    }

    // --- Pump: event-driven parent→worker message delivery ---
    // The runtime bootstrap runs before the user's worker body. Starting the
    // pump here would let an immediately-posted parent message dispatch before
    // `self.onmessage = ...` / addEventListener calls in that body execute,
    // losing a task that Chromium queues until worker-script initialization
    // completes. Rust invokes this private symbol hook immediately after the
    // initial classic/module script evaluation finishes.
    let _workerPumpStarted = false;
    function _startWorkerMessagePump() {
        if (_workerPumpStarted || _closed) return;
        _workerPumpStarted = true;
        (async function _workerPump() {
            while (!_closed) {
                let s;
                try {
                    s = await ops.op_worker_self_await_message();
                } catch (_e) {
                    break;
                }
                if (!s) break; // "" ⇒ terminated
                if (s === '__browser_oxide_message_port_wake__') {
                    _pumpRemotePorts();
                    try {
                        const pumpBroadcasts = _browser_oxide && _browser_oxide._pumpBroadcastChannels;
                        if (typeof pumpBroadcasts === 'function') pumpBroadcasts();
                    } catch (_) {}
                    continue;
                }
                _dispatchWorkerMessage(s);
            }
        })();
    }
    try {
        Object.defineProperty(
            globalThis,
            Symbol.for('__browser_oxide_worker_start_pump__'),
            { value: _startWorkerMessagePump, configurable: true, enumerable: false }
        );
    } catch (_) {}

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

    // WebCrypto is installed for every Worker realm by
    // shared_apis_bootstrap.js before this bootstrap runs. Keep a single
    // implementation shared with Window so HMAC/PBKDF2/CryptoKey semantics
    // cannot drift between realms. The former worker-local fallback only
    // implemented digest and replaced every other SubtleCrypto operation with
    // NotSupportedError stubs; under the current bootstrap ordering it was
    // unreachable, and retaining it created a misleading second source of
    // truth for the Worker WebCrypto surface.

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
    // FileReaderSync is a real worker-only API, not an illegal-constructor
    // surface stub. binary_fetch_webidl_bootstrap keeps normalized Blob bytes
    // in a WeakMap and exposes a short-lived engine-private snapshot closure
    // for structured clone. Capture that closure here before cleanup removes
    // the bridge, so synchronous worker reads never have to re-introduce a
    // page-visible Blob `_data` field.
    const _fileReaderBlobSnapshot = (() => {
        for (const bridge of [globalThis._browser_oxide, globalThis.__browser_oxide]) {
            if (bridge && typeof bridge.blobSnapshotForClone === 'function') {
                return bridge.blobSnapshotForClone;
            }
        }
        return null;
    })();
    function FileReaderSync() {
        if (!new.target) {
            throw new TypeError(
                "Failed to construct 'FileReaderSync': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        }
    }
    const _fileReaderBytes = (self, method, args, blob) => {
        if (!(self instanceof FileReaderSync)) throw new TypeError('Illegal invocation');
        if (args.length < 1) {
            throw new TypeError(
                `Failed to execute '${method}' on 'FileReaderSync': 1 argument required, but only 0 present.`
            );
        }
        const snapshot = _fileReaderBlobSnapshot && _fileReaderBlobSnapshot(blob);
        if (!snapshot || !(snapshot.bytes instanceof Uint8Array)) {
            throw new TypeError(
                `Failed to execute '${method}' on 'FileReaderSync': parameter 1 is not of type 'Blob'.`
            );
        }
        return snapshot;
    };
    const _copyFileReaderBytes = (bytes) => {
        const out = new Uint8Array(bytes.byteLength);
        out.set(bytes);
        return out;
    };
    const _binaryString = (bytes) => {
        let out = '';
        const chunk = 0x8000;
        for (let i = 0; i < bytes.length; i += chunk) {
            out += String.fromCharCode(...bytes.subarray(i, i + chunk));
        }
        return out;
    };
    const _base64 = (bytes) => {
        const binary = _binaryString(bytes);
        if (typeof globalThis.btoa === 'function') return globalThis.btoa(binary);
        const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
        let out = '';
        for (let i = 0; i < binary.length; i += 3) {
            const a = binary.charCodeAt(i);
            const hasB = i + 1 < binary.length;
            const hasC = i + 2 < binary.length;
            const b = hasB ? binary.charCodeAt(i + 1) : 0;
            const c = hasC ? binary.charCodeAt(i + 2) : 0;
            out += alphabet[a >> 2];
            out += alphabet[((a & 3) << 4) | (b >> 4)];
            out += hasB ? alphabet[((b & 15) << 2) | (c >> 6)] : '=';
            out += hasC ? alphabet[c & 63] : '=';
        }
        return out;
    };
    const _fileReaderMethod = (name, value) => {
        Object.defineProperty(FileReaderSync.prototype, name, {
            configurable: true,
            enumerable: true,
            writable: true,
            value,
        });
        if (typeof _maskFunction === 'function') _maskFunction(value, name);
    };
    delete FileReaderSync.prototype.constructor;
    _fileReaderMethod('readAsArrayBuffer', function readAsArrayBuffer(blob) {
        const snapshot = _fileReaderBytes(this, 'readAsArrayBuffer', arguments, blob);
        return _copyFileReaderBytes(snapshot.bytes).buffer;
    });
    _fileReaderMethod('readAsBinaryString', function readAsBinaryString(blob) {
        const snapshot = _fileReaderBytes(this, 'readAsBinaryString', arguments, blob);
        return _binaryString(snapshot.bytes);
    });
    _fileReaderMethod('readAsDataURL', function readAsDataURL(blob) {
        const snapshot = _fileReaderBytes(this, 'readAsDataURL', arguments, blob);
        const type = snapshot.type || 'application/octet-stream';
        return `data:${type};base64,${_base64(snapshot.bytes)}`;
    });
    _fileReaderMethod('readAsText', function readAsText(blob) {
        const snapshot = _fileReaderBytes(this, 'readAsText', arguments, blob);
        const label = arguments.length > 1 && arguments[1] !== undefined
            ? String(arguments[1])
            : 'utf-8';
        return new TextDecoder(label).decode(snapshot.bytes);
    });
    Object.defineProperty(FileReaderSync.prototype, 'constructor', {
        configurable: true,
        enumerable: false,
        writable: true,
        value: FileReaderSync,
    });
    Object.defineProperty(FileReaderSync.prototype, Symbol.toStringTag, {
        value: 'FileReaderSync', configurable: true,
    });
    globalThis.FileReaderSync = FileReaderSync;
    if (typeof _maskFunction === 'function') _maskFunction(FileReaderSync, 'FileReaderSync');
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
    // Origin-private file system (OPFS). The directory tree and file bytes
    // are origin-scoped host state shared with Window and sibling Workers;
    // this realm only owns WebIDL wrappers and SyncAccessHandle lifecycle bits.
    const FileSystemHandle = globalThis.FileSystemHandle;
    const FileSystemDirectoryHandle = globalThis.FileSystemDirectoryHandle;
    const FileSystemFileHandle = globalThis.FileSystemFileHandle;
    const _opfsHandleState = new WeakMap();
    const _opfsAccessState = new WeakMap();
    const _opfsOrigin = () => String(
        (globalThis.location && globalThis.location.origin) || 'null'
    );

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
    const _opfsWrap = (snapshot, origin) => {
        if (!snapshot || snapshot.status !== 1) return null;
        const prototype = snapshot.kind === 'directory'
            ? FileSystemDirectoryHandle.prototype
            : FileSystemFileHandle.prototype;
        const handle = Object.create(prototype);
        _opfsHandleState.set(handle, {
            origin,
            id: snapshot.id,
            parentId: snapshot.parentId,
            kind: snapshot.kind,
            name: snapshot.name,
        });
        return handle;
    };
    const _opfsStat = (handle) => {
        const state = _opfsHandleState.get(handle);
        if (!state) return null;
        const snapshot = ops.op_opfs_stat(state.origin, state.id);
        if (snapshot && snapshot.status === 1) {
            state.parentId = snapshot.parentId;
            state.kind = snapshot.kind;
            state.name = snapshot.name;
            return { state, snapshot };
        }
        return { state, snapshot: null };
    };
    const _opfsResult = (snapshot, origin) => {
        if (snapshot && snapshot.status === 1) return Promise.resolve(_opfsWrap(snapshot, origin));
        if (snapshot && snapshot.status === -1) {
            return Promise.reject(new DOMException(
                'The path exists but is not an entry of the requested type.',
                'TypeMismatchError',
            ));
        }
        return Promise.reject(new DOMException(
            'A requested file or directory could not be found',
            'NotFoundError',
        ));
    };

    Object.setPrototypeOf(FileSystemDirectoryHandle.prototype, FileSystemHandle.prototype);
    Object.setPrototypeOf(FileSystemFileHandle.prototype, FileSystemHandle.prototype);
    _opfsDefineGetter(FileSystemHandle.prototype, 'kind', function kind() {
        const current = _opfsStat(this);
        return current?.snapshot?.kind || current?.state?.kind || '';
    });
    _opfsDefineGetter(FileSystemHandle.prototype, 'name', function name() {
        const current = _opfsStat(this);
        return current?.snapshot?.name || current?.state?.name || '';
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'isSameEntry', function isSameEntry(other) {
        const a = _opfsHandleState.get(this);
        const b = _opfsHandleState.get(other);
        return Promise.resolve(!!a && !!b && a.origin === b.origin && a.id === b.id);
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'queryPermission', function queryPermission() {
        return Promise.resolve('granted');
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'remove', function remove(options = {}) {
        const current = _opfsStat(this);
        if (!current?.snapshot || current.snapshot.parentId === 0) return Promise.resolve();
        const status = ops.op_opfs_remove_child(
            current.state.origin,
            current.snapshot.parentId,
            current.snapshot.name,
            !!options.recursive,
        );
        if (status === -1) {
            return Promise.reject(new DOMException('The directory is not empty.', 'InvalidModificationError'));
        }
        return Promise.resolve();
    });
    _opfsDefineMethod(FileSystemHandle.prototype, 'requestPermission', function requestPermission() {
        return Promise.resolve('granted');
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'getDirectoryHandle', function getDirectoryHandle(name, options = {}) {
        const state = _opfsHandleState.get(this);
        if (!state) return Promise.reject(new TypeError('Illegal invocation'));
        return _opfsResult(
            ops.op_opfs_get_child(state.origin, state.id, String(name), 'directory', !!options.create),
            state.origin,
        );
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'getFileHandle', function getFileHandle(name, options = {}) {
        const state = _opfsHandleState.get(this);
        if (!state) return Promise.reject(new TypeError('Illegal invocation'));
        return _opfsResult(
            ops.op_opfs_get_child(state.origin, state.id, String(name), 'file', !!options.create),
            state.origin,
        );
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'removeEntry', function removeEntry(name, options = {}) {
        const state = _opfsHandleState.get(this);
        if (!state) return Promise.reject(new TypeError('Illegal invocation'));
        const status = ops.op_opfs_remove_child(state.origin, state.id, String(name), !!options.recursive);
        if (status === 0) {
            return Promise.reject(new DOMException(
                'A requested file or directory could not be found',
                'NotFoundError',
            ));
        }
        if (status === -1) {
            return Promise.reject(new DOMException('The directory is not empty.', 'InvalidModificationError'));
        }
        return Promise.resolve();
    });
    _opfsDefineMethod(FileSystemDirectoryHandle.prototype, 'resolve', function resolve(handle) {
        const state = _opfsHandleState.get(this);
        const target = _opfsHandleState.get(handle);
        if (!state || !target || state.origin !== target.origin) return Promise.resolve(null);
        return Promise.resolve(ops.op_opfs_resolve(state.origin, state.id, target.id));
    });
    const _opfsIterator = (kind) => async function* iterator() {
        const state = _opfsHandleState.get(this);
        if (!state) return;
        for (const snapshot of ops.op_opfs_list(state.origin, state.id)) {
            const handle = _opfsWrap(snapshot, state.origin);
            if (kind === 'keys') yield snapshot.name;
            else if (kind === 'values') yield handle;
            else yield [snapshot.name, handle];
        }
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
        const current = _opfsStat(this);
        if (!current?.snapshot) {
            return Promise.reject(new DOMException('The file no longer exists.', 'NotFoundError'));
        }
        const bytes = ops.op_opfs_read(current.state.origin, current.state.id);
        return Promise.resolve(new File([bytes], current.snapshot.name));
    });
    _opfsDefineMethod(FileSystemFileHandle.prototype, 'createWritable', function createWritable() {
        return Promise.resolve(Object.create(globalThis.FileSystemWritableFileStream.prototype));
    });
    _opfsDefineMethod(FileSystemFileHandle.prototype, 'move', function move(name) {
        const state = _opfsHandleState.get(this);
        if (!state || !ops.op_opfs_move(state.origin, state.id, String(name))) {
            return Promise.reject(new DOMException('The entry could not be moved.', 'InvalidModificationError'));
        }
        return Promise.resolve();
    });
    _opfsDefineMethod(FileSystemFileHandle.prototype, 'createSyncAccessHandle', function createSyncAccessHandle() {
        const state = _opfsHandleState.get(this);
        if (!state) return Promise.reject(new TypeError('Illegal invocation'));
        const access = Object.create(FileSystemSyncAccessHandle.prototype);
        _opfsAccessState.set(access, {
            origin: state.origin,
            id: state.id,
            closed: false,
            mode: 'readwrite',
        });
        return Promise.resolve(access);
    });
    const _opfsOpenAccess = (handle) => {
        const state = _opfsAccessState.get(handle);
        if (!state || state.closed) {
            throw new DOMException('The access handle is closed', 'InvalidStateError');
        }
        return state;
    };
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'close', function close() {
        const state = _opfsAccessState.get(this);
        if (state) state.closed = true;
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'flush', function flush() {
        _opfsOpenAccess(this);
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'getSize', function getSize() {
        const state = _opfsOpenAccess(this);
        return ops.op_opfs_size(state.origin, state.id);
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'read', function read(buffer, options = {}) {
        const state = _opfsOpenAccess(this);
        const target = ArrayBuffer.isView(buffer)
            ? new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
            : new Uint8Array(buffer);
        const at = Math.max(0, Math.trunc(Number(options.at) || 0));
        const source = ops.op_opfs_read(state.origin, state.id);
        const length = Math.min(target.length, Math.max(0, source.length - at));
        target.set(source.subarray(at, at + length));
        return length;
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'truncate', function truncate(size) {
        const state = _opfsOpenAccess(this);
        ops.op_opfs_truncate(
            state.origin,
            state.id,
            Math.max(0, Math.trunc(Number(size) || 0)),
        );
    });
    _opfsDefineMethod(FileSystemSyncAccessHandle.prototype, 'write', function write(buffer, options = {}) {
        const state = _opfsOpenAccess(this);
        const source = ArrayBuffer.isView(buffer)
            ? new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
            : new Uint8Array(buffer);
        const at = Math.max(0, Math.trunc(Number(options.at) || 0));
        return ops.op_opfs_write(state.origin, state.id, at, source);
    });
    _opfsDefineGetter(FileSystemSyncAccessHandle.prototype, 'mode', function mode() {
        return _opfsAccessState.get(this)?.mode || 'readwrite';
    });

    const StorageManager = _workerStub('StorageManager', ['estimate', 'getDirectory', 'persisted']);
    // WorkerNavigator is assembled before worker-only interfaces are
    // installed. Upgrade its eagerly-created storage singleton now that the
    // real StorageManager prototype exists.
    Object.setPrototypeOf(globalThis.navigator.storage, StorageManager.prototype);
    StorageManager.prototype.estimate = function estimate() {
        return Promise.resolve({ quota: 10 * 1024 * 1024 * 1024, usage: 0, usageDetails: {} });
    };
    StorageManager.prototype.persisted = function persisted() { return Promise.resolve(false); };
    StorageManager.prototype.getDirectory = function getDirectory() {
        const allowed = !ops.op_worker_storage_directory_allowed
            || ops.op_worker_storage_directory_allowed();
        if (!allowed) {
            return Promise.reject(new DOMException(
                'Storage directory access is denied.',
                'SecurityError',
            ));
        }
        const origin = _opfsOrigin();
        return Promise.resolve(_opfsWrap(ops.op_opfs_root(origin), origin));
    };
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
        // Chromium exposes nested DedicatedWorkers. The worker runtime already
        // installs the same Rust worker ops as the owning Window runtime, so a
        // nested worker can reuse the process-global worker registry and wire
        // format instead of inventing a second transport.
        const _nestedWorkerState = new WeakMap();
        const _refNestedWorkerOp = Deno.core.refOpPromise;
        const _unrefNestedWorkerOp = Deno.core.unrefOpPromise;

        function _nestedStateFor(worker) {
            const state = _nestedWorkerState.get(worker);
            if (!state) throw new TypeError('Illegal invocation');
            return state;
        }

        function _resolveNestedWorker(rawUrl) {
            let resolved;
            try {
                resolved = new URL(
                    String(rawUrl),
                    self.location && self.location.href || undefined,
                ).href;
            } catch (_) {
                resolved = String(rawUrl);
            }
            if (resolved.startsWith('blob:')) {
                try {
                    return { url: resolved, source: ops.op_blob_fetch_text(resolved) || '' };
                } catch (_) {
                    return { url: resolved, source: '' };
                }
            }
            if (resolved.startsWith('data:')) {
                const comma = resolved.indexOf(',');
                if (comma < 0) return { url: resolved, source: '' };
                try {
                    const meta = resolved.slice(5, comma);
                    const body = resolved.slice(comma + 1);
                    return {
                        url: resolved,
                        source: meta.endsWith(';base64')
                            ? atob(decodeURIComponent(body))
                            : decodeURIComponent(body),
                    };
                } catch (_) {
                    return { url: resolved, source: '' };
                }
            }
            try {
                return { url: resolved, source: ops.op_worker_sync_fetch(resolved) || '' };
            } catch (_) {
                return { url: resolved, source: '' };
            }
        }

        function _deliverNestedWorkerRaw(worker, state, raw) {
            if (!raw || !state.id) return;
            if (raw === '__browser_oxide_worker_ready__') {
                state.initializing = false;
                if (state.pendingReceive && state.refNextReceive) {
                    state.refNextReceive = false;
                } else if (_unrefNestedWorkerOp && state.pendingReceive) {
                    try { _unrefNestedWorkerOp(state.pendingReceive); } catch (_) {}
                }
                return;
            }
            let payload;
            try { payload = JSON.parse(raw); }
            catch (_) { return; }
            const deserializer = _browser_oxide && _browser_oxide.deserializeFromWire;
            const portCache = new Map();
            const data = deserializer
                ? deserializer(payload && payload.data, portCache)
                : payload && payload.data;
            const ports = _browser_oxide && _browser_oxide.adoptTransferredPorts
                ? _browser_oxide.adoptTransferredPorts(
                    (payload && payload.ports) || [],
                    portCache,
                )
                : [];
            worker.dispatchEvent(_markTrustedEvent(new MessageEvent('message', {
                data,
                origin: '',
                lastEventId: '',
                source: null,
                ports,
            })));
        }

        globalThis.Worker = class Worker extends EventTarget {
            constructor(scriptURL, options) {
                super();
                const type = options && options.type !== undefined
                    ? String(options.type)
                    : 'classic';
                if (type !== 'classic' && type !== 'module') {
                    throw new TypeError(
                        "Failed to construct 'Worker': Failed to read the 'type' property from 'WorkerOptions': The provided value '"
                        + type + "' is not a valid enum value of type WorkerType."
                    );
                }
                const name = options && options.name !== undefined
                    ? String(options.name)
                    : '';
                const resolved = _resolveNestedWorker(scriptURL);
                const state = {
                    id: 0,
                    initializing: true,
                    pendingReceive: null,
                    refNextReceive: false,
                    handlers: { message: null, error: null },
                };
                _nestedWorkerState.set(this, state);

                if (!resolved.source) {
                    Promise.resolve().then(() => {
                        this.dispatchEvent(_markTrustedEvent(new ErrorEvent('error', {
                            message: 'Worker script could not be resolved: ' + resolved.url,
                            filename: resolved.url,
                            lineno: 0,
                            colno: 0,
                        })));
                    });
                    return;
                }

                const storageAllowed = !ops.op_worker_storage_directory_allowed
                    || ops.op_worker_storage_directory_allowed();
                state.id = ops.op_worker_spawn(
                    resolved.source,
                    name,
                    type === 'module',
                    resolved.url,
                    storageAllowed,
                );
                if (state.id <= 0) {
                    state.id = 0;
                    return;
                }

                const _drainOnce = () => {
                    if (!state.id) return;
                    const pending = ops.op_worker_await_message(state.id);
                    state.pendingReceive = pending;
                    const keepRefed = state.initializing || state.refNextReceive;
                    if (state.refNextReceive) state.refNextReceive = false;
                    if (_unrefNestedWorkerOp && !keepRefed) {
                        try { _unrefNestedWorkerOp(pending); } catch (_) {}
                    }
                    pending.then((raw) => {
                        if (state.pendingReceive === pending) state.pendingReceive = null;
                        if (!raw || !state.id) return;
                        _deliverNestedWorkerRaw(this, state, raw);
                        _drainOnce();
                    }).catch(() => {});
                };
                _drainOnce();
            }

            postMessage(message, transfer) {
                const state = _nestedStateFor(this);
                if (!state.id) return;
                const prepared = _browser_oxide && _browser_oxide.prepareWireMessage
                    ? _browser_oxide.prepareWireMessage(
                        message,
                        transfer,
                        'postMessage',
                        'Worker',
                    )
                    : { data: message, ports: [], transferState: null };
                if (_browser_oxide && _browser_oxide.commitPreparedTransfers) {
                    _browser_oxide.commitPreparedTransfers(prepared.transferState);
                }
                let payload;
                try {
                    payload = JSON.stringify({
                        data: prepared.data,
                        ports: prepared.ports,
                    });
                } catch (_) {
                    payload = JSON.stringify({ data: null });
                }
                ops.op_worker_post_to_worker(state.id, payload);
                if (_refNestedWorkerOp && state.pendingReceive) {
                    try { _refNestedWorkerOp(state.pendingReceive); } catch (_) {}
                    if (state.initializing) state.refNextReceive = true;
                } else {
                    state.refNextReceive = true;
                }
            }

            terminate() {
                const state = _nestedStateFor(this);
                if (!state.id) return;
                const id = state.id;
                state.id = 0;
                try { ops.op_worker_terminate(id); } catch (_) {}
            }
        };

        const _nestedPostMessage = globalThis.Worker.prototype.postMessage;
        const _nestedTerminate = globalThis.Worker.prototype.terminate;
        for (const name of Object.getOwnPropertyNames(globalThis.Worker.prototype)) {
            delete globalThis.Worker.prototype[name];
        }
        const _defineNestedHandler = (type) => {
            Object.defineProperty(globalThis.Worker.prototype, 'on' + type, {
                configurable: true,
                enumerable: true,
                get() { return _nestedStateFor(this).handlers[type]; },
                set(value) {
                    _nestedStateFor(this).handlers[type] =
                        typeof value === 'function' ? value : null;
                },
            });
        };
        _defineNestedHandler('message');
        Object.defineProperty(globalThis.Worker.prototype, 'postMessage', {
            value: _nestedPostMessage,
            writable: true,
            enumerable: true,
            configurable: true,
        });
        Object.defineProperty(globalThis.Worker.prototype, 'terminate', {
            value: _nestedTerminate,
            writable: true,
            enumerable: true,
            configurable: true,
        });
        Object.defineProperty(globalThis.Worker.prototype, 'constructor', {
            value: globalThis.Worker,
            writable: true,
            enumerable: false,
            configurable: true,
        });
        _defineNestedHandler('error');
        Object.defineProperty(globalThis.Worker.prototype, Symbol.toStringTag, {
            value: 'Worker',
            configurable: true,
        });
        if (typeof _maskFunction === 'function') _maskFunction(globalThis.Worker, 'Worker');
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(globalThis.Worker.prototype,
                'onmessage', 'postMessage', 'terminate', 'onerror');
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
    // Worker globals expose `name` as an own accessor even though the IDL
    // attribute is readonly. Chromium's global-exotic reflection includes a
    // setter; assignment is therefore accepted but must not change the
    // constructor-supplied worker name. Shared workers likewise expose
    // `onconnect` as a global own accessor, not as a concrete-prototype member.
    try {
        Object.defineProperty(globalThis, 'name', {
            enumerable: true,
            configurable: true,
            get() { return _workerName; },
            set(_value) {},
        });
        if (_isSharedWorker) {
            Object.defineProperty(globalThis, 'onconnect', {
                enumerable: true,
                configurable: true,
                get() { return _sharedWorkerOnConnect; },
                set(value) {
                    _sharedWorkerOnConnect = typeof value === 'function' ? value : null;
                },
            });
        }
    } catch (_) {}
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
        const concreteProto = _isSharedWorker
            ? globalThis.SharedWorkerGlobalScope.prototype
            : globalThis.DedicatedWorkerGlobalScope.prototype;
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
                    : (name === 'crossOriginIsolated'
                        ? !!(ops.op_cross_origin_isolated && ops.op_cross_origin_isolated())
                        : (name === 'isSecureContext'
                            ? !!(ops.op_is_secure_context && ops.op_is_secure_context())
                            : (name === 'origin'
                                ? String(globalThis.location && globalThis.location.origin || '')
                                : (name === 'self' ? globalThis : {}))));
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
        // Class construction installs constructor/@@toStringTag before this
        // late WebIDL normalization. Blink exposes the concrete worker scope
        // in the order constants, constructor, @@toStringTag, so temporarily
        // remove the configurable entries and reinsert them after the two
        // non-configurable constants.
        const concreteCtor = Object.getOwnPropertyDescriptor(concreteProto, 'constructor');
        const concreteTag = Object.getOwnPropertyDescriptor(concreteProto, Symbol.toStringTag);
        try { delete concreteProto.constructor; } catch (_) {}
        try { delete concreteProto[Symbol.toStringTag]; } catch (_) {}
        Object.defineProperties(concreteProto, {
            TEMPORARY: { value: 0, writable: false, enumerable: true, configurable: false },
            PERSISTENT: { value: 1, writable: false, enumerable: true, configurable: false },
        });
        if (concreteCtor) {
            try { Object.defineProperty(concreteProto, 'constructor', concreteCtor); } catch (_) {}
        }
        try {
            Object.defineProperty(concreteProto, Symbol.toStringTag, concreteTag || {
                value: _isSharedWorker ? 'SharedWorkerGlobalScope' : 'DedicatedWorkerGlobalScope',
                configurable: true,
            });
        } catch (_) {}
        Object.setPrototypeOf(globalThis, concreteProto);
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
            const FunctionProxy = new Proxy(NativeFunction, {
                apply(target, thisArg, args) {
                    const body = args.length ? args[args.length - 1] : "";
                    record(typeof body === "string"
                        ? body
                        : (body != null ? String(body) : ""));
                    return Reflect.apply(target, thisArg, args);
                },
                construct(target, args, newTarget) {
                    const body = args.length ? args[args.length - 1] : "";
                    record(typeof body === "string"
                        ? body
                        : (body != null ? String(body) : ""));
                    return Reflect.construct(target, args, newTarget);
                },
            });
            globalThis.Function = FunctionProxy;
            Object.defineProperty(NativeFunction.prototype, "constructor", {
                value: FunctionProxy,
                writable: true,
                configurable: true,
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
