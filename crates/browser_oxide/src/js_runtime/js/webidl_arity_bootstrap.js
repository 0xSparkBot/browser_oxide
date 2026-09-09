// Normalize Function.length for WebIDL methods whose JavaScript fallback
// implementations use rest/default parameters that do not reflect Blink's
// published operation arity. This is descriptor-only: implementations and
// behavior remain unchanged.
((globalThis) => {
    const setLength = (owner, name, length) => {
        try {
            const fn = owner && owner[name];
            if (typeof fn !== 'function') return;
            Object.defineProperty(fn, 'length', {
                value: length,
                configurable: true,
            });
        } catch (_) {}
    };

    if (globalThis.TextEncoder?.prototype) {
        setLength(globalThis.TextEncoder.prototype, 'encode', 0);
    }
    if (globalThis.TextDecoder?.prototype) {
        setLength(globalThis.TextDecoder.prototype, 'decode', 0);
    }
    if (globalThis.SubtleCrypto?.prototype) {
        const arities = {
            decrypt: 3,
            deriveBits: 2,
            deriveKey: 5,
            digest: 2,
            encrypt: 3,
            exportKey: 2,
            generateKey: 3,
            importKey: 5,
            sign: 3,
            unwrapKey: 7,
            verify: 4,
            wrapKey: 4,
        };
        for (const [name, length] of Object.entries(arities)) {
            setLength(globalThis.SubtleCrypto.prototype, name, length);
        }
    }
    if (globalThis.MouseEvent?.prototype) {
        setLength(globalThis.MouseEvent.prototype, 'initMouseEvent', 1);
    }
    if (globalThis.KeyboardEvent?.prototype) {
        setLength(globalThis.KeyboardEvent.prototype, 'initKeyboardEvent', 1);
    }
})(globalThis);
