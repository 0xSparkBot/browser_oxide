// IntersectionObserver WebIDL surface with BrowserOxide's existing headless
// delivery policy. Internal observer/entry state is hidden in WeakMaps.
((globalThis) => {
    if (!globalThis.document) return;

    const observerState = new WeakMap();
    const entryState = new WeakMap();
    const requireState = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const normalizeMargin = value => {
        const parts = String(value == null ? '0px' : value).trim().split(/\s+/).filter(Boolean);
        const normalized = parts.length ? parts : ['0px'];
        if (normalized.length === 1) return `${normalized[0]} ${normalized[0]} ${normalized[0]} ${normalized[0]}`;
        if (normalized.length === 2) return `${normalized[0]} ${normalized[1]} ${normalized[0]} ${normalized[1]}`;
        if (normalized.length === 3) return `${normalized[0]} ${normalized[1]} ${normalized[2]} ${normalized[1]}`;
        return `${normalized[0]} ${normalized[1]} ${normalized[2]} ${normalized[3]}`;
    };
    const normalizeThresholds = value => {
        const input = value === undefined ? [0] : (Array.isArray(value) ? value : [value]);
        const thresholds = input.map(value => Math.fround(Number(value)));
        for (const threshold of thresholds) {
            if (!Number.isFinite(threshold) || threshold < 0 || threshold > 1) {
                throw new RangeError("Failed to construct 'IntersectionObserver': Threshold values must be numbers between 0 and 1");
            }
        }
        thresholds.sort((a, b) => a - b);
        return thresholds;
    };

    function IntersectionObserverEntry() {
        throw new TypeError("Failed to construct 'IntersectionObserverEntry': Illegal constructor");
    }
    const entryGetters = [
        'time', 'rootBounds', 'boundingClientRect', 'intersectionRect',
        'isIntersecting', 'isVisible', 'intersectionRatio', 'target',
    ];
    for (const name of entryGetters) {
        Object.defineProperty(IntersectionObserverEntry.prototype, name, {
            get: function () { return requireState(entryState, this)[name]; },
            enumerable: true,
            configurable: true,
        });
    }
    Object.defineProperty(IntersectionObserverEntry.prototype, Symbol.toStringTag, {
        value: 'IntersectionObserverEntry', configurable: true,
    });
    const makeEntry = init => {
        const entry = Object.create(IntersectionObserverEntry.prototype);
        entryState.set(entry, {
            time: Number(init.time || 0),
            rootBounds: init.rootBounds ?? null,
            boundingClientRect: init.boundingClientRect ?? null,
            intersectionRect: init.intersectionRect ?? null,
            isIntersecting: !!init.isIntersecting,
            isVisible: init.isVisible === undefined ? !!init.isIntersecting : !!init.isVisible,
            intersectionRatio: Number(init.intersectionRatio || 0),
            target: init.target ?? null,
        });
        return entry;
    };

    function IntersectionObserver(callback) {
        if (!new.target) {
            throw new TypeError("Failed to construct 'IntersectionObserver': Please use the 'new' operator");
        }
        if (arguments.length < 1) {
            throw new TypeError("Failed to construct 'IntersectionObserver': 1 argument required, but only 0 present.");
        }
        if (typeof callback !== 'function') {
            throw new TypeError("Failed to construct 'IntersectionObserver': parameter 1 is not of type 'Function'.");
        }
        const options = arguments[1] || {};
        observerState.set(this, {
            callback,
            elements: new Set(),
            records: [],
            root: options.root ?? null,
            rootMargin: normalizeMargin(options.rootMargin),
            scrollMargin: normalizeMargin(options.scrollMargin),
            thresholds: normalizeThresholds(options.threshold),
            delay: Math.max(0, Number(options.delay || 0)),
            trackVisibility: !!options.trackVisibility,
        });
    }
    const observerGetters = ['root', 'rootMargin', 'scrollMargin', 'thresholds', 'delay', 'trackVisibility'];
    for (const name of observerGetters) {
        Object.defineProperty(IntersectionObserver.prototype, name, {
            get: function () {
                const value = requireState(observerState, this)[name];
                return name === 'thresholds' ? Object.freeze(value.slice()) : value;
            },
            enumerable: true,
            configurable: true,
        });
    }
    const observerMethods = {
        observe(target) {
            const state = requireState(observerState, this);
            if (!target || typeof target !== 'object' || Number(target.nodeType) !== 1) {
                throw new TypeError("Failed to execute 'observe' on 'IntersectionObserver': parameter 1 is not of type 'Element'.");
            }
            state.elements.add(target);
            const observer = this;
            Promise.resolve().then(() => {
                if (!state.elements.has(target)) return;
                let rect = null;
                try { rect = target.getBoundingClientRect ? target.getBoundingClientRect() : null; } catch (_) {}
                const entry = makeEntry({
                    target,
                    isIntersecting: true,
                    isVisible: state.trackVisibility ? true : false,
                    intersectionRatio: 1,
                    boundingClientRect: rect,
                    intersectionRect: rect,
                    rootBounds: null,
                    time: globalThis.performance?.now?.() || 0,
                });
                try { state.callback.call(observer, [entry], observer); } catch (error) {
                    try { globalThis.reportError?.(error); } catch (_) {}
                }
            });
        },
        unobserve(target) { requireState(observerState, this).elements.delete(target); },
        disconnect() { requireState(observerState, this).elements.clear(); },
        takeRecords() {
            const state = requireState(observerState, this);
            const records = state.records.slice();
            state.records.length = 0;
            return records;
        },
    };
    const methodLengths = { observe:1, unobserve:1, disconnect:0, takeRecords:0 };
    for (const [name, method] of Object.entries(observerMethods)) {
        Object.defineProperty(method, 'length', { value:methodLengths[name], configurable:true });
        Object.defineProperty(IntersectionObserver.prototype, name, {
            value:method, writable:true, enumerable:true, configurable:true,
        });
    }
    Object.defineProperty(IntersectionObserver.prototype, Symbol.toStringTag, {
        value:'IntersectionObserver', configurable:true,
    });

    globalThis.IntersectionObserverEntry = IntersectionObserverEntry;
    globalThis.IntersectionObserver = IntersectionObserver;

    try {
        if (typeof _maskFunction === 'function') {
            _maskFunction(IntersectionObserverEntry, 'IntersectionObserverEntry');
            _maskFunction(IntersectionObserver, 'IntersectionObserver');
        }
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(IntersectionObserverEntry.prototype, ...entryGetters);
            _maskAsNative(IntersectionObserver.prototype, ...observerGetters, ...Object.keys(observerMethods));
        }
    } catch (_) {}
})(globalThis);
