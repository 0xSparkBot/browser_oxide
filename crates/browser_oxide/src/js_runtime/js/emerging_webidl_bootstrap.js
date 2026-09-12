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

    // ------------------------------------------------------------------
    // URLPattern — Window + Worker.
    //
    // The interface bootstrap historically exposed only an Illegal-
    // constructor stub. Use rust-urlpattern's browser parser/canonicalizer
    // through two small ops, then execute the generated ECMAScript regexps in
    // V8. This keeps URL canonicalization spec-aligned without substituting
    // Rust regex semantics for JavaScript regex semantics.
    // ------------------------------------------------------------------
    {
        const ops = globalThis.Deno?.core?.ops;
        if (ops
            && typeof ops.op_url_pattern_compile === 'function'
            && typeof ops.op_url_pattern_match_input === 'function') {
            const states = new WeakMap();
            const componentNames = [
                'protocol', 'username', 'password', 'hostname',
                'port', 'pathname', 'search', 'hash',
            ];
            // Web IDL dictionary members are observed in lexicographic order.
            // Chrome returns this normalized copy from URLPatternResult.inputs.
            const initKeys = [
                'baseURL', 'hash', 'hostname', 'password', 'pathname',
                'port', 'protocol', 'search', 'username',
            ];

            const isInitInput = value => value == null || typeof value === 'object';
            const normalizeInit = value => {
                const source = value == null ? {} : Object(value);
                const output = {};
                for (const key of initKeys) {
                    const item = source[key];
                    if (item !== undefined) output[key] = String(item);
                }
                return output;
            };
            const normalizeInput = value => isInitInput(value)
                ? normalizeInit(value)
                : String(value);
            const readIgnoreCase = value => {
                if (value == null) return false;
                return Boolean(Object(value).ignoreCase);
            };
            const stateOf = receiver => {
                const state = states.get(receiver);
                if (!state) throw new TypeError('Illegal invocation');
                return state;
            };
            const callOp = (name, payload) => {
                const raw = ops[name](JSON.stringify(payload));
                return JSON.parse(String(raw));
            };
            const initBaseUrlError = (operation, value) => new TypeError(
                `Failed to ${operation} 'URLPattern': Invalid second argument baseURL '${String(value)}' provided with a URLPatternInit input. Use the URLPatternInit.baseURL property instead.`
            );
            const compileError = (result, input, baseURL) => {
                if (result.errorKind === 'baseUrlRequired') {
                    return new TypeError(
                        `Failed to construct 'URLPattern': Relative constructor string '${String(input)}' must have a base URL passed as the second argument.`
                    );
                }
                if (result.errorKind === 'baseUrlWithInit') {
                    return initBaseUrlError('construct', baseURL);
                }
                return new TypeError(
                    `Failed to construct 'URLPattern': ${result.error || 'Invalid URL pattern'}`
                );
            };
            const matchError = (method, result, baseURL) => {
                if (result.errorKind === 'baseUrlWithInit') {
                    return initBaseUrlError(`execute '${method}' on`, baseURL);
                }
                return new TypeError(
                    `Failed to execute '${method}' on 'URLPattern': ${result.error || 'Invalid URL'}`
                );
            };

            class URLPatternImpl {
                constructor() {
                    const args = arguments;
                    const rawInput = args.length === 0 || args[0] == null ? {} : args[0];
                    const initInput = isInitInput(rawInput);
                    let baseURL = null;
                    let options;

                    if (initInput) {
                        // Chrome's URLPatternInit overload treats the second
                        // argument as options. Supplying a third argument
                        // selects the string/baseURL/options overload and is
                        // rejected because a separate baseURL cannot accompany
                        // URLPatternInit (even when that argument is undefined).
                        if (args.length >= 3) throw initBaseUrlError('construct', args[1]);
                        options = args.length >= 2 ? args[1] : undefined;
                    } else {
                        if (args.length >= 2 && args[1] !== undefined) {
                            baseURL = String(args[1]);
                        }
                        options = args.length >= 3 ? args[2] : undefined;
                    }

                    const input = normalizeInput(rawInput);
                    const ignoreCase = readIgnoreCase(options);
                    const result = callOp('op_url_pattern_compile', {
                        input,
                        baseURL,
                        ignoreCase,
                    });
                    if (!result.ok) throw compileError(result, rawInput, args[1]);
                    states.set(this, { compiled: result.pattern, ignoreCase });
                }
            }

            const URLPattern = new Proxy(URLPatternImpl, {
                apply() {
                    throw new TypeError(
                        "Failed to construct 'URLPattern': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
                    );
                },
            });
            Object.defineProperty(URLPattern, 'name', { value: 'URLPattern', configurable: true });
            Object.defineProperty(URLPattern, 'length', { value: 0, configurable: true });
            mask(URLPattern, 'URLPattern');

            const proto = URLPatternImpl.prototype;
            delete proto.constructor;
            try { delete proto[Symbol.toStringTag]; } catch (_) {}

            const match = (receiver, method, args) => {
                const state = stateOf(receiver);
                if (args.length === 0) return null;

                const rawInput = args[0];
                const initInput = isInitInput(rawInput);
                if (initInput && args.length >= 2) {
                    throw initBaseUrlError(`execute '${method}' on`, args[1]);
                }
                const input = normalizeInput(rawInput);
                let baseURL = null;
                if (!initInput && args.length >= 2 && args[1] !== undefined) {
                    baseURL = String(args[1]);
                }
                const canonical = callOp('op_url_pattern_match_input', { input, baseURL });
                if (!canonical.ok) throw matchError(method, canonical, args[1]);
                if (!canonical.matched) return null;

                const matches = {};
                for (const component of componentNames) {
                    const definition = state.compiled[component];
                    const flags = state.ignoreCase
                        && (component === 'pathname' || component === 'search' || component === 'hash')
                        ? 'ui'
                        : 'u';
                    const regexp = new RegExp(definition.regexpString, flags);
                    const exec = regexp.exec(canonical.matched[component]);
                    if (!exec) return null;
                    const groups = {};
                    for (let i = 0; i < definition.groupNameList.length; i++) {
                        groups[definition.groupNameList[i]] = exec[i + 1];
                    }
                    matches[component] = { groups, input: canonical.matched[component] };
                }

                const inputs = [initInput ? input : String(rawInput)];
                if (!initInput && args.length >= 2 && args[1] !== undefined) {
                    inputs.push(String(args[1]));
                }
                // URLPatternResult property order is observable through
                // Reflect.ownKeys and matches Chrome 148 exactly.
                return {
                    hash: matches.hash,
                    hostname: matches.hostname,
                    inputs,
                    password: matches.password,
                    pathname: matches.pathname,
                    port: matches.port,
                    protocol: matches.protocol,
                    search: matches.search,
                    username: matches.username,
                };
            };

            for (const component of componentNames) {
                Object.defineProperty(proto, component, {
                    enumerable: true,
                    configurable: true,
                    get: getter(component, receiver => stateOf(receiver).compiled[component].patternString),
                });
            }
            Object.defineProperty(proto, 'hasRegExpGroups', {
                enumerable: true,
                configurable: true,
                get: getter('hasRegExpGroups', receiver => stateOf(receiver).compiled.hasRegExpGroups),
            });
            Object.defineProperty(proto, 'exec', {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function exec() { return match(this, 'exec', arguments); }, 'exec'),
            });
            Object.defineProperty(proto, 'test', {
                enumerable: true,
                configurable: true,
                writable: true,
                value: mask(function test() { return match(this, 'test', arguments) !== null; }, 'test'),
            });
            Object.defineProperty(proto, 'constructor', {
                value: URLPattern,
                writable: true,
                enumerable: false,
                configurable: true,
            });
            Object.defineProperty(proto, Symbol.toStringTag, {
                value: 'URLPattern',
                writable: false,
                enumerable: false,
                configurable: true,
            });

            defineGlobal('URLPattern', URLPattern);
        }
    }
})(globalThis);
