/**
 * Network API WebIDL surface alignment.
 *
 * The transport implementations live in window_bootstrap.js. This layer
 * only restores Chromium's public prototype hierarchy/descriptors while
 * preserving those existing fetch/WebSocket operations.
 */
((globalThis) => {
    const _reflectOwnKeysRaw = Reflect.ownKeys;
    const _reflectGetOwnPropertyDescriptorRaw = Reflect.getOwnPropertyDescriptor;
    const _rawInstances = new WeakMap();
    const _raw = value => _rawInstances.get(value) || value;

    const _hideImplementationSlots = (target, readonlyNames = []) => {
        const hidden = new Set(_reflectOwnKeysRaw(target));
        const readonly = new Set(readonlyNames);
        const eventMethodCache = new Map();
        const proxy = new Proxy(target, {
            ownKeys(obj) {
                return _reflectOwnKeysRaw(obj).filter(key => {
                    if (!hidden.has(key)) return true;
                    const descriptor = _reflectGetOwnPropertyDescriptorRaw(obj, key);
                    return descriptor && !descriptor.configurable;
                });
            },
            getOwnPropertyDescriptor(obj, key) {
                const descriptor = _reflectGetOwnPropertyDescriptorRaw(obj, key);
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
                if (hidden.has(key)) {
                    let proto = Object.getPrototypeOf(obj);
                    while (proto) {
                        const descriptor = _reflectGetOwnPropertyDescriptorRaw(proto, key);
                        if (descriptor) {
                            if (typeof descriptor.get === 'function') {
                                return descriptor.get.call(receiver);
                            }
                            break;
                        }
                        proto = Object.getPrototypeOf(proto);
                    }
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
        for (const name of ['onopen','onmessage','onerror','onclose']) _handler(P, name);

        const _wsBinaryType = new WeakMap();
        Object.defineProperty(P, 'binaryType', {
            get: _native(function() {
                return _wsBinaryType.get(_raw(this)) || 'blob';
            }, 'get binaryType'),
            set: _native(function(value) {
                const normalized = String(value);
                if (normalized === 'blob' || normalized === 'arraybuffer') {
                    _wsBinaryType.set(_raw(this), normalized);
                }
            }, 'set binaryType'),
            enumerable: true,
            configurable: true,
        });

        const oldSend = P.send;
        const oldClose = P.close;
        _method(P, 'send', 1, function(data) {
            const raw = _raw(this);
            if (raw.readyState === 0) {
                throw new DOMException(
                    "Failed to execute 'send' on 'WebSocket': Still in CONNECTING state.",
                    'InvalidStateError',
                );
            }
            return oldSend.call(raw, data);
        });
        _method(P, 'close', 0, function(code, reason) {
            const raw = _raw(this);
            if (arguments.length > 0 && code !== undefined) {
                const numeric = Number(code);
                const converted = Number.isFinite(numeric) ? (Math.trunc(numeric) & 0xffff) : 0;
                if (converted !== 1000 && (converted < 3000 || converted > 4999)) {
                    throw new DOMException(
                        `Failed to execute 'close' on 'WebSocket': The close code must be either 1000, or between 3000 and 4999. ${converted} is neither.`,
                        'InvalidAccessError',
                    );
                }
                code = converted;
            }
            if (arguments.length > 1) {
                reason = String(reason);
                if (new TextEncoder().encode(reason).byteLength > 123) {
                    throw new DOMException(
                        "Failed to execute 'close' on 'WebSocket': The close reason must not be greater than 123 UTF-8 bytes.",
                        'SyntaxError',
                    );
                }
            }
            return oldClose.call(raw, code, reason);
        });
        Object.defineProperty(P, Symbol.toStringTag, { value: 'WebSocket', configurable: true });

        const _normalizeWebSocketUrl = (input) => {
            const source = String(input);
            if (!/^[A-Za-z][A-Za-z0-9+.-]*:/.test(source)) {
                throw new DOMException(
                    `Failed to construct 'WebSocket': The URL '${source}' is invalid.`,
                    'SyntaxError',
                );
            }
            let parsed;
            try { parsed = new URL(source); } catch (_) {
                throw new DOMException(
                    `Failed to construct 'WebSocket': The URL '${source}' is invalid.`,
                    'SyntaxError',
                );
            }
            if (parsed.hash) {
                throw new DOMException(
                    `Failed to construct 'WebSocket': The URL contains a fragment identifier ('${parsed.hash.slice(1)}'). Fragment identifiers are not allowed in WebSocket URLs.`,
                    'SyntaxError',
                );
            }
            if (parsed.protocol === 'http:') parsed.protocol = 'ws:';
            else if (parsed.protocol === 'https:') parsed.protocol = 'wss:';
            else if (parsed.protocol !== 'ws:' && parsed.protocol !== 'wss:') {
                throw new DOMException(
                    `Failed to construct 'WebSocket': The URL's scheme must be either 'http', 'https', 'ws', or 'wss'. '${parsed.protocol.slice(0, -1)}' is not allowed.`,
                    'SyntaxError',
                );
            }
            return parsed.href;
        };
        const _normalizeProtocols = (protocols) => {
            if (protocols === undefined) return undefined;
            const values = typeof protocols === 'string' ? [protocols] : Array.from(protocols);
            const seen = new Set();
            const token = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
            return values.map((value) => {
                const protocol = String(value);
                if (!token.test(protocol)) {
                    throw new DOMException(
                        `Failed to construct 'WebSocket': The subprotocol '${protocol}' is invalid.`,
                        'SyntaxError',
                    );
                }
                if (seen.has(protocol)) {
                    throw new DOMException(
                        `Failed to construct 'WebSocket': The subprotocol '${protocol}' is duplicated.`,
                        'SyntaxError',
                    );
                }
                seen.add(protocol);
                return protocol;
            });
        };

        const Public = _publicConstructor('WebSocket', 1, Impl, function(url) {
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'WebSocket': 1 argument required, but only 0 present.");
            }
            const normalizedUrl = _normalizeWebSocketUrl(url);
            const protocols = _normalizeProtocols(arguments[1]);
            const target = protocols === undefined
                ? new Impl(normalizedUrl)
                : new Impl(normalizedUrl, protocols);
            _wsBinaryType.set(target, 'blob');
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
            'readyState','response','responseURL','responseXML',
            'status','statusText','upload',
        ]) _accessor(P, name, false);
        Object.defineProperty(P, 'responseText', {
            get: _native(function() {
                const raw = _raw(this);
                const responseType = String(raw.responseType || '');
                if (responseType !== '' && responseType !== 'text') {
                    throw new DOMException(
                        `Failed to read the 'responseText' property from 'XMLHttpRequest': The value is only accessible if the object's 'responseType' is '' or 'text' (was '${responseType}').`,
                        'InvalidStateError',
                    );
                }
                const own = Object.getOwnPropertyDescriptor(raw, 'responseText');
                return own && 'value' in own ? own.value : '';
            }, 'get responseText'),
            set: undefined,
            enumerable: true,
            configurable: true,
        });
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
