// worker_bootstrap.js — runs inside a dedicated Worker V8 isolate.
//
// Sets up the worker-side surface: `self`, postMessage, onmessage dispatch,
// close, and a navigator stub. A setInterval-driven poll loop drains
// parent→worker messages via op_worker_self_recv and fires onmessage events.

((globalThis) => {
    const ops = Deno.core.ops;
    const _browser_oxide = globalThis.__browser_oxide;

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
                self.location = Object.create(null);
                self.location.href = _u.href;
                self.location.origin = _u.origin;
                self.location.protocol = _u.protocol;
                self.location.host = _u.host;
                self.location.hostname = _u.hostname;
                self.location.port = _u.port;
                self.location.pathname = _u.pathname;
                self.location.search = _u.search;
                self.location.hash = _u.hash;
                self.location.toString = function () { return _u.href; };
                Object.defineProperty(self.location, Symbol.toStringTag, {
                    value: 'WorkerLocation', configurable: true,
                });
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

    // --- WorkerNavigator (matches StealthProfile) ---
    if (!self.navigator) {
        // navigator.userAgentData — must be present in Worker realm AND
        // return values consistent with the main thread. Some scripts
        // spawn a Worker that reads `navigator.userAgentData?.mobile`.
        // Main returns false, worker previously returned "NA" — a
        // cross-realm contradiction. Now both return false.
        const _osName = _p("os_name", "Windows");
        const _browserMajor = _p("browser_version", "147.0.7727.117").split(".")[0];
        const _browserFull = _p("browser_version", "147.0.7727.117");
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
                        default: /* ignore unknown hints — Chrome silently drops */ break;
                    }
                }
                return Promise.resolve(out);
            }
            toJSON() { return { brands: _brands.slice(), mobile: false, platform: _osName }; }
        }
        Object.defineProperty(WorkerNavigatorUAData.prototype, Symbol.toStringTag, {
            value: "NavigatorUAData", configurable: true,
        });

        const workerNavigator = {
            userAgent: _p("user_agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36"),
            appVersion: _p("app_version", "5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36"),
            language: _p("language", "en-US"),
            languages: _pJson("languages", ["en-US", "en"]),
            platform: _p("platform", "Win32"),
            onLine: true,
            cookieEnabled: true,
            hardwareConcurrency: _pInt("hardware_concurrency", 8),
            deviceMemory: _pInt("device_memory", 8),
            appName: "Netscape",
            product: "Gecko",
            productSub: _p("product_sub", "20030107"),
            vendor: _p("vendor", "Google Inc."),
            vendorSub: _p("vendor_sub", ""),
            doNotTrack: null,
            pdfViewerEnabled: _p("pdf_viewer_enabled", "true") === "true",
            webdriver: false,
            userAgentData: new WorkerNavigatorUAData(),
        };
        if (_isFirefox()) {
            // Gecko: no userAgentData/deviceMemory (Chrome-only); vendor is "";
            // productSub is "20100101"; oscpu + buildID are Gecko-only.
            delete workerNavigator.userAgentData;
            delete workerNavigator.deviceMemory;
            workerNavigator.vendor = "";
            workerNavigator.productSub = "20100101";
            const _ffUa = _p("user_agent", "");
            const _ffM = _ffUa.match(/\(([^)]*)\)/);
            workerNavigator.oscpu = _ffM
                ? _ffM[1].replace(/;?\s*rv:[0-9.]+\s*/, "").replace(/^Macintosh;\s*/, "").trim()
                : "";
            workerNavigator.buildID = "20181001000000";
        }
        Object.defineProperty(workerNavigator, Symbol.toStringTag, { value: "WorkerNavigator", configurable: true });
        self.navigator = workerNavigator;
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
        try { ops.op_worker_diag_note("tx len=" + payload.length + " @" + Math.round(performance.now())); } catch (_) {}
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
            try { ops.op_worker_diag_note("rx-parse-fail len=" + s.length); } catch (_) {}
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
        try { ops.op_worker_diag_note("rx " + shape + " @" + Math.round(performance.now())); } catch (_) {}
        self.dispatchEvent(new MessageEvent("message", { data }));
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
    try {
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

    try {
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
