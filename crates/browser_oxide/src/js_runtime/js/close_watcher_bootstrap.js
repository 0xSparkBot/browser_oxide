// CloseWatcher — HTML close-watcher lifecycle (Chrome 148 shape/semantics).
// Window-only: Chromium does not expose CloseWatcher in DedicatedWorkerGlobalScope.
((globalThis) => {
    if (typeof globalThis.EventTarget !== 'function' || typeof globalThis.Event !== 'function') return;

    const EventTargetCtor = globalThis.EventTarget;
    const EventCtor = globalThis.Event;
    const AbortSignalCtor = globalThis.AbortSignal;
    const markTrusted =
        (globalThis.__browser_oxide && globalThis.__browser_oxide._markTrustedEvent)
        || globalThis.__bo_mark_trusted
        || ((event) => event);
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

    const state = new WeakMap();
    const requireState = (value) => {
        const s = state.get(value);
        if (!s) throw new TypeError('Illegal invocation');
        return s;
    };
    const deactivate = (watcher, s) => {
        if (!s.active) return false;
        s.active = false;
        if (s.signal && s.abortListener) {
            try { s.signal.removeEventListener('abort', s.abortListener); } catch (_) {}
        }
        s.abortListener = null;
        return true;
    };
    const trustedEvent = (type, options) => markTrusted(new EventCtor(type, options));
    const dispatchClose = (watcher, s) => {
        if (!deactivate(watcher, s)) return;
        watcher.dispatchEvent(trustedEvent('close'));
    };
    const setHandler = (_watcher, s, type, value) => {
        // EventTarget's dispatcher already invokes target[`on${type}`] once,
        // after the capture phase and before ordinary bubbling listeners. Keep
        // only the WebIDL event-handler slot here; registering a second listener
        // would double-fire the callback.
        s[`on${type}`] = typeof value === 'function' ? value : null;
    };

    const CloseWatcherImpl = class CloseWatcher extends EventTargetCtor {
        constructor(options = undefined) {
            super();
            const opts = options == null ? {} : Object(options);
            let signal = null;
            if ('signal' in opts) {
                signal = opts.signal;
                if (signal != null && !(typeof AbortSignalCtor === 'function' && signal instanceof AbortSignalCtor)) {
                    throw new TypeError(
                        "Failed to construct 'CloseWatcher': Failed to read the 'signal' property from 'CloseWatcherOptions': Failed to convert value to 'AbortSignal'."
                    );
                }
            }
            const s = {
                active: true,
                signal,
                abortListener: null,
                oncancel: null,
                onclose: null,
            };
            state.set(this, s);
            if (signal) {
                if (signal.aborted) {
                    s.active = false;
                } else {
                    const abortListener = () => { deactivate(this, s); };
                    s.abortListener = abortListener;
                    signal.addEventListener('abort', abortListener, { once: true });
                }
            }
        }
    };

    // A class gives Chrome's constructor own-key shape (length/name/prototype),
    // while the proxy supplies the WebIDL call-without-new diagnostic.
    const CloseWatcher = new Proxy(CloseWatcherImpl, {
        apply() {
            throw new TypeError(
                "Failed to construct 'CloseWatcher': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        },
    });
    mask(CloseWatcher, 'CloseWatcher');

    const proto = CloseWatcherImpl.prototype;
    try { delete proto.constructor; } catch (_) {}
    Object.defineProperty(proto, 'oncancel', {
        enumerable: true,
        configurable: true,
        get: getter('oncancel', watcher => requireState(watcher).oncancel),
        set: setter('oncancel', (watcher, value) => setHandler(watcher, requireState(watcher), 'cancel', value)),
    });
    Object.defineProperty(proto, 'onclose', {
        enumerable: true,
        configurable: true,
        get: getter('onclose', watcher => requireState(watcher).onclose),
        set: setter('onclose', (watcher, value) => setHandler(watcher, requireState(watcher), 'close', value)),
    });
    Object.defineProperty(proto, 'close', {
        enumerable: true,
        configurable: true,
        writable: true,
        value: mask(function close() {
            const s = requireState(this);
            dispatchClose(this, s);
        }, 'close'),
    });
    Object.defineProperty(proto, 'destroy', {
        enumerable: true,
        configurable: true,
        writable: true,
        value: mask(function destroy() {
            deactivate(this, requireState(this));
        }, 'destroy'),
    });
    Object.defineProperty(proto, 'requestClose', {
        enumerable: true,
        configurable: true,
        writable: true,
        value: mask(function requestClose() {
            const s = requireState(this);
            if (!s.active) return;
            const cancel = trustedEvent('cancel', { cancelable: true });
            if (!this.dispatchEvent(cancel)) return;
            dispatchClose(this, s);
        }, 'requestClose'),
    });
    Object.defineProperty(proto, 'constructor', {
        enumerable: false,
        configurable: true,
        writable: true,
        value: CloseWatcher,
    });
    Object.defineProperty(proto, Symbol.toStringTag, {
        enumerable: false,
        configurable: true,
        writable: false,
        value: 'CloseWatcher',
    });

    try { Object.setPrototypeOf(CloseWatcher, EventTargetCtor); } catch (_) {}
    Object.defineProperty(globalThis, 'CloseWatcher', {
        value: CloseWatcher,
        writable: true,
        enumerable: false,
        configurable: true,
    });
})(globalThis);
