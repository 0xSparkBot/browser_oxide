// WHATWG Structured Clone Algorithm.
//
// Implements `globalThis.structuredClone(value, options?)` per
// https://html.spec.whatwg.org/multipage/structured-data.html#structured-cloning
//
// Supported types: primitives (number, string, boolean, null, undefined,
// bigint), Date, RegExp, Map, Set, Array, plain Object, ArrayBuffer,
// all TypedArray variants + DataView, Blob (via our Blob class). Cycles
// are preserved via a WeakMap of seen → cloned objects.
//
// Throws `DOMException("...", "DataCloneError")` on functions, symbols,
// and DOM nodes. Throws for anything with a Proxy trap that rejects the
// read (we don't try to defeat the proxy).
//
// This is used by:
//   1. `Worker.postMessage(msg)` — serializes before the thread hop so
//      the receiver sees a real deep copy, not a JSON round-trip that
//      loses TypedArrays / Dates / Maps.
//   2. `IndexedDB` stored values — values must survive put→get cycles.
//   3. Any site code calling `structuredClone()` directly.

((globalThis) => {
    // Wire serialization (serializeForWire / deserializeFromWire) must always
    // be registered for Worker.postMessage to survive the JSON thread-hop.
    // These are independent of whether structuredClone is natively present.
    // The structuredClone polyfill guard is applied separately below.

    const TAG = "__boxsc";

    function _b64encodeBytes(u8) {
        let bin = "";
        for (let i = 0; i < u8.length; i++) bin += String.fromCharCode(u8[i]);
        return (typeof globalThis.btoa === "function"
            ? globalThis.btoa(bin)
            : bin);
    }

    function _b64decodeToUint8(str) {
        const bin =
            typeof globalThis.atob === "function"
                ? globalThis.atob(str)
                : str;
        const out = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
        return out;
    }

    // Keep direct references to the engine bridge objects. cleanup_bootstrap
    // intentionally removes their global names from page-observable scope,
    // but structured-clone/Worker deserialization still needs the hooks that
    // realm-specific MessagePort bootstrap code installs onto those objects.
    let _bridgeRefs = [];

    function _messagePortHooks() {
        for (const bridge of _bridgeRefs) {
            if (bridge && bridge.messagePortHooks) return bridge.messagePortHooks;
        }
        const live = [globalThis._browser_oxide, globalThis.__browser_oxide];
        for (const bridge of live) {
            if (bridge && bridge.messagePortHooks) return bridge.messagePortHooks;
        }
        return null;
    }

    function _serializeForWire(value, seen, transferState) {
        if (value === null) return null;
        const t = typeof value;
        if (t === "undefined") return { [TAG]: "undefined" };
        if (t === "bigint") return { [TAG]: "bigint", v: value.toString() };
        if (t === "number" || t === "string" || t === "boolean") return value;
        if (t === "function" || t === "symbol") {
            const err = new Error(t + " values cannot be transferred via postMessage");
            err.name = "DataCloneError";
            throw err;
        }
        seen = seen || new WeakMap();
        if (seen.has(value)) return null;
        const portHooks = _messagePortHooks();
        if (portHooks && portHooks.isMessagePort(value)) {
            const endpointId = transferState && transferState.portIds
                ? transferState.portIds.get(value)
                : 0;
            if (!endpointId) {
                const method = transferState?.method || "postMessage";
                const iface = transferState?.iface || "Worker";
                throw _dataCloneError(
                    `Failed to execute '${method}' on '${iface}': A MessagePort could not be cloned because it was not transferred.`
                );
            }
            return { [TAG]: "MessagePort", id: endpointId };
        }
        if (value instanceof Date) {
            return { [TAG]: "Date", v: value.getTime() };
        }
        if (value instanceof RegExp) {
            return { [TAG]: "RegExp", source: value.source, flags: value.flags };
        }
        if (value instanceof ArrayBuffer) {
            const u8 = new Uint8Array(value);
            return { [TAG]: "ArrayBuffer", b: _b64encodeBytes(u8) };
        }
        if (ArrayBuffer.isView(value) && !(value instanceof DataView)) {
            const u8 = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
            return { [TAG]: "TypedArray", ctor: value.constructor.name, b: _b64encodeBytes(u8) };
        }
        if (value instanceof DataView) {
            const u8 = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
            return { [TAG]: "DataView", b: _b64encodeBytes(u8) };
        }
        if (value instanceof Map) {
            seen.set(value, true);
            const entries = [];
            for (const [k, v] of value) {
                entries.push([
                    _serializeForWire(k, seen, transferState),
                    _serializeForWire(v, seen, transferState),
                ]);
            }
            return { [TAG]: "Map", entries };
        }
        if (value instanceof Set) {
            seen.set(value, true);
            const items = [];
            for (const v of value) items.push(_serializeForWire(v, seen, transferState));
            return { [TAG]: "Set", items };
        }
        if (Array.isArray(value)) {
            seen.set(value, true);
            const out = new Array(value.length);
            for (let i = 0; i < value.length; i++) {
                out[i] = _serializeForWire(value[i], seen, transferState);
            }
            return out;
        }
        seen.set(value, true);
        const out = {};
        for (const key of Object.keys(value)) {
            out[key] = _serializeForWire(value[key], seen, transferState);
        }
        return out;
    }

    function _deserializeFromWire(value, portCache) {
        if (value === null) return null;
        const t = typeof value;
        if (t === "number" || t === "string" || t === "boolean") return value;
        if (t !== "object") return value;
        if (Array.isArray(value)) {
            const out = new Array(value.length);
            for (let i = 0; i < value.length; i++) {
                out[i] = _deserializeFromWire(value[i], portCache);
            }
            return out;
        }
        const tag = value[TAG];
        if (tag) {
            switch (tag) {
                case "undefined": return undefined;
                case "bigint": return BigInt(value.v);
                case "Date": return new Date(value.v);
                case "RegExp": return new RegExp(value.source, value.flags);
                case "ArrayBuffer": {
                    const u8 = _b64decodeToUint8(value.b);
                    const ab = new ArrayBuffer(u8.byteLength);
                    new Uint8Array(ab).set(u8);
                    return ab;
                }
                case "TypedArray": {
                    const u8 = _b64decodeToUint8(value.b);
                    const ab = new ArrayBuffer(u8.byteLength);
                    new Uint8Array(ab).set(u8);
                    const Ctor = globalThis[value.ctor] || Uint8Array;
                    try { return new Ctor(ab); } catch (_) { return new Uint8Array(ab); }
                }
                case "DataView": {
                    const u8 = _b64decodeToUint8(value.b);
                    const ab = new ArrayBuffer(u8.byteLength);
                    new Uint8Array(ab).set(u8);
                    return new DataView(ab);
                }
                case "MessagePort": {
                    const endpointId = Number(value.id) || 0;
                    if (!endpointId) return null;
                    portCache = portCache || new Map();
                    if (portCache.has(endpointId)) return portCache.get(endpointId);
                    const hooks = _messagePortHooks();
                    if (!hooks || typeof hooks.adopt !== "function") return null;
                    const port = hooks.adopt(endpointId);
                    portCache.set(endpointId, port);
                    return port;
                }
                case "Map": {
                    const m = new Map();
                    for (const [k, v] of value.entries) {
                        m.set(
                            _deserializeFromWire(k, portCache),
                            _deserializeFromWire(v, portCache),
                        );
                    }
                    return m;
                }
                case "Set": {
                    const s = new Set();
                    for (const v of value.items) s.add(_deserializeFromWire(v, portCache));
                    return s;
                }
                default: break;
            }
        }
        const out = {};
        for (const key of Object.keys(value)) {
            out[key] = _deserializeFromWire(value[key], portCache);
        }
        return out;
    }

    // Install wire-serialization onto BOTH the long-lived `_browser_oxide` bag
    // (single underscore, owned by window_bootstrap.js — survives the
    // cleanup pass) AND the legacy `__browser_oxide` (double underscore, deleted
    // by cleanup_bootstrap.js's internals purge). window_bootstrap's
    // Worker class captures the single-underscore object via closure, so
    // installing onto it lets the Worker postMessage / onmessage path
    // round-trip ArrayBuffer / TypedArray / Map / Set / Date / RegExp
    // payloads. The double-underscore copy stays for any code that
    // captured a reference to it before cleanup runs (e.g.
    // worker_bootstrap.js's `const _browser_oxide = globalThis.__browser_oxide;`).
    if (!globalThis.__browser_oxide) globalThis.__browser_oxide = {};
    _bridgeRefs = [globalThis._browser_oxide, globalThis.__browser_oxide]
        .filter(Boolean);
    globalThis.__browser_oxide.serializeForWire = _serializeForWire;
    globalThis.__browser_oxide.deserializeFromWire = _deserializeFromWire;
    if (globalThis._browser_oxide) {
        globalThis._browser_oxide.serializeForWire = _serializeForWire;
        globalThis._browser_oxide.deserializeFromWire = _deserializeFromWire;
    }

    // Lazy lookup of the DOMException constructor. window_bootstrap.js
    // installs it, so by the time structuredClone runs at page load
    // it exists; but during bootstrap execution order it may not.
    function _dataCloneError(msg) {
        try {
            return new DOMException(msg, "DataCloneError");
        } catch (_e) {
            const err = new Error(msg);
            err.name = "DataCloneError";
            return err;
        }
    }

    // V8's serializer owns the only correct ArrayBuffer-detach primitive in
    // this runtime. Capture it before cleanup hides Deno from page code. We do
    // NOT use it to clone the whole value because BrowserOxide has JS-backed
    // host objects (Blob/File/etc.) whose state is intentionally not exposed as
    // own properties. Instead, the normal clone runs first and V8 is used only
    // to detach the explicitly transferred backing stores.
    const _core = globalThis.Deno && globalThis.Deno.core;
    const _coreSerialize = _core && typeof _core.serialize === "function"
        ? (value, options) => _core.serialize(value, options)
        : null;
    const _coreDeserialize = _core && typeof _core.deserialize === "function"
        ? (value, options) => _core.deserialize(value, options)
        : null;
    const _coreIsArrayBuffer = _core && typeof _core.isArrayBuffer === "function"
        ? (value) => _core.isArrayBuffer(value)
        : (value) => Object.prototype.toString.call(value) === "[object ArrayBuffer]";

    function _transferSequenceTypeError(method, iface) {
        if (method === "structuredClone" && iface === "Window") {
            return new TypeError(
                "Failed to execute 'structuredClone' on 'Window': Failed to read the 'transfer' property from 'StructuredSerializeOptions': The provided value cannot be converted to a sequence."
            );
        }
        return new TypeError(
            `Failed to execute '${method}' on '${iface}': The provided value cannot be converted to a sequence.`
        );
    }

    function _normalizeTransferList(input, method, iface) {
        if (input === undefined) return [];
        let iterator;
        try { iterator = input != null ? input[Symbol.iterator] : undefined; }
        catch (e) { throw e; }
        if (typeof iterator !== "function") {
            throw _transferSequenceTypeError(method, iface);
        }
        const out = [];
        const seen = new Set();
        const portHooks = _messagePortHooks();
        let index = 0;
        for (const value of input) {
            const isArrayBuffer = _coreIsArrayBuffer(value);
            const isMessagePort = !!(
                portHooks
                && typeof portHooks.isMessagePort === "function"
                && portHooks.isMessagePort(value)
            );
            if (!isArrayBuffer && !isMessagePort) {
                throw _dataCloneError(
                    `Failed to execute '${method}' on '${iface}': Value at index ${index} does not have a transferable type.`
                );
            }
            if (seen.has(value)) {
                const detail = isMessagePort
                    ? `Message port at index ${index} is a duplicate of an earlier port.`
                    : `ArrayBuffer at index ${index} is a duplicate of an earlier ArrayBuffer.`;
                throw _dataCloneError(
                    `Failed to execute '${method}' on '${iface}': ${detail}`
                );
            }
            seen.add(value);
            out.push(value);
            index++;
        }
        return out;
    }

    function _detachTransferList(list) {
        if (!list || list.length === 0) return;
        const buffers = list.filter((value) => _coreIsArrayBuffer(value));
        if (buffers.length === 0) return;
        if (!_coreSerialize || !_coreDeserialize) {
            throw _dataCloneError("ArrayBuffer transfer is not available in this runtime.");
        }
        // op_serialize mutates transferredArrayBuffers entries into backing-
        // store ids. Use a private copy so the caller's sequence is untouched;
        // immediately deserialize to consume those ids from the shared store.
        const slots = buffers.slice();
        const wire = _coreSerialize(slots, { transferredArrayBuffers: slots });
        _coreDeserialize(wire, { transferredArrayBuffers: slots });
    }

    function _prepareTransferState(input, method, iface) {
        const list = _normalizeTransferList(input, method, iface);
        const portHooks = _messagePortHooks();
        const portIds = new WeakMap();
        const portRecords = [];
        for (const value of list) {
            if (_coreIsArrayBuffer(value)) continue;
            if (!portHooks || typeof portHooks.prepareTransfer !== "function") {
                throw _dataCloneError(
                    `Failed to execute '${method}' on '${iface}': Value does not have a transferable type.`
                );
            }
            const endpointId = portHooks.prepareTransfer(value);
            if (!endpointId) {
                throw _dataCloneError(
                    `Failed to execute '${method}' on '${iface}': MessagePort could not be transferred.`
                );
            }
            portIds.set(value, endpointId);
            portRecords.push({ port: value, id: endpointId });
        }
        return {
            list,
            method,
            iface,
            portIds,
            portRecords,
            placeholders: [],
            committed: false,
        };
    }

    function _commitPreparedTransfers(state) {
        if (!state || state.committed) return;
        _detachTransferList(state.list);
        const portHooks = _messagePortHooks();
        for (const record of state.portRecords) {
            if (!portHooks || typeof portHooks.commitTransfer !== "function"
                || !portHooks.commitTransfer(record.port)) {
                throw _dataCloneError(
                    `Failed to execute '${state.method}' on '${state.iface}': MessagePort could not be transferred.`
                );
            }
        }
        state.committed = true;
        if (portHooks && typeof portHooks.finalizePlaceholder === "function") {
            for (const placeholder of state.placeholders) {
                portHooks.finalizePlaceholder(placeholder);
            }
        }
    }

    function _prepareWireMessage(value, transfer, method, iface) {
        const transferState = _prepareTransferState(transfer, method, iface);
        const data = _serializeForWire(value, new WeakMap(), transferState);
        return {
            data,
            ports: transferState.portRecords.map((record) => record.id),
            transferState,
        };
    }

    function _adoptTransferredPorts(endpointIds, cache) {
        const hooks = _messagePortHooks();
        if (!hooks || typeof hooks.adopt !== "function") return [];
        const portCache = cache || new Map();
        const out = [];
        for (const rawId of endpointIds || []) {
            const endpointId = Number(rawId) || 0;
            if (!endpointId) continue;
            let port = portCache.get(endpointId);
            if (!port) {
                port = hooks.adopt(endpointId);
                portCache.set(endpointId, port);
            }
            out.push(port);
        }
        return out;
    }

    function _transferArrayBuffers(input, method, iface) {
        const list = _normalizeTransferList(input, method, iface);
        _detachTransferList(list);
        return list;
    }

    for (const bridge of [globalThis.__browser_oxide, globalThis._browser_oxide]) {
        if (!bridge) continue;
        try {
            Object.defineProperty(bridge, "normalizeTransferList", {
                value: _normalizeTransferList,
                writable: false,
                enumerable: false,
                configurable: true,
            });
            Object.defineProperty(bridge, "detachTransferList", {
                value: _detachTransferList,
                writable: false,
                enumerable: false,
                configurable: true,
            });
            Object.defineProperty(bridge, "transferArrayBuffers", {
                value: _transferArrayBuffers,
                writable: false,
                enumerable: false,
                configurable: true,
            });
            Object.defineProperty(bridge, "prepareWireMessage", {
                value: _prepareWireMessage,
                writable: false,
                enumerable: false,
                configurable: true,
            });
            Object.defineProperty(bridge, "commitPreparedTransfers", {
                value: _commitPreparedTransfers,
                writable: false,
                enumerable: false,
                configurable: true,
            });
            Object.defineProperty(bridge, "adoptTransferredPorts", {
                value: _adoptTransferredPorts,
                writable: false,
                enumerable: false,
                configurable: true,
            });
        } catch (_) {}
    }

    // structuredClone polyfill — only install if V8 doesn't provide it natively.
    if (typeof globalThis.structuredClone === "function") {
        return;
    }

    function _isTypedArray(v) {
        return ArrayBuffer.isView(v) && !(v instanceof DataView);
    }

    function clone(value, seen, transferState) {
        // Primitives + null + undefined — return as-is.
        if (value === null) return null;
        const t = typeof value;
        if (t === "undefined" || t === "number" || t === "string" || t === "boolean" || t === "bigint") {
            return value;
        }
        if (t === "function" || t === "symbol") {
            throw _dataCloneError(
                t + " values cannot be serialized by structuredClone"
            );
        }
        // From here on, `value` is an object.
        if (seen.has(value)) {
            return seen.get(value);
        }

        const portHooks = _messagePortHooks();
        if (portHooks && portHooks.isMessagePort(value)) {
            const endpointId = transferState && transferState.portIds
                ? transferState.portIds.get(value)
                : 0;
            if (!endpointId) {
                const method = transferState?.method || "structuredClone";
                const iface = transferState?.iface || "Window";
                throw _dataCloneError(
                    `Failed to execute '${method}' on '${iface}': A MessagePort could not be cloned because it was not transferred.`
                );
            }
            const placeholder = portHooks.createPlaceholder(endpointId);
            seen.set(value, placeholder);
            transferState.placeholders.push(placeholder);
            return placeholder;
        }

        // Date — clone with same time value.
        if (value instanceof Date) {
            const c = new Date(value.getTime());
            seen.set(value, c);
            return c;
        }
        // RegExp — preserve source and flags, reset lastIndex to 0 per spec.
        if (value instanceof RegExp) {
            const c = new RegExp(value.source, value.flags);
            seen.set(value, c);
            return c;
        }
        // ArrayBuffer — copy all bytes.
        if (value instanceof ArrayBuffer) {
            const c = value.slice(0);
            seen.set(value, c);
            return c;
        }
        // DataView — clone the underlying buffer and construct a new view
        // over the same byte range.
        if (value instanceof DataView) {
            const bufCopy = clone(value.buffer, seen, transferState);
            const c = new DataView(bufCopy, value.byteOffset, value.byteLength);
            seen.set(value, c);
            return c;
        }
        // TypedArray — clone the backing buffer through `seen` so sibling
        // views and an explicitly cloned `.buffer` preserve their shared
        // backing-store identity, as the structured clone algorithm requires.
        if (_isTypedArray(value)) {
            const bufCopy = clone(value.buffer, seen, transferState);
            const c = new value.constructor(bufCopy, value.byteOffset, value.length);
            seen.set(value, c);
            return c;
        }
        // Map — preserve insertion order, clone keys and values.
        if (value instanceof Map) {
            const c = new Map();
            seen.set(value, c);
            for (const [k, v] of value) {
                c.set(
                    clone(k, seen, transferState),
                    clone(v, seen, transferState),
                );
            }
            return c;
        }
        // Set — same idea.
        if (value instanceof Set) {
            const c = new Set();
            seen.set(value, c);
            for (const v of value) {
                c.add(clone(v, seen, transferState));
            }
            return c;
        }
        // Array — clone in order. Sparse arrays preserve holes via
        // `i in value` tests (real Chrome does this).
        if (Array.isArray(value)) {
            const c = new Array(value.length);
            seen.set(value, c);
            for (let i = 0; i < value.length; i++) {
                if (i in value) {
                    c[i] = clone(value[i], seen, transferState);
                }
            }
            return c;
        }
        // Blob — copy the underlying bytes + type. `new Blob([blob])`
        // clones the contents but drops .type, so we reattach it.
        if (typeof Blob !== "undefined" && value instanceof Blob) {
            const c = new Blob([value], { type: value.type || "" });
            seen.set(value, c);
            return c;
        }
        // Error objects — structured clone preserves name + message;
        // stack is implementation-defined (Chrome preserves it, we do too).
        if (value instanceof Error) {
            const Ctor = value.constructor || Error;
            let c;
            try {
                c = new Ctor(value.message);
            } catch (_e) {
                c = new Error(value.message);
            }
            c.name = value.name;
            if (value.stack) c.stack = value.stack;
            seen.set(value, c);
            return c;
        }
        // DOM nodes / Windows / other host objects — DataCloneError.
        // We detect the most common host types by name or internal slots.
        const ctorName = value.constructor && value.constructor.name;
        if (
            ctorName &&
            (ctorName.startsWith("HTML") ||
                ctorName === "Node" ||
                ctorName === "Element" ||
                ctorName === "Document" ||
                ctorName === "Window" ||
                ctorName === "PluginArray" ||
                ctorName === "MimeTypeArray" ||
                ctorName === "Plugin" ||
                ctorName === "MimeType" ||
                ctorName === "AudioContext" ||
                ctorName === "OfflineAudioContext" ||
                ctorName === "BaseAudioContext" ||
                ctorName === "AudioNode" ||
                ctorName === "AudioParam")
        ) {
            throw _dataCloneError(
                `Failed to execute 'structuredClone' on 'Window': ${ctorName} object could not be cloned.`
            );
        }
        // Plain object — enumerable own string keys, in insertion order
        // (Object.keys matches Chrome's behaviour for plain objects).
        // Symbols are NOT cloned (spec).
        const proto = Object.getPrototypeOf(value);
        if (proto !== null && proto !== Object.prototype) {
            // Subclasses of Object / instances with a custom prototype:
            // the spec says to still clone the own enumerable string
            // properties as a plain object, discarding the prototype.
            // Real Chrome follows this for simple cases; anything more
            // exotic (Proxy, getters throwing, etc.) falls through to
            // the same path.
        }
        const c = {};
        seen.set(value, c);
        for (const key of Object.keys(value)) {
            c[key] = clone(value[key], seen, transferState);
        }
        return c;
    }

    globalThis.structuredClone = function structuredClone(value, options) {
        const transferInput = options == null ? undefined : options.transfer;
        const transferState = _prepareTransferState(
            transferInput,
            "structuredClone",
            "Window"
        );
        const result = clone(value, new WeakMap(), transferState);
        _commitPreparedTransfers(transferState);
        return result;
    };
    // Mask as native — some scripts inspect the toString() of built-ins
    // for raw JS bodies of polyfills. Without this, `structuredClone
    // .toString()` returns the function source and identifies the engine
    // as non-Chrome. Helpers are registered in stealth_bootstrap.js.
    try {
        if (typeof globalThis._maskFunction === 'function') {
            globalThis._maskFunction(globalThis.structuredClone, 'structuredClone');
        }
    } catch (_e) {}

})(globalThis);
