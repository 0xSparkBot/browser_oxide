// WHATWG Compression Streams API.
((globalThis) => {
    'use strict';

    const ops = globalThis.Deno && Deno.core && Deno.core.ops;
    if (!ops || typeof ops.op_compression_stream_transform !== 'function'
        || typeof globalThis.ReadableStream !== 'function'
        || typeof globalThis.WritableStream !== 'function') {
        return;
    }

    const FORMATS = new Set(['gzip', 'deflate', 'deflate-raw']);
    const normalizeFormat = (kind, args) => {
        if (args.length < 1) {
            throw new TypeError(
                `Failed to construct '${kind}': 1 argument required, but only 0 present.`
            );
        }
        const format = String(args[0]);
        if (!FORMATS.has(format)) {
            throw new TypeError(
                `Failed to construct '${kind}': Unsupported compression format: '${format}'`
            );
        }
        return format;
    };

    const copyBufferSource = (chunk) => {
        if (chunk instanceof ArrayBuffer) return new Uint8Array(chunk).slice();
        if (ArrayBuffer.isView(chunk)) {
            return new Uint8Array(chunk.buffer, chunk.byteOffset, chunk.byteLength).slice();
        }
        throw new TypeError('CompressionStream input must be an ArrayBuffer or ArrayBufferView');
    };

    const concatChunks = (chunks) => {
        let total = 0;
        for (const chunk of chunks) total += chunk.byteLength;
        const input = new Uint8Array(total);
        let offset = 0;
        for (const chunk of chunks) {
            input.set(chunk, offset);
            offset += chunk.byteLength;
        }
        return input;
    };

    const createPair = (format, decompress) => {
        const chunks = [];
        let readableController = null;
        const readable = new ReadableStream({
            start(controller) { readableController = controller; },
        });
        const writable = new WritableStream({
            write(chunk) { chunks.push(copyBufferSource(chunk)); },
            close() {
                const input = concatChunks(chunks);
                const result = ops.op_compression_stream_transform(format, decompress, input);
                if (!result || !result.ok) {
                    const error = new TypeError(
                        decompress ? 'Error during decompression.' : 'Error during compression.'
                    );
                    try { readableController?.error(error); } catch (_) {}
                    throw error;
                }
                const output = new Uint8Array(result.data || []);
                if (output.byteLength) readableController?.enqueue(output);
                readableController?.close();
            },
            abort(reason) {
                try { readableController?.error(reason); } catch (_) {}
            },
        });
        return { readable, writable };
    };

    const install = (name, decompress) => {
        const states = new WeakMap();
        const Ctor = {
            [name]: function (format) {
                if (!new.target) {
                    throw new TypeError(
                        `Failed to construct '${name}': Please use the 'new' operator, this DOM object constructor cannot be called as a function.`
                    );
                }
                const normalized = normalizeFormat(name, arguments);
                states.set(this, createPair(normalized, decompress));
            },
        }[name];

        // Blink installs the WebIDL accessors before `constructor` on these
        // prototypes, so remove the default Function.prototype constructor
        // entry and reinsert it after the accessors.
        const constructorDescriptor = Object.getOwnPropertyDescriptor(Ctor.prototype, 'constructor');
        delete Ctor.prototype.constructor;
        Object.defineProperty(Ctor.prototype, 'readable', {
            enumerable: true,
            configurable: true,
            get() {
                const state = states.get(this);
                if (!state) throw new TypeError('Illegal invocation');
                return state.readable;
            },
        });
        Object.defineProperty(Ctor.prototype, 'writable', {
            enumerable: true,
            configurable: true,
            get() {
                const state = states.get(this);
                if (!state) throw new TypeError('Illegal invocation');
                return state.writable;
            },
        });
        Object.defineProperty(Ctor.prototype, 'constructor', constructorDescriptor);
        Object.defineProperty(Ctor.prototype, Symbol.toStringTag, {
            value: name,
            configurable: true,
        });
        if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(Ctor, name);
        if (typeof globalThis._maskAsNative === 'function') {
            globalThis._maskAsNative(Ctor.prototype, 'readable', 'writable');
        }
        globalThis[name] = Ctor;
    };

    install('CompressionStream', false);
    install('DecompressionStream', true);
})(globalThis);
