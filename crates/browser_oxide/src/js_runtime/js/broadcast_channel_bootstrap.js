// Cross-context BroadcastChannel transport.
//
// The legacy Window/Worker shims kept a realm-local Map, which meant a
// same-origin Window and DedicatedWorker using the same channel name could
// never see each other's messages. Keep the Chrome-matching WebIDL surface,
// but route serialized payloads through the process-global Rust registry so
// every runtime/context participates in the same named channel.
((globalThis) => {
    'use strict';

    const ops = globalThis.Deno?.core?.ops;
    if (!ops
        || typeof ops.op_broadcast_channel_register !== 'function'
        || typeof ops.op_broadcast_channel_post !== 'function'
        || typeof ops.op_broadcast_channel_try_recv !== 'function'
        || typeof ops.op_broadcast_channel_close !== 'function'
        || typeof globalThis.EventTarget !== 'function'
        || typeof globalThis.MessageEvent !== 'function') {
        return;
    }

    const bridge = globalThis._browser_oxide || globalThis.__browser_oxide || null;
    const markTrusted =
        (bridge && bridge._markTrustedEvent)
        || globalThis.__bo_mark_trusted
        || ((event) => event);
    const serializeForWire = bridge && bridge.serializeForWire;
    const deserializeFromWire = bridge && bridge.deserializeFromWire;

    const states = new WeakMap();
    const endpoints = new Map();

    function currentOrigin() {
        try {
            const origin = globalThis.location?.origin;
            if (origin) return String(origin);
        } catch (_) {}
        return 'null';
    }

    function requireState(receiver) {
        const state = states.get(receiver);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    }

    function encode(message) {
        const serialized = typeof serializeForWire === 'function'
            ? serializeForWire(message, undefined, {
                method: 'postMessage',
                iface: 'BroadcastChannel',
                portIds: new Map(),
            })
            : (typeof globalThis.structuredClone === 'function'
                ? globalThis.structuredClone(message)
                : message);
        return JSON.stringify(serialized);
    }

    function decode(raw) {
        const parsed = JSON.parse(raw);
        return typeof deserializeFromWire === 'function'
            ? deserializeFromWire(parsed, new Map())
            : parsed;
    }

    class BroadcastChannelImpl extends globalThis.EventTarget {
        constructor(name) {
            super();
            const channelName = String(name);
            const id = ops.op_broadcast_channel_register(currentOrigin(), channelName);
            const state = {
                id,
                name: channelName,
                closed: false,
                onmessage: null,
                onmessageerror: null,
            };
            states.set(this, state);
            endpoints.set(id, this);
        }
    }

    function BroadcastChannel(name) {
        if (!new.target) {
            throw new TypeError(
                "Failed to construct 'BroadcastChannel': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        }
        if (arguments.length < 1) {
            throw new TypeError(
                "Failed to construct 'BroadcastChannel': 1 argument required, but only 0 present."
            );
        }
        return Reflect.construct(BroadcastChannelImpl, [name], new.target);
    }

    Object.setPrototypeOf(BroadcastChannel, globalThis.EventTarget);
    BroadcastChannel.prototype = BroadcastChannelImpl.prototype;

    // WebIDL prototype order is observable through Reflect.ownKeys().
    delete BroadcastChannel.prototype.constructor;
    Object.defineProperty(BroadcastChannel.prototype, 'name', {
        get: function name() { return requireState(this).name; },
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(BroadcastChannel.prototype, 'onmessage', {
        get: function onmessage() { return requireState(this).onmessage; },
        set: function onmessage(value) {
            requireState(this).onmessage = typeof value === 'function' ? value : null;
        },
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(BroadcastChannel.prototype, 'onmessageerror', {
        get: function onmessageerror() { return requireState(this).onmessageerror; },
        set: function onmessageerror(value) {
            requireState(this).onmessageerror = typeof value === 'function' ? value : null;
        },
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(BroadcastChannel.prototype, 'close', {
        value: function close() {
            const state = requireState(this);
            if (state.closed) return;
            state.closed = true;
            endpoints.delete(state.id);
            ops.op_broadcast_channel_close(state.id);
        },
        writable: true,
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(BroadcastChannel.prototype, 'postMessage', {
        value: function postMessage(message) {
            const state = requireState(this);
            if (state.closed) {
                throw new DOMException(
                    "Failed to execute 'postMessage' on 'BroadcastChannel': Channel is closed",
                    'InvalidStateError',
                );
            }
            ops.op_broadcast_channel_post(state.id, encode(message));
        },
        writable: true,
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(BroadcastChannel.prototype, 'constructor', {
        value: BroadcastChannel,
        writable: true,
        enumerable: false,
        configurable: true,
    });
    Object.defineProperty(BroadcastChannel.prototype, Symbol.toStringTag, {
        value: 'BroadcastChannel',
        configurable: true,
    });

    if (typeof globalThis._maskFunction === 'function') {
        globalThis._maskFunction(BroadcastChannel, 'BroadcastChannel');
        globalThis._maskFunction(
            Object.getOwnPropertyDescriptor(BroadcastChannel.prototype, 'name').get,
            'get name',
        );
        globalThis._maskFunction(
            Object.getOwnPropertyDescriptor(BroadcastChannel.prototype, 'onmessage').get,
            'get onmessage',
        );
        globalThis._maskFunction(
            Object.getOwnPropertyDescriptor(BroadcastChannel.prototype, 'onmessage').set,
            'set onmessage',
        );
        globalThis._maskFunction(
            Object.getOwnPropertyDescriptor(BroadcastChannel.prototype, 'onmessageerror').get,
            'get onmessageerror',
        );
        globalThis._maskFunction(
            Object.getOwnPropertyDescriptor(BroadcastChannel.prototype, 'onmessageerror').set,
            'set onmessageerror',
        );
        globalThis._maskFunction(BroadcastChannel.prototype.close, 'close');
        globalThis._maskFunction(BroadcastChannel.prototype.postMessage, 'postMessage');
    } else if (typeof globalThis._maskAsNative === 'function') {
        globalThis._maskAsNative(BroadcastChannel.prototype, 'close', 'postMessage');
    }

    function pumpBroadcastChannels() {
        let delivered = 0;
        for (const [id, channel] of Array.from(endpoints)) {
            const state = states.get(channel);
            if (!state || state.closed) {
                endpoints.delete(id);
                try { ops.op_broadcast_channel_close(id); } catch (_) {}
                continue;
            }
            for (;;) {
                let raw = '';
                try { raw = ops.op_broadcast_channel_try_recv(id); }
                catch (_) { break; }
                if (!raw) break;

                let data;
                try {
                    data = decode(raw);
                } catch (_) {
                    const errorEvent = markTrusted(new globalThis.MessageEvent('messageerror', {
                        data: null,
                        origin: '',
                        lastEventId: '',
                        source: null,
                        ports: [],
                    }));
                    globalThis.setTimeout(() => {
                        if (!states.get(channel)?.closed) channel.dispatchEvent(errorEvent);
                    }, 0);
                    delivered += 1;
                    continue;
                }

                globalThis.setTimeout(() => {
                    if (states.get(channel)?.closed) return;
                    channel.dispatchEvent(markTrusted(new globalThis.MessageEvent('message', {
                        data,
                        origin: '',
                        lastEventId: '',
                        source: null,
                        ports: [],
                    })));
                }, 0);
                delivered += 1;
            }
        }
        return delivered;
    }

    // ---------------------------------------------------------------------
    // Web Locks API.
    //
    // Lock ownership is coordinated by the same process-global Rust extension
    // used above for BroadcastChannel so same-origin Windows and Workers share
    // one scheduler. Only the callback itself runs in the requesting realm.
    // ---------------------------------------------------------------------
    const webLocksSecure = globalThis.isSecureContext === true;
    const navigatorPrototype = (() => {
        try { return globalThis.navigator && Object.getPrototypeOf(globalThis.navigator); }
        catch (_) { return null; }
    })();
    if (!webLocksSecure) {
        try {
            if (navigatorPrototype) delete navigatorPrototype.locks;
        } catch (_) {}
        try { delete globalThis.LockManager; } catch (_) {}
        try { delete globalThis.Lock; } catch (_) {}
    } else if (navigatorPrototype) {
        try {
            const descriptor = Object.getOwnPropertyDescriptor(navigatorPrototype, 'locks');
            if (descriptor) {
                Object.defineProperty(navigatorPrototype, 'locks', {
                    ...descriptor,
                    enumerable: true,
                    configurable: true,
                });
            }
        } catch (_) {}
    }

    const webLocksAvailable = webLocksSecure
        && typeof ops.op_web_lock_enqueue === 'function'
        && typeof ops.op_web_lock_wait === 'function'
        && typeof ops.op_web_lock_cancel === 'function'
        && typeof ops.op_web_lock_release === 'function'
        && typeof ops.op_web_lock_wait_break === 'function'
        && typeof ops.op_web_lock_query === 'function'
        && typeof globalThis.LockManager === 'function'
        && typeof globalThis.Lock === 'function';

    if (webLocksAvailable) {
        const lockManagerPrototype = globalThis.LockManager.prototype;
        const lockPrototype = globalThis.Lock.prototype;
        const lockManagers = new WeakSet();
        const lockStates = new WeakMap();

        const makeClientId = () => {
            try {
                const bytes = new Uint8Array(16);
                globalThis.crypto.getRandomValues(bytes);
                return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('').toUpperCase();
            } catch (_) {
                let out = '';
                for (let i = 0; i < 4; i++) {
                    out += Math.floor(Math.random() * 0x100000000)
                        .toString(16)
                        .padStart(8, '0');
                }
                return out.toUpperCase();
            }
        };
        const clientId = makeClientId();

        const manager = (() => {
            try { return globalThis.navigator && globalThis.navigator.locks; }
            catch (_) { return null; }
        })();

        function makeIllegalConstructor(name, prototype) {
            const target = function() {
                if (new.target) {
                    throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
                }
                throw new TypeError('Illegal constructor');
            };
            const ctor = target.bind(null);
            Object.defineProperty(ctor, 'name', { value: name, configurable: true });
            Object.defineProperty(ctor, 'length', { value: 0, configurable: true });
            Object.defineProperty(ctor, 'prototype', {
                value: prototype,
                writable: false,
                enumerable: false,
                configurable: false,
            });
            if (typeof globalThis._maskFunction === 'function') {
                globalThis._maskFunction(ctor, name);
            }
            return ctor;
        }

        const LockManagerConstructor = makeIllegalConstructor('LockManager', lockManagerPrototype);
        const LockConstructor = makeIllegalConstructor('Lock', lockPrototype);
        Object.defineProperty(globalThis, 'LockManager', {
            value: LockManagerConstructor,
            writable: true,
            enumerable: false,
            configurable: true,
        });
        Object.defineProperty(globalThis, 'Lock', {
            value: LockConstructor,
            writable: true,
            enumerable: false,
            configurable: true,
        });

        function requireLockManager(receiver) {
            if (!lockManagers.has(receiver)) throw new TypeError('Illegal invocation');
        }

        function requireLock(receiver) {
            const state = lockStates.get(receiver);
            if (!state) throw new TypeError('Illegal invocation');
            return state;
        }

        function makeLock(name, mode) {
            const lock = Object.create(lockPrototype);
            lockStates.set(lock, { name, mode });
            return lock;
        }

        function abortReason(signal) {
            try {
                if (signal && signal.reason !== undefined) return signal.reason;
            } catch (_) {}
            return new DOMException('signal is aborted without reason', 'AbortError');
        }

        const query = function query() {
            requireLockManager(this);
            let parsed = { held: [], pending: [] };
            try { parsed = JSON.parse(ops.op_web_lock_query(currentOrigin())); }
            catch (_) {}
            return Promise.resolve(parsed);
        };

        // Deliberately declare two formal parameters: Chrome's
        // LockManager.prototype.request.length is 2 even though the options
        // overload consumes a third callback argument.
        const request = function request(name, optionsOrCallback) {
            requireLockManager(this);
            if (arguments.length < 2) {
                throw new TypeError(
                    "Failed to execute 'request' on 'LockManager': 2 arguments required, but only "
                    + arguments.length + ' present.'
                );
            }

            name = String(name);
            let options = {};
            let callback;
            if (typeof optionsOrCallback === 'function') {
                callback = optionsOrCallback;
            } else {
                options = optionsOrCallback == null ? {} : Object(optionsOrCallback);
                callback = arguments[2];
            }
            if (typeof callback !== 'function') {
                throw new TypeError(
                    "Failed to execute 'request' on 'LockManager': parameter 2 is not of type 'Function'."
                );
            }

            const mode = options.mode === undefined ? 'exclusive' : String(options.mode);
            if (mode !== 'exclusive' && mode !== 'shared') {
                throw new TypeError(
                    `Failed to execute 'request' on 'LockManager': The provided value '${mode}' is not a valid enum value of type LockMode.`
                );
            }
            const ifAvailable = Boolean(options.ifAvailable);
            const steal = Boolean(options.steal);
            if (steal && mode !== 'exclusive') {
                throw new DOMException(
                    "Failed to execute 'request' on 'LockManager': The 'steal' option may only be used with 'exclusive' locks.",
                    'NotSupportedError',
                );
            }
            if (steal && ifAvailable) {
                throw new DOMException(
                    "Failed to execute 'request' on 'LockManager': The 'steal' and 'ifAvailable' options cannot be used together.",
                    'NotSupportedError',
                );
            }

            const signal = options.signal === undefined ? null : options.signal;
            if (signal !== null && typeof globalThis.AbortSignal === 'function'
                && !(signal instanceof globalThis.AbortSignal)) {
                throw new TypeError(
                    "Failed to execute 'request' on 'LockManager': Failed to read the 'signal' property from 'LockOptions': The provided value is not of type 'AbortSignal'."
                );
            }
            if (signal && signal.aborted) return Promise.reject(abortReason(signal));

            return (async () => {
                const requestId = ops.op_web_lock_enqueue(
                    currentOrigin(),
                    name,
                    clientId,
                    mode === 'shared',
                    ifAvailable,
                    steal,
                );

                let abortListener = null;
                let abortPromise = null;
                if (signal) {
                    abortPromise = new Promise((_, reject) => {
                        abortListener = () => {
                            let canceled = false;
                            try { canceled = ops.op_web_lock_cancel(requestId); }
                            catch (_) {}
                            if (canceled) reject(abortReason(signal));
                        };
                        signal.addEventListener('abort', abortListener, { once: true });
                    });
                }

                let status;
                try {
                    const waitPromise = ops.op_web_lock_wait(requestId);
                    status = abortPromise
                        ? await Promise.race([waitPromise, abortPromise])
                        : await waitPromise;
                } finally {
                    if (signal && abortListener) {
                        try { signal.removeEventListener('abort', abortListener); }
                        catch (_) {}
                    }
                }

                if (status === 'u') return await callback(null);
                if (status === 'c') throw abortReason(signal);
                if (typeof status !== 'string' || !status.startsWith('g:')) {
                    throw new DOMException('The lock request was canceled.', 'AbortError');
                }

                const lockId = Number(status.slice(2));
                const lock = makeLock(name, mode);
                try {
                    const callbackPromise = Promise.resolve().then(() => callback(lock));
                    const brokenPromise = ops.op_web_lock_wait_break(lockId).then(broken => {
                        if (broken) {
                            throw new DOMException(
                                "Lock broken by another request with the 'steal' option.",
                                'AbortError',
                            );
                        }
                        // Normal release happens only after the callback wins
                        // this race. Keep this branch permanently pending so it
                        // can never replace the callback's return value.
                        return new Promise(() => {});
                    });
                    return await Promise.race([callbackPromise, brokenPromise]);
                } finally {
                    try { ops.op_web_lock_release(lockId); }
                    catch (_) {}
                }
            })();
        };

        // Rebuild observable prototype order to match Chromium.
        for (const key of ['query', 'request', 'constructor']) {
            try { delete lockManagerPrototype[key]; } catch (_) {}
        }
        try { delete lockManagerPrototype[Symbol.toStringTag]; } catch (_) {}
        Object.defineProperty(lockManagerPrototype, 'query', {
            value: query, writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(lockManagerPrototype, 'request', {
            value: request, writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(lockManagerPrototype, 'constructor', {
            value: LockManagerConstructor, writable: true, enumerable: false, configurable: true,
        });
        Object.defineProperty(lockManagerPrototype, Symbol.toStringTag, {
            value: 'LockManager', configurable: true,
        });

        for (const key of ['name', 'mode', 'constructor']) {
            try { delete lockPrototype[key]; } catch (_) {}
        }
        try { delete lockPrototype[Symbol.toStringTag]; } catch (_) {}
        const getName = function name() { return requireLock(this).name; };
        const getMode = function mode() { return requireLock(this).mode; };
        Object.defineProperty(lockPrototype, 'name', {
            get: getName, enumerable: true, configurable: true,
        });
        Object.defineProperty(lockPrototype, 'mode', {
            get: getMode, enumerable: true, configurable: true,
        });
        Object.defineProperty(lockPrototype, 'constructor', {
            value: LockConstructor, writable: true, enumerable: false, configurable: true,
        });
        Object.defineProperty(lockPrototype, Symbol.toStringTag, {
            value: 'Lock', configurable: true,
        });

        if (manager) {
            try { delete manager.query; } catch (_) {}
            try { delete manager.request; } catch (_) {}
            try { delete manager[Symbol.toStringTag]; } catch (_) {}
            lockManagers.add(manager);
        }

        if (typeof globalThis._maskFunction === 'function') {
            globalThis._maskFunction(query, 'query');
            globalThis._maskFunction(request, 'request');
            globalThis._maskFunction(getName, 'get name');
            globalThis._maskFunction(getMode, 'get mode');
        }
    }

    globalThis.BroadcastChannel = BroadcastChannel;

    for (const target of [globalThis._browser_oxide, globalThis.__browser_oxide]) {
        if (!target) continue;
        try {
            Object.defineProperty(target, '_pumpBroadcastChannels', {
                value: pumpBroadcastChannels,
                configurable: true,
                enumerable: false,
                writable: false,
            });
        } catch (_) {}
    }
})(globalThis);
