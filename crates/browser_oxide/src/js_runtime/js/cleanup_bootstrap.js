((globalThis) => {
    const ops = Deno && Deno.core && Deno.core.ops;
    // -- Per-page secure-context gating (Phase 7) --------------------
    // The V8 snapshot bootstraps with is_secure_context=true so all
    // [SecureContext]-only Web Platform APIs are baked in. On insecure
    // pages (data:/http:/about:blank) we strip them here to match real
    // Chrome.
    try {
        const _ops = Deno && Deno.core && Deno.core.ops;
        const _isSecure = _ops && _ops.op_is_secure_context && _ops.op_is_secure_context();
        if (!_isSecure) {
            // Methods + globals registered as values in the snapshot.
            // Navigator getters (mediaDevices, clipboard, ...) gate
            // themselves lazily so they don't need stripping.
            try { delete globalThis.Navigator.prototype.getBattery; } catch (_e) {}
            for (const k of ['caches', 'cookieStore', 'IdleDetector', 'EyeDropper', 'WebTransport']) {
                try { delete globalThis[k]; } catch (_e) {}
            }
            // Phase 7 — also strip the constructor *interfaces* for the
            // [SecureContext] APIs. Real Chrome 147 hides these from
            // `Object.getOwnPropertyNames(window)` on insecure pages.
            // Some scripts hash the global namespace.
            // Also: ApplePaySession, SharedArrayBuffer, webkitAudioContext,
            // DedicatedWorkerGlobalScope, WorkerGlobalScope, CSSPseudoElement
            // are absent from Chrome 147's globalThis on insecure pages —
            // verified against a real browser.
            for (const k of [
                "SharedArrayBuffer", "webkitAudioContext",
                "DedicatedWorkerGlobalScope", "WorkerGlobalScope",
                "CSSPseudoElement",
                "ApplePaySession", "AuthenticatorAssertionResponse",
                "AuthenticatorAttestationResponse", "AuthenticatorResponse",
                "BatteryManager", "Bluetooth", "CacheStorage", "CookieStore",
                "Credential", "CredentialsContainer", "DevicePosture",
                "FederatedCredential", "FileSystemDirectoryHandle",
                "FileSystemFileHandle", "FileSystemHandle",
                "FileSystemWritableFileStream", "IdentityCredential",
                "IdentityProvider", "Keyboard", "KeyboardLayoutMap",
                "MediaDevices", "PasswordCredential", "PaymentRequest",
                "Presentation", "PresentationConnection",
                "PublicKeyCredential", "ServiceWorker",
                "ServiceWorkerContainer", "StorageManager", "SubtleCrypto",
                "VirtualKeyboard", "XRSession", "XRSystem",
                // Generic Sensor API — also [SecureContext]
                "Sensor", "Accelerometer", "AbsoluteOrientationSensor",
                "GravitySensor", "Gyroscope", "LinearAccelerationSensor",
                "Magnetometer", "OrientationSensor",
                "RelativeOrientationSensor",
            ]) {
                try { delete globalThis[k]; } catch (_e) {}
            }
            // crypto.subtle + crypto.randomUUID are [SecureContext]. They
            // come from deno_core's crypto extension and are non-configurable
            // own properties. `delete` fails — replace `globalThis.crypto`
            // with a Proxy that hides those two keys.
            if (globalThis.crypto) {
                const _origCrypto = globalThis.crypto;
                const _maskedCrypto = new Proxy(_origCrypto, {
                    get(target, prop, receiver) {
                        if (prop === 'subtle' || prop === 'randomUUID') return undefined;
                        const v = Reflect.get(target, prop, receiver);
                        return typeof v === 'function' ? v.bind(target) : v;
                    },
                    has(target, prop) {
                        if (prop === 'subtle' || prop === 'randomUUID') return false;
                        return Reflect.has(target, prop);
                    },
                    ownKeys(target) {
                        return Reflect.ownKeys(target).filter(
                            (k) => k !== 'subtle' && k !== 'randomUUID',
                        );
                    },
                    getOwnPropertyDescriptor(target, prop) {
                        if (prop === 'subtle' || prop === 'randomUUID') return undefined;
                        return Reflect.getOwnPropertyDescriptor(target, prop);
                    },
                });
                try {
                    Object.defineProperty(globalThis, 'crypto', {
                        value: _maskedCrypto, configurable: true, enumerable: true, writable: true,
                    });
                } catch (_e) {}
            }
        }
    } catch (_e) { /* secure-context cleanup is best-effort */ }

    // -- Profile-conditional installs --------------------------------
    // These run AFTER the V8 startup snapshot is restored, so the
    // stealth profile is loaded and op-based reads return real values.
    // (Snapshot-time bootstraps see profile=None and would mis-gate.)
    try {
        const _hasProfile = ops && ops.op_has_stealth_profile && ops.op_has_stealth_profile();
        const _osName = (_hasProfile && ops.op_get_profile_value)
            ? (ops.op_get_profile_value("os_name") || "Linux")
            : "Linux";
        const _browserName = (_hasProfile && ops.op_get_profile_value)
            ? (ops.op_get_profile_value("browser_name") || "Chrome")
            : "Chrome";

        // ApplePaySession — present only on macOS Chrome AND only on
        // secure contexts (Apple Pay requires https). A missing constructor
        // on a macOS UA is a strong inconsistency versus a real browser.
        // Constructor + statics shaped to match
        // Chrome 147's ApplePaySession surface.
        const _ops2 = Deno && Deno.core && Deno.core.ops;
        const _isSecureForAP = _ops2 && _ops2.op_is_secure_context && _ops2.op_is_secure_context();
        if (_browserName === "Safari" && _isSecureForAP && typeof globalThis.ApplePaySession === "undefined") {
            const _APP = function ApplePaySession(_version, _paymentRequest) {
                this.onvalidatemerchant = null;
                this.onpaymentauthorized = null;
                this.onpaymentmethodselected = null;
                this.onshippingcontactselected = null;
                this.onshippingmethodselected = null;
                this.oncouponcodechanged = null;
                this.oncancel = null;
            };
            _APP.prototype = {
                begin() {},
                abort() {},
                completeMerchantValidation() {},
                completePayment() {},
                completePaymentMethodSelection() {},
                completeShippingContactSelection() {},
                completeShippingMethodSelection() {},
                completeCouponCodeChange() {},
                addEventListener() {},
                removeEventListener() {},
            };
            _APP.STATUS_SUCCESS = 0;
            _APP.STATUS_FAILURE = 1;
            _APP.STATUS_INVALID_BILLING_POSTAL_ADDRESS = 2;
            _APP.STATUS_INVALID_SHIPPING_POSTAL_ADDRESS = 3;
            _APP.STATUS_INVALID_SHIPPING_CONTACT = 4;
            _APP.STATUS_PIN_REQUIRED = 5;
            _APP.STATUS_PIN_INCORRECT = 6;
            _APP.STATUS_PIN_LOCKOUT = 7;
            _APP.canMakePayments = function canMakePayments() { return true; };
            _APP.canMakePaymentsWithActiveCard = function canMakePaymentsWithActiveCard(_id) { return Promise.resolve(false); };
            _APP.openPaymentSetup = function openPaymentSetup(_id) { return Promise.resolve(false); };
            _APP.supportsVersion = function supportsVersion(version) { return version >= 1 && version <= 14; };
            Object.defineProperty(globalThis, 'ApplePaySession', {
                value: _APP,
                configurable: true,
                writable: true,
            });
        }
        if (_browserName !== "Safari") {
            try { delete globalThis.ApplePaySession; } catch (_e) {}
        }

        // Window and worker Web IDL namespaces are disjoint. Chromium does
        // not expose worker-global constructors in a document realm; sensor
        // and legacy aliases below are likewise absent from current desktop
        // Chrome. Their presence is more distinctive than a missing optional
        // API and is directly visible to global namespace fingerprinting.
        if (_browserName === "Chrome" && typeof globalThis.document !== 'undefined') {
            for (const k of [
                'WorkerGlobalScope', 'DedicatedWorkerGlobalScope',
                'CSSPseudoElement',
                'Magnetometer', 'SpeechRecognitionAlternative',
                'USBIsochronousOutPacket', 'webkitAudioContext',
                'defaultStatus',
            ]) {
                try { delete globalThis[k]; } catch (_e) {}
            }

            // Late-bound Window aliases/functions. interfaces_bootstrap runs
            // before timers, URL and the Window API implementations, so the
            // old eager aliases were permanently initialized to `undefined`.
            // Install them here, after every bootstrap, and use method syntax
            // so native Window operations remain non-constructable.
            try {
                const _gpuConstants = {
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
                };
                for (const [name, value] of Object.entries(_gpuConstants)) {
                    if (typeof globalThis[name] !== 'object') {
                        Object.defineProperty(globalThis, name, {
                            value: Object.freeze(value),
                            writable: false, enumerable: true, configurable: true,
                        });
                    }
                }
                const _lateWindowMethods = {
                    blur() {},
                    focus() {},
                    moveBy() {},
                    moveTo() {},
                    resizeBy() {},
                    resizeTo() {},
                    captureEvents() {},
                    releaseEvents() {},
                    find() { return false; },
                    fetchLater(input, init) {
                        return globalThis.FetchLaterResult
                            ? Object.create(globalThis.FetchLaterResult.prototype)
                            : { activated: false, input, init };
                    },
                    queryLocalFonts() { return Promise.resolve([]); },
                    showDirectoryPicker() { return Promise.reject(new DOMException('The request is not allowed by the user agent or the platform in the current context.', 'SecurityError')); },
                    showOpenFilePicker() { return Promise.reject(new DOMException('The request is not allowed by the user agent or the platform in the current context.', 'SecurityError')); },
                    showSaveFilePicker() { return Promise.reject(new DOMException('The request is not allowed by the user agent or the platform in the current context.', 'SecurityError')); },
                    getScreenDetails() { return Promise.resolve({ screens: [globalThis.screen], currentScreen: globalThis.screen }); },
                    webkitRequestFileSystem() {},
                    webkitResolveLocalFileSystemURL() {},
                };
                const _mask = globalThis._maskFunction;
                for (const [name, fn] of Object.entries(_lateWindowMethods)) {
                    if (typeof globalThis[name] !== 'function') {
                        if (typeof _mask === 'function') _mask(fn, name);
                        Object.defineProperty(globalThis, name, {
                            value: fn, writable: true, enumerable: true, configurable: true,
                        });
                    }
                }
                for (const [alias, name] of [
                    ['webkitSpeechGrammar', 'SpeechGrammar'],
                    ['webkitSpeechGrammarList', 'SpeechGrammarList'],
                    ['webkitSpeechRecognition', 'SpeechRecognition'],
                    ['webkitSpeechRecognitionError', 'SpeechRecognitionErrorEvent'],
                    ['webkitSpeechRecognitionEvent', 'SpeechRecognitionEvent'],
                ]) {
                    if (typeof globalThis[alias] !== 'function' && typeof globalThis[name] === 'function') {
                        Object.defineProperty(globalThis, alias, {
                            value: globalThis[name], writable: true, enumerable: true, configurable: true,
                        });
                    }
                }
                if (typeof globalThis.cancelAnimationFrame === 'function') {
                    globalThis.webkitCancelAnimationFrame = globalThis.cancelAnimationFrame;
                }
                if (typeof globalThis.requestAnimationFrame === 'function') {
                    globalThis.webkitRequestAnimationFrame = globalThis.requestAnimationFrame;
                }
                if (typeof globalThis.URL === 'function') globalThis.webkitURL = globalThis.URL;
                for (const name of ['event', 'ondevicemotion', 'ondeviceorientation',
                    'ondeviceorientationabsolute', 'onerror']) {
                    if (!Object.prototype.hasOwnProperty.call(globalThis, name)) {
                        Object.defineProperty(globalThis, name, {
                            value: name === 'event' ? undefined : null,
                            writable: true, enumerable: true, configurable: true,
                        });
                    }
                }
                if (!globalThis.navigation && globalThis.Navigation) {
                    globalThis.navigation = Object.create(globalThis.Navigation.prototype);
                }
                if (!globalThis.sharedStorage && globalThis.SharedStorage) {
                    globalThis.sharedStorage = Object.create(globalThis.SharedStorage.prototype);
                }
                // Web-IDL interface members are enumerable in Chromium. Class
                // syntax and several older bootstrap definitions defaulted to
                // non-enumerable, producing a large descriptor-level mismatch
                // even after the prototype member sets matched exactly.
                for (const Ctor of [globalThis.Navigator, globalThis.Document]) {
                    const proto = Ctor && Ctor.prototype;
                    if (!proto) continue;
                    for (const name of Object.getOwnPropertyNames(proto)) {
                        if (name === 'constructor') continue;
                        const desc = Object.getOwnPropertyDescriptor(proto, name);
                        if (!desc || desc.enumerable) continue;
                        desc.enumerable = true;
                        try { Object.defineProperty(proto, name, desc); } catch (_e2) {}
                    }
                }
            } catch (_e) {}
        }

        // -- iOS Safari profile: strip 16 declined APIs + add iOS globals --
        // Per Apple's "16 web APIs declined for privacy" policy. The
        // single highest-ROI mobile patch — many leaks vanish at once.
        const _deviceClass = (_hasProfile && ops.op_get_profile_value)
            ? ops.op_get_profile_value("device_class")
            : "Desktop";
        if (_deviceClass === "MobileIOS") {
            // 1. Delete 16 declined APIs from globalThis
            const _iosDeleted = [
                "Bluetooth", "USB", "USBAlternateInterface", "USBConfiguration",
                "USBConnectionEvent", "USBDevice", "USBEndpoint",
                "USBInTransferResult", "USBInterface",
                "USBIsochronousInTransferPacket", "USBIsochronousInTransferResult",
                "USBIsochronousOutPacket", "USBIsochronousOutTransferResult",
                "USBOutTransferResult",
                "HID", "HIDConnectionEvent", "HIDDevice", "HIDInputReportEvent",
                "Serial", "SerialPort",
                "NetworkInformation", "BatteryManager",
                "IdleDetector", "EyeDropper",
                // Chrome-only interfaces real Safari does NOT expose.
                // A `'X' in window` check against an iOS UA would flag these.
                "UserActivation", "Scheduling",
                "Sensor", "Accelerometer", "AbsoluteOrientationSensor",
                "GravitySensor", "Gyroscope", "LinearAccelerationSensor",
                "Magnetometer", "OrientationSensor", "RelativeOrientationSensor",
                // WebGPU is feature-flagged on iOS 18+ but defaults off
                "GPU", "GPUAdapter", "GPUDevice", "GPUQueue", "GPUBuffer",
                "GPUTexture", "GPUSampler", "GPUBindGroup", "GPUBindGroupLayout",
                "GPUPipelineLayout", "GPUShaderModule", "GPURenderPipeline",
                "GPUComputePipeline", "GPUCommandEncoder", "GPUCommandBuffer",
                "GPURenderPassEncoder", "GPUComputePassEncoder",
                "GPURenderBundleEncoder", "GPURenderBundle", "GPUCanvasContext",
                "GPUColorWrite", "GPUMapMode", "GPUTextureUsage",
                "GPUBufferUsage", "GPUShaderStage",
                // Speech recognition has limited iOS support, but webkit-prefixed
                // is the only form Safari ships
                "SpeechRecognition", "SpeechRecognitionEvent",
                "SpeechRecognitionErrorEvent",
            ];
            for (const k of _iosDeleted) {
                try { delete globalThis[k]; } catch (_e) {}
            }

            // 2. Strip Navigator.prototype methods/getters that iOS doesn't have.
            // Defense in depth: window_bootstrap.js W1.5 gate avoids
            // installing these on iOS profiles, but we also delete here in
            // case any prior pass re-installed them. Use `delete` (not
            // redefine-with-undefined-getter) so `'X' in navigator` returns
            // false — the descriptor must not be present.
            const _NavProto = globalThis.Navigator && globalThis.Navigator.prototype;
            if (_NavProto) {
                for (const k of [
                    "bluetooth", "usb", "serial", "hid", "requestMIDIAccess",
                    "getBattery", "connection", "getInstalledRelatedApps",
                    "scheduling", "userActivation",
                    // userAgentData absent on Safari (no UA-CH at all)
                    "userAgentData",
                    // deviceMemory absent on Safari
                    "deviceMemory",
                ]) {
                    try { delete _NavProto[k]; } catch (_e) {}
                }
            }

            // 3. PaymentRequest.prototype.hasEnrolledInstrument is Chrome/Edge-only
            //    Safari MUST NOT have it.
            if (globalThis.PaymentRequest && globalThis.PaymentRequest.prototype) {
                try { delete globalThis.PaymentRequest.prototype.hasEnrolledInstrument; } catch (_e) {}
            }

            // 4. window.orientation — legacy iOS-only property. Desktop browsers
            //    do NOT have this. Setting to 0 = portrait.
            try {
                Object.defineProperty(globalThis, "orientation", {
                    get: function() { return 0; },
                    configurable: true, enumerable: true,
                });
            } catch (_e) {}

            // 5. ontouchstart on window — every detection script's cheapest
            //    mobile-vs-desktop check
            try {
                Object.defineProperty(globalThis, "ontouchstart", {
                    value: null, configurable: true, writable: true, enumerable: true,
                });
            } catch (_e) {}

            // 6. DeviceMotionEvent.requestPermission + DeviceOrientationEvent.requestPermission
            //    iOS 13+ requires user-gesture-gated permission for these. The presence
            //    of these static methods is itself a strong iOS signal — Android does NOT
            //    expose these statics.
            if (globalThis.DeviceMotionEvent
                && typeof globalThis.DeviceMotionEvent.requestPermission !== "function") {
                try {
                    globalThis.DeviceMotionEvent.requestPermission =
                        function requestPermission() { return Promise.resolve("denied"); };
                } catch (_e) {}
            }
            if (globalThis.DeviceOrientationEvent
                && typeof globalThis.DeviceOrientationEvent.requestPermission !== "function") {
                try {
                    globalThis.DeviceOrientationEvent.requestPermission =
                        function requestPermission() { return Promise.resolve("denied"); };
                } catch (_e) {}
            }

            // 7. Sec-CH-UA-* JS surface absent on Safari — already handled
            //    above via userAgentData getter returning undefined.

            // 8. window.chrome must be absent on iOS Safari. Some scripts
            //    explicitly probe `typeof window.chrome` — Chrome
            //    returns "object", Safari "undefined". A positive hit under
            //    an iOS UA is a strong inconsistency.
            try { delete globalThis.chrome; } catch (_e) {}

            // 8b. navigator.permissions.query() — Safari 18 supports a much
            //     narrower permission name set than Chrome. Per WebKit:
            //     allowed = notifications, push, camera, microphone,
            //               geolocation, persistent-storage.
            //     Chrome-only names (midi, accelerometer, gyroscope,
            //     magnetometer, ambient-light-sensor, background-fetch,
            //     background-sync, clipboard-read, clipboard-write,
            //     display-capture, screen-wake-lock, system-wake-lock,
            //     window-management) must reject with TypeError on Safari
            //     to match real WebKit behavior. PLAN W1.5 (Plan §0 #6).
            try {
                if (globalThis.navigator && globalThis.navigator.permissions) {
                    const _safariAllowed = new Set([
                        'notifications', 'push', 'camera', 'microphone',
                        'geolocation', 'persistent-storage',
                    ]);
                    const _PProto = globalThis.navigator.permissions
                        && Object.getPrototypeOf(globalThis.navigator.permissions);
                    if (_PProto && typeof _PProto.query === 'function') {
                        const _origQuery = _PProto.query;
                        const safariQuery = function query(desc) {
                            const name = desc && typeof desc === 'object' ? desc.name : undefined;
                            if (typeof name !== 'string' || !_safariAllowed.has(name)) {
                                return Promise.reject(new TypeError(
                                    "Failed to execute 'query' on 'Permissions': "
                                    + (typeof name === 'string'
                                        ? "The provided value '" + name + "' is not a valid enum value of type PermissionName."
                                        : "parameter 1 is not of type 'PermissionDescriptor'.")
                                ));
                            }
                            return _origQuery.call(this, desc);
                        };
                        Object.defineProperty(_PProto, 'query', {
                            value: safariQuery, writable: true, enumerable: false, configurable: true,
                        });
                        // Preserve native-shape Function.prototype.toString output
                        // via the _nativeTag symbol installed by stealth_bootstrap.js.
                        const _tag = globalThis._nativeTag;
                        if (_tag) {
                            try { Object.defineProperty(safariQuery, _tag, { value: 'query', configurable: true }); } catch (_e) {}
                            try { Object.defineProperty(safariQuery, 'name', { value: 'query', configurable: true }); } catch (_e) {}
                        }
                    }
                }
            } catch (_e) {}

            // 9. navigator.plugins / navigator.mimeTypes empty on iOS
            //    (PluginArray length 0 is the canonical mobile-Safari shape).
            try {
                if (globalThis.navigator) {
                    const _emptyPlugins = Object.create(globalThis.PluginArray ? globalThis.PluginArray.prototype : null);
                    Object.defineProperty(_emptyPlugins, 'length', { get: () => 0, enumerable: true });
                    Object.defineProperty(_emptyPlugins, 'item', {
                        value: function item() { return null; },
                        writable: true, enumerable: false, configurable: true,
                    });
                    Object.defineProperty(_emptyPlugins, 'namedItem', {
                        value: function namedItem() { return null; },
                        writable: true, enumerable: false, configurable: true,
                    });
                    Object.defineProperty(_emptyPlugins, 'refresh', {
                        value: function refresh() {},
                        writable: true, enumerable: false, configurable: true,
                    });
                    Object.defineProperty(_emptyPlugins, Symbol.iterator, {
                        value: function* () {},
                        writable: true, enumerable: false, configurable: true,
                    });
                    Object.defineProperty(_NavProto, 'plugins', {
                        get: function() { return _emptyPlugins; },
                        configurable: true, enumerable: false,
                    });
                    const _emptyMimeTypes = Object.create(globalThis.MimeTypeArray ? globalThis.MimeTypeArray.prototype : null);
                    Object.defineProperty(_emptyMimeTypes, 'length', { get: () => 0, enumerable: true });
                    Object.defineProperty(_emptyMimeTypes, 'item', {
                        value: function item() { return null; },
                        writable: true, enumerable: false, configurable: true,
                    });
                    Object.defineProperty(_emptyMimeTypes, 'namedItem', {
                        value: function namedItem() { return null; },
                        writable: true, enumerable: false, configurable: true,
                    });
                    Object.defineProperty(_NavProto, 'mimeTypes', {
                        get: function() { return _emptyMimeTypes; },
                        configurable: true, enumerable: false,
                    });
                    // pdfViewerEnabled is false on mobile (no integrated PDF viewer)
                    Object.defineProperty(_NavProto, 'pdfViewerEnabled', {
                        get: function() { return false; },
                        configurable: true, enumerable: false,
                    });
                }
            } catch (_e) {}
        }
    } catch (_e) { /* profile-conditional installs are best-effort */ }

    // -- native-source masking of Web Platform constructors --------
    // Some scripts dump `String(globalThis.<ctor>)` for a
    // rotating list of Web Platform constructors/functions and feed
    // the result into a browser-fingerprint score. Without masking,
    // many probed names leak our polyfill source —
    // raw `class Worker {…}` / `function(input, init){…}` bodies, or
    // the wrong native name (constructors that extend our internal
    // EventTarget reported `function EventTarget() { [native code] }`,
    // `clearTimeout` reported `clearInterval`). Real Chrome returns
    // `function <Name>() { [native code] }` for every one of these.
    //
    // This MUST run here, not in stealth_bootstrap.js: the constructors
    // are defined by interfaces/shared_apis/streams/window/worker
    // bootstraps that are concatenated AFTER stealth_bootstrap.js (and
    // shared_apis/worker run at runtime, after the snapshot). This is
    // the universal last pass — it runs always for the page (even from
    // snapshot) and last for workers — and `_maskFunction` is still on
    // globalThis here (the `internals` purge below removes it after).
    try {
        const _mask = globalThis._maskFunction;
        if (typeof _mask === 'function') {
            // De-alias Chrome-distinct pairs our impl points at one
            // object. The fresh /tl `sfc` probe caught these: real
            // Chrome has clearTimeout!==clearInterval,
            // scroll!==scrollTo, DOMMatrix!==DOMMatrixReadOnly — each
            // is its own named native, so a single shared object can't
            // satisfy `String(globalThis[name])` for both names. We
            // split the secondary into a distinct delegator/subclass
            // (more Chrome-faithful; zero behavior change).
            try {
                if (typeof globalThis.clearTimeout === 'function'
                    && globalThis.clearInterval === globalThis.clearTimeout) {
                    const _ct = globalThis.clearTimeout;
                    globalThis.clearInterval = { clearInterval(id) { return _ct(id); } }.clearInterval;
                }
                if (typeof globalThis.scrollTo === 'function'
                    && globalThis.scroll === globalThis.scrollTo) {
                    const _st = globalThis.scrollTo;
                    globalThis.scroll = { scroll() { return _st.apply(this, arguments); } }.scroll;
                }
                if (typeof globalThis.DOMMatrix === 'function'
                    && globalThis.DOMMatrixReadOnly === globalThis.DOMMatrix) {
                    globalThis.DOMMatrixReadOnly = class DOMMatrixReadOnly extends globalThis.DOMMatrix {};
                }
            } catch (_e) {}

            // Native NON-constructor functions must have NO own
            // `prototype` and must be non-constructable (`new fetch()`
            // throws in Chrome). A CLEAN production probe
            // (the challenge-vendor native-fn-shape clean probe — no capture shim)
            // confirmed setTimeout/setInterval/clearTimeout/
            // clearInterval/queueMicrotask/structuredClone are plain
            // `function` decls → carry `.prototype` + are
            // constructable (a real-browser inconsistency).
            // `function f(){}`'s `.prototype` is non-configurable so
            // `delete` fails — the only fix is to REPLACE with a
            // method-shorthand (`{[k](){}}[k]`): no `.prototype`,
            // non-constructable, name===k. Forwarding wrapper
            // preserves behavior (none use `this`/`new`). Only the
            // probe-confirmed-broken set is touched; already-correct
            // async/shorthand natives (fetch/atob/btoa/scrollTo/
            // reportError/console.*) are left alone.
            const _natMethod = (holder, key, nm) => {
                try {
                    const o = holder && holder[key];
                    if (typeof o !== 'function') return;
                    if (!Object.prototype.hasOwnProperty.call(o, 'prototype')) {
                        _mask(o, nm || key);
                        return;
                    }
                    const w = { [key]() { return o.apply(this, arguments); } }[key];
                    _mask(w, nm || key);
                    try { holder[key] = w; } catch (_e2) {}
                } catch (_e2) {}
            };
            for (const _k of ['setTimeout', 'setInterval', 'clearTimeout',
                'clearInterval', 'queueMicrotask', 'structuredClone']) {
                _natMethod(globalThis, _k);
            }
            try {
                const _ca = globalThis.chrome && globalThis.chrome.app;
                if (_ca) {
                    for (const _m of ['getDetails', 'getIsInstalled',
                        'installState', 'runningState']) {
                        _natMethod(_ca, _m);
                    }
                }
            } catch (_e) {}

            // (chrome.app.* are handled by _natMethod above — it both
            // native-masks toString [otherwise a probe would leak
            // "function getDetails() { return null; }"] and removes the
            // illegal `.prototype`/constructability.)
            // The commonly probed names, plus adjacent
            // standard constructors — all are
            // genuinely `[native code]` in real Chrome, so masking any
            // that exist on this profile is correct (missing ones are a
            // safe no-op via `_maskFunction`'s `if (!fn) return`).
            // [globalKey, maskName]. maskName differs from globalKey
            // only for the legacy webkit-prefixed aliases: in real
            // Chrome `webkitAudioContext === AudioContext` (same object),
            // so `String(webkitAudioContext)` is
            // `function AudioContext() { [native code] }`. Masking them
            // to their prefixed key would itself be a divergence.
            const _sfcNames = [
                ['webkitMediaStream', 'MediaStream'],
                ['webkitAudioContext', 'AudioContext'],
                ['webkitRTCPeerConnection', 'RTCPeerConnection'],
                'fetch', 'clearTimeout', 'clearInterval', 'setTimeout',
                'setInterval', 'TouchEvent', 'AudioContext', 'OffscreenCanvas',
                'Bluetooth', 'StorageManager', 'scrollTo', 'scroll', 'scrollBy',
                'Worker', 'SharedWorker', 'ServiceWorker', 'WorkerGlobalScope',
                'DedicatedWorkerGlobalScope', 'FileReader', 'ImageBitmap',
                'DOMMatrix', 'DOMMatrixReadOnly', 'PerformanceObserver',
                'PerformanceEntry', 'ReportingObserver', 'ReadableStream',
                'WritableStream', 'TransformStream', 'ReadableStreamDefaultReader',
                'WritableStreamDefaultWriter', 'ReadableStreamDefaultController',
                'BroadcastChannel', 'MessagePort', 'MessageChannel',
                'EventSource', 'CompressionStream', 'DecompressionStream',
                'Crypto', 'SubtleCrypto', 'CloseEvent', 'AbortController',
                'AbortSignal', 'DOMException', 'URL', 'URLSearchParams',
                'FormData', 'Blob', 'File', 'FileList', 'RTCPeerConnection',
                'PressureObserver', 'InputDeviceCapabilities', 'MediaSession',
                'Touch', 'TouchList', 'EyeDropper', 'XMLHttpRequest',
                'XMLHttpRequestUpload', 'WebSocket', 'Notification', 'Image',
                'Audio', 'Headers', 'Request', 'Response', 'createImageBitmap',
                'structuredClone', 'queueMicrotask', 'reportError', 'atob',
                'btoa', 'ResizeObserver', 'IntersectionObserver',
                'MutationObserver', 'TextEncoder', 'TextDecoder', 'EventTarget',
                'Event', 'CustomEvent', 'MediaStream', 'MediaStreamTrack',
                'MediaRecorder', 'DOMRect', 'DOMRectReadOnly', 'DOMPoint',
                'DOMPointReadOnly', 'DOMQuad',
                // The WebGL/Canvas context
                // constructor OBJECTS themselves. Their prototype methods are
                // masked by the universal sweep, but String(WebGLRenderingContext)
                // is commonly enumerated and must be `[native code]`.
                'WebGLRenderingContext', 'WebGL2RenderingContext',
                'CanvasRenderingContext2D', 'WebGLContextEvent',
                // Event-subclass constructor
                // objects. event_bootstrap.js defines them as JS classes, so
                // String(MouseEvent) leaked `class MouseEvent extends ...`,
                // which differs from real Chrome. Masking sets `[native code]`
                // + the correct own `.name`. Real Chrome: every one is native.
                'UIEvent', 'MouseEvent', 'KeyboardEvent', 'InputEvent',
                'FocusEvent', 'PointerEvent', 'WheelEvent', 'MessageEvent',
                'ErrorEvent', 'ProgressEvent', 'AnimationEvent',
                'TransitionEvent', 'ClipboardEvent', 'PopStateEvent',
                'HashChangeEvent', 'StorageEvent', 'PageTransitionEvent',
                'BeforeUnloadEvent', 'DragEvent', 'SecurityPolicyViolationEvent',
                'CompositionEvent', 'DeviceMotionEvent', 'DeviceOrientationEvent',
            ];
            for (const _e of _sfcNames) {
                try {
                    const _key = Array.isArray(_e) ? _e[0] : _e;
                    const _nm = Array.isArray(_e) ? _e[1] : _e;
                    const _fn = globalThis[_key];
                    if (typeof _fn === 'function') _mask(_fn, _nm);
                } catch (_e2) {}
            }
        }
    } catch (_e) { /* sfc masking is best-effort */ }

    // -- Universal prototype mask sweep ----------
    // Many scripts inspect Function.prototype.toString
    // on patched prototype methods (Headers/Request/Response, XHR,
    // Observers, Streams, Event subclasses, IDB, Range, etc.). Walk
    // every globalThis constructor that has a .prototype, mask every
    // own-function method to `function NAME() { [native code] }`.
    // Runs AFTER all bootstraps (interfaces / shared_apis / streams /
    // events / canvas / window / worker) so it covers every prototype
    // installed by them — including bootstraps that run post-snapshot.
    // Safe on real V8 natives: `_maskAsNative` is idempotent — sets the
    // Symbol(__browser_oxide_native__) tag; if the function was already
    // native-toString-ing it stays so.
    try {
        const _mask = globalThis._maskAsNative;
        if (typeof _mask === 'function') {
            const _SKIP = new Set([Object.prototype, Function.prototype]);
            for (const _gname of Object.getOwnPropertyNames(globalThis)) {
                // Never read through an accessor while sweeping the global
                // namespace. Window numeric frame properties (`window[0]`) and
                // many Web APIs are getters; invoking them here can create
                // realms or trigger observable work before page setup finishes.
                let _desc;
                try { _desc = Object.getOwnPropertyDescriptor(globalThis, _gname); } catch (_e) { continue; }
                if (!_desc || !Object.prototype.hasOwnProperty.call(_desc, 'value')) continue;
                const _v = _desc.value;
                if (typeof _v !== 'function') continue;
                const _p = _v.prototype;
                if (!_p || _SKIP.has(_p)) continue;
                const _methods = [];
                let _ns;
                try { _ns = Object.getOwnPropertyNames(_p); } catch (_e) { continue; }
                for (const _n of _ns) {
                    if (_n === 'constructor') continue;
                    let _d;
                    try { _d = Object.getOwnPropertyDescriptor(_p, _n); } catch (_e) { continue; }
                    // Collect ACCESSOR props too
                    // (get/set), not just data-value methods. _maskAsNative
                    // already masks desc.get/desc.set (stealth_bootstrap.js:94),
                    // but the sweep previously skipped accessor-only props, so
                    // ~15 injected getters/setters (Request.signal, Response.*,
                    // ReadableStream.locked, MessagePort.onmessage,
                    // URLSearchParams.size, WebSocket.*) leaked JS source under
                    // `getOwnPropertyDescriptor(proto,name).get.toString()` —
                    // a Function.toString integrity tell ~11 vendors probe.
                    if (
                        _d &&
                        (typeof _d.value === 'function' ||
                            typeof _d.get === 'function' ||
                            typeof _d.set === 'function')
                    ) {
                        _methods.push(_n);
                    }
                }
                if (_methods.length) {
                    try { _mask(_p, ..._methods); } catch (_e) {}
                }
            }
        }
    } catch (_e) { /* universal mask sweep is best-effort */ }

    const internals = [
        'Deno',
        'ops',
        '_maskFunction',
        '_maskAsNative',
        '_nativeTag',
        '_customElementsRegistry',
        '__bootstrap',
        '__browser_oxide',
        '__syncCookiesFromNet',
        '__documentReadyState',
        '__drainCspViolations',
        '__onNodeInserted',
        '__errors',
    ];

    // -- Worker Scope Isolation (Phase 8) ---------------------------
    // Real Chrome Web Workers (DedicatedWorkerGlobalScope) have a very
    // clean namespace. They do NOT expose DOM, CSSOM, or Hardware APIs.
    // If we're in a worker, purge the illegal globals.
    const _isWorker = typeof DedicatedWorkerGlobalScope !== 'undefined' && 
                      globalThis instanceof DedicatedWorkerGlobalScope;
    if (_isWorker) {
        const _workerPurge = [
            'window', 'document', 'history', 'locationbar', 'menubar', 
            'personalbar', 'scrollbars', 'statusbar', 'toolbar', 'frames', 
            'parent', 'top', 'opener', 'frameElement', 'styleMedia', 
            'getComputedStyle', 'getSelection', 'matchMedia', 'alert', 
            'confirm', 'prompt', 'print', 'stop', 'open',
            'focus', 'blur', 'moveBy', 'moveTo', 'resizeBy', 'resizeTo', 
            'scroll', 'scrollBy', 'scrollTo',
            'requestIdleCallback', 'cancelIdleCallback',
            // Constructors
            'Node', 'Element', 'HTMLElement', 'HTMLDocument', 'Document', 
            'CharacterData', 'Text', 'Comment', 'CDATASection', 'DocumentFragment', 
            'DocumentType', 'NamedNodeMap', 'Attr', 'NodeList', 'HTMLCollection', 
            'HTMLAllCollection', 'DOMTokenList', 'DOMImplementation', 'Range', 
            'Selection', 'DOMParser', 'XMLSerializer', 'XPathEvaluator', 
            'XPathExpression', 'XPathResult', 'XSLTProcessor', 'MutationObserver', 
            'MutationRecord', 'IntersectionObserver', 'ResizeObserver', 
            'PermissionStatus', 'Screen', 'ScreenOrientation', 'VisualViewport',
            'ViewTransition', 'Highlight', 'HighlightRegistry',
            // Hardware/Media (not allowed in workers)
            'Bluetooth', 'USB', 'HID', 'Serial', 'Gamepad', 'GamepadButton', 
            'GamepadEvent', 'GamepadHapticActuator', 'MediaStream', 'MediaStreamTrack', 
            'MediaRecorder', 'RTCPeerConnection', 'RTCDataChannel', 'RTCSessionDescription', 
            'RTCIceCandidate', 'RTCCertificate', 'Presentation', 'PresentationRequest',
            // CSS classes (100+)
            'CSS', 'CSSStyleSheet', 'CSSRule', 'CSSStyleRule', 'CSSMediaRule', 
            'CSSImportRule', 'CSSFontFaceRule', 'CSSPageRule', 'CSSKeyframesRule', 
            'CSSKeyframeRule', 'CSSNamespaceRule', 'CSSSupportsRule', 'CSSCounterStyleRule',
            // ... and all HTML*Element subclasses
        ];
        for (const k of Object.keys(globalThis)) {
            if (k.startsWith('HTML') || k.startsWith('SVG') || k.startsWith('CSS') || _workerPurge.includes(k)) {
                try { delete globalThis[k]; } catch (_) {}
            }
        }

        // Chrome 148's DedicatedWorkerGlobalScope exposes a strict 335-name
        // namespace. interfaces_bootstrap is intentionally shared with the
        // document runtime, but its non-enumerable Window constructors escaped
        // the old Object.keys-only purge and inflated workers past 1,000 names.
        // Use the captured Chrome-for-Testing allowlist and
        // Object.getOwnPropertyNames so worker/window namespace separation is
        // deterministic rather than relying on descriptor enumerability.
        // Capture the masking helper before the allowlist removes its private
        // global binding. The function itself remains usable from this closure.
        const _workerMaskFunction = globalThis._maskFunction;
        const _chromeWorkerGlobals = new Set((
            'AbortController AbortSignal AggregateError Array ArrayBuffer AsyncDisposableStack Atomics AudioData '
            + 'AudioDecoder AudioEncoder BackgroundFetchManager BackgroundFetchRecord BackgroundFetchRegistration BarcodeDetector BigInt BigInt64Array '
            + 'BigUint64Array Blob Boolean BroadcastChannel ByteLengthQueuingStrategy CSSSkewX CSSSkewY Cache '
            + 'CacheStorage CanvasGradient CanvasPattern CloseEvent CompressionStream CountQueuingStrategy CreateMonitor CropTarget '
            + 'Crypto CryptoKey CustomEvent DOMException DOMMatrix DOMMatrixReadOnly DOMPoint DOMPointReadOnly '
            + 'DOMQuad DOMRect DOMRectReadOnly DOMStringList DataView Date DecompressionStream DedicatedWorkerGlobalScope '
            + 'DisposableStack EncodedAudioChunk EncodedVideoChunk Error ErrorEvent EvalError Event EventSource '
            + 'EventTarget File FileList FileReader FileReaderSync FileSystemDirectoryHandle FileSystemFileHandle FileSystemHandle '
            + 'FileSystemObserver FileSystemSyncAccessHandle FileSystemWritableFileStream FinalizationRegistry Float16Array Float32Array Float64Array FontFace '
            + 'FormData Function GPU GPUAdapter GPUAdapterInfo GPUBindGroup GPUBindGroupLayout GPUBuffer '
            + 'GPUBufferUsage GPUCanvasContext GPUColorWrite GPUCommandBuffer GPUCommandEncoder GPUCompilationInfo GPUCompilationMessage GPUComputePassEncoder '
            + 'GPUComputePipeline GPUDevice GPUDeviceLostInfo GPUError GPUExternalTexture GPUInternalError GPUMapMode GPUOutOfMemoryError '
            + 'GPUPipelineError GPUPipelineLayout GPUQuerySet GPUQueue GPURenderBundle GPURenderBundleEncoder GPURenderPassEncoder GPURenderPipeline '
            + 'GPUSampler GPUShaderModule GPUShaderStage GPUSupportedFeatures GPUSupportedLimits GPUTexture GPUTextureUsage GPUTextureView '
            + 'GPUUncapturedErrorEvent GPUValidationError HID HIDConnectionEvent HIDDevice HIDInputReportEvent Headers IDBCursor '
            + 'IDBCursorWithValue IDBDatabase IDBFactory IDBIndex IDBKeyRange IDBObjectStore IDBOpenDBRequest IDBRecord '
            + 'IDBRequest IDBTransaction IDBVersionChangeEvent IdleDetector ImageBitmap ImageBitmapRenderingContext ImageData ImageDecoder '
            + 'ImageTrack ImageTrackList Infinity Int16Array Int32Array Int8Array Intl Iterator '
            + 'JSON Lock LockManager Map Math MediaCapabilities MediaSource MediaSourceHandle '
            + 'MessageChannel MessageEvent MessagePort NaN NavigationPreloadManager NavigatorUAData NetworkInformation Notification '
            + 'Number Object Observable OffscreenCanvas OffscreenCanvasRenderingContext2D Origin Path2D Performance '
            + 'PerformanceEntry PerformanceMark PerformanceMeasure PerformanceObserver PerformanceObserverEntryList PerformanceResourceTiming PerformanceServerTiming PeriodicSyncManager '
            + 'PermissionStatus Permissions PressureObserver PressureRecord ProgressEvent Promise PromiseRejectionEvent Proxy '
            + 'PushManager PushSubscription PushSubscriptionOptions QuotaExceededError RTCDataChannel RTCEncodedAudioFrame RTCEncodedVideoFrame RTCRtpScriptTransformer '
            + 'RTCTransformEvent RangeError ReadableByteStreamController ReadableStream ReadableStreamBYOBReader ReadableStreamBYOBRequest ReadableStreamDefaultController ReadableStreamDefaultReader '
            + 'ReferenceError Reflect RegExp ReportBody ReportingObserver Request Response RestrictionTarget '
            + 'Scheduler SecurityPolicyViolationEvent Serial SerialPort ServiceWorkerRegistration Set SourceBuffer SourceBufferList '
            + 'StorageBucket StorageBucketManager StorageManager String Subscriber SubtleCrypto SuppressedError Symbol '
            + 'SyncManager SyntaxError TaskController TaskPriorityChangeEvent TaskSignal Temporal TextDecoder TextDecoderStream '
            + 'TextEncoder TextEncoderStream TextMetrics TransformStream TransformStreamDefaultController TrustedHTML TrustedScript TrustedScriptURL '
            + 'TrustedTypePolicy TrustedTypePolicyFactory TypeError URIError URL URLPattern URLSearchParams USB '
            + 'USBAlternateInterface USBConfiguration USBConnectionEvent USBDevice USBEndpoint USBInTransferResult USBInterface USBIsochronousInTransferPacket '
            + 'USBIsochronousInTransferResult USBIsochronousOutTransferPacket USBIsochronousOutTransferResult USBOutTransferResult Uint16Array Uint32Array Uint8Array Uint8ClampedArray '
            + 'UserActivation VideoColorSpace VideoDecoder VideoEncoder VideoFrame WGSLLanguageFeatures WeakMap WeakRef '
            + 'WeakSet WebAssembly WebGL2RenderingContext WebGLActiveInfo WebGLBuffer WebGLContextEvent WebGLFramebuffer WebGLObject '
            + 'WebGLProgram WebGLQuery WebGLRenderbuffer WebGLRenderingContext WebGLSampler WebGLShader WebGLShaderPrecisionFormat WebGLSync '
            + 'WebGLTexture WebGLTransformFeedback WebGLUniformLocation WebGLVertexArrayObject WebSocket WebSocketError WebSocketStream WebTransport '
            + 'WebTransportBidirectionalStream WebTransportDatagramDuplexStream WebTransportError Worker WorkerGlobalScope WorkerLocation WorkerNavigator WritableStream '
            + 'WritableStreamDefaultController WritableStreamDefaultWriter XMLHttpRequest XMLHttpRequestEventTarget XMLHttpRequestUpload cancelAnimationFrame close console '
            + 'decodeURI decodeURIComponent encodeURI encodeURIComponent escape eval globalThis isFinite '
            + 'isNaN name onmessage onmessageerror onrtctransform parseFloat parseInt postMessage '
            + 'requestAnimationFrame undefined unescape webkitRequestFileSystem webkitRequestFileSystemSync webkitResolveLocalFileSystemSyncURL webkitResolveLocalFileSystemURL'
        ).split(' '));
        for (const k of Object.getOwnPropertyNames(globalThis)) {
            if (_chromeWorkerGlobals.has(k)) continue;
            try { delete globalThis[k]; } catch (_) {}
        }

        // WebIDL constructors are non-enumerable own properties of a worker
        // global. Only DedicatedWorkerGlobalScope's methods and event-handler
        // attributes enumerate. Shared bootstraps used ordinary assignment for
        // several interfaces, leaking dozens of entries through Object.keys().
        const _enumerableWorkerGlobals = new Set((
            'cancelAnimationFrame close name onmessage onmessageerror '
            + 'onrtctransform postMessage requestAnimationFrame '
            + 'webkitRequestFileSystem webkitRequestFileSystemSync '
            + 'webkitResolveLocalFileSystemSyncURL webkitResolveLocalFileSystemURL'
        ).split(' '));
        const _readonlyEcmaGlobals = new Set(['Infinity', 'NaN', 'undefined']);
        for (const k of _chromeWorkerGlobals) {
            const descriptor = Object.getOwnPropertyDescriptor(globalThis, k);
            if (!descriptor) continue;
            const normalized = {
                ...descriptor,
                enumerable: _enumerableWorkerGlobals.has(k),
            };
            if ('value' in normalized && !_readonlyEcmaGlobals.has(k)) {
                normalized.writable = true;
            }
            try { Object.defineProperty(globalThis, k, normalized); } catch (_) {}
            if (typeof descriptor.value === 'function'
                && typeof _workerMaskFunction === 'function') {
                _workerMaskFunction(descriptor.value, k);
            }
        }

        // DedicatedWorkerGlobalScope event attributes are accessor properties,
        // not writable data slots. The accessors also preserve assignment for
        // the EventTarget dispatcher used by the worker message pump.
        const _eventAttributeValues = new Map([
            ['name', typeof globalThis.name === 'string' ? globalThis.name : ''],
            ['onmessage', globalThis.onmessage || null],
            ['onmessageerror', globalThis.onmessageerror || null],
            ['onrtctransform', globalThis.onrtctransform || null],
        ]);
        for (const k of _eventAttributeValues.keys()) {
            const getter = function () { return _eventAttributeValues.get(k); };
            const setter = function (value) { _eventAttributeValues.set(k, value); };
            if (typeof _workerMaskFunction === 'function') {
                _workerMaskFunction(getter, `get ${k}`);
                _workerMaskFunction(setter, `set ${k}`);
            }
            try {
                Object.defineProperty(globalThis, k, {
                    configurable: true,
                    enumerable: true,
                    get: getter,
                    set: setter,
                });
            } catch (_) {}
        }
    }

    if (ops && ops.op_cross_origin_isolated && !ops.op_cross_origin_isolated()) {
        internals.push('SharedArrayBuffer');
    }

    // -- Warm-reuse global-namespace reset ---------------------------
    // The last retention source for a pooled `Page`: properties page
    // scripts hang straight off the global (`window.__APP_STATE = …`,
    // `window.onscroll = …`, framework singletons). `globalThis` is the
    // same object for the whole life of the `JsRuntime`, so on the warm
    // path every one of those — and everything they transitively
    // reference — survives into the next navigation. A real browser gives
    // each navigation a fresh global; this is the closest equivalent that
    // keeps the expensive bootstrap intact.
    //
    // `__markGlobalsBaseline()` snapshots the engine-owned key set;
    // `__resetPageGlobals()` deletes everything added since. Rust re-marks
    // the baseline once more after it installs the post-bootstrap
    // instrumentation (`__cookieWrites` / `__scriptErrors` / the fetch +
    // XHR wrappers), which is why those names are also allowlisted below —
    // construction paths that skip the re-mark must not lose them.
    // Note `window === globalThis` here (dom_bootstrap.js), so scrubbing
    // the global object covers both.
    // Guarded: this file is executed TWICE per page — once from
    // `BrowserJsRuntime`'s constructor (before any page script) and again
    // from `build_page_with_scripts_*` after the document's scripts have
    // run. Only the first execution may seed the baseline; re-running the
    // definitions would also reset the closure variable and throw the real
    // baseline away.
    if (typeof globalThis.__resetPageGlobals !== 'function') {
        let _globalsBaseline = null;
        let _onHandlerBaseline = null;
        const _BASELINE_ALWAYS = [
            '_browser_oxide', '__cookieWrites', '__scriptErrors',
            '__bo_input_events', '__jsCookies',
        ];

        // `on*` handlers need value-level treatment, not just key-level.
        // `onscroll`, `onerror`, … already EXIST as own properties of the
        // global at bootstrap (default `null`), so a page that assigns
        // `window.onscroll = fn` mutates a baseline key rather than adding
        // one — the key-set diff below cannot see it, and the closure (plus
        // everything it captures) survives the navigation.
        //
        // Blanket-nulling them is wrong: the engine itself installs
        // `window.onerror` as its script-error instrumentation, once, and
        // does NOT re-install it on the warm path. So snapshot the values
        // at baseline and RESTORE them, which nulls page assignments while
        // preserving the engine's.
        const _snapshotOnHandlers = (target) => {
            const m = new Map();
            if (!target) return m;
            let names;
            try { names = Object.getOwnPropertyNames(target); } catch (_e) { return m; }
            for (const k of names) {
                if (!k.startsWith('on')) continue;
                try { m.set(k, target[k]); } catch (_e) {}
            }
            return m;
        };
        const _restoreOnHandlers = (target, baseline) => {
            if (!target || !baseline) return;
            let names;
            try { names = Object.getOwnPropertyNames(target); } catch (_e) { return; }
            for (const k of names) {
                if (!k.startsWith('on')) continue;
                try {
                    if (typeof target[k] !== 'function') continue;
                    const orig = baseline.get(k);
                    // Already the engine's own handler ⇒ leave it alone.
                    if (orig === target[k]) continue;
                    target[k] = (typeof orig === 'function') ? orig : null;
                } catch (_e) {}
            }
        };

        Object.defineProperty(globalThis, '__markGlobalsBaseline', {
            value: function __markGlobalsBaseline() {
                const seen = new Set(_BASELINE_ALWAYS);
                for (const k of Object.getOwnPropertyNames(globalThis)) seen.add(k);
                for (const s of Object.getOwnPropertySymbols(globalThis)) seen.add(s);
                _globalsBaseline = seen;
                // `document` is a singleton that survives `replace_dom`, so
                // `document.onclick = fn` persists exactly like the window
                // case and needs the same treatment.
                _onHandlerBaseline = {
                    global: _snapshotOnHandlers(globalThis),
                    document: _snapshotOnHandlers(globalThis.document),
                };
            },
            writable: true, configurable: true, enumerable: false,
        });
        Object.defineProperty(globalThis, '__resetPageGlobals', {
            value: function __resetPageGlobals() {
                // No baseline ⇒ nothing to compare against; deleting on a
                // guess would strip the engine's own globals.
                if (!_globalsBaseline) return 0;
                let removed = 0;
                const keys = Object.getOwnPropertyNames(globalThis)
                    .concat(Object.getOwnPropertySymbols(globalThis));
                for (const k of keys) {
                    if (_globalsBaseline.has(k)) continue;
                    // Best-effort: a page can install a non-configurable
                    // property, and `delete` cannot remove those.
                    try { if (delete globalThis[k]) removed++; } catch (_e) {}
                }
                if (_onHandlerBaseline) {
                    _restoreOnHandlers(globalThis, _onHandlerBaseline.global);
                    _restoreOnHandlers(globalThis.document, _onHandlerBaseline.document);
                }
                return removed;
            },
            writable: true, configurable: true, enumerable: false,
        });
        // Seed the baseline on this first execution: it runs as the last
        // bootstrap, before anything page-authored, so the global namespace
        // is exactly the engine's. Rust re-marks once more after installing
        // the post-bootstrap instrumentation. The `internals` purge below
        // only ever REMOVES keys, so marking before it is safe.
        globalThis.__markGlobalsBaseline();
    }

    // -- Hide engine-owned bridge names from global namespace enumeration --
    // Several runtime services still need a global rendezvous point because
    // Rust drives them after bootstrap (frame setup/pumping, warm-page reset,
    // navigation state). They are implementation details, not Web Platform
    // globals, and exposing them through Object/Reflect enumeration is a
    // strong and unnecessary fingerprint. Keep direct access for the driver,
    // make the current properties non-enumerable, and filter only the engine's
    // reserved names when page code enumerates the global object. Page-owned
    // names such as `__NEXT_DATA__` remain visible.
    if (!globalThis.__browserOxideOwnKeysPatched) {
        const _hiddenExact = new Set([
            '_browser_oxide',
            '__bgSetTimeout', '__bo_input_events', '__cancelAllListeners',
            '__cancelAllTimers', '__completeDocumentLifecycle',
            '__cookieWrites', '__deliverMessage', '__ifAppendCount',
            '__jsCookies', '__keepLongTimersRefed', '__markGlobalsBaseline',
            '__onNodeInserted', '__pendingNavigation',
            '__pumpFrameMessages', '__resetCustomElements',
            '__resetDomRegistries', '__resetPageGlobals', '__scriptErrors',
            '__syncCookiesFromNet', '__browserOxideOwnKeysPatched',
        ]);
        const _isEngineInternalName = (name) => typeof name === 'string' && (
            _hiddenExact.has(name)
            || name.startsWith('__bo_')
            || name.startsWith('__frame')
            || name.startsWith('__parentFrame')
            || name.startsWith('__topFrame')
            || name.startsWith('__ox')
            || name.startsWith('__oxide')
            || name.startsWith('__browser_oxide')
            || name.startsWith('__OX')
        );
        const _isGlobalTarget = (target) => target === globalThis
            || (globalThis.window && target === globalThis.window);
        const _isPhantomFrameIndex = (name) => typeof name === 'string'
            && /^(0|[1-9]\d*)$/.test(name)
            && Number(name) >= Number(globalThis.length || 0);
        const _isHiddenGlobalName = (name) => _isEngineInternalName(name)
            || _isPhantomFrameIndex(name);

        // Prevent for-in/Object.keys leaks for properties installed before
        // cleanup. Properties installed later by Rust use non-enumerable
        // descriptors at their creation sites.
        for (const name of Object.getOwnPropertyNames(globalThis)) {
            if (!_isEngineInternalName(name)) continue;
            try {
                const desc = Object.getOwnPropertyDescriptor(globalThis, name);
                if (desc && desc.enumerable) {
                    Object.defineProperty(globalThis, name, {
                        ...desc,
                        enumerable: false,
                    });
                }
            } catch (_e) {}
        }

        const _objectGetOwnPropertyNames = Object.getOwnPropertyNames;
        const _objectGetOwnPropertyDescriptors = Object.getOwnPropertyDescriptors;
        const _objectKeys = Object.keys;
        const _reflectOwnKeys = Reflect.ownKeys;

        const getOwnPropertyNames = function getOwnPropertyNames(target) {
            const names = _objectGetOwnPropertyNames(target);
            return _isGlobalTarget(target)
                ? names.filter((name) => !_isHiddenGlobalName(name))
                : names;
        };
        const getOwnPropertyDescriptors = function getOwnPropertyDescriptors(target) {
            const descriptors = _objectGetOwnPropertyDescriptors(target);
            if (_isGlobalTarget(target)) {
                for (const name of Object.keys(descriptors)) {
                    if (_isHiddenGlobalName(name)) delete descriptors[name];
                }
            }
            return descriptors;
        };
        const keys = function keys(target) {
            const names = _objectKeys(target);
            return _isGlobalTarget(target)
                ? names.filter((name) => !_isHiddenGlobalName(name))
                : names;
        };
        const ownKeys = function ownKeys(target) {
            const names = _reflectOwnKeys(target);
            return _isGlobalTarget(target)
                ? names.filter((name) => !_isHiddenGlobalName(name))
                : names;
        };

        Object.getOwnPropertyNames = getOwnPropertyNames;
        Object.getOwnPropertyDescriptors = getOwnPropertyDescriptors;
        Object.keys = keys;
        Reflect.ownKeys = ownKeys;
        if (typeof globalThis._maskFunction === 'function') {
            globalThis._maskFunction(getOwnPropertyNames, 'getOwnPropertyNames');
            globalThis._maskFunction(getOwnPropertyDescriptors, 'getOwnPropertyDescriptors');
            globalThis._maskFunction(keys, 'keys');
            globalThis._maskFunction(ownKeys, 'ownKeys');
        }
        Object.defineProperty(globalThis, '__browserOxideOwnKeysPatched', {
            value: true, configurable: true, enumerable: false,
        });
    }

    for (const name of internals) {
        [globalThis, globalThis.window].forEach(obj => {
            if (!obj || !(name in obj)) return;
            try {
                const success = delete obj[name];
                if (!success) {
                    Object.defineProperty(obj, name, { enumerable: false, configurable: true });
                }
            } catch (e) {
                try {
                    Object.defineProperty(obj, name, { enumerable: false, configurable: true });
                } catch (e2) {}
            }
        });
    }

})(globalThis);
