// WebTransport WebIDL + fail-closed session lifecycle.
//
// BrowserOxide has an HTTP/3 client, but it does not yet implement the
// WebTransport extended-CONNECT/session protocol.  Expose Chromium's public
// WebTransport object model without pretending that a session connected:
// valid HTTPS construction succeeds synchronously, then ready/closed reject
// asynchronously with a session WebTransportError.  The internal stream
// objects are real ReadableStream/WritableStream instances so generic feature
// detection and error handling observe browser-shaped values.
((globalThis) => {
    'use strict';

    if (typeof globalThis.ReadableStream !== 'function'
        || typeof globalThis.WritableStream !== 'function'
        || typeof globalThis.DOMException !== 'function') {
        return;
    }

    const _mask = typeof globalThis._maskFunction === 'function'
        ? globalThis._maskFunction
        : (fn) => fn;
    const _internal = {};
    const _transportState = new WeakMap();
    const _bidiState = new WeakMap();
    const _datagramState = new WeakMap();
    const _errorState = new WeakMap();

    const _native = (fn, name) => {
        try { Object.defineProperty(fn, 'name', { value: name, configurable: true }); } catch (_) {}
        try { _mask(fn, name); } catch (_) {}
        return fn;
    };
    const _method = (prototype, name, fn, length = 0) => {
        _native(fn, name);
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        Object.defineProperty(prototype, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };
    const _getter = (prototype, name, get, set = undefined) => {
        _native(get, `get ${name}`);
        if (set) _native(set, `set ${name}`);
        Object.defineProperty(prototype, name, {
            get, set, enumerable: true, configurable: true,
        });
    };
    const _tag = (prototype, name) => {
        Object.defineProperty(prototype, Symbol.toStringTag, {
            value: name, writable: false, enumerable: false, configurable: true,
        });
    };
    const _installGlobal = (name, value) => {
        Object.defineProperty(globalThis, name, {
            value, writable: true, enumerable: false, configurable: true,
        });
    };
    const _illegalConstructor = (name) => {
        throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
    };
    const _illegalInvocation = () => {
        throw new TypeError('Illegal invocation');
    };
    const _webTransportError = (message, source = 'stream', streamErrorCode = null) => {
        const error = new WebTransportError({ message, streamErrorCode });
        const state = _errorState.get(error);
        if (state) state.source = source;
        return error;
    };
    const _networkError = (method) => new DOMException(
        `Failed to execute '${method}' on 'WebTransport': No connection.`,
        'NetworkError',
    );

    // ---------------------------------------------------------------------
    // WebTransportError
    // ---------------------------------------------------------------------
    const WebTransportError = {
        WebTransportError: function(init = undefined) {
            if (!new.target) {
                throw new TypeError(
                    "Failed to construct 'WebTransportError': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
                );
            }
            if (init !== undefined && init !== null && typeof init !== 'object') {
                throw new TypeError(
                    "Failed to construct 'WebTransportError': The provided value is not of type 'WebTransportErrorInit'."
                );
            }
            const dict = init == null ? {} : init;
            const message = dict.message === undefined ? '' : String(dict.message);
            let streamErrorCode = null;
            if (dict.streamErrorCode !== undefined && dict.streamErrorCode !== null) {
                let value = Number(dict.streamErrorCode);
                if (!Number.isFinite(value)) value = 0;
                value = Math.max(0, Math.trunc(value));
                streamErrorCode = value;
            }
            const object = Reflect.construct(
                DOMException,
                [message, 'WebTransportError'],
                new.target,
            );
            _errorState.set(object, { source: 'stream', streamErrorCode });
            return object;
        },
    }.WebTransportError;
    Object.setPrototypeOf(WebTransportError, DOMException);
    Object.setPrototypeOf(WebTransportError.prototype, DOMException.prototype);
    const _errorCtorDescriptor = Object.getOwnPropertyDescriptor(
        WebTransportError.prototype, 'constructor'
    );
    delete WebTransportError.prototype.constructor;
    _getter(WebTransportError.prototype, 'streamErrorCode', function streamErrorCode() {
        const state = _errorState.get(this);
        if (!state) return _illegalInvocation();
        return state.streamErrorCode;
    });
    _getter(WebTransportError.prototype, 'source', function source() {
        const state = _errorState.get(this);
        if (!state) return _illegalInvocation();
        return state.source;
    });
    Object.defineProperty(WebTransportError.prototype, 'constructor', {
        ..._errorCtorDescriptor, value: WebTransportError,
    });
    _tag(WebTransportError.prototype, 'WebTransportError');
    _native(WebTransportError, 'WebTransportError');

    // ---------------------------------------------------------------------
    // Internal stream wrappers. Public construction remains illegal.
    // ---------------------------------------------------------------------
    function WebTransportBidirectionalStream() {
        if (arguments[0] !== _internal) _illegalConstructor('WebTransportBidirectionalStream');
        _bidiState.set(this, { readable: arguments[1], writable: arguments[2] });
    }
    const _bidiCtorDescriptor = Object.getOwnPropertyDescriptor(
        WebTransportBidirectionalStream.prototype, 'constructor'
    );
    delete WebTransportBidirectionalStream.prototype.constructor;
    _getter(WebTransportBidirectionalStream.prototype, 'readable', function readable() {
        const state = _bidiState.get(this);
        if (!state) return _illegalInvocation();
        return state.readable;
    });
    _getter(WebTransportBidirectionalStream.prototype, 'writable', function writable() {
        const state = _bidiState.get(this);
        if (!state) return _illegalInvocation();
        return state.writable;
    });
    Object.defineProperty(WebTransportBidirectionalStream.prototype, 'constructor', {
        ..._bidiCtorDescriptor, value: WebTransportBidirectionalStream,
    });
    _tag(WebTransportBidirectionalStream.prototype, 'WebTransportBidirectionalStream');
    _native(WebTransportBidirectionalStream, 'WebTransportBidirectionalStream');

    const _normalizeHighWaterMark = (value, property) => {
        const number = Number(value);
        if (!Number.isFinite(number)) {
            throw new TypeError(
                `Failed to set the '${property}' property on 'WebTransportDatagramDuplexStream': The provided double value is non-finite.`
            );
        }
        return Math.max(0, Math.trunc(number));
    };
    const _normalizeMaxAge = (value, property) => {
        if (value === null || value === undefined) return null;
        const number = Number(value);
        if (!Number.isFinite(number)) {
            throw new TypeError(
                `Failed to set the '${property}' property on 'WebTransportDatagramDuplexStream': The provided double value is non-finite.`
            );
        }
        return number <= 0 ? null : number;
    };

    function WebTransportDatagramDuplexStream() {
        if (arguments[0] !== _internal) _illegalConstructor('WebTransportDatagramDuplexStream');
        _datagramState.set(this, {
            readable: arguments[1],
            writable: arguments[2],
            maxDatagramSize: 1024,
            incomingMaxAge: null,
            outgoingMaxAge: null,
            incomingHighWaterMark: 1,
            outgoingHighWaterMark: 1,
        });
    }
    const _datagramCtorDescriptor = Object.getOwnPropertyDescriptor(
        WebTransportDatagramDuplexStream.prototype, 'constructor'
    );
    delete WebTransportDatagramDuplexStream.prototype.constructor;
    _getter(WebTransportDatagramDuplexStream.prototype, 'readable', function readable() {
        const state = _datagramState.get(this);
        if (!state) return _illegalInvocation();
        return state.readable;
    });
    _getter(WebTransportDatagramDuplexStream.prototype, 'writable', function writable() {
        const state = _datagramState.get(this);
        if (!state) return _illegalInvocation();
        return state.writable;
    });
    _getter(WebTransportDatagramDuplexStream.prototype, 'maxDatagramSize', function maxDatagramSize() {
        const state = _datagramState.get(this);
        if (!state) return _illegalInvocation();
        return state.maxDatagramSize;
    });
    _getter(
        WebTransportDatagramDuplexStream.prototype,
        'incomingMaxAge',
        function incomingMaxAge() {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            return state.incomingMaxAge;
        },
        function incomingMaxAge(value) {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            state.incomingMaxAge = _normalizeMaxAge(value, 'incomingMaxAge');
        },
    );
    _getter(
        WebTransportDatagramDuplexStream.prototype,
        'outgoingMaxAge',
        function outgoingMaxAge() {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            return state.outgoingMaxAge;
        },
        function outgoingMaxAge(value) {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            state.outgoingMaxAge = _normalizeMaxAge(value, 'outgoingMaxAge');
        },
    );
    _getter(
        WebTransportDatagramDuplexStream.prototype,
        'incomingHighWaterMark',
        function incomingHighWaterMark() {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            return state.incomingHighWaterMark;
        },
        function incomingHighWaterMark(value) {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            state.incomingHighWaterMark = _normalizeHighWaterMark(value, 'incomingHighWaterMark');
        },
    );
    _getter(
        WebTransportDatagramDuplexStream.prototype,
        'outgoingHighWaterMark',
        function outgoingHighWaterMark() {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            return state.outgoingHighWaterMark;
        },
        function outgoingHighWaterMark(value) {
            const state = _datagramState.get(this);
            if (!state) return _illegalInvocation();
            state.outgoingHighWaterMark = _normalizeHighWaterMark(value, 'outgoingHighWaterMark');
        },
    );
    Object.defineProperty(WebTransportDatagramDuplexStream.prototype, 'constructor', {
        ..._datagramCtorDescriptor, value: WebTransportDatagramDuplexStream,
    });
    _tag(WebTransportDatagramDuplexStream.prototype, 'WebTransportDatagramDuplexStream');
    _native(WebTransportDatagramDuplexStream, 'WebTransportDatagramDuplexStream');

    const _readableEndpoint = () => {
        let controller = null;
        const stream = new ReadableStream({
            start(value) { controller = value; },
        });
        return { stream, controller };
    };
    const _writableEndpoint = () => {
        let controller = null;
        const stream = new WritableStream({
            start(value) { controller = value; },
            write() { throw _networkError('write'); },
        });
        return { stream, controller };
    };
    const _failEndpoint = (endpoint, error) => {
        try { endpoint.controller?.error(error); } catch (_) {}
    };

    // ---------------------------------------------------------------------
    // WebTransport
    // ---------------------------------------------------------------------
    function WebTransport(url) {
        if (!new.target) {
            throw new TypeError(
                "Failed to construct 'WebTransport': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        }
        if (arguments.length < 1) {
            throw new TypeError(
                "Failed to construct 'WebTransport': 1 argument required, but only 0 present."
            );
        }

        const source = String(url);
        // Chromium requires an absolute URL string here; unlike many DOM URL
        // consumers, WebTransport never resolves a relative input against the
        // incumbent document.  BrowserOxide's generic URL shim accepts some
        // empty/relative inputs, so gate them before constructing URL.
        if (!/^[A-Za-z][A-Za-z0-9+.-]*:/.test(source)) {
            throw new DOMException(
                `Failed to construct 'WebTransport': The URL '${source}' is invalid.`,
                'SyntaxError',
            );
        }
        let parsed;
        try {
            parsed = new URL(source);
        } catch (_) {
            throw new DOMException(
                `Failed to construct 'WebTransport': The URL '${source}' is invalid.`,
                'SyntaxError',
            );
        }
        const scheme = String(parsed.protocol || '').replace(/:$/, '');
        if (scheme !== 'https') {
            throw new DOMException(
                `Failed to construct 'WebTransport': The URL's scheme must be 'https'. '${scheme}' is not allowed.`,
                'SyntaxError',
            );
        }
        if (parsed.hash) {
            throw new DOMException(
                `Failed to construct 'WebTransport': The URL contains a fragment identifier ('${parsed.hash}'). Fragment identifiers are not allowed in WebTransport URLs.`,
                'SyntaxError',
            );
        }

        const incomingUni = _readableEndpoint();
        const incomingBi = _readableEndpoint();
        const datagramReadable = _readableEndpoint();
        const datagramWritable = _writableEndpoint();
        const datagrams = new WebTransportDatagramDuplexStream(
            _internal,
            datagramReadable.stream,
            datagramWritable.stream,
        );

        let readyResolve;
        let readyReject;
        let closedResolve;
        let closedReject;
        const ready = new Promise((resolve, reject) => {
            readyResolve = resolve;
            readyReject = reject;
        });
        const closed = new Promise((resolve, reject) => {
            closedResolve = resolve;
            closedReject = reject;
        });
        // The original promises stay observable as rejected promises, while an
        // internal handler prevents an unused transport from becoming a host-
        // level unhandled-rejection artifact.
        ready.catch(() => {});
        closed.catch(() => {});

        const state = {
            phase: 'connecting',
            protocol: '',
            ready,
            readyResolve,
            readyReject,
            closed,
            closedResolve,
            closedReject,
            incomingUni,
            incomingBi,
            datagramReadable,
            datagramWritable,
            datagrams,
        };
        _transportState.set(this, state);

        // No WebTransport extended-CONNECT backend exists yet. Match browser
        // construction semantics (synchronous success) and fail the session
        // asynchronously instead of lying that a QUIC/WebTransport session is
        // connected.
        queueMicrotask(() => {
            if (state.phase !== 'connecting') return;
            state.phase = 'failed';
            const error = _webTransportError('Opening handshake failed.', 'session', null);
            _failEndpoint(state.incomingUni, error);
            _failEndpoint(state.incomingBi, error);
            _failEndpoint(state.datagramReadable, error);
            _failEndpoint(state.datagramWritable, error);
            state.readyReject(error);
            state.closedReject(error);
        });
    }

    const _transportCtorDescriptor = Object.getOwnPropertyDescriptor(
        WebTransport.prototype, 'constructor'
    );
    delete WebTransport.prototype.constructor;
    _getter(WebTransport.prototype, 'incomingUnidirectionalStreams', function incomingUnidirectionalStreams() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        return state.incomingUni.stream;
    });
    _getter(WebTransport.prototype, 'incomingBidirectionalStreams', function incomingBidirectionalStreams() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        return state.incomingBi.stream;
    });
    _getter(WebTransport.prototype, 'datagrams', function datagrams() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        return state.datagrams;
    });
    _getter(WebTransport.prototype, 'ready', function ready() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        return state.ready;
    });
    _getter(WebTransport.prototype, 'closed', function closed() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        return state.closed;
    });
    _method(WebTransport.prototype, 'close', function close() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        // A not-yet-connected fail-closed transport has no wire session to
        // close. Chrome returns undefined here; the queued opening failure
        // continues to settle ready/closed.
        return undefined;
    });
    _method(WebTransport.prototype, 'createBidirectionalStream', function createBidirectionalStream() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        if (state.phase !== 'connected') {
            return Promise.reject(_networkError('createBidirectionalStream'));
        }
        const readable = _readableEndpoint().stream;
        const writable = _writableEndpoint().stream;
        return Promise.resolve(new WebTransportBidirectionalStream(_internal, readable, writable));
    });
    _method(WebTransport.prototype, 'createUnidirectionalStream', function createUnidirectionalStream() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        if (state.phase !== 'connected') {
            return Promise.reject(_networkError('createUnidirectionalStream'));
        }
        return Promise.resolve(_writableEndpoint().stream);
    });
    _getter(WebTransport.prototype, 'protocol', function protocol() {
        const state = _transportState.get(this);
        if (!state) return _illegalInvocation();
        return state.protocol;
    });
    Object.defineProperty(WebTransport.prototype, 'constructor', {
        ..._transportCtorDescriptor, value: WebTransport,
    });
    _tag(WebTransport.prototype, 'WebTransport');
    _native(WebTransport, 'WebTransport');

    _installGlobal('WebTransport', WebTransport);
    _installGlobal('WebTransportBidirectionalStream', WebTransportBidirectionalStream);
    _installGlobal('WebTransportDatagramDuplexStream', WebTransportDatagramDuplexStream);
    _installGlobal('WebTransportError', WebTransportError);
})(globalThis);
