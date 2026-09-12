// CSS Custom Highlight API — Chrome 148 collection semantics.
((globalThis) => {
    if (!globalThis.CSS || typeof globalThis.CSS !== 'object') return;

    const mask = (fn, name) => {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };
    const getter = (name, read) => {
        const holder = { get [name]() { return read(this); } };
        return mask(Object.getOwnPropertyDescriptor(holder, name).get, `get ${name}`);
    };
    const setter = (name, write) => {
        const holder = { set [name](value) { write(this, value); } };
        return mask(Object.getOwnPropertyDescriptor(holder, name).set, `set ${name}`);
    };
    const method = (proto, name, length, fn) => {
        mask(fn, name);
        Object.defineProperty(fn, 'length', { value: length, configurable: true });
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
        return fn;
    };

    const highlightState = new WeakMap();
    const registryState = new WeakMap();

    function isAbstractRange(value) {
        if (!value || (typeof value !== 'object' && typeof value !== 'function')) return false;
        let tag = '';
        try { tag = Object.prototype.toString.call(value); } catch (_) {}
        return tag === '[object Range]' || tag === '[object StaticRange]';
    }
    function requireHighlight(value) {
        const state = highlightState.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    }
    function requireRegistry(value) {
        const state = registryState.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    }
    function rangeArg(value, methodName) {
        if (!isAbstractRange(value)) {
            throw new TypeError(
                `Failed to execute '${methodName}' on 'Highlight': parameter 1 is not of type 'AbstractRange'.`
            );
        }
        return value;
    }
    function domString(value, methodName) {
        if (typeof value === 'symbol') {
            throw new TypeError(
                `Failed to execute '${methodName}' on 'HighlightRegistry': Cannot convert a Symbol value to a string`
            );
        }
        return String(value);
    }
    function toLong(value) {
        let number = +value;
        if (!Number.isFinite(number) || number === 0) return 0;
        return Math.trunc(number) | 0;
    }

    const HighlightImpl = class Highlight {
        constructor(...ranges) {
            const set = new Set();
            for (let i = 0; i < ranges.length; i++) {
                if (!isAbstractRange(ranges[i])) {
                    throw new TypeError(
                        `Failed to construct 'Highlight': parameter ${i + 1} is not of type 'AbstractRange'.`
                    );
                }
                set.add(ranges[i]);
            }
            highlightState.set(this, { set, priority: 0, type: 'highlight' });
        }
    };
    const Highlight = new Proxy(HighlightImpl, {
        apply() {
            throw new TypeError(
                "Failed to construct 'Highlight': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        },
    });
    mask(Highlight, 'Highlight');
    const hp = HighlightImpl.prototype;
    delete hp.constructor;

    Object.defineProperty(hp, 'priority', {
        enumerable: true, configurable: true,
        get: getter('priority', self => requireHighlight(self).priority),
        set: setter('priority', (self, value) => { requireHighlight(self).priority = toLong(value); }),
    });
    Object.defineProperty(hp, 'type', {
        enumerable: true, configurable: true,
        get: getter('type', self => requireHighlight(self).type),
        set: setter('type', (self, value) => {
            if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol value to a string');
            const next = String(value);
            const state = requireHighlight(self);
            if (next === 'highlight' || next === 'spelling-error' || next === 'grammar-error') state.type = next;
        }),
    });
    Object.defineProperty(hp, 'size', {
        enumerable: true, configurable: true,
        get: getter('size', self => requireHighlight(self).set.size),
    });
    method(hp, 'add', 1, function add(range) {
        requireHighlight(this).set.add(rangeArg(range, 'add'));
        return this;
    });
    method(hp, 'clear', 0, function clear() { requireHighlight(this).set.clear(); });
    method(hp, 'delete', 1, function delete_(range) {
        return requireHighlight(this).set.delete(rangeArg(range, 'delete'));
    });
    method(hp, 'entries', 0, function entries() { return requireHighlight(this).set.entries(); });
    method(hp, 'forEach', 1, function forEach(callback, thisArg = undefined) {
        const state = requireHighlight(this);
        if (typeof callback !== 'function') {
            throw new TypeError("Failed to execute 'forEach' on 'Highlight': parameter 1 is not of type 'Function'.");
        }
        state.set.forEach(value => callback.call(thisArg, value, value, this));
    });
    method(hp, 'has', 1, function has(range) { return requireHighlight(this).set.has(rangeArg(range, 'has')); });
    method(hp, 'keys', 0, function keys() { return requireHighlight(this).set.keys(); });
    const hValues = method(hp, 'values', 0, function values() { return requireHighlight(this).set.values(); });
    Object.defineProperty(hp, 'constructor', {
        value: Highlight, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(hp, Symbol.toStringTag, {
        value: 'Highlight', writable: false, enumerable: false, configurable: true,
    });
    Object.defineProperty(hp, Symbol.iterator, {
        value: hValues, writable: true, enumerable: false, configurable: true,
    });

    const HighlightRegistryImpl = class HighlightRegistry {};
    const HighlightRegistry = new Proxy(HighlightRegistryImpl, {
        apply() { throw new TypeError('Illegal constructor'); },
        construct() { throw new TypeError("Failed to construct 'HighlightRegistry': Illegal constructor"); },
    });
    mask(HighlightRegistry, 'HighlightRegistry');
    const rp = HighlightRegistryImpl.prototype;
    delete rp.constructor;

    Object.defineProperty(rp, 'size', {
        enumerable: true, configurable: true,
        get: getter('size', self => requireRegistry(self).size),
    });
    method(rp, 'clear', 0, function clear() { requireRegistry(this).clear(); });
    method(rp, 'delete', 1, function delete_(name) { return requireRegistry(this).delete(domString(name, 'delete')); });
    const rEntries = method(rp, 'entries', 0, function entries() { return requireRegistry(this).entries(); });
    method(rp, 'forEach', 1, function forEach(callback, thisArg = undefined) {
        const state = requireRegistry(this);
        if (typeof callback !== 'function') {
            throw new TypeError("Failed to execute 'forEach' on 'HighlightRegistry': parameter 1 is not of type 'Function'.");
        }
        state.forEach((value, key) => callback.call(thisArg, value, key, this));
    });
    method(rp, 'get', 1, function get(name) { return requireRegistry(this).get(domString(name, 'get')); });
    method(rp, 'has', 1, function has(name) { return requireRegistry(this).has(domString(name, 'has')); });
    method(rp, 'keys', 0, function keys() { return requireRegistry(this).keys(); });
    method(rp, 'set', 2, function set(name, highlight) {
        const state = requireRegistry(this);
        const key = domString(name, 'set');
        if (!highlightState.has(highlight)) {
            throw new TypeError(
                "Failed to execute 'set' on 'HighlightRegistry': parameter 2 is not of type 'Highlight'."
            );
        }
        state.set(key, highlight);
        return this;
    });
    method(rp, 'values', 0, function values() { return requireRegistry(this).values(); });
    method(rp, 'highlightsFromPoint', 2, function highlightsFromPoint(x, y, options = undefined) {
        requireRegistry(this);
        +x; +y;
        if (options != null) Object(options);
        return [];
    });
    Object.defineProperty(rp, 'constructor', {
        value: HighlightRegistry, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(rp, Symbol.toStringTag, {
        value: 'HighlightRegistry', writable: false, enumerable: false, configurable: true,
    });
    Object.defineProperty(rp, Symbol.iterator, {
        value: rEntries, writable: true, enumerable: false, configurable: true,
    });

    const registry = Object.create(rp);
    registryState.set(registry, new Map());
    Object.defineProperty(globalThis, 'Highlight', {
        value: Highlight, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(globalThis, 'HighlightRegistry', {
        value: HighlightRegistry, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(globalThis.CSS, 'highlights', {
        enumerable: true, configurable: true,
        get: getter('highlights', () => registry),
    });
})(globalThis);
