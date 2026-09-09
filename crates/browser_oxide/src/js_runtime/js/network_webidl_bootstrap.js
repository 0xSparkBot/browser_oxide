/**
 * Network API WebIDL surface alignment.
 *
 * The transport implementations live in window_bootstrap.js. This layer
 * only restores Chromium's public prototype hierarchy/descriptors while
 * preserving those existing fetch/WebSocket operations.
 */
((globalThis) => {
    const _rawInstances = new WeakMap();
    const _raw = value => _rawInstances.get(value) || value;

    const _hideImplementationSlots = (target, readonlyNames = []) => {
        const hidden = new Set(Reflect.ownKeys(target));
        const readonly = new Set(readonlyNames);
        const eventMethodCache = new Map();
        const proxy = new Proxy(target, {
            ownKeys(obj) {
                return Reflect.ownKeys(obj).filter(key => {
                    if (!hidden.has(key)) return true;
                    const descriptor = Reflect.getOwnPropertyDescriptor(obj, key);
                    return descriptor && !descriptor.configurable;
                });
            },
            getOwnPropertyDescriptor(obj, key) {
                const descriptor = Reflect.getOwnPropertyDescriptor(obj, key);
                if (hidden.has(key) && descriptor?.configurable) return undefined;
                return descriptor;
            },
            get(obj, key, receiver) {
                if (key === 'addEventListener' || key === 'removeEventListener' || key === 'dispatchEvent') {
                    if (!eventMethodCache.has(key)) {
                        const fn = Reflect.get(obj, key, obj);
                        eventMethodCache.set(key, function() { return fn.apply(obj, arguments); });
                    }
                    return eventMethodCache.get(key);
                }
                return Reflect.get(obj, key, receiver);
            },
            set(obj, key, value, receiver) {
                if (readonly.has(key)) return true;
                return Reflect.set(obj, key, value, receiver);
            },
            deleteProperty(obj, key) {
                if (hidden.has(key)) return true;
                return Reflect.deleteProperty(obj, key);
            },
        });
        _rawInstances.set(target, target);
        _rawInstances.set(proxy, target);
        return proxy;
    };
    const _native = (fn, name) => {
        try { Object.defineProperty(fn, 'name', { value: name, configurable: true }); } catch (_) {}
        try { if (typeof _maskFunction === 'function') _maskFunction(fn, name); } catch (_) {}
        return fn;
    };

    const _method = (proto, name, length, fn) => {
        _native(fn, name);
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };

    const _accessor = (proto, name, writable = false) => {
        const original = Object.getOwnPropertyDescriptor(proto, name);
        const get = _native(function() {
            const raw = _raw(this);
            const own = Object.getOwnPropertyDescriptor(raw, name);
            if (own && 'value' in own) return own.value;
            if (typeof original?.get === 'function') return original.get.call(raw);
            return undefined;
        }, `get ${name}`);
        let set;
        if (writable) {
            set = _native(function(value) {
                const raw = _raw(this);
                const own = Object.getOwnPropertyDescriptor(raw, name);
                if (own && 'value' in own) {
                    Reflect.set(raw, name, value, raw);
                } else if (typeof original?.set === 'function') {
                    original.set.call(raw, value);
                } else {
                    Object.defineProperty(raw, name, {
                        value, writable: true, enumerable: true, configurable: true,
                    });
                }
            }, `set ${name}`);
        }
        Object.defineProperty(proto, name, {
            get,
            set,
            enumerable: true,
            configurable: true,
        });
    };

    const _handlerState = new WeakMap();
    const _handler = (proto, name) => {
        const get = _native(function() {
            const raw = _raw(this);
            const own = Object.getOwnPropertyDescriptor(raw, name);
            if (own && 'value' in own) return typeof own.value === 'function' ? own.value : null;
            return _handlerState.get(raw)?.[name] ?? null;
        }, `get ${name}`);
        const set = _native(function(value) {
            const raw = _raw(this);
            const normalized = typeof value === 'function' ? value : null;
            const own = Object.getOwnPropertyDescriptor(raw, name);
            if (own && 'value' in own) {
                Reflect.set(raw, name, normalized, raw);
                return;
            }
            let state = _handlerState.get(raw);
            if (!state) { state = Object.create(null); _handlerState.set(raw, state); }
            state[name] = normalized;
        }, `set ${name}`);
        Object.defineProperty(proto, name, { get, set, enumerable: true, configurable: true });
    };

    const _constant = (ctor, proto, name, value) => {
        for (const target of [ctor, proto]) {
            try {
                Object.defineProperty(target, name, {
                    value, writable: false, enumerable: true, configurable: false,
                });
            } catch (_) {}
        }
    };

    const _publicConstructor = (name, length, Impl, create) => {
        const Public = _native(function() {
            if (!new.target) {
                throw new TypeError(`Failed to construct '${name}': Please use the 'new' operator, this DOM object constructor cannot be called as a function.`);
            }
            return create.apply(null, arguments);
        }, name);
        try { Object.defineProperty(Public, 'length', { value: length, configurable: true }); } catch (_) {}
        Object.defineProperty(Public, 'prototype', { value: Impl.prototype });
        Object.defineProperty(Impl.prototype, 'constructor', {
            value: Public, writable: true, enumerable: false, configurable: true,
        });
        return Public;
    };

    // WebSocket ------------------------------------------------------------
    if (typeof globalThis.WebSocket === 'function') {
        const Impl = globalThis.WebSocket;
        const P = Impl.prototype;
        try { Object.defineProperty(Impl, 'length', { value: 1, configurable: true }); } catch (_) {}
        for (const [name, value] of [['CONNECTING',0],['OPEN',1],['CLOSING',2],['CLOSED',3]]) {
            _constant(Impl, P, name, value);
        }
        for (const name of ['url','readyState','bufferedAmount','extensions','protocol']) _accessor(P, name, false);
        _accessor(P, 'binaryType', true);
        for (const name of ['onopen','onmessage','onerror','onclose']) _handler(P, name);
        const oldSend = P.send;
        const oldClose = P.close;
        _method(P, 'send', 1, function(data) { return oldSend.call(_raw(this), data); });
        _method(P, 'close', 0, function() { return oldClose.apply(_raw(this), arguments); });
        Object.defineProperty(P, Symbol.toStringTag, { value: 'WebSocket', configurable: true });

        const Public = _publicConstructor('WebSocket', 1, Impl, function(url) {
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'WebSocket': 1 argument required, but only 0 present.");
            }
            const target = new Impl(url, arguments[1]);
            return _hideImplementationSlots(target, [
                'url','readyState','bufferedAmount','extensions','protocol',
            ]);
        });
        for (const [name, value] of [['CONNECTING',0],['OPEN',1],['CLOSING',2],['CLOSED',3]]) {
            try {
                Object.defineProperty(Public, name, {
                    value, writable: false, enumerable: true, configurable: false,
                });
            } catch (_) {}
        }
        globalThis.WebSocket = Public;
    }

    // EventSource -----------------------------------------------------------
    if (typeof globalThis.EventSource === 'function') {
        const Impl = globalThis.EventSource;
        const P = Impl.prototype;
        try { Object.defineProperty(Impl, 'length', { value: 1, configurable: true }); } catch (_) {}
        for (const [name, value] of [['CONNECTING',0],['OPEN',1],['CLOSED',2]]) _constant(Impl, P, name, value);
        for (const name of ['url','readyState','withCredentials']) _accessor(P, name, false);
        for (const name of ['onopen','onmessage','onerror']) _handler(P, name);
        const oldClose = P.close;
        _method(P, 'close', 0, function() { return oldClose.call(_raw(this)); });
        Object.defineProperty(P, Symbol.toStringTag, { value: 'EventSource', configurable: true });

        const Public = _publicConstructor('EventSource', 1, Impl, function(url) {
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'EventSource': 1 argument required, but only 0 present.");
            }
            const target = new Impl(url, arguments[1]);
            return _hideImplementationSlots(target, ['url','readyState','withCredentials']);
        });
        for (const [name, value] of [['CONNECTING',0],['OPEN',1],['CLOSED',2]]) {
            try {
                Object.defineProperty(Public, name, {
                    value, writable: false, enumerable: true, configurable: false,
                });
            } catch (_) {}
        }
        globalThis.EventSource = Public;
    }

    // XMLHttpRequestEventTarget -------------------------------------------
    if (typeof globalThis.XMLHttpRequestEventTarget === 'function') {
        const C = globalThis.XMLHttpRequestEventTarget;
        const P = C.prototype;
        try { Object.setPrototypeOf(P, globalThis.EventTarget.prototype); } catch (_) {}
        for (const name of ['onloadstart','onprogress','onabort','onerror','onload','ontimeout','onloadend']) {
            _handler(P, name);
        }
        Object.defineProperty(P, Symbol.toStringTag, {
            value: 'XMLHttpRequestEventTarget', configurable: true,
        });

        if (typeof globalThis.XMLHttpRequestUpload === 'function') {
            try { Object.setPrototypeOf(globalThis.XMLHttpRequestUpload.prototype, P); } catch (_) {}
            try {
                Object.defineProperty(globalThis.XMLHttpRequestUpload.prototype, Symbol.toStringTag, {
                    value: 'XMLHttpRequestUpload', configurable: true,
                });
            } catch (_) {}
        }
    }

    // XMLHttpRequest -------------------------------------------------------
    if (typeof globalThis.XMLHttpRequest === 'function') {
        const Impl = globalThis.XMLHttpRequest;
        const P = Impl.prototype;
        const eventProto = globalThis.XMLHttpRequestEventTarget?.prototype;
        if (eventProto) {
            try { Object.setPrototypeOf(P, eventProto); } catch (_) {}
        }
        for (const [name, value] of [
            ['UNSENT',0],['OPENED',1],['HEADERS_RECEIVED',2],['LOADING',3],['DONE',4],
        ]) _constant(Impl, P, name, value);

        for (const name of [
            'readyState','response','responseText','responseURL','responseXML',
            'status','statusText','upload',
        ]) _accessor(P, name, false);
        for (const name of ['responseType','timeout','withCredentials']) _accessor(P, name, true);
        _handler(P, 'onreadystatechange');

        const originals = Object.create(null);
        for (const name of [
            'open','setRequestHeader','send','abort','getResponseHeader',
            'getAllResponseHeaders','overrideMimeType',
        ]) originals[name] = P[name];
        _method(P, 'open', 2, function(method, url) { return originals.open.apply(_raw(this), arguments); });
        _method(P, 'setRequestHeader', 2, function(name, value) { return originals.setRequestHeader.call(_raw(this), name, value); });
        _method(P, 'send', 0, function() { return originals.send.apply(_raw(this), arguments); });
        _method(P, 'abort', 0, function() { return originals.abort.call(_raw(this)); });
        _method(P, 'getResponseHeader', 1, function(name) { return originals.getResponseHeader.call(_raw(this), name); });
        _method(P, 'getAllResponseHeaders', 0, function() { return originals.getAllResponseHeaders.call(_raw(this)); });
        _method(P, 'overrideMimeType', 1, function(mime) { return originals.overrideMimeType.call(_raw(this), mime); });
        _method(P, 'setAttributionReporting', 1, function() {});
        _method(P, 'setPrivateToken', 1, function() {});
        Object.defineProperty(P, Symbol.toStringTag, { value: 'XMLHttpRequest', configurable: true });

        const Public = _publicConstructor('XMLHttpRequest', 0, Impl, function() {
            const target = new Impl();
            if (target.upload && typeof globalThis.XMLHttpRequestUpload === 'function') {
                try { Object.setPrototypeOf(target.upload, globalThis.XMLHttpRequestUpload.prototype); } catch (_) {}
                target.upload = _hideImplementationSlots(target.upload);
            }
            return _hideImplementationSlots(target, [
                'readyState','response','responseText','responseURL','responseXML',
                'status','statusText','upload',
            ]);
        });
        for (const [name, value] of [
            ['UNSENT',0],['OPENED',1],['HEADERS_RECEIVED',2],['LOADING',3],['DONE',4],
        ]) {
            try {
                Object.defineProperty(Public, name, {
                    value, writable: false, enumerable: true, configurable: false,
                });
            } catch (_) {}
        }
        globalThis.XMLHttpRequest = Public;
    }
})(globalThis);
