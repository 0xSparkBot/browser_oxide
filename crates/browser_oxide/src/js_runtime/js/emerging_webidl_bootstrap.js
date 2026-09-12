// Chrome 148 WebIDL normalization for small emerging APIs whose early
// implementations only provided feature-detection shape.
//
// Exposure:
//   * Compute Pressure: [SecureContext], Window + DedicatedWorker.
//   * MediaSourceHandle: Window + DedicatedWorker, not SecureContext-gated.
//   * Document Picture-in-Picture: [SecureContext], Window only.
((globalThis) => {
    const mask = (fn, name) => {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') {
                globalThis._maskFunction(fn, name);
            }
        } catch (_) {}
        return fn;
    };
    const defineGlobal = (name, value) => {
        Object.defineProperty(globalThis, name, {
            value,
            writable: true,
            enumerable: false,
            configurable: true,
        });
    };
    const deleteGlobal = (name) => {
        try { delete globalThis[name]; } catch (_) {}
    };
    const getter = (name, read) => {
        const holder = { get [name]() { return read(this); } };
        return mask(Object.getOwnPropertyDescriptor(holder, name).get, `get ${name}`);
    };
    const setter = (name, write) => {
        const holder = { set [name](value) { write(this, value); } };
        return mask(Object.getOwnPropertyDescriptor(holder, name).set, `set ${name}`);
    };
    const illegalInvocation = () => { throw new TypeError('Illegal invocation'); };
    const secure = globalThis.isSecureContext === true;

    // ------------------------------------------------------------------
    // Compute Pressure API — secure Window + DedicatedWorker.
    // ------------------------------------------------------------------
    if (secure) {
        const observerState = new WeakMap();
        const recordState = new WeakMap();
        const recordToken = Symbol('PressureRecord internal');
        const knownSources = Object.freeze(['cpu']);

        class PressureRecordImpl {
            constructor() {
                if (arguments[0] !== recordToken) {
                    throw new TypeError("Failed to construct 'PressureRecord': Illegal constructor");
                }
                recordState.set(this, {
                    source: arguments[1],
                    state: arguments[2],
                    time: arguments[3],
                });
            }
        }
        const PressureRecord = new Proxy(PressureRecordImpl, {
            apply() { throw new TypeError('Illegal constructor'); },
        });
        mask(PressureRecord, 'PressureRecord');
        const recordProto = PressureRecordImpl.prototype;
        delete recordProto.constructor;
        Object.defineProperties(recordProto, {
            source: {
                enumerable: true,
                configurable: true,
                get: getter('source', value => {
                    const state = recordState.get(value);
                    if (!state) return illegalInvocation();
                    return state.source;
                }),
            },
            state: {
                enumerable: true,
                configurable: true,
                get: getter('state', value => {
                    const state = recordState.get(value);
                    if (!state) return illegalInvocation();
                    return state.state;
                }),
            },
            time: {
                enumerable: true,
                configurable: true,
                get: getter('time', value => {
                    const state = recordState.get(value);
                    if (!state) return illegalInvocation();
                    return state.time;
                }),
            },
            toJSON: {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function toJSON() {
                    const state = recordState.get(this);
                    if (!state) return illegalInvocation();
                    return { source: state.source, state: state.state, time: state.time };
                }, 'toJSON'),
            },
            constructor: {
                value: PressureRecord,
                writable: true,
                enumerable: false,
                configurable: true,
            },
            [Symbol.toStringTag]: {
                value: 'PressureRecord',
                writable: false,
                enumerable: false,
                configurable: true,
            },
        });

        class PressureObserverImpl {
            constructor(callback) {
                if (arguments.length < 1) {
                    throw new TypeError(
                        "Failed to construct 'PressureObserver': 1 argument required, but only 0 present."
                    );
                }
                if (typeof callback !== 'function') {
                    throw new TypeError(
                        "Failed to construct 'PressureObserver': parameter 1 is not of type 'Function'."
                    );
                }
                observerState.set(this, { callback, observing: new Set(), records: [] });
            }
        }
        const PressureObserver = new Proxy(PressureObserverImpl, {
            apply() {
                throw new TypeError(
                    "Failed to construct 'PressureObserver': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
                );
            },
        });
        mask(PressureObserver, 'PressureObserver');
        const observerProto = PressureObserverImpl.prototype;
        delete observerProto.constructor;
        Object.defineProperties(observerProto, {
            disconnect: {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function disconnect() {
                    const state = observerState.get(this);
                    if (!state) return illegalInvocation();
                    state.observing.clear();
                    state.records.length = 0;
                }, 'disconnect'),
            },
            observe: {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function observe(source, options = undefined) {
                    const state = observerState.get(this);
                    if (!state) return illegalInvocation();
                    if (arguments.length < 1) {
                        return Promise.reject(new TypeError(
                            "Failed to execute 'observe' on 'PressureObserver': 1 argument required, but only 0 present."
                        ));
                    }
                    const value = String(source);
                    if (value !== 'cpu') {
                        return Promise.reject(new TypeError(
                            "Failed to execute 'observe' on 'PressureObserver': The provided value '" +
                            value + "' is not a valid enum value of type PressureSource."
                        ));
                    }
                    void options;
                    state.observing.add(value);
                    return Promise.resolve(undefined);
                }, 'observe'),
            },
            takeRecords: {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function takeRecords() {
                    const state = observerState.get(this);
                    if (!state) return illegalInvocation();
                    const records = state.records.slice();
                    state.records.length = 0;
                    return records;
                }, 'takeRecords'),
            },
            unobserve: {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function unobserve(source) {
                    const state = observerState.get(this);
                    if (!state) return illegalInvocation();
                    if (arguments.length < 1) {
                        throw new TypeError(
                            "Failed to execute 'unobserve' on 'PressureObserver': 1 argument required, but only 0 present."
                        );
                    }
                    const value = String(source);
                    if (value !== 'cpu') {
                        throw new TypeError(
                            "Failed to execute 'unobserve' on 'PressureObserver': The provided value '" +
                            value + "' is not a valid enum value of type PressureSource."
                        );
                    }
                    state.observing.delete(value);
                }, 'unobserve'),
            },
            constructor: {
                value: PressureObserver,
                writable: true,
                enumerable: false,
                configurable: true,
            },
            [Symbol.toStringTag]: {
                value: 'PressureObserver',
                writable: false,
                enumerable: false,
                configurable: true,
            },
        });
        const knownSourcesGetter = getter('knownSources', () => knownSources);
        Object.defineProperty(PressureObserver, 'knownSources', {
            enumerable: true,
            configurable: true,
            get: knownSourcesGetter,
        });

        defineGlobal('PressureObserver', PressureObserver);
        defineGlobal('PressureRecord', PressureRecord);
    } else {
        deleteGlobal('PressureObserver');
        deleteGlobal('PressureRecord');
    }

    // ------------------------------------------------------------------
    // MediaSourceHandle — native interface exists even where MediaSource
    // does not expose a usable `.handle` in headless Chrome.
    // ------------------------------------------------------------------
    {
        class MediaSourceHandleImpl {
            constructor() {
                throw new TypeError("Failed to construct 'MediaSourceHandle': Illegal constructor");
            }
        }
        const MediaSourceHandle = new Proxy(MediaSourceHandleImpl, {
            apply() { throw new TypeError('Illegal constructor'); },
        });
        mask(MediaSourceHandle, 'MediaSourceHandle');
        Object.defineProperty(MediaSourceHandleImpl.prototype, 'constructor', {
            value: MediaSourceHandle,
            writable: true,
            enumerable: false,
            configurable: true,
        });
        Object.defineProperty(MediaSourceHandleImpl.prototype, Symbol.toStringTag, {
            value: 'MediaSourceHandle',
            writable: false,
            enumerable: false,
            configurable: true,
        });
        defineGlobal('MediaSourceHandle', MediaSourceHandle);
    }

    // ------------------------------------------------------------------
    // Document Picture-in-Picture — secure Window only.
    // ------------------------------------------------------------------
    if (secure && typeof globalThis.document !== 'undefined' && typeof EventTarget === 'function') {
        const dpipState = new WeakMap();
        const dpipToken = Symbol('DocumentPictureInPicture internal');

        class DocumentPictureInPictureImpl extends EventTarget {
            constructor() {
                super();
                if (arguments[0] !== dpipToken) {
                    throw new TypeError(
                        "Failed to construct 'DocumentPictureInPicture': Illegal constructor"
                    );
                }
                dpipState.set(this, { window: null, onenter: null });
            }
        }
        const DocumentPictureInPicture = new Proxy(DocumentPictureInPictureImpl, {
            apply() { throw new TypeError('Illegal constructor'); },
        });
        mask(DocumentPictureInPicture, 'DocumentPictureInPicture');
        const proto = DocumentPictureInPictureImpl.prototype;
        delete proto.constructor;
        Object.defineProperties(proto, {
            window: {
                enumerable: true,
                configurable: true,
                get: getter('window', value => {
                    const state = dpipState.get(value);
                    if (!state) return illegalInvocation();
                    return state.window;
                }),
            },
            onenter: {
                enumerable: true,
                configurable: true,
                get: getter('onenter', value => {
                    const state = dpipState.get(value);
                    if (!state) return illegalInvocation();
                    return state.onenter;
                }),
                set: setter('onenter', (value, handler) => {
                    const state = dpipState.get(value);
                    if (!state) return illegalInvocation();
                    state.onenter = typeof handler === 'function' ? handler : null;
                }),
            },
            requestWindow: {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function requestWindow(options = undefined) {
                    if (!dpipState.has(this)) return illegalInvocation();
                    void options;
                    return Promise.reject(new DOMException(
                        "Failed to execute 'requestWindow' on 'DocumentPictureInPicture': Document PiP requires user activation",
                        'NotAllowedError'
                    ));
                }, 'requestWindow'),
            },
            constructor: {
                value: DocumentPictureInPicture,
                writable: true,
                enumerable: false,
                configurable: true,
            },
            [Symbol.toStringTag]: {
                value: 'DocumentPictureInPicture',
                writable: false,
                enumerable: false,
                configurable: true,
            },
        });
        defineGlobal('DocumentPictureInPicture', DocumentPictureInPicture);

        const documentPictureInPicture = new DocumentPictureInPictureImpl(dpipToken);
        const getDocumentPictureInPicture = getter(
            'documentPictureInPicture',
            () => documentPictureInPicture
        );
        Object.defineProperty(globalThis, 'documentPictureInPicture', {
            enumerable: true,
            configurable: true,
            get: getDocumentPictureInPicture,
        });
    } else {
        deleteGlobal('DocumentPictureInPicture');
        deleteGlobal('documentPictureInPicture');
    }
})(globalThis);
