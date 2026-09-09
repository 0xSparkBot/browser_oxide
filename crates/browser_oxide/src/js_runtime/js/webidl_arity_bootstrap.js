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

    const setConstructorLength = (name, length) => {
        try {
            const fn = globalThis[name];
            if (typeof fn !== 'function') return;
            Object.defineProperty(fn, 'length', {
                value: length,
                configurable: true,
            });
        } catch (_) {}
    };

    if (globalThis.EventTarget?.prototype) {
        setLength(globalThis.EventTarget.prototype, 'addEventListener', 2);
        setLength(globalThis.EventTarget.prototype, 'removeEventListener', 2);
    }
    if (globalThis.CustomEvent?.prototype) {
        setLength(globalThis.CustomEvent.prototype, 'initCustomEvent', 1);
    }
    if (globalThis.MessageEvent?.prototype) {
        setLength(globalThis.MessageEvent.prototype, 'initMessageEvent', 1);
    }
    setConstructorLength('Node', 0);
    setConstructorLength('DocumentFragment', 0);
    setConstructorLength('URL', 1);
    setConstructorLength('URLSearchParams', 0);
    setConstructorLength('Worker', 1);
    setConstructorLength('Response', 0);
    if (globalThis.URLSearchParams?.prototype) {
        setLength(globalThis.URLSearchParams.prototype, 'forEach', 1);
    }
    if (globalThis.Worker?.prototype) {
        setLength(globalThis.Worker.prototype, 'postMessage', 1);
    }
    if (globalThis.SpeechSynthesis?.prototype) {
        setLength(globalThis.SpeechSynthesis.prototype, 'speak', 1);
    }
    if (globalThis.RTCPeerConnection?.prototype) {
        setLength(globalThis.RTCPeerConnection.prototype, 'addIceCandidate', 0);
        setLength(globalThis.RTCPeerConnection.prototype, 'addTransceiver', 1);
        setLength(globalThis.RTCPeerConnection.prototype, 'getStats', 0);
        setLength(globalThis.RTCPeerConnection.prototype, 'setLocalDescription', 0);
    }
    if (globalThis.History?.prototype) {
        setLength(globalThis.History.prototype, 'go', 0);
        setLength(globalThis.History.prototype, 'pushState', 2);
        setLength(globalThis.History.prototype, 'replaceState', 2);
    }
    if (globalThis.PerformanceObserver?.prototype) {
        setLength(globalThis.PerformanceObserver.prototype, 'observe', 0);
    }
    if (globalThis.PerformanceObserverEntryList?.prototype) {
        setLength(globalThis.PerformanceObserverEntryList.prototype, 'getEntriesByName', 1);
    }

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
