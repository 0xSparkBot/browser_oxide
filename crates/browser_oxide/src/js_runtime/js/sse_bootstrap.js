// EventSource (Server-Sent Events) transport + EventTarget integration.
// Public WebIDL descriptor normalization is completed later by
// network_webidl_bootstrap.js; this layer owns the real connection state.
((globalThis) => {
    const ops = Deno.core.ops;
    const CONNECTING = 0;
    const OPEN = 1;
    const CLOSED = 2;
    const state = new WeakMap();
    const markTrusted = globalThis.__bo_mark_trusted || ((event) => event);

    function eventOrigin(url) {
        try { return new URL(url).origin; } catch (_) { return ''; }
    }

    function dispatch(source, _type, event) {
        markTrusted(event);
        // EventTarget.dispatchEvent already invokes the corresponding on*
        // handler in BrowserOxide; calling it manually would double-fire it.
        source.dispatchEvent(event);
    }

    function normalizeUrl(input) {
        const base = globalThis.location && globalThis.location.href
            ? globalThis.location.href
            : undefined;
        return new URL(String(input), base).href;
    }

    async function connect(source, generation) {
        const current = state.get(source);
        if (!current || current.readyState === CLOSED || current.generation !== generation) return;

        try {
            const result = await ops.op_sse_connect(current.url);
            const active = state.get(source);
            if (!active || active.readyState === CLOSED || active.generation !== generation) {
                if (result && result.id >= 0) {
                    try { ops.op_sse_close(result.id); } catch (_) {}
                }
                return;
            }
            if (!result || !result.ok) {
                throw new Error(result && result.error ? result.error : 'EventSource connection failed');
            }

            active.sseId = result.id;
            active.readyState = OPEN;
            dispatch(source, 'open', new Event('open'));

            while (active.readyState === OPEN && active.sseId >= 0 && active.generation === generation) {
                const message = await ops.op_sse_recv(active.sseId);
                if (active.readyState === CLOSED || active.generation !== generation) break;

                if (!message || message.status === 'closed' || message.status === 'error') {
                    const id = active.sseId;
                    active.sseId = -1;
                    if (id >= 0) {
                        try { ops.op_sse_close(id); } catch (_) {}
                    }
                    active.readyState = CLOSED;
                    dispatch(source, 'error', new Event('error'));
                    break;
                }

                const type = message.event || 'message';
                const event = new MessageEvent(type, {
                    data: message.data,
                    origin: eventOrigin(active.url),
                    lastEventId: message.id || '',
                    source: null,
                    ports: [],
                });
                dispatch(source, type, event);
            }
        } catch (_) {
            const active = state.get(source);
            if (!active || active.readyState === CLOSED || active.generation !== generation) return;
            const id = active.sseId;
            active.sseId = -1;
            if (id >= 0) {
                try { ops.op_sse_close(id); } catch (_) {}
            }
            active.readyState = CLOSED;
            dispatch(source, 'error', new Event('error'));
        }
    }

    class EventSource extends EventTarget {
        constructor(url, options = {}) {
            super();
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'EventSource': 1 argument required, but only 0 present.");
            }
            const normalized = normalizeUrl(url);
            const entry = {
                url: normalized,
                withCredentials: !!options.withCredentials,
                readyState: CONNECTING,
                sseId: -1,
                generation: 1,
                handlers: Object.create(null),
            };
            state.set(this, entry);
            void connect(this, entry.generation);
        }

        get url() { return state.get(this)?.url ?? ''; }
        get withCredentials() { return state.get(this)?.withCredentials ?? false; }
        get readyState() { return state.get(this)?.readyState ?? CLOSED; }

        close() {
            const entry = state.get(this);
            if (!entry) return;
            entry.generation += 1;
            const id = entry.sseId;
            entry.sseId = -1;
            entry.readyState = CLOSED;
            if (id >= 0) {
                try { ops.op_sse_close(id); } catch (_) {}
            }
        }
    }

    const P = EventSource.prototype;
    for (const name of ['url', 'withCredentials', 'readyState']) {
        const descriptor = Object.getOwnPropertyDescriptor(P, name);
        if (descriptor) {
            Object.defineProperty(P, name, { ...descriptor, enumerable: true });
            try { if (typeof _maskFunction === 'function') _maskFunction(descriptor.get, `get ${name}`); } catch (_) {}
        }
    }
    for (const name of ['onopen', 'onmessage', 'onerror']) {
        const get = function() {
            const entry = state.get(this);
            return entry?.handlers[name] ?? null;
        };
        const set = function(value) {
            const entry = state.get(this);
            if (entry) entry.handlers[name] = typeof value === 'function' ? value : null;
        };
        try {
            Object.defineProperty(get, 'name', { value: `get ${name}`, configurable: true });
            Object.defineProperty(set, 'name', { value: `set ${name}`, configurable: true });
            if (typeof _maskFunction === 'function') {
                _maskFunction(get, `get ${name}`);
                _maskFunction(set, `set ${name}`);
            }
        } catch (_) {}
        Object.defineProperty(P, name, { get, set, enumerable: true, configurable: true });
    }
    const closeDescriptor = Object.getOwnPropertyDescriptor(P, 'close');
    if (closeDescriptor) {
        Object.defineProperty(P, 'close', { ...closeDescriptor, enumerable: true });
        try { if (typeof _maskFunction === 'function') _maskFunction(closeDescriptor.value, 'close'); } catch (_) {}
    }
    Object.defineProperty(P, Symbol.toStringTag, { value: 'EventSource', configurable: true });
    try { if (typeof _maskFunction === 'function') _maskFunction(EventSource, 'EventSource'); } catch (_) {}

    for (const [name, value] of [['CONNECTING', CONNECTING], ['OPEN', OPEN], ['CLOSED', CLOSED]]) {
        Object.defineProperty(EventSource, name, {
            value,
            writable: false,
            enumerable: true,
            configurable: false,
        });
        Object.defineProperty(EventSource.prototype, name, {
            value,
            writable: false,
            enumerable: true,
            configurable: false,
        });
    }
    globalThis.EventSource = EventSource;
})(globalThis);
