// WHATWG Streams — lightweight browser-compatible Readable/Writable/Transform streams.
// Internal state intentionally lives in WeakMaps so WebIDL instances expose the
// same empty own-property surface as Chromium.
((globalThis) => {
    // A structural guard avoids an observable implementation marker on any
    // constructor/prototype while still making bootstrap re-runs idempotent.
    if (
        globalThis.ReadableStream?.prototype?.[Symbol.toStringTag] === 'ReadableStream' &&
        typeof globalThis.ReadableStream.prototype.values === 'function' &&
        globalThis.WritableStream?.prototype?.[Symbol.toStringTag] === 'WritableStream' &&
        globalThis.TransformStream?.prototype?.[Symbol.toStringTag] === 'TransformStream'
    ) {
        return;
    }

    const _internal = {};
    const _readableStates = new WeakMap();
    const _readerStates = new WeakMap();
    const _readableControllerStates = new WeakMap();
    const _writableStates = new WeakMap();
    const _writerStates = new WeakMap();
    const _writableControllerStates = new WeakMap();
    const _transformStates = new WeakMap();

    const _require = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const _rs = value => _require(_readableStates, value);
    const _rr = value => _require(_readerStates, value);
    const _rc = value => _require(_readableControllerStates, value);
    const _ws = value => _require(_writableStates, value);
    const _ww = value => _require(_writerStates, value);
    const _wc = value => _require(_writableControllerStates, value);
    const _ts = value => _require(_transformStates, value);

    const _setIdlEnumerable = (prototype, names) => {
        for (const name of names) {
            const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
            if (!descriptor) continue;
            descriptor.enumerable = true;
            Object.defineProperty(prototype, name, descriptor);
        }
    };
    const _setTag = (prototype, tag) => {
        Object.defineProperty(prototype, Symbol.toStringTag, {
            value: tag,
            configurable: true,
        });
    };

    const _settleReaderClosed = (reader, state) => {
        if (state.closedSettled) return;
        const streamState = state.stream ? _rs(state.stream) : null;
        if (!streamState) return;
        if (streamState.state === 'closed') {
            state.closedSettled = true;
            state.closedResolve();
        } else if (streamState.state === 'errored') {
            state.closedSettled = true;
            state.closedReject(streamState.error);
        }
    };

    const _drainReader = (reader) => {
        const readerState = _rr(reader);
        const stream = readerState.stream;
        if (!stream) return;
        const state = _rs(stream);
        while (readerState.pending.length > 0) {
            if (state.queue.length > 0) {
                readerState.pending.shift().resolve({
                    value: state.queue.shift(),
                    done: false,
                });
            } else if (state.state === 'closed') {
                readerState.pending.shift().resolve({ value: undefined, done: true });
            } else if (state.state === 'errored') {
                readerState.pending.shift().reject(state.error);
            } else {
                break;
            }
        }
        _settleReaderClosed(reader, readerState);
    };

    const _drainReadable = (stream) => {
        const state = _rs(stream);
        if (state.reader) _drainReader(state.reader);
    };

    const _pullReadable = (stream) => {
        const state = _rs(stream);
        if (state.pullInFlight || state.state !== 'readable') return;
        if (typeof state.underlyingSource.pull !== 'function') return;
        state.pullInFlight = true;
        try {
            const result = state.underlyingSource.pull(state.controller);
            Promise.resolve(result).then(
                () => { state.pullInFlight = false; },
                error => {
                    state.pullInFlight = false;
                    if (state.controller) state.controller.error(error);
                },
            );
        } catch (error) {
            state.pullInFlight = false;
            if (state.controller) state.controller.error(error);
        }
    };

    const _startReadable = (stream) => {
        const state = _rs(stream);
        if (state.started) return;
        state.started = true;
        const controller = new ReadableStreamDefaultController(_internal, stream);
        state.controller = controller;
        if (typeof state.underlyingSource.start === 'function') {
            try {
                const result = state.underlyingSource.start(controller);
                if (result && typeof result.then === 'function') {
                    result.catch(error => controller.error(error));
                }
            } catch (error) {
                controller.error(error);
                return;
            }
        }
        _pullReadable(stream);
    };

    class ReadableStreamDefaultController {
        constructor() {
            if (arguments[0] !== _internal) {
                throw new TypeError("Illegal constructor");
            }
            _readableControllerStates.set(this, { stream: arguments[1] });
        }
        get desiredSize() {
            const state = _rs(_rc(this).stream);
            return state.state === 'readable' ? 1 : 0;
        }
        close() {
            const stream = _rc(this).stream;
            const state = _rs(stream);
            if (state.state !== 'readable') return;
            state.state = 'closed';
            _drainReadable(stream);
        }
        enqueue() {
            const stream = _rc(this).stream;
            const state = _rs(stream);
            if (state.state !== 'readable') {
                throw new TypeError(
                    'ReadableStreamDefaultController.enqueue called on ' +
                    state.state + ' stream'
                );
            }
            state.queue.push(arguments[0]);
            _drainReadable(stream);
        }
        error() {
            const stream = _rc(this).stream;
            const state = _rs(stream);
            if (state.state !== 'readable') return;
            state.state = 'errored';
            state.error = arguments[0];
            _drainReadable(stream);
        }
    }

    class ReadableStreamDefaultReader {
        constructor(stream) {
            const streamState = _rs(stream);
            if (streamState.locked) {
                throw new TypeError('ReadableStream is locked to another reader');
            }
            streamState.locked = true;
            let closedResolve;
            let closedReject;
            const closed = new Promise((resolve, reject) => {
                closedResolve = resolve;
                closedReject = reject;
            });
            const state = {
                stream,
                pending: [],
                closed,
                closedResolve,
                closedReject,
                closedSettled: false,
            };
            _readerStates.set(this, state);
            _settleReaderClosed(this, state);
        }
        get closed() { return _rr(this).closed; }
        read() {
            const readerState = _rr(this);
            if (!readerState.stream) {
                return Promise.reject(new TypeError('reader released'));
            }
            const stream = readerState.stream;
            const state = _rs(stream);
            if (state.state === 'errored') return Promise.reject(state.error);
            if (state.queue.length > 0) {
                const value = state.queue.shift();
                queueMicrotask(() => _pullReadable(stream));
                return Promise.resolve({ value, done: false });
            }
            if (state.state === 'closed') {
                return Promise.resolve({ value: undefined, done: true });
            }
            return new Promise((resolve, reject) => {
                readerState.pending.push({ resolve, reject });
                queueMicrotask(() => _pullReadable(stream));
            });
        }
        cancel() {
            const readerState = _rr(this);
            if (!readerState.stream) return Promise.resolve();
            const stream = readerState.stream;
            const state = _rs(stream);
            state.state = 'closed';
            state.queue.length = 0;
            _drainReader(this);
            const cancel = state.underlyingSource?.cancel;
            if (typeof cancel !== 'function') return Promise.resolve();
            try { return Promise.resolve(cancel.call(state.underlyingSource, arguments[0])); }
            catch (error) { return Promise.reject(error); }
        }
        releaseLock() {
            const readerState = _rr(this);
            if (!readerState.stream) return;
            const state = _rs(readerState.stream);
            state.locked = false;
            if (state.reader === this) state.reader = null;
            readerState.stream = null;
        }
    }

    class ReadableStream {
        constructor() {
            const underlyingSource = arguments[0] || {};
            _readableStates.set(this, {
                underlyingSource,
                queue: [],
                state: 'readable',
                error: null,
                locked: false,
                reader: null,
                pullInFlight: false,
                started: false,
                controller: null,
            });
            queueMicrotask(() => _startReadable(this));
        }
        get locked() { return _rs(this).locked; }
        cancel() {
            const state = _rs(this);
            if (state.state === 'closed') return Promise.resolve();
            state.state = 'closed';
            state.queue.length = 0;
            _drainReadable(this);
            const cancel = state.underlyingSource.cancel;
            if (typeof cancel !== 'function') return Promise.resolve();
            try { return Promise.resolve(cancel.call(state.underlyingSource, arguments[0])); }
            catch (error) { return Promise.reject(error); }
        }
        getReader() {
            const reader = new ReadableStreamDefaultReader(this);
            const state = _rs(this);
            state.reader = reader;
            _drainReader(reader);
            return reader;
        }
        tee() {
            const sourceReader = this.getReader();
            let firstController = null;
            let secondController = null;
            const first = new ReadableStream({
                start(controller) { firstController = controller; },
                cancel() { return sourceReader.cancel(); },
            });
            const second = new ReadableStream({
                start(controller) { secondController = controller; },
                cancel() { return sourceReader.cancel(); },
            });
            const pump = () => {
                sourceReader.read().then(
                    ({ done, value }) => {
                        if (done) {
                            try { firstController?.close(); } catch (_) {}
                            try { secondController?.close(); } catch (_) {}
                            return;
                        }
                        try { firstController?.enqueue(value); } catch (_) {}
                        try { secondController?.enqueue(value); } catch (_) {}
                        pump();
                    },
                    error => {
                        try { firstController?.error(error); } catch (_) {}
                        try { secondController?.error(error); } catch (_) {}
                    },
                );
            };
            queueMicrotask(() => queueMicrotask(pump));
            return [first, second];
        }
        pipeTo(destination) {
            if (!(destination instanceof WritableStream)) {
                return Promise.reject(new TypeError('pipeTo requires a WritableStream'));
            }
            const reader = this.getReader();
            const writer = destination.getWriter();
            return new Promise((resolve, reject) => {
                const step = () => {
                    reader.read().then(
                        ({ done, value }) => {
                            if (done) {
                                writer.close().then(resolve, reject);
                                return;
                            }
                            writer.write(value).then(step, reject);
                        },
                        error => {
                            writer.abort(error).then(() => reject(error), () => reject(error));
                        },
                    );
                };
                step();
            });
        }
        pipeThrough(transform) {
            if (!transform || !transform.readable || !transform.writable) {
                throw new TypeError('pipeThrough requires a TransformStream');
            }
            this.pipeTo(transform.writable, arguments[1]).catch(() => {});
            return transform.readable;
        }
        values() {
            const reader = this.getReader();
            const options = arguments[0] || {};
            return {
                next() { return reader.read(); },
                return() {
                    const finish = () => {
                        reader.releaseLock();
                        return { value: undefined, done: true };
                    };
                    if (options.preventCancel) return Promise.resolve(finish());
                    return Promise.resolve(reader.cancel()).then(finish);
                },
                [Symbol.asyncIterator]() { return this; },
            };
        }
    }

    const _readableValues = ReadableStream.prototype.values;
    Object.defineProperty(ReadableStream.prototype, Symbol.asyncIterator, {
        value: _readableValues,
        writable: true,
        configurable: true,
    });

    const _settleWriterClosed = (writer, error) => {
        if (!writer) return;
        const state = _ww(writer);
        if (state.closedSettled) return;
        state.closedSettled = true;
        if (error === undefined) state.closedResolve();
        else state.closedReject(error);
    };

    class WritableStreamDefaultController {
        constructor() {
            if (arguments[0] !== _internal) throw new TypeError('Illegal constructor');
            const stream = arguments[1];
            let signal;
            try { signal = new AbortController().signal; } catch (_) { signal = undefined; }
            _writableControllerStates.set(this, { stream, signal });
        }
        get signal() { return _wc(this).signal; }
        error() {
            const stream = _wc(this).stream;
            const state = _ws(stream);
            state.state = 'errored';
            state.error = arguments[0];
            _settleWriterClosed(state.writer, state.error);
        }
    }

    class WritableStreamDefaultWriter {
        constructor(stream) {
            const streamState = _ws(stream);
            if (streamState.locked) {
                throw new TypeError('WritableStream is locked to another writer');
            }
            streamState.locked = true;
            let closedResolve;
            let closedReject;
            const closed = new Promise((resolve, reject) => {
                closedResolve = resolve;
                closedReject = reject;
            });
            _writerStates.set(this, {
                stream,
                ready: Promise.resolve(),
                closed,
                closedResolve,
                closedReject,
                closedSettled: false,
            });
            streamState.writer = this;
            if (streamState.state === 'closed') _settleWriterClosed(this);
            if (streamState.state === 'errored') _settleWriterClosed(this, streamState.error);
        }
        get closed() { return _ww(this).closed; }
        get desiredSize() {
            const state = _ww(this);
            if (!state.stream) return null;
            return _ws(state.stream).state === 'writable' ? 1 : 0;
        }
        get ready() { return _ww(this).ready; }
        write() {
            const writerState = _ww(this);
            if (!writerState.stream) return Promise.reject(new TypeError('released'));
            const streamState = _ws(writerState.stream);
            if (streamState.state !== 'writable') {
                return Promise.reject(new TypeError('write on ' + streamState.state + ' stream'));
            }
            const write = streamState.underlyingSink.write;
            if (typeof write !== 'function') return Promise.resolve();
            try {
                return Promise.resolve(write.call(
                    streamState.underlyingSink,
                    arguments[0],
                    streamState.controller,
                ));
            } catch (error) {
                return Promise.reject(error);
            }
        }
        close() {
            const writerState = _ww(this);
            if (!writerState.stream) return Promise.reject(new TypeError('released'));
            const streamState = _ws(writerState.stream);
            if (streamState.state !== 'writable') {
                return Promise.reject(new TypeError('close on ' + streamState.state + ' stream'));
            }
            streamState.state = 'closed';
            const close = streamState.underlyingSink.close;
            const result = typeof close === 'function'
                ? Promise.resolve().then(() => close.call(streamState.underlyingSink))
                : Promise.resolve();
            return result.then(() => {
                _settleWriterClosed(this);
            });
        }
        abort() {
            const writerState = _ww(this);
            if (!writerState.stream) return Promise.resolve();
            const streamState = _ws(writerState.stream);
            const reason = arguments[0];
            streamState.state = 'errored';
            streamState.error = reason;
            const abort = streamState.underlyingSink.abort;
            const result = typeof abort === 'function'
                ? Promise.resolve().then(() => abort.call(streamState.underlyingSink, reason))
                : Promise.resolve();
            return result.then(() => {
                _settleWriterClosed(this, reason);
            });
        }
        releaseLock() {
            const writerState = _ww(this);
            if (!writerState.stream) return;
            const streamState = _ws(writerState.stream);
            streamState.locked = false;
            if (streamState.writer === this) streamState.writer = null;
            writerState.stream = null;
        }
    }

    class WritableStream {
        constructor() {
            const underlyingSink = arguments[0] || {};
            const state = {
                underlyingSink,
                state: 'writable',
                error: null,
                locked: false,
                controller: null,
                writer: null,
            };
            _writableStates.set(this, state);
            state.controller = new WritableStreamDefaultController(_internal, this);
            if (typeof underlyingSink.start === 'function') {
                try { underlyingSink.start(state.controller); }
                catch (error) {
                    state.state = 'errored';
                    state.error = error;
                }
            }
        }
        get locked() { return _ws(this).locked; }
        getWriter() { return new WritableStreamDefaultWriter(this); }
        abort() {
            const state = _ws(this);
            const reason = arguments[0];
            state.state = 'errored';
            state.error = reason;
            const abort = state.underlyingSink.abort;
            const result = typeof abort === 'function'
                ? Promise.resolve().then(() => abort.call(state.underlyingSink, reason))
                : Promise.resolve();
            return result.then(() => _settleWriterClosed(state.writer, reason));
        }
        close() {
            const state = _ws(this);
            if (state.state !== 'writable') return Promise.resolve();
            state.state = 'closed';
            const close = state.underlyingSink.close;
            const result = typeof close === 'function'
                ? Promise.resolve().then(() => close.call(state.underlyingSink))
                : Promise.resolve();
            return result.then(() => _settleWriterClosed(state.writer));
        }
    }

    class TransformStream {
        constructor() {
            const transformer = arguments[0] || {};
            let readableController = null;
            const readable = new ReadableStream({
                start(controller) { readableController = controller; },
            });
            const writable = new WritableStream({
                async write(chunk) {
                    if (typeof transformer.transform === 'function') {
                        await transformer.transform(chunk, readableController);
                    } else {
                        readableController?.enqueue(chunk);
                    }
                },
                close() {
                    if (typeof transformer.flush === 'function') {
                        try { transformer.flush(readableController); } catch (_) {}
                    }
                    readableController?.close();
                },
                abort(error) { readableController?.error(error); },
            });
            _transformStates.set(this, { readable, writable });
            if (typeof transformer.start === 'function') {
                queueMicrotask(() => {
                    try { transformer.start(readableController); } catch (_) {}
                });
            }
        }
        get readable() { return _ts(this).readable; }
        get writable() { return _ts(this).writable; }
    }

    _setIdlEnumerable(ReadableStream.prototype, [
        'locked', 'cancel', 'getReader', 'pipeThrough', 'pipeTo', 'tee', 'values',
    ]);
    _setIdlEnumerable(ReadableStreamDefaultReader.prototype, [
        'closed', 'cancel', 'read', 'releaseLock',
    ]);
    _setIdlEnumerable(ReadableStreamDefaultController.prototype, [
        'desiredSize', 'close', 'enqueue', 'error',
    ]);
    _setIdlEnumerable(WritableStream.prototype, [
        'locked', 'abort', 'close', 'getWriter',
    ]);
    _setIdlEnumerable(WritableStreamDefaultWriter.prototype, [
        'closed', 'desiredSize', 'ready', 'abort', 'close', 'releaseLock', 'write',
    ]);
    _setIdlEnumerable(WritableStreamDefaultController.prototype, ['signal', 'error']);
    _setIdlEnumerable(TransformStream.prototype, ['readable', 'writable']);

    _setTag(ReadableStream.prototype, 'ReadableStream');
    _setTag(ReadableStreamDefaultReader.prototype, 'ReadableStreamDefaultReader');
    _setTag(ReadableStreamDefaultController.prototype, 'ReadableStreamDefaultController');
    _setTag(WritableStream.prototype, 'WritableStream');
    _setTag(WritableStreamDefaultWriter.prototype, 'WritableStreamDefaultWriter');
    _setTag(WritableStreamDefaultController.prototype, 'WritableStreamDefaultController');
    _setTag(TransformStream.prototype, 'TransformStream');

    globalThis.ReadableStream = ReadableStream;
    globalThis.ReadableStreamDefaultReader = ReadableStreamDefaultReader;
    globalThis.ReadableStreamDefaultController = ReadableStreamDefaultController;
    globalThis.WritableStream = WritableStream;
    globalThis.WritableStreamDefaultWriter = WritableStreamDefaultWriter;
    globalThis.WritableStreamDefaultController = WritableStreamDefaultController;
    globalThis.TransformStream = TransformStream;
})(globalThis);
