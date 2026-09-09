// Data-bearing Event subclasses whose payload fields are WebIDL prototype
// accessors in Chromium. Keep payload state in WeakMaps so instances expose
// only the base Event's browser-owned isTrusted own property.
((globalThis) => {
    if (typeof globalThis.Event !== 'function') return;

    const states = new WeakMap();
    const requireState = value => {
        const state = states.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const required = (name, count, actual) => {
        if (actual < count) {
            throw new TypeError(
                `Failed to construct '${name}': ${count} argument${count === 1 ? '' : 's'} required, but only ${actual} present.`
            );
        }
    };
    const installGetters = (Ctor, names) => {
        for (const name of names) {
            Object.defineProperty(Ctor.prototype, name, {
                get: function () { return requireState(this)[name]; },
                enumerable: true,
                configurable: true,
            });
        }
        Object.defineProperty(Ctor.prototype, Symbol.toStringTag, {
            value:Ctor.name, configurable:true,
        });
    };
    const maskClass = (Ctor, getterNames = []) => {
        try {
            if (typeof _maskFunction === 'function') _maskFunction(Ctor, Ctor.name);
            if (typeof _maskAsNative === 'function' && getterNames.length) {
                _maskAsNative(Ctor.prototype, ...getterNames);
            }
        } catch (_) {}
    };

    class ProgressEvent extends globalThis.Event {
        constructor(type) {
            required('ProgressEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, {
                lengthComputable: !!options.lengthComputable,
                loaded: Number(options.loaded || 0),
                total: Number(options.total || 0),
            });
        }
    }
    installGetters(ProgressEvent, ['lengthComputable','loaded','total']);

    class ErrorEvent extends globalThis.Event {
        constructor(type) {
            required('ErrorEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, {
                message: String(options.message || ''),
                filename: String(options.filename || ''),
                lineno: Number(options.lineno || 0) >>> 0,
                colno: Number(options.colno || 0) >>> 0,
                error: options.error === undefined ? null : options.error,
            });
        }
    }
    installGetters(ErrorEvent, ['message','filename','lineno','colno','error']);

    class PromiseRejectionEvent extends globalThis.Event {
        constructor(type, init) {
            required('PromiseRejectionEvent', 2, arguments.length);
            const options = init || {};
            super(type, options);
            states.set(this, {
                promise: options.promise,
                reason: options.reason,
            });
        }
    }
    installGetters(PromiseRejectionEvent, ['promise','reason']);

    class ClipboardEvent extends globalThis.Event {
        constructor(type) {
            required('ClipboardEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, { bubbles:true, cancelable:true, ...options });
            states.set(this, { clipboardData: options.clipboardData || null });
        }
    }
    installGetters(ClipboardEvent, ['clipboardData']);

    class PopStateEvent extends globalThis.Event {
        constructor(type) {
            required('PopStateEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, {
                state: options.state === undefined ? null : options.state,
                hasUAVisualTransition: !!options.hasUAVisualTransition,
            });
        }
    }
    installGetters(PopStateEvent, ['state','hasUAVisualTransition']);

    class HashChangeEvent extends globalThis.Event {
        constructor(type) {
            required('HashChangeEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, {
                oldURL: String(options.oldURL || ''),
                newURL: String(options.newURL || ''),
            });
        }
    }
    installGetters(HashChangeEvent, ['oldURL','newURL']);

    class PageTransitionEvent extends globalThis.Event {
        constructor(type) {
            required('PageTransitionEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, { persisted: !!options.persisted });
        }
    }
    installGetters(PageTransitionEvent, ['persisted']);

    class DragEvent extends globalThis.MouseEvent {
        constructor(type) {
            required('DragEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, { dataTransfer: options.dataTransfer || null });
        }
    }
    installGetters(DragEvent, ['dataTransfer']);

    class SubmitEvent extends globalThis.Event {
        constructor(type) {
            required('SubmitEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, { submitter: options.submitter || null });
        }
    }
    installGetters(SubmitEvent, ['submitter']);

    class FormDataEvent extends globalThis.Event {
        constructor(type, init) {
            required('FormDataEvent', 2, arguments.length);
            const options = init || {};
            super(type, options);
            states.set(this, { formData: options.formData });
        }
    }
    installGetters(FormDataEvent, ['formData']);

    class CloseEvent extends globalThis.Event {
        constructor(type) {
            required('CloseEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, {
                wasClean: !!options.wasClean,
                code: Number(options.code || 0) >>> 0,
                reason: String(options.reason || ''),
            });
        }
    }
    installGetters(CloseEvent, ['wasClean','code','reason']);

    class SecurityPolicyViolationEvent extends globalThis.Event {
        constructor(type) {
            required('SecurityPolicyViolationEvent', 1, arguments.length);
            const options = arguments[1] || {};
            super(type, options);
            states.set(this, {
                documentURI: String(options.documentURI || ''),
                referrer: String(options.referrer || ''),
                blockedURI: String(options.blockedURI || ''),
                violatedDirective: String(options.violatedDirective || ''),
                effectiveDirective: String(options.effectiveDirective || ''),
                originalPolicy: String(options.originalPolicy || ''),
                sourceFile: String(options.sourceFile || ''),
                sample: String(options.sample || ''),
                disposition: String(options.disposition || 'enforce'),
                statusCode: Number(options.statusCode || 0) >>> 0,
                lineNumber: Number(options.lineNumber || 0) >>> 0,
                columnNumber: Number(options.columnNumber || 0) >>> 0,
            });
        }
    }
    const securityFields = [
        'documentURI','referrer','blockedURI','violatedDirective','effectiveDirective',
        'originalPolicy','sourceFile','sample','disposition','statusCode','lineNumber','columnNumber',
    ];
    installGetters(SecurityPolicyViolationEvent, securityFields);

    function BeforeUnloadEvent() {
        throw new TypeError("Failed to construct 'BeforeUnloadEvent': Illegal constructor");
    }
    try { Object.setPrototypeOf(BeforeUnloadEvent, globalThis.Event); } catch (_) {}
    BeforeUnloadEvent.prototype = Object.create(globalThis.Event.prototype);
    Object.defineProperty(BeforeUnloadEvent.prototype, 'constructor', {
        value:BeforeUnloadEvent, writable:true, configurable:true,
    });
    Object.defineProperty(BeforeUnloadEvent.prototype, 'returnValue', {
        get:function returnValue() { return requireState(this).returnValue; },
        set:function returnValue(value) { requireState(this).returnValue = String(value); },
        enumerable:true,
        configurable:true,
    });
    Object.defineProperty(BeforeUnloadEvent.prototype, Symbol.toStringTag, {
        value:'BeforeUnloadEvent', configurable:true,
    });
    const replacements = {
        ProgressEvent, ErrorEvent, PromiseRejectionEvent, ClipboardEvent,
        PopStateEvent, HashChangeEvent, PageTransitionEvent, DragEvent,
        SubmitEvent, FormDataEvent, CloseEvent, SecurityPolicyViolationEvent,
        BeforeUnloadEvent,
    };
    for (const [name, Ctor] of Object.entries(replacements)) {
        globalThis[name] = Ctor;
    }

    if (globalThis.PointerEvent?.prototype && typeof globalThis.PointerEvent.prototype.getCoalescedEvents !== 'function') {
        const getCoalescedEvents = function getCoalescedEvents() { return []; };
        Object.defineProperty(globalThis.PointerEvent.prototype, 'getCoalescedEvents', {
            value:getCoalescedEvents, writable:true, enumerable:true, configurable:true,
        });
        try { if (typeof _maskFunction === 'function') _maskFunction(getCoalescedEvents, 'getCoalescedEvents'); } catch (_) {}
    }

    for (const Ctor of [
        ProgressEvent, ErrorEvent, PromiseRejectionEvent, ClipboardEvent,
        PopStateEvent, HashChangeEvent, PageTransitionEvent, DragEvent,
        SubmitEvent, FormDataEvent, CloseEvent, SecurityPolicyViolationEvent,
    ]) {
        maskClass(Ctor, Object.getOwnPropertyNames(Ctor.prototype).filter(name => name !== 'constructor'));
    }
    maskClass(BeforeUnloadEvent, ['returnValue']);
})(globalThis);
