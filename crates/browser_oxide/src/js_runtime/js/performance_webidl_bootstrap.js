// Performance Timeline WebIDL compatibility layer.
//
// The lower performance implementation owns timing collection. Keep those
// collectors intact, and adapt their records at the public API boundary so
// externally visible entries have Chromium-style prototypes and no own state.
((globalThis) => {
    const mask = (fn, name) => {
        try {
            if (typeof _maskFunction === 'function' && typeof fn === 'function') {
                _maskFunction(fn, name);
            }
        } catch (_) {}
        return fn;
    };
    const setLength = (fn, length) => {
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
    };
    const defineGetter = (proto, name, getter) => {
        mask(getter, `get ${name}`);
        Object.defineProperty(proto, name, {
            get: getter, enumerable: true, configurable: true,
        });
    };
    const defineMethod = (proto, name, fn, length = 0) => {
        setLength(fn, length);
        mask(fn, name);
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };
    const defineTag = (proto, tag) => {
        try {
            Object.defineProperty(proto, Symbol.toStringTag, {
                value: tag, writable: false, enumerable: false, configurable: true,
            });
        } catch (_) {}
    };
    const makeIllegalCtor = (name, length = 0) => {
        const Ctor = function() {
            throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
        };
        try { Object.defineProperty(Ctor, 'name', { value: name, configurable: true }); } catch (_) {}
        setLength(Ctor, length);
        mask(Ctor, name);
        return Ctor;
    };
    const attachPrototype = (Ctor, parentProto, tag) => {
        Ctor.prototype = Object.create(parentProto || Object.prototype);
        Object.defineProperty(Ctor.prototype, 'constructor', {
            value: Ctor, writable: true, enumerable: false, configurable: true,
        });
        defineTag(Ctor.prototype, tag);
    };

    const entryState = new WeakMap();
    const stateOf = (entry) => {
        const state = entryState.get(entry);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const cloneDetail = (value) => {
        if (value === undefined) return null;
        try {
            if (typeof globalThis.structuredClone === 'function') return globalThis.structuredClone(value);
        } catch (_) {}
        return value;
    };

    const PerformanceEntry = makeIllegalCtor('PerformanceEntry');
    attachPrototype(PerformanceEntry, Object.prototype, 'PerformanceEntry');

    for (const name of ['duration', 'entryType', 'name', 'startTime']) {
        defineGetter(PerformanceEntry.prototype, name, function() { return stateOf(this)[name]; });
    }
    defineMethod(PerformanceEntry.prototype, 'toJSON', function toJSON() {
        const state = stateOf(this);
        return {
            name: state.name,
            entryType: state.entryType,
            startTime: state.startTime,
            duration: state.duration,
        };
    });

    const PerformanceResourceTiming = makeIllegalCtor('PerformanceResourceTiming');
    attachPrototype(PerformanceResourceTiming, PerformanceEntry.prototype, 'PerformanceResourceTiming');
    const resourceFields = [
        'connectEnd', 'connectStart', 'contentEncoding', 'contentType',
        'decodedBodySize', 'deliveryType', 'domainLookupEnd', 'domainLookupStart',
        'encodedBodySize', 'fetchStart', 'finalResponseHeadersStart',
        'firstInterimResponseStart', 'initiatorType', 'nextHopProtocol',
        'redirectEnd', 'redirectStart', 'renderBlockingStatus', 'requestStart',
        'responseEnd', 'responseStart', 'responseStatus', 'secureConnectionStart',
        'serverTiming', 'transferSize', 'workerCacheLookupStart',
        'workerFinalSourceType', 'workerMatchedSourceType',
        'workerRouterEvaluationStart', 'workerStart',
    ];
    for (const name of resourceFields) {
        defineGetter(PerformanceResourceTiming.prototype, name, function() { return stateOf(this)[name]; });
    }
    defineMethod(PerformanceResourceTiming.prototype, 'toJSON', function toJSON() {
        const state = stateOf(this);
        const out = PerformanceEntry.prototype.toJSON.call(this);
        for (const name of [
            'initiatorType', 'deliveryType', 'nextHopProtocol', 'renderBlockingStatus',
            'contentType', 'contentEncoding', 'workerStart', 'workerRouterEvaluationStart',
            'workerCacheLookupStart', 'workerMatchedSourceType', 'workerFinalSourceType',
            'redirectStart', 'redirectEnd', 'fetchStart', 'domainLookupStart',
            'domainLookupEnd', 'connectStart', 'secureConnectionStart', 'connectEnd',
            'requestStart', 'responseStart', 'firstInterimResponseStart',
            'finalResponseHeadersStart', 'responseEnd', 'transferSize',
            'encodedBodySize', 'decodedBodySize', 'responseStatus', 'serverTiming',
        ]) out[name] = state[name];
        return out;
    });

    const PerformanceNavigationTiming = makeIllegalCtor('PerformanceNavigationTiming');
    attachPrototype(
        PerformanceNavigationTiming,
        PerformanceResourceTiming.prototype,
        'PerformanceNavigationTiming',
    );
    const navigationFields = [
        'activationStart', 'confidence', 'criticalCHRestart', 'domComplete',
        'domContentLoadedEventEnd', 'domContentLoadedEventStart', 'domInteractive',
        'loadEventEnd', 'loadEventStart', 'notRestoredReasons', 'redirectCount',
        'type', 'unloadEventEnd', 'unloadEventStart',
    ];
    for (const name of navigationFields) {
        defineGetter(PerformanceNavigationTiming.prototype, name, function() { return stateOf(this)[name]; });
    }
    defineMethod(PerformanceNavigationTiming.prototype, 'toJSON', function toJSON() {
        const state = stateOf(this);
        const out = PerformanceResourceTiming.prototype.toJSON.call(this);
        for (const name of [
            'unloadEventStart', 'unloadEventEnd', 'domInteractive',
            'domContentLoadedEventStart', 'domContentLoadedEventEnd', 'domComplete',
            'loadEventStart', 'loadEventEnd', 'type', 'redirectCount',
            'activationStart', 'criticalCHRestart', 'notRestoredReasons', 'confidence',
        ]) out[name] = state[name];
        return out;
    });

    const PerformanceMeasure = makeIllegalCtor('PerformanceMeasure');
    attachPrototype(PerformanceMeasure, PerformanceEntry.prototype, 'PerformanceMeasure');
    defineGetter(PerformanceMeasure.prototype, 'detail', function detail() { return stateOf(this).detail; });

    function PerformanceMark(name) {
        if (!new.target) {
            throw new TypeError("Failed to construct 'PerformanceMark': Please use the 'new' operator, this DOM object constructor cannot be called as a function.");
        }
        if (arguments.length < 1) {
            throw new TypeError("Failed to construct 'PerformanceMark': 1 argument required, but only 0 present.");
        }
        const options = arguments.length > 1 && arguments[1] != null ? Object(arguments[1]) : {};
        const startTime = options.startTime === undefined
            ? (globalThis.performance?.now?.() || 0)
            : Number(options.startTime);
        if (!Number.isFinite(startTime) || startTime < 0) {
            throw new TypeError("Failed to construct 'PerformanceMark': 'startTime' must be a finite non-negative number.");
        }
        entryState.set(this, {
            name: String(name), entryType: 'mark', startTime, duration: 0,
            detail: cloneDetail(options.detail),
        });
    }
    setLength(PerformanceMark, 1);
    mask(PerformanceMark, 'PerformanceMark');
    attachPrototype(PerformanceMark, PerformanceEntry.prototype, 'PerformanceMark');
    defineGetter(PerformanceMark.prototype, 'detail', function detail() { return stateOf(this).detail; });

    const numberOr = (value, fallback = 0) => {
        const number = Number(value);
        return Number.isFinite(number) ? number : fallback;
    };
    const resourceDefaults = (raw) => ({
        initiatorType: raw?.initiatorType || '',
        deliveryType: raw?.deliveryType || '',
        nextHopProtocol: raw?.nextHopProtocol || '',
        renderBlockingStatus: raw?.renderBlockingStatus || 'non-blocking',
        contentType: raw?.contentType || '',
        contentEncoding: raw?.contentEncoding || '',
        workerStart: numberOr(raw?.workerStart),
        workerRouterEvaluationStart: numberOr(raw?.workerRouterEvaluationStart),
        workerCacheLookupStart: numberOr(raw?.workerCacheLookupStart),
        workerMatchedSourceType: raw?.workerMatchedSourceType || '',
        workerFinalSourceType: raw?.workerFinalSourceType || '',
        redirectStart: numberOr(raw?.redirectStart),
        redirectEnd: numberOr(raw?.redirectEnd),
        fetchStart: numberOr(raw?.fetchStart),
        domainLookupStart: numberOr(raw?.domainLookupStart),
        domainLookupEnd: numberOr(raw?.domainLookupEnd),
        connectStart: numberOr(raw?.connectStart),
        secureConnectionStart: numberOr(raw?.secureConnectionStart),
        connectEnd: numberOr(raw?.connectEnd),
        requestStart: numberOr(raw?.requestStart),
        responseStart: numberOr(raw?.responseStart),
        firstInterimResponseStart: numberOr(raw?.firstInterimResponseStart),
        finalResponseHeadersStart: numberOr(raw?.finalResponseHeadersStart, numberOr(raw?.responseStart)),
        responseEnd: numberOr(raw?.responseEnd),
        transferSize: numberOr(raw?.transferSize),
        encodedBodySize: numberOr(raw?.encodedBodySize),
        decodedBodySize: numberOr(raw?.decodedBodySize),
        responseStatus: numberOr(raw?.responseStatus),
        serverTiming: Array.isArray(raw?.serverTiming) ? raw.serverTiming.slice() : [],
    });
    const createEntry = (proto, state) => {
        const entry = Object.create(proto);
        entryState.set(entry, state);
        return entry;
    };
    const wrapEntry = (raw) => {
        if (!raw || typeof raw !== 'object') return raw;
        if (entryState.has(raw)) return raw;
        const base = {
            name: String(raw.name ?? ''),
            entryType: String(raw.entryType ?? ''),
            startTime: numberOr(raw.startTime),
            duration: numberOr(raw.duration),
        };
        if (base.entryType === 'navigation') {
            return createEntry(PerformanceNavigationTiming.prototype, {
                ...base, ...resourceDefaults(raw),
                unloadEventStart: numberOr(raw.unloadEventStart),
                unloadEventEnd: numberOr(raw.unloadEventEnd),
                domInteractive: numberOr(raw.domInteractive),
                domContentLoadedEventStart: numberOr(raw.domContentLoadedEventStart),
                domContentLoadedEventEnd: numberOr(raw.domContentLoadedEventEnd),
                domComplete: numberOr(raw.domComplete),
                loadEventStart: numberOr(raw.loadEventStart),
                loadEventEnd: numberOr(raw.loadEventEnd),
                type: raw.type || 'navigate',
                redirectCount: numberOr(raw.redirectCount),
                activationStart: numberOr(raw.activationStart),
                criticalCHRestart: numberOr(raw.criticalCHRestart),
                notRestoredReasons: raw.notRestoredReasons ?? null,
                confidence: raw.confidence ?? null,
            });
        }
        if (base.entryType === 'resource') {
            return createEntry(PerformanceResourceTiming.prototype, {
                ...base, ...resourceDefaults(raw),
            });
        }
        if (base.entryType === 'mark') {
            return createEntry(PerformanceMark.prototype, {
                ...base, detail: cloneDetail(raw.detail),
            });
        }
        if (base.entryType === 'measure') {
            return createEntry(PerformanceMeasure.prototype, {
                ...base, detail: cloneDetail(raw.detail),
            });
        }
        return createEntry(PerformanceEntry.prototype, base);
    };

    const perf = globalThis.performance;
    if (perf) {
        const proto = Object.getPrototypeOf(perf);
        const oldGetEntries = typeof perf.getEntries === 'function' ? perf.getEntries.bind(perf) : () => [];
        const oldGetEntriesByType = typeof perf.getEntriesByType === 'function'
            ? perf.getEntriesByType.bind(perf) : null;
        const oldGetEntriesByName = typeof perf.getEntriesByName === 'function'
            ? perf.getEntriesByName.bind(perf) : null;
        const marks = [];
        const measures = [];

        const installPerfMethod = (name, fn, length) => {
            if (!proto) return;
            setLength(fn, length);
            mask(fn, name);
            Object.defineProperty(proto, name, {
                value: fn, writable: true, enumerable: true, configurable: true,
            });
        };

        installPerfMethod('getEntries', function getEntries() {
            const base = Array.from(oldGetEntries() || []).map(wrapEntry);
            return [...base, ...marks, ...measures].sort((a, b) => a.startTime - b.startTime);
        }, 0);
        installPerfMethod('getEntriesByType', function getEntriesByType(type) {
            const wanted = String(type);
            if (wanted === 'mark') return marks.slice();
            if (wanted === 'measure') return measures.slice();
            if (oldGetEntriesByType) return Array.from(oldGetEntriesByType(wanted) || []).map(wrapEntry);
            return this.getEntries().filter((entry) => entry.entryType === wanted);
        }, 1);
        installPerfMethod('getEntriesByName', function getEntriesByName(name) {
            const wanted = String(name);
            const type = arguments.length > 1 ? String(arguments[1]) : '';
            const stored = [...marks, ...measures].filter(
                (entry) => entry.name === wanted && (!type || entry.entryType === type),
            );
            if (oldGetEntriesByName) {
                const base = Array.from(oldGetEntriesByName(wanted, ...(type ? [type] : [])) || []).map(wrapEntry);
                return [...base, ...stored].sort((a, b) => a.startTime - b.startTime);
            }
            return this.getEntries().filter(
                (entry) => entry.name === wanted && (!type || entry.entryType === type),
            );
        }, 1);
        installPerfMethod('mark', function mark(name) {
            const options = arguments.length > 1 ? arguments[1] : undefined;
            const mark = new PerformanceMark(name, options);
            marks.push(mark);
            return mark;
        }, 1);
        installPerfMethod('measure', function measure(name) {
            const options = arguments.length > 1 ? arguments[1] : undefined;
            const endMark = arguments.length > 2 ? arguments[2] : undefined;
            let start = 0;
            let end = globalThis.performance.now();
            let duration;
            let detail = null;

            const resolvePoint = (value, fallback) => {
                if (typeof value === 'number') return value;
                if (typeof value === 'string') {
                    const candidates = [...marks, ...measures].filter((entry) => entry.name === value);
                    if (candidates.length) return candidates[candidates.length - 1].startTime;
                }
                return fallback;
            };
            if (options && typeof options === 'object') {
                detail = cloneDetail(options.detail);
                if (options.start !== undefined) start = resolvePoint(options.start, 0);
                if (options.end !== undefined) end = resolvePoint(options.end, end);
                if (options.duration !== undefined) duration = Number(options.duration);
                if (duration !== undefined && options.start !== undefined && options.end === undefined) end = start + duration;
                if (duration !== undefined && options.end !== undefined && options.start === undefined) start = end - duration;
            } else {
                if (options !== undefined) start = resolvePoint(options, 0);
                if (endMark !== undefined) end = resolvePoint(endMark, end);
            }
            const state = {
                name: String(name), entryType: 'measure', startTime: numberOr(start),
                duration: Math.max(0, numberOr(duration, end - start)), detail,
            };
            const entry = createEntry(PerformanceMeasure.prototype, state);
            measures.push(entry);
            return entry;
        }, 1);
        installPerfMethod('clearMarks', function clearMarks() {
            const name = arguments.length ? String(arguments[0]) : null;
            for (let i = marks.length - 1; i >= 0; i--) {
                if (name === null || marks[i].name === name) marks.splice(i, 1);
            }
        }, 0);
        installPerfMethod('clearMeasures', function clearMeasures() {
            const name = arguments.length ? String(arguments[0]) : null;
            for (let i = measures.length - 1; i >= 0; i--) {
                if (name === null || measures[i].name === name) measures.splice(i, 1);
            }
        }, 0);
    }

    globalThis.PerformanceEntry = PerformanceEntry;
    globalThis.PerformanceResourceTiming = PerformanceResourceTiming;
    globalThis.PerformanceNavigationTiming = PerformanceNavigationTiming;
    globalThis.PerformanceMeasure = PerformanceMeasure;
    globalThis.PerformanceMark = PerformanceMark;
})(globalThis);
