// Chrome 148 PaymentRequest WebIDL/state normalization.
((globalThis) => {
    if (globalThis.isSecureContext !== true || typeof globalThis.EventTarget !== 'function') return;

    const mask = (fn, name) => {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };
    const state = new WeakMap();
    const requireState = value => {
        const current = state.get(value);
        if (!current) throw new TypeError('Illegal invocation');
        return current;
    };
    const getter = (name, read) => {
        const holder = { get [name]() { return read(this); } };
        return mask(Object.getOwnPropertyDescriptor(holder, name).get, `get ${name}`);
    };
    const setter = (name, write) => {
        const holder = { set [name](value) { write(this, value); } };
        return mask(Object.getOwnPropertyDescriptor(holder, name).set, `set ${name}`);
    };
    const method = (name, impl, length = 0) => {
        const fn = mask({ [name](...args) { return impl(this, args); } }[name], name);
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        return fn;
    };
    const domError = (message, name) => {
        try { return new DOMException(message, name); }
        catch (_) { const error = new Error(message); error.name = name; return error; }
    };

    function PaymentRequest(methodData, details, options) {
        if (!new.target) {
            throw new TypeError("Failed to construct 'PaymentRequest': Please use the 'new' operator, this DOM object constructor cannot be called as a function.");
        }
        if (arguments.length < 1) {
            throw new TypeError("Failed to construct 'PaymentRequest': 1 argument required, but only 0 present.");
        }
        if (!Array.isArray(methodData) || methodData.length === 0) {
            throw new TypeError("Failed to construct 'PaymentRequest': At least one payment method is required");
        }
        if (details == null || typeof details !== 'object' || details.total == null) {
            throw new TypeError("Failed to construct 'PaymentRequest': required member total is undefined.");
        }

        const self = Reflect.construct(globalThis.EventTarget, [], new.target);
        const id = details.id || (globalThis.crypto && typeof globalThis.crypto.randomUUID === 'function'
            ? globalThis.crypto.randomUUID()
            : Math.random().toString(36).slice(2));
        state.set(self, {
            methods: methodData.slice(),
            details,
            options: options || {},
            id,
            shippingAddress: null,
            shippingOption: null,
            shippingType: null,
            active: false,
            handlers: {
                onshippingaddresschange: null,
                onshippingoptionchange: null,
                onpaymentmethodchange: null,
            },
        });
        return self;
    }
    Object.defineProperty(PaymentRequest, 'length', { value: 1, configurable: true });
    Object.setPrototypeOf(PaymentRequest, globalThis.EventTarget);
    PaymentRequest.prototype = Object.create(globalThis.EventTarget.prototype);

    const proto = PaymentRequest.prototype;
    Object.defineProperties(proto, {
        id: { enumerable: true, configurable: true, get: getter('id', value => requireState(value).id) },
        shippingAddress: { enumerable: true, configurable: true, get: getter('shippingAddress', value => requireState(value).shippingAddress) },
        shippingOption: { enumerable: true, configurable: true, get: getter('shippingOption', value => requireState(value).shippingOption) },
        shippingType: { enumerable: true, configurable: true, get: getter('shippingType', value => requireState(value).shippingType) },
        onshippingaddresschange: {
            enumerable: true, configurable: true,
            get: getter('onshippingaddresschange', value => requireState(value).handlers.onshippingaddresschange),
            set: setter('onshippingaddresschange', (value, next) => { requireState(value).handlers.onshippingaddresschange = typeof next === 'function' ? next : null; }),
        },
        onshippingoptionchange: {
            enumerable: true, configurable: true,
            get: getter('onshippingoptionchange', value => requireState(value).handlers.onshippingoptionchange),
            set: setter('onshippingoptionchange', (value, next) => { requireState(value).handlers.onshippingoptionchange = typeof next === 'function' ? next : null; }),
        },
        abort: {
            enumerable: true, configurable: true, writable: true,
            value: method('abort', value => {
                const current = requireState(value);
                if (!current.active) {
                    return Promise.reject(domError(
                        "Failed to execute 'abort' on 'PaymentRequest': No show() or retry() in progress, so nothing to abort",
                        'InvalidStateError'
                    ));
                }
                current.active = false;
                return Promise.resolve(undefined);
            }),
        },
        canMakePayment: {
            enumerable: true, configurable: true, writable: true,
            value: method('canMakePayment', value => {
                const current = requireState(value);
                // `basic-card` is no longer a supported Chrome payment method.
                // Keep the existing deterministic Google Pay feature signal;
                // all other methods fail closed on a fresh profile.
                const supported = current.methods.some(item =>
                    item && item.supportedMethods === 'https://google.com/pay'
                );
                return Promise.resolve(supported);
            }),
        },
        hasEnrolledInstrument: {
            enumerable: true, configurable: true, writable: true,
            value: method('hasEnrolledInstrument', value => {
                requireState(value);
                return Promise.resolve(false);
            }),
        },
        show: {
            enumerable: true, configurable: true, writable: true,
            value: method('show', value => {
                const current = requireState(value);
                if (current.active) {
                    return Promise.reject(domError(
                        "Failed to execute 'show' on 'PaymentRequest': Already called show() once",
                        'InvalidStateError'
                    ));
                }
                current.active = true;
                // Headless BrowserOxide has no payment UI. Fail closed rather
                // than fabricating a PaymentResponse, and end the active flow.
                current.active = false;
                return Promise.reject(domError('User closed the Payment Request UI.', 'AbortError'));
            }, 0),
        },
        onpaymentmethodchange: {
            enumerable: true, configurable: true,
            get: getter('onpaymentmethodchange', value => requireState(value).handlers.onpaymentmethodchange),
            set: setter('onpaymentmethodchange', (value, next) => { requireState(value).handlers.onpaymentmethodchange = typeof next === 'function' ? next : null; }),
        },
        constructor: { value: PaymentRequest, writable: true, enumerable: false, configurable: true },
        [Symbol.toStringTag]: { value: 'PaymentRequest', configurable: true },
    });

    const spc = mask(function securePaymentConfirmationAvailability() {
        return Promise.resolve('unavailable-no-user-verifying-platform-authenticator');
    }, 'securePaymentConfirmationAvailability');
    Object.defineProperty(PaymentRequest, 'securePaymentConfirmationAvailability', {
        value: spc, writable: true, enumerable: false, configurable: true,
    });
    mask(PaymentRequest, 'PaymentRequest');
    Object.defineProperty(globalThis, 'PaymentRequest', {
        value: PaymentRequest, writable: true, enumerable: false, configurable: true,
    });
})(globalThis);

// Related Payment Request interfaces whose early implementation only exposed
// feature-detection shape.
((globalThis) => {
    if (globalThis.isSecureContext !== true || typeof globalThis.Event !== 'function' || typeof globalThis.EventTarget !== 'function') return;

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
    const method = (name, impl, length = 0) => {
        const fn = mask({ [name](...args) { return impl(this, args); } }[name], name);
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        return fn;
    };
    const defineGlobal = (name, value) => Object.defineProperty(globalThis, name, {
        value, writable: true, enumerable: false, configurable: true,
    });
    const invalidState = message => {
        try { return new DOMException(message, 'InvalidStateError'); }
        catch (_) { const error = new Error(message); error.name = 'InvalidStateError'; return error; }
    };

    // PaymentResponse is browser-created only. BrowserOxide does not fabricate
    // one while its headless PaymentRequest.show() rejects, but the full WebIDL
    // surface still matters to ordinary feature/descriptor probes.
    const responseState = new WeakMap();
    class PaymentResponse extends globalThis.EventTarget {
        constructor() {
            super();
            throw new TypeError("Failed to construct 'PaymentResponse': Illegal constructor");
        }
    }
    mask(PaymentResponse, 'PaymentResponse');
    const responseProto = PaymentResponse.prototype;
    delete responseProto.constructor;
    const responseRead = (value, key) => {
        const current = responseState.get(value);
        if (!current) throw new TypeError('Illegal invocation');
        return current[key];
    };
    Object.defineProperties(responseProto, {
        requestId: { enumerable: true, configurable: true, get: getter('requestId', value => responseRead(value, 'requestId')) },
        methodName: { enumerable: true, configurable: true, get: getter('methodName', value => responseRead(value, 'methodName')) },
        details: { enumerable: true, configurable: true, get: getter('details', value => responseRead(value, 'details')) },
        shippingAddress: { enumerable: true, configurable: true, get: getter('shippingAddress', value => responseRead(value, 'shippingAddress')) },
        shippingOption: { enumerable: true, configurable: true, get: getter('shippingOption', value => responseRead(value, 'shippingOption')) },
        payerName: { enumerable: true, configurable: true, get: getter('payerName', value => responseRead(value, 'payerName')) },
        payerEmail: { enumerable: true, configurable: true, get: getter('payerEmail', value => responseRead(value, 'payerEmail')) },
        payerPhone: { enumerable: true, configurable: true, get: getter('payerPhone', value => responseRead(value, 'payerPhone')) },
        onpayerdetailchange: {
            enumerable: true, configurable: true,
            get: getter('onpayerdetailchange', value => responseRead(value, 'onpayerdetailchange')),
            set: setter('onpayerdetailchange', (value, next) => {
                const current = responseState.get(value);
                if (!current) throw new TypeError('Illegal invocation');
                current.onpayerdetailchange = typeof next === 'function' ? next : null;
            }),
        },
        complete: { enumerable: true, configurable: true, writable: true, value: method('complete', value => {
            if (!responseState.has(value)) throw new TypeError('Illegal invocation');
            return Promise.resolve(undefined);
        }) },
        retry: { enumerable: true, configurable: true, writable: true, value: method('retry', value => {
            if (!responseState.has(value)) throw new TypeError('Illegal invocation');
            return Promise.resolve(undefined);
        }) },
        toJSON: { enumerable: true, configurable: true, writable: true, value: method('toJSON', value => {
            const current = responseState.get(value);
            if (!current) throw new TypeError('Illegal invocation');
            return {
                requestId: current.requestId,
                methodName: current.methodName,
                details: current.details,
                shippingAddress: current.shippingAddress,
                shippingOption: current.shippingOption,
                payerName: current.payerName,
                payerEmail: current.payerEmail,
                payerPhone: current.payerPhone,
            };
        }) },
        constructor: { value: PaymentResponse, writable: true, enumerable: false, configurable: true },
        [Symbol.toStringTag]: { value: 'PaymentResponse', configurable: true },
    });
    defineGlobal('PaymentResponse', PaymentResponse);

    const updateEventBrand = new WeakSet();
    class PaymentRequestUpdateEvent extends globalThis.Event {
        constructor(type, init = {}) {
            super(type, init);
            updateEventBrand.add(this);
        }
    }
    mask(PaymentRequestUpdateEvent, 'PaymentRequestUpdateEvent');
    const updateProto = PaymentRequestUpdateEvent.prototype;
    delete updateProto.constructor;
    const updateWith = method('updateWith', value => {
        if (!updateEventBrand.has(value)) throw new TypeError('Illegal invocation');
        if (value.isTrusted !== true) {
            throw invalidState("Failed to execute 'updateWith' on 'PaymentRequestUpdateEvent': Cannot update details when the event is not trusted");
        }
        return undefined;
    }, 1);
    Object.defineProperties(updateProto, {
        updateWith: { value: updateWith, writable: true, enumerable: true, configurable: true },
        constructor: { value: PaymentRequestUpdateEvent, writable: true, enumerable: false, configurable: true },
        [Symbol.toStringTag]: { value: 'PaymentRequestUpdateEvent', configurable: true },
    });
    defineGlobal('PaymentRequestUpdateEvent', PaymentRequestUpdateEvent);

    const methodEventState = new WeakMap();
    class PaymentMethodChangeEvent extends PaymentRequestUpdateEvent {
        constructor(type, init = {}) {
            super(type, init);
            methodEventState.set(this, {
                methodName: init && Object.prototype.hasOwnProperty.call(init, 'methodName') ? String(init.methodName) : '',
                methodDetails: init && Object.prototype.hasOwnProperty.call(init, 'methodDetails') ? init.methodDetails : null,
            });
        }
    }
    mask(PaymentMethodChangeEvent, 'PaymentMethodChangeEvent');
    const methodProto = PaymentMethodChangeEvent.prototype;
    delete methodProto.constructor;
    Object.defineProperties(methodProto, {
        methodName: {
            enumerable: true, configurable: true,
            get: getter('methodName', value => {
                const current = methodEventState.get(value);
                if (!current) throw new TypeError('Illegal invocation');
                return current.methodName;
            }),
        },
        methodDetails: {
            enumerable: true, configurable: true,
            get: getter('methodDetails', value => {
                const current = methodEventState.get(value);
                if (!current) throw new TypeError('Illegal invocation');
                return current.methodDetails;
            }),
        },
        constructor: { value: PaymentMethodChangeEvent, writable: true, enumerable: false, configurable: true },
        [Symbol.toStringTag]: { value: 'PaymentMethodChangeEvent', configurable: true },
    });
    defineGlobal('PaymentMethodChangeEvent', PaymentMethodChangeEvent);
})(globalThis);
