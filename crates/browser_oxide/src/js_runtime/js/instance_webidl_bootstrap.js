// Instance-shape normalization for Web Platform objects whose behavior is
// already implemented by earlier bootstraps.  Keep implementation state in
// WeakMaps so normal instances do not expose engine-private own properties.
((globalThis) => {
    const mask = (fn, name) => {
        if (typeof fn !== 'function') return fn;
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };
    const setLength = (fn, length) => {
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
    };
    const tag = (Ctor, name) => {
        if (!Ctor || !Ctor.prototype) return;
        try {
            Object.defineProperty(Ctor.prototype, Symbol.toStringTag, {
                value: name, configurable: true,
            });
        } catch (_) {}
    };
    const getter = (proto, name, get) => {
        setLength(get, 0); mask(get, `get ${name}`);
        Object.defineProperty(proto, name, {
            get, enumerable: true, configurable: true,
        });
    };
    const method = (proto, name, fn, length) => {
        setLength(fn, length); mask(fn, name);
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };

    tag(globalThis.DOMParser, 'DOMParser');
    tag(globalThis.FormData, 'FormData');
    tag(globalThis.URLSearchParams, 'URLSearchParams');
    tag(globalThis.TextEncoder, 'TextEncoder');

    // Window uses a non-global WindowProperties prototype between
    // Window.prototype and EventTarget.prototype. Keep Window.prototype itself
    // (its constructor's `prototype` property is non-writable/non-configurable)
    // and normalize the chain/order in place.
    if (globalThis.Window?.prototype && globalThis.EventTarget?.prototype) {
        const wp = globalThis.Window.prototype;
        const ctorDescriptor = Object.getOwnPropertyDescriptor(wp, 'constructor');
        const tagDescriptor = Object.getOwnPropertyDescriptor(wp, Symbol.toStringTag);
        const windowProperties = Object.create(globalThis.EventTarget.prototype);
        try {
            Object.defineProperty(windowProperties, Symbol.toStringTag, {
                value: 'WindowProperties', configurable: true,
            });
        } catch (_) {}
        try { Object.setPrototypeOf(wp, windowProperties); } catch (_) {}
        // TEMPORARY/PERSISTENT are non-configurable and already precede these
        // keys in Chrome. Reinsert the configurable members after them to match
        // Blink's own-key order: constants, constructor, @@toStringTag.
        try { delete wp.constructor; } catch (_) {}
        try { delete wp[Symbol.toStringTag]; } catch (_) {}
        if (ctorDescriptor) {
            try { Object.defineProperty(wp, 'constructor', ctorDescriptor); } catch (_) {}
        }
        try {
            Object.defineProperty(wp, Symbol.toStringTag, tagDescriptor || {
                value: 'Window', configurable: true,
            });
        } catch (_) {}
    }

    // TextDecoder: preserve the existing UTF-8 decode behavior while moving
    // label/fatal/BOM flags off the instance.
    if (typeof globalThis.TextDecoder === 'function' && globalThis.TextDecoder.prototype) {
        const Original = globalThis.TextDecoder;
        const proto = Original.prototype;
        const oldEncoding = Object.getOwnPropertyDescriptor(proto, 'encoding');
        const oldFatal = Object.getOwnPropertyDescriptor(proto, 'fatal');
        const oldIgnoreBOM = Object.getOwnPropertyDescriptor(proto, 'ignoreBOM');
        const states = new WeakMap();
        const stateOf = self => {
            let state = states.get(self);
            if (state) return state;
            state = {
                label: self && self._label !== undefined
                    ? String(self._label).toLowerCase()
                    : (oldEncoding?.get ? String(oldEncoding.get.call(self)).toLowerCase() : 'utf-8'),
                fatal: self && self._fatal !== undefined
                    ? !!self._fatal
                    : !!(oldFatal?.get && oldFatal.get.call(self)),
                ignoreBOM: self && self._ignoreBOM !== undefined
                    ? !!self._ignoreBOM
                    : !!(oldIgnoreBOM?.get && oldIgnoreBOM.get.call(self)),
            };
            states.set(self, state);
            return state;
        };
        function TextDecoder(label = 'utf-8', options = {}) {
            if (!new.target) {
                throw new TypeError(
                    "Failed to construct 'TextDecoder': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
                );
            }
            // Reuse the original constructor for label/options normalization,
            // then copy only semantic state into a clean instance.
            const temp = Reflect.construct(Original, [label, options], Original);
            const state = stateOf(temp);
            const obj = Object.create(new.target.prototype);
            states.set(obj, { ...state });
            return obj;
        }
        TextDecoder.prototype = proto;
        try { Object.setPrototypeOf(TextDecoder, Object.getPrototypeOf(Original)); } catch (_) {}
        setLength(TextDecoder, 0); mask(TextDecoder, 'TextDecoder');
        try {
            Object.defineProperty(proto, 'constructor', {
                value: TextDecoder, writable: true, enumerable: false, configurable: true,
            });
        } catch (_) {}
        getter(proto, 'encoding', function encoding() { return stateOf(this).label || 'utf-8'; });
        getter(proto, 'fatal', function fatal() { return stateOf(this).fatal; });
        getter(proto, 'ignoreBOM', function ignoreBOM() { return stateOf(this).ignoreBOM; });
        method(proto, 'decode', function decode(buf, _options) {
            if (buf === undefined) return '';
            const state = stateOf(this);
            let bytes;
            if (buf instanceof ArrayBuffer) bytes = new Uint8Array(buf);
            else if (ArrayBuffer.isView(buf)) bytes = new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
            else bytes = new Uint8Array(buf);
            let str = '';
            let i = 0;
            if (!state.ignoreBOM && bytes.length >= 3
                && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) i = 3;
            while (i < bytes.length) {
                const b0 = bytes[i];
                if (b0 < 0x80) { str += String.fromCharCode(b0); i++; }
                else if ((b0 & 0xe0) === 0xc0 && i + 1 < bytes.length) {
                    const cp = ((b0 & 0x1f) << 6) | (bytes[i + 1] & 0x3f);
                    str += String.fromCharCode(cp); i += 2;
                } else if ((b0 & 0xf0) === 0xe0 && i + 2 < bytes.length) {
                    const cp = ((b0 & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f);
                    str += String.fromCharCode(cp); i += 3;
                } else if ((b0 & 0xf8) === 0xf0 && i + 3 < bytes.length) {
                    let cp = ((b0 & 0x07) << 18) | ((bytes[i + 1] & 0x3f) << 12)
                        | ((bytes[i + 2] & 0x3f) << 6) | (bytes[i + 3] & 0x3f);
                    cp -= 0x10000;
                    str += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
                    i += 4;
                } else {
                    if (state.fatal) throw new TypeError('The encoded data was not valid.');
                    str += '\uFFFD'; i++;
                }
            }
            return str;
        }, 0);
        tag(TextDecoder, 'TextDecoder');
        globalThis.TextDecoder = TextDecoder;
    }

    // ResizeObserver: same headless callback policy as before, but no public
    // `_callback` / `_elements` fields.
    if (typeof globalThis.ResizeObserver === 'function' && globalThis.ResizeObserver.prototype) {
        const Original = globalThis.ResizeObserver;
        const proto = Original.prototype;
        const states = new WeakMap();
        function ResizeObserver(callback) {
            if (!new.target) {
                throw new TypeError(
                    "Failed to construct 'ResizeObserver': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
                );
            }
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'ResizeObserver': 1 argument required, but only 0 present.");
            }
            const obj = Object.create(new.target.prototype);
            states.set(obj, { callback, elements: new Set() });
            return obj;
        }
        ResizeObserver.prototype = proto;
        try { Object.setPrototypeOf(ResizeObserver, Object.getPrototypeOf(Original)); } catch (_) {}
        setLength(ResizeObserver, 1); mask(ResizeObserver, 'ResizeObserver');
        try {
            Object.defineProperty(proto, 'constructor', {
                value: ResizeObserver, writable: true, enumerable: false, configurable: true,
            });
        } catch (_) {}
        const get = self => {
            const state = states.get(self);
            if (!state) throw new TypeError('Illegal invocation');
            return state;
        };
        method(proto, 'observe', function observe(target) {
            const state = get(this);
            state.elements.add(target);
            Promise.resolve().then(() => {
                if (!state.elements.has(target)) return;
                const rect = target?.getBoundingClientRect
                    ? target.getBoundingClientRect()
                    : { x: 0, y: 0, width: 0, height: 0 };
                const entry = {
                    target,
                    contentRect: rect,
                    borderBoxSize: [{ inlineSize: target?.offsetWidth || 0, blockSize: target?.offsetHeight || 0 }],
                    contentBoxSize: [{ inlineSize: target?.offsetWidth || 0, blockSize: target?.offsetHeight || 0 }],
                };
                if (typeof state.callback === 'function') state.callback([entry], this);
            });
        }, 1);
        method(proto, 'unobserve', function unobserve(target) { get(this).elements.delete(target); }, 1);
        method(proto, 'disconnect', function disconnect() { get(this).elements.clear(); }, 0);
        tag(ResizeObserver, 'ResizeObserver');
        globalThis.ResizeObserver = ResizeObserver;
    }
})(globalThis);
