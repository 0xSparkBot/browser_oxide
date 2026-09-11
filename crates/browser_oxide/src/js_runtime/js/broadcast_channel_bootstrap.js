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
