((globalThis) => {
    const _eventPrivateTraceEnabled = (() => {
        try {
            return !!(Deno.core.ops.op_event_private_trace_enabled
                && Deno.core.ops.op_event_private_trace_enabled());
        } catch (_) {
            return false;
        }
    })();
    const _eventPrivateTrace = (row) => {
        if (!_eventPrivateTraceEnabled) return;
        try { Deno.core.ops.op_event_private_trace(JSON.stringify(row)); } catch (_) {}
    };
    const _eventDiagNote = (() => {
        if (globalThis.__browser_oxide_debug !== true
            && globalThis.__oxideDiagnostics !== true) return null;
        try {
            const note = Deno.core.ops.op_worker_diag_note;
            return typeof note === 'function' ? note : null;
        } catch (_) {
            return null;
        }
    })();
    // ---- Trusted-event authenticity (v0.1.0 behavioral E1) ----------------
    // `isTrusted` MUST be both unforgeable and shaped like a real browser's:
    //   * an own, enumerable, non-configurable GETTER on every Event instance.
    //     Chrome 148 exposes no Event.prototype.isTrusted property at all.
    //   * backed by a MODULE-PRIVATE WeakSet that page JS cannot reach. The
    //     old design keyed trust off `Symbol.for('__bo_trusted__')` — the
    //     GLOBAL symbol registry — so any page could re-derive the symbol and
    //     forge a trusted event (`new Event('x', {[Symbol.for(...)]: true})`).
    // Only our privileged init scripts mint trust, via `_markTrusted`, handed
    // off below through a temp global they capture-and-delete before any page
    // script runs. There is no in-band (options/symbol) path from page JS.
    const _trustedEvents = new WeakSet();
    const _eventState = new WeakMap();
    const _customEventState = new WeakMap();
    const _uiEventState = new WeakMap();
    const _mouseEventState = new WeakMap();
    const _keyboardEventState = new WeakMap();
    const _messageEventState = new WeakMap();
    const _inputEventState = new WeakMap();
    const _focusEventState = new WeakMap();
    const _pointerEventState = new WeakMap();
    const _wheelEventState = new WeakMap();

    const _stateFor = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const _native = (fn, name) => {
        try { Object.defineProperty(fn, 'name', { value: name, configurable: true }); } catch (_) {}
        if (typeof _maskFunction === 'function') _maskFunction(fn, name);
        return fn;
    };
    const _getter = (name, read) => {
        const holder = { get [name]() { return read(this); } };
        return _native(Object.getOwnPropertyDescriptor(holder, name).get, `get ${name}`);
    };
    const _setter = (name, write) => {
        const holder = { set [name](value) { write(this, value); } };
        return _native(Object.getOwnPropertyDescriptor(holder, name).set, `set ${name}`);
    };
    const _defineGetter = (prototype, name, map, stateKey = name) => {
        Object.defineProperty(prototype, name, {
            configurable: true,
            enumerable: true,
            get: _getter(name, value => _stateFor(map, value)[stateKey]),
        });
    };
    const _moveConstructorLast = (Ctor) => {
        const descriptor = Object.getOwnPropertyDescriptor(Ctor.prototype, 'constructor');
        try { delete Ctor.prototype.constructor; } catch (_) {}
        Object.defineProperty(Ctor.prototype, 'constructor', descriptor);
    };

    const _getIsTrusted = _getter('isTrusted', value => _trustedEvents.has(value));
    const _markTrusted = (ev) => {
        try { if (ev && typeof ev === 'object') _trustedEvents.add(ev); } catch (_) {}
        return ev;
    };

    class Event {
        constructor(type, options = {}) {
            _eventState.set(this, {
                type: String(type),
                target: null,
                currentTarget: null,
                eventPhase: 0,
                bubbles: !!options.bubbles,
                cancelable: !!options.cancelable,
                defaultPrevented: false,
                composed: !!options.composed,
                timeStamp: performance.now(),
                stopped: false,
                stoppedImmediate: false,
                dispatching: false,
                path: [],
            });
            // Blink exposes isTrusted as the Event instance's sole own
            // property. Reusing one closure also matches getter identity
            // across different events.
            Object.defineProperty(this, 'isTrusted', {
                configurable: false,
                enumerable: true,
                get: _getIsTrusted,
            });
        }
    }

    // WebIDL state lives in WeakMaps, never as JS-visible expando fields. This
    // makes Object.getOwnPropertyNames(new Event('x')) exactly ['isTrusted'].
    for (const name of ['type', 'target', 'currentTarget', 'eventPhase', 'bubbles',
        'cancelable', 'defaultPrevented', 'composed', 'timeStamp']) {
        _defineGetter(Event.prototype, name, _eventState);
    }
    _defineGetter(Event.prototype, 'srcElement', _eventState, 'target');
    Object.defineProperty(Event.prototype, 'returnValue', {
        configurable: true,
        enumerable: true,
        get: _getter('returnValue', value => !_stateFor(_eventState, value).defaultPrevented),
        set: _setter('returnValue', (value, next) => {
            const state = _stateFor(_eventState, value);
            if (!next && state.cancelable) state.defaultPrevented = true;
        }),
    });
    Object.defineProperty(Event.prototype, 'cancelBubble', {
        configurable: true,
        enumerable: true,
        get: _getter('cancelBubble', value => _stateFor(_eventState, value).stopped),
        set: _setter('cancelBubble', (value, next) => {
            if (next) _stateFor(_eventState, value).stopped = true;
        }),
    });
    for (const [name, value] of [['NONE', 0], ['CAPTURING_PHASE', 1],
        ['AT_TARGET', 2], ['BUBBLING_PHASE', 3]]) {
        Object.defineProperty(Event.prototype, name, {
            value, writable: false, enumerable: true, configurable: false,
        });
        Object.defineProperty(Event, name, {
            value, writable: false, enumerable: true, configurable: false,
        });
    }
    const _eventComposedPath = { composedPath() {
        return _stateFor(_eventState, this).path.slice();
    } }.composedPath;
    const _eventInit = { initEvent(type, bubbles = false, cancelable = false) {
        const state = _stateFor(_eventState, this);
        if (state.dispatching) return;
        state.type = String(type);
        state.bubbles = !!bubbles;
        state.cancelable = !!cancelable;
        state.defaultPrevented = false;
        state.stopped = false;
        state.stoppedImmediate = false;
    } }.initEvent;
    const _eventPreventDefault = { preventDefault() {
        const state = _stateFor(_eventState, this);
        if (state.cancelable) state.defaultPrevented = true;
    } }.preventDefault;
    const _eventStopImmediate = { stopImmediatePropagation() {
        const state = _stateFor(_eventState, this);
        state.stopped = true;
        state.stoppedImmediate = true;
    } }.stopImmediatePropagation;
    const _eventStop = { stopPropagation() {
        _stateFor(_eventState, this).stopped = true;
    } }.stopPropagation;
    for (const [name, fn] of [
        ['composedPath', _eventComposedPath], ['initEvent', _eventInit],
        ['preventDefault', _eventPreventDefault],
        ['stopImmediatePropagation', _eventStopImmediate],
        ['stopPropagation', _eventStop],
    ]) {
        Object.defineProperty(Event.prototype, name, {
            value: _native(fn, name), writable: true, enumerable: true, configurable: true,
        });
    }
    _moveConstructorLast(Event);

    class CustomEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            _customEventState.set(this, {
                detail: options.detail !== undefined ? options.detail : null,
            });
        }
    }
    _defineGetter(CustomEvent.prototype, 'detail', _customEventState);
    const _initCustomEvent = { initCustomEvent(type, bubbles, cancelable, detail) {
        _eventInit.call(this, type, bubbles, cancelable);
        _stateFor(_customEventState, this).detail = detail;
    } }.initCustomEvent;
    Object.defineProperty(CustomEvent.prototype, 'initCustomEvent', {
        value: _native(_initCustomEvent, 'initCustomEvent'),
        writable: true, enumerable: true, configurable: true,
    });
    _moveConstructorLast(CustomEvent);

    // --- UI Event hierarchy ---
    class UIEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            _uiEventState.set(this, {
                view: options.view !== undefined ? options.view : null,
                detail: Number(options.detail) || 0,
                which: 0,
            });
        }
    }
    for (const name of ['view', 'detail', 'which']) {
        _defineGetter(UIEvent.prototype, name, _uiEventState);
    }
    const _initUIEvent = { initUIEvent(type, bubbles, cancelable, view, detail) {
        _eventInit.call(this, type, bubbles, cancelable);
        Object.assign(_stateFor(_uiEventState, this), { view: view || null, detail: Number(detail) || 0 });
    } }.initUIEvent;
    Object.defineProperty(UIEvent.prototype, 'initUIEvent', {
        value: _native(_initUIEvent, 'initUIEvent'),
        writable: true, enumerable: true, configurable: true,
    });
    _moveConstructorLast(UIEvent);

    class MouseEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, { bubbles: true, cancelable: true, ...options });
            const clientX = Number(options.clientX) || 0;
            const clientY = Number(options.clientY) || 0;
            _mouseEventState.set(this, {
                screenX: Number(options.screenX) || 0,
                screenY: Number(options.screenY) || 0,
                clientX,
                clientY,
                ctrlKey: !!options.ctrlKey,
                shiftKey: !!options.shiftKey,
                altKey: !!options.altKey,
                metaKey: !!options.metaKey,
                button: Number(options.button) || 0,
                buttons: Number(options.buttons) || 0,
                relatedTarget: options.relatedTarget || null,
                pageX: options.pageX !== undefined ? Number(options.pageX) || 0 : clientX,
                pageY: options.pageY !== undefined ? Number(options.pageY) || 0 : clientY,
                offsetX: Number(options.offsetX) || 0,
                offsetY: Number(options.offsetY) || 0,
                movementX: Number(options.movementX) || 0,
                movementY: Number(options.movementY) || 0,
                fromElement: null,
                toElement: null,
                layerX: Number(options.layerX) || 0,
                layerY: Number(options.layerY) || 0,
            });
        }
    }
    for (const name of ['screenX', 'screenY', 'clientX', 'clientY', 'ctrlKey',
        'shiftKey', 'altKey', 'metaKey', 'button', 'buttons', 'relatedTarget',
        'pageX', 'pageY']) {
        _defineGetter(MouseEvent.prototype, name, _mouseEventState);
    }
    _defineGetter(MouseEvent.prototype, 'x', _mouseEventState, 'clientX');
    _defineGetter(MouseEvent.prototype, 'y', _mouseEventState, 'clientY');
    for (const name of ['offsetX', 'offsetY', 'movementX', 'movementY',
        'fromElement', 'toElement', 'layerX', 'layerY']) {
        _defineGetter(MouseEvent.prototype, name, _mouseEventState);
    }
    const _mouseGetModifierState = { getModifierState(key) {
        const state = _stateFor(_mouseEventState, this);
        return !!state[`${String(key).toLowerCase()}Key`];
    } }.getModifierState;
    const _initMouseEvent = { initMouseEvent(type, bubbles, cancelable, view, detail,
        screenX, screenY, clientX, clientY, ctrlKey, altKey, shiftKey, metaKey,
        button, relatedTarget) {
        _initUIEvent.call(this, type, bubbles, cancelable, view, detail);
        Object.assign(_stateFor(_mouseEventState, this), {
            screenX: Number(screenX) || 0, screenY: Number(screenY) || 0,
            clientX: Number(clientX) || 0, clientY: Number(clientY) || 0,
            ctrlKey: !!ctrlKey, altKey: !!altKey, shiftKey: !!shiftKey,
            metaKey: !!metaKey, button: Number(button) || 0,
            relatedTarget: relatedTarget || null,
        });
    } }.initMouseEvent;
    for (const [name, fn] of [['getModifierState', _mouseGetModifierState],
        ['initMouseEvent', _initMouseEvent]]) {
        Object.defineProperty(MouseEvent.prototype, name, {
            value: _native(fn, name), writable: true, enumerable: true, configurable: true,
        });
    }
    _moveConstructorLast(MouseEvent);

    class KeyboardEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, options);
            const keyCode = (Number(options.keyCode) || 0) >>> 0;
            _keyboardEventState.set(this, {
                key: options.key === undefined ? "" : String(options.key),
                code: options.code === undefined ? "" : String(options.code),
                location: (Number(options.location) || 0) >>> 0,
                ctrlKey: !!options.ctrlKey,
                shiftKey: !!options.shiftKey,
                altKey: !!options.altKey,
                metaKey: !!options.metaKey,
                repeat: !!options.repeat,
                isComposing: !!options.isComposing,
                charCode: (Number(options.charCode) || 0) >>> 0,
                keyCode,
            });
            // UIEvent owns the legacy `which` getter in Blink. Constructor
            // dictionaries ignore an explicit `which`; synthetic key events
            // expose keyCode through it instead.
            _stateFor(_uiEventState, this).which = keyCode;
        }
    }
    for (const name of ['key', 'code', 'location', 'ctrlKey', 'shiftKey',
        'altKey', 'metaKey', 'repeat', 'isComposing', 'charCode', 'keyCode']) {
        _defineGetter(KeyboardEvent.prototype, name, _keyboardEventState);
    }
    for (const [name, value] of [
        ['DOM_KEY_LOCATION_STANDARD', 0], ['DOM_KEY_LOCATION_LEFT', 1],
        ['DOM_KEY_LOCATION_RIGHT', 2], ['DOM_KEY_LOCATION_NUMPAD', 3],
    ]) {
        for (const target of [KeyboardEvent, KeyboardEvent.prototype]) {
            Object.defineProperty(target, name, {
                value, writable: false, enumerable: true, configurable: false,
            });
        }
    }
    const _keyboardGetModifierState = { getModifierState(key) {
        const state = _stateFor(_keyboardEventState, this);
        switch (String(key)) {
            case 'Control': return state.ctrlKey;
            case 'Shift': return state.shiftKey;
            case 'Alt': return state.altKey;
            case 'Meta': return state.metaKey;
            default: return false;
        }
    } }.getModifierState;
    const _initKeyboardEvent = { initKeyboardEvent(type, bubbles, cancelable, view,
        _char, location, ctrlKey, altKey, shiftKey, metaKey) {
        _initUIEvent.call(this, type, bubbles, cancelable, view, 0);
        Object.assign(_stateFor(_keyboardEventState, this), {
            key: '', code: '', location: (Number(location) || 0) >>> 0,
            ctrlKey: !!ctrlKey, altKey: !!altKey, shiftKey: !!shiftKey,
            metaKey: !!metaKey, repeat: false, isComposing: false,
            charCode: 0, keyCode: 0,
        });
        _stateFor(_uiEventState, this).which = 0;
    } }.initKeyboardEvent;
    for (const [name, fn] of [['getModifierState', _keyboardGetModifierState],
        ['initKeyboardEvent', _initKeyboardEvent]]) {
        Object.defineProperty(KeyboardEvent.prototype, name, {
            value: _native(fn, name), writable: true, enumerable: true, configurable: true,
        });
    }
    _moveConstructorLast(KeyboardEvent);

    class InputEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, { bubbles: true, cancelable: false, ...options });
            _inputEventState.set(this, {
                data: options.data === undefined ? null : options.data,
                isComposing: !!options.isComposing,
                inputType: String(options.inputType || ""),
                dataTransfer: options.dataTransfer || null,
                targetRanges: options.targetRanges ? Array.from(options.targetRanges) : [],
            });
        }
    }
    for (const name of ['data', 'isComposing', 'inputType', 'dataTransfer']) {
        _defineGetter(InputEvent.prototype, name, _inputEventState);
    }
    const _inputGetTargetRanges = { getTargetRanges() {
        return _stateFor(_inputEventState, this).targetRanges.slice();
    } }.getTargetRanges;
    Object.defineProperty(InputEvent.prototype, 'getTargetRanges', {
        value: _native(_inputGetTargetRanges, 'getTargetRanges'),
        writable: true, enumerable: true, configurable: true,
    });
    _moveConstructorLast(InputEvent);

    class FocusEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, options);
            _focusEventState.set(this, { relatedTarget: options.relatedTarget || null });
        }
    }
    _defineGetter(FocusEvent.prototype, 'relatedTarget', _focusEventState);
    _moveConstructorLast(FocusEvent);

    class PointerEvent extends MouseEvent {
        constructor(type, options = {}) {
            super(type, options);
            _pointerEventState.set(this, {
                pointerId: Number(options.pointerId) || 0,
                width: options.width === undefined ? 1 : Number(options.width) || 0,
                height: options.height === undefined ? 1 : Number(options.height) || 0,
                pressure: Number(options.pressure) || 0,
                tiltX: Number(options.tiltX) || 0,
                tiltY: Number(options.tiltY) || 0,
                azimuthAngle: Number(options.azimuthAngle) || 0,
                altitudeAngle: options.altitudeAngle === undefined
                    ? Math.PI / 2
                    : Number(options.altitudeAngle) || 0,
                tangentialPressure: Number(options.tangentialPressure) || 0,
                twist: Number(options.twist) || 0,
                pointerType: String(options.pointerType || ""),
                isPrimary: !!options.isPrimary,
                persistentDeviceId: Number(options.persistentDeviceId) || 0,
            });
        }
    }
    for (const name of ['pointerId', 'width', 'height', 'pressure', 'tiltX',
        'tiltY', 'azimuthAngle', 'altitudeAngle', 'tangentialPressure', 'twist',
        'pointerType', 'isPrimary', 'persistentDeviceId']) {
        _defineGetter(PointerEvent.prototype, name, _pointerEventState);
    }
    const _pointerGetPredictedEvents = { getPredictedEvents() { return []; } }.getPredictedEvents;
    Object.defineProperty(PointerEvent.prototype, 'getPredictedEvents', {
        value: _native(_pointerGetPredictedEvents, 'getPredictedEvents'),
        writable: true, enumerable: true, configurable: true,
    });
    _moveConstructorLast(PointerEvent);

    class WheelEvent extends MouseEvent {
        constructor(type, options = {}) {
            super(type, options);
            const deltaX = Number(options.deltaX) || 0;
            const deltaY = Number(options.deltaY) || 0;
            _wheelEventState.set(this, {
                deltaX,
                deltaY,
                deltaZ: Number(options.deltaZ) || 0,
                deltaMode: (Number(options.deltaMode) || 0) >>> 0,
                wheelDeltaX: deltaX ? -Math.sign(deltaX) * 120 : 0,
                wheelDeltaY: deltaY ? -Math.sign(deltaY) * 120 : 0,
                wheelDelta: deltaY ? -Math.sign(deltaY) * 120 : 0,
            });
        }
    }
    for (const name of ['deltaX', 'deltaY', 'deltaZ', 'deltaMode',
        'wheelDeltaX', 'wheelDeltaY', 'wheelDelta']) {
        _defineGetter(WheelEvent.prototype, name, _wheelEventState);
    }
    for (const [name, value] of [
        ['DOM_DELTA_PIXEL', 0], ['DOM_DELTA_LINE', 1], ['DOM_DELTA_PAGE', 2],
    ]) {
        for (const target of [WheelEvent, WheelEvent.prototype]) {
            Object.defineProperty(target, name, {
                value, writable: false, enumerable: true, configurable: false,
            });
        }
    }
    _moveConstructorLast(WheelEvent);

    class TouchEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, { bubbles: true, cancelable: true, ...options });
            this.touches = options.touches || [];
            this.targetTouches = options.targetTouches || [];
            this.changedTouches = options.changedTouches || [];
            this.ctrlKey = !!options.ctrlKey;
            this.shiftKey = !!options.shiftKey;
            this.altKey = !!options.altKey;
            this.metaKey = !!options.metaKey;
        }
    }

    class MessageEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            _messageEventState.set(this, {
                data: options.data !== undefined ? options.data : null,
                origin: String(options.origin || ''),
                lastEventId: String(options.lastEventId || ''),
                source: options.source || null,
                ports: options.ports ? Array.from(options.ports) : [],
                userActivation: options.userActivation || null,
            });
        }
    }
    for (const name of ['data', 'origin', 'lastEventId', 'source', 'ports', 'userActivation']) {
        _defineGetter(MessageEvent.prototype, name, _messageEventState);
    }
    const _initMessageEvent = { initMessageEvent(type, bubbles, cancelable, data,
        origin, lastEventId, source, ports) {
        _eventInit.call(this, type, bubbles, cancelable);
        Object.assign(_stateFor(_messageEventState, this), {
            data: data === undefined ? null : data,
            origin: String(origin || ''), lastEventId: String(lastEventId || ''),
            source: source || null, ports: ports ? Array.from(ports) : [],
        });
    } }.initMessageEvent;
    Object.defineProperty(MessageEvent.prototype, 'initMessageEvent', {
        value: _native(_initMessageEvent, 'initMessageEvent'),
        writable: true, enumerable: true, configurable: true,
    });
    _moveConstructorLast(MessageEvent);

    class ErrorEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            this.message = options.message || "";
            this.filename = options.filename || "";
            this.lineno = options.lineno || 0;
            this.colno = options.colno || 0;
            this.error = options.error || null;
        }
    }

    class ProgressEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            this.lengthComputable = !!options.lengthComputable;
            this.loaded = options.loaded || 0;
            this.total = options.total || 0;
        }
    }

    class AnimationEvent extends Event {
        constructor(type, options = {}) {
            super(type, { bubbles: true, ...options });
            this.animationName = options.animationName || "";
            this.elapsedTime = options.elapsedTime || 0;
            this.pseudoElement = options.pseudoElement || "";
        }
    }

    class TransitionEvent extends Event {
        constructor(type, options = {}) {
            super(type, { bubbles: true, ...options });
            this.propertyName = options.propertyName || "";
            this.elapsedTime = options.elapsedTime || 0;
            this.pseudoElement = options.pseudoElement || "";
        }
    }

    class ClipboardEvent extends Event {
        constructor(type, options = {}) {
            super(type, { bubbles: true, cancelable: true, ...options });
            this.clipboardData = options.clipboardData || null;
        }
    }

    class PopStateEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            this.state = options.state !== undefined ? options.state : null;
        }
    }

    class HashChangeEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            this.oldURL = options.oldURL || "";
            this.newURL = options.newURL || "";
        }
    }

    class StorageEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            this.key = options.key || null;
            this.oldValue = options.oldValue || null;
            this.newValue = options.newValue || null;
            this.url = options.url || "";
            this.storageArea = options.storageArea || null;
        }
    }

    class PageTransitionEvent extends Event {
        constructor(type, options = {}) {
            super(type, options);
            this.persisted = !!options.persisted;
        }
    }

    class BeforeUnloadEvent extends Event {
        constructor(type, options = {}) {
            super(type, { cancelable: true, ...options });
            this.returnValue = "";
        }
    }

    class DragEvent extends MouseEvent {
        constructor(type, options = {}) {
            super(type, options);
            this.dataTransfer = options.dataTransfer || null;
        }
    }

    // Every WebIDL event interface owns its @@toStringTag. This is invisible
    // to getOwnPropertyNames(), but controls Object#toString in both the top
    // page and the mirrored iframe realms.
    for (const Ctor of [Event, CustomEvent, UIEvent, MouseEvent,
        KeyboardEvent, InputEvent, FocusEvent,
        PointerEvent, WheelEvent, TouchEvent, MessageEvent, ErrorEvent,
        ProgressEvent, AnimationEvent, TransitionEvent, ClipboardEvent,
        PopStateEvent, HashChangeEvent, StorageEvent, PageTransitionEvent,
        BeforeUnloadEvent, DragEvent]) {
        try {
            Object.defineProperty(Ctor.prototype, Symbol.toStringTag, {
                value: Ctor.name,
                writable: false,
                enumerable: false,
                configurable: true,
            });
        } catch (_) {}
    }

    // --- EventTarget core logic ---
    const _nodeListeners = new Map(); // nodeId → Map<eventType, [{callback, capture, once}]>
    let _objListeners = new WeakMap(); // object → Map<eventType, [{callback, capture, once}]>

    // Warm-reuse listener reaper — the events-side analogue of
    // `timer_bootstrap.js`'s `__cancelAllTimers()`. A pooled `Page`
    // (`PagePool` / `Page::navigate_warm`) keeps ONE `JsRuntime` alive across
    // navigations, so both registries above outlive the document they were
    // populated for. Two distinct failures follow:
    //
    //   * Leak. `_objListeners` is keyed by target *object*; listeners a page
    //     attaches to `window`/`globalThis` (analytics, scroll handlers, …)
    //     are keyed against the one global that is never collected for the
    //     life of the isolate, so those callbacks — and every closure
    //     variable they capture, which can be the page's whole object graph —
    //     are retained forever. `_nodeListeners` is worse: it is a *strong*
    //     Map that is never pruned at all. Measured at ~10 MB/page of live
    //     (non-GC-able) V8 heap on real product pages, unbounded.
    //   * Cross-page misfire. `_nodeListeners` is keyed by `nodeId`, and node
    //     IDs restart from zero when `replace_dom` swaps the document. The
    //     previous page's handler for node 42 therefore fires on the *new*
    //     page's node 42.
    //
    // Called from `Page::reset_for_reuse` alongside `__cancelAllTimers()`.
    // Non-enumerable so it does not widen `Object.getOwnPropertyNames(window)`.
    Object.defineProperty(globalThis, '__cancelAllListeners', {
        value: function __cancelAllListeners() {
            _nodeListeners.clear();
            // Reassign rather than clear: WeakMap has no `clear()`, and the
            // whole point is to drop the `window`-keyed entry.
            _objListeners = new WeakMap();
        },
        writable: true,
        configurable: true,
        enumerable: false,
    });

    const _getNodeIdOrMinusOne = (globalThis.__browser_oxide && globalThis.__browser_oxide._getNodeId)
        ? globalThis.__browser_oxide._getNodeId
        : (() => -1);

    function _getListenersMap(target) {
        const nodeId = _getNodeIdOrMinusOne(target);
        // Node IDs: >0 for elements/text, 0 for document (sometimes), -999 for window.
        // We use the Map for any node that has a stable ID.
        if (nodeId !== -1) {
            let m = _nodeListeners.get(nodeId);
            if (!m) { m = new Map(); _nodeListeners.set(nodeId, m); }
            return m;
        } else {
            let m = _objListeners.get(target);
            if (!m) { m = new Map(); _objListeners.set(target, m); }
            return m;
        }
    }

    function _getListeners(target, type) {
        const nodeMap = _getListenersMap(target);
        let arr = nodeMap.get(type);
        if (!arr) { arr = []; nodeMap.set(type, arr); }
        return arr;
    }

    const _addEventListener = function addEventListener(type, callback, options) {
        if (typeof callback !== "function" && typeof callback !== "object") return;
        if (type === "voiceschanged") {
            try {
                if (Deno.core.ops.op_speech_private_trace_enabled
                    && Deno.core.ops.op_speech_private_trace_enabled()) {
                    const handler = typeof callback === "function"
                        ? callback : callback && callback.handleEvent;
                    Deno.core.ops.op_speech_private_trace(JSON.stringify({
                        phase: "add-listener",
                        origin: String(globalThis.location && globalThis.location.origin || ""),
                        at: Number(globalThis.performance && globalThis.performance.now() || 0),
                        targetCtor: String(this && this.constructor && this.constructor.name || ""),
                        sourceLength: handler ? String(handler).length : 0,
                    }));
                }
            } catch (_) {}
        }
        const capture = typeof options === "boolean" ? options : !!(options && options.capture);
        const once = typeof options === "object" && options ? !!options.once : false;
        const passive = typeof options === "object" && options ? !!options.passive : false;
        const listeners = _getListeners(this, type);
        // Prevent duplicate
        if (listeners.some(l => l.callback === callback && l.capture === capture)) return;
        listeners.push({ callback, capture, once, passive });
    };

    const _removeEventListener = function removeEventListener(type, callback, options) {
        const capture = typeof options === "boolean" ? options : !!(options && options.capture);
        const listeners = _getListeners(this, type);
        const idx = listeners.findIndex(l => l.callback === callback && l.capture === capture);
        if (idx !== -1) listeners.splice(idx, 1);
    };

    const _dispatchEvent = function dispatchEvent(event) {
        if (!(event instanceof Event)) {
            throw new TypeError("Failed to execute 'dispatchEvent' on 'EventTarget': parameter 1 is not of type 'Event'.");
        }
        const eventState = _stateFor(_eventState, event);
        if (eventState.dispatching) {
            throw new DOMException('The event is already being dispatched.', 'InvalidStateError');
        }
        const privateMessageTrace = _eventPrivateTraceEnabled && event.type === "message";
        const privateDispatchStart = privateMessageTrace ? performance.now() : 0;
        eventState.dispatching = true;
        eventState.target = this;
        const nodeId = _getNodeIdOrMinusOne(this);

        // Build propagation path (target → root) if it's a DOM node.
        // Real Chrome's EventTarget.prototype.dispatchEvent handles the
        // tree-walk automatically if 'this' is a Node.
        const path = [];
        if (nodeId !== -1 && this.parentNode !== undefined) {
            let current = this;
            while (current) {
                path.push(current);
                current = current.parentNode;
            }
        }
        // Non-Node EventTargets still expose themselves while dispatch is in
        // progress. Blink clears the composed path immediately afterwards.
        if (path.length === 0) path.push(this);
        eventState.path = path.slice();

        // Capture phase (root → target)
        if (path.length > 0 && !eventState.stopped) {
            for (let i = path.length - 1; i > 0; i--) {
                eventState.currentTarget = path[i];
                eventState.eventPhase = 1;
                _fireListeners(path[i], event, true);
                if (eventState.stopped) break;
            }
        }

        // Target phase
        if (!eventState.stopped) {
            eventState.currentTarget = this;
            eventState.eventPhase = 2;
            _fireListeners(this, event, true);
            _fireListeners(this, event, false);
        }

        // Bubble phase (target → root)
        if (path.length > 0 && !eventState.stopped && eventState.bubbles) {
            for (let i = 1; i < path.length; i++) {
                eventState.currentTarget = path[i];
                eventState.eventPhase = 3;
                _fireListeners(path[i], event, false);
                if (eventState.stopped) break;
            }
        }

        eventState.eventPhase = 0;
        eventState.currentTarget = null;
        eventState.path = [];
        eventState.dispatching = false;
        if (privateMessageTrace) {
            let listenerCount = 0;
            let hasHandler = false;
            try {
                listenerCount = _getListeners(this, event.type).length;
                hasHandler = typeof this.onmessage === "function";
            } catch (_) {}
            _eventPrivateTrace({
                phase: "dispatch",
                type: "message",
                duration: performance.now() - privateDispatchStart,
                listenerCount,
                hasHandler,
                trusted: !!event.isTrusted,
            });
        }
        return !eventState.defaultPrevented;
    };

    function _fireListeners(target, event, capturePhase) {
        // --- 1. Fire on* handler (Target phase only, not capture phase) ---
        const eventState = _stateFor(_eventState, event);
        if (!capturePhase && !eventState.stoppedImmediate) {
            const handlerName = `on${event.type}`;
            const handler = target[handlerName];
            if (typeof handler === "function") {
                const privateStart = _eventPrivateTraceEnabled && event.type === "message"
                    ? performance.now() : 0;
                try {
                    handler.call(target, event);
                } catch (e) {
                    try {
                        _eventDiagNote && _eventDiagNote(
                            "event-handler-error " + event.type + " "
                            + String((e && e.stack) || e).slice(0, 1000)
                        );
                    } catch (_) {}
                    console.error(e);
                } finally {
                    if (privateStart) {
                        _eventPrivateTrace({
                            phase: "handler",
                            type: "message",
                            duration: performance.now() - privateStart,
                            name: String(handler.name || ""),
                            sourceLength: String(handler).length,
                        });
                    }
                }
            }
        }

        // --- 2. Fire registered listeners ---
        const listeners = _getListeners(target, event.type);
        // The event listener list is snapshotted for each target/phase. A
        // listener registered by a callback must not observe the dispatch
        // already in progress. Removed listeners are skipped, however, so
        // keep the live list as the membership source of truth.
        const snapshot = listeners.slice();
        for (const l of snapshot) {
            if (!listeners.includes(l)) continue;
            if (l.capture !== capturePhase) continue;
            if (eventState.stoppedImmediate) break;
            // Remove once-listeners before invoking them. Nested dispatches
            // from the callback must not re-enter the same listener.
            if (l.once) {
                const index = listeners.indexOf(l);
                if (index !== -1) listeners.splice(index, 1);
            }
            // Web event-listener exceptions are reported, not rethrown from
            // dispatchEvent.  Dispatch must continue with later listeners;
            // otherwise one faulty observer can suppress the target's actual
            // handler/state-machine callback.
            try {
                const privateStart = _eventPrivateTraceEnabled && event.type === "message"
                    ? performance.now() : 0;
                if (typeof l.callback === "function") {
                    l.callback.call(target, event);
                } else if (l.callback && typeof l.callback.handleEvent === "function") {
                    l.callback.handleEvent(event);
                }
                if (privateStart) {
                    const callback = typeof l.callback === "function"
                        ? l.callback : l.callback && l.callback.handleEvent;
                    _eventPrivateTrace({
                        phase: "listener",
                        type: "message",
                        duration: performance.now() - privateStart,
                        name: String((callback && callback.name) || ""),
                        sourceLength: callback ? String(callback).length : 0,
                    });
                }
            } catch (e) {
                try {
                    _eventDiagNote && _eventDiagNote(
                        "event-listener-error " + event.type + " "
                        + String((e && e.stack) || e).slice(0, 1000)
                    );
                } catch (_) {}
                console.error(e);
            }
        }
    }

    // Install on EventTarget.prototype — this is the canonical location.
    // Real Chrome has them as configurable/writable/enumerable=true.
    const _ET = globalThis.EventTarget;
    if (_ET && _ET.prototype) {
        const proto = _ET.prototype;
        const constructorDescriptor = Object.getOwnPropertyDescriptor(proto, 'constructor');
        try { delete proto.constructor; } catch (_) {}
        Object.defineProperty(proto, 'addEventListener', {
            value: _addEventListener, writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(proto, 'dispatchEvent', {
            value: _dispatchEvent, writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(proto, 'removeEventListener', {
            value: _removeEventListener, writable: true, enumerable: true, configurable: true,
        });
        const _when = { when(type, options = {}) {
            const target = this;
            return new Promise((resolve, reject) => {
                const signal = options && options.signal;
                const done = event => {
                    target.removeEventListener(type, done);
                    resolve(event);
                };
                if (signal && signal.aborted) {
                    reject(signal.reason || new DOMException('The operation was aborted', 'AbortError'));
                    return;
                }
                target.addEventListener(type, done, { once: true });
                if (signal && typeof signal.addEventListener === 'function') {
                    signal.addEventListener('abort', () => {
                        target.removeEventListener(type, done);
                        reject(signal.reason || new DOMException('The operation was aborted', 'AbortError'));
                    }, { once: true });
                }
            });
        } }.when;
        Object.defineProperty(proto, 'when', {
            value: _native(_when, 'when'), writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(proto, 'constructor', constructorDescriptor);
    }

    // Ensure Node.prototype does NOT shadow these. Real Chrome's
    // Node.prototype does not have its own addEventListener.
    const origNodeProto = globalThis.Node.prototype;
    if (origNodeProto) {
        delete origNodeProto.addEventListener;
        delete origNodeProto.removeEventListener;
        delete origNodeProto.dispatchEvent;
    }

    // Native-code masking — some scripts run
    // `Function.prototype.toString.call(addEventListener)` against both
    // window-level and prototype-level methods. Each must serialize as
    // `function NAME() { [native code] }`, as in a real browser.
    if (typeof _maskFunction === 'function') {
        _maskFunction(_addEventListener, 'addEventListener');
        _maskFunction(_removeEventListener, 'removeEventListener');
        _maskFunction(_dispatchEvent, 'dispatchEvent');
    }

    // Window (globalThis) inheritance: real Chrome's Window inherits from
    // EventTarget via the prototype chain. Our Window setup (Window →
    // WindowProperties → EventTarget) should already handle this, but
    // we ensure the global aliases are correct.
    const _winProto = Object.getPrototypeOf(globalThis);
    if (_winProto && _winProto !== Object.prototype) {
        // Just ensure they are there if not inherited.
        if (!('addEventListener' in _winProto)) {
            Object.defineProperty(_winProto, 'addEventListener', {
                value: _addEventListener, writable: true, enumerable: true, configurable: true,
            });
        }
        if (!('removeEventListener' in _winProto)) {
            Object.defineProperty(_winProto, 'removeEventListener', {
                value: _removeEventListener, writable: true, enumerable: true, configurable: true,
            });
        }
        if (!('dispatchEvent' in _winProto)) {
            Object.defineProperty(_winProto, 'dispatchEvent', {
                value: _dispatchEvent, writable: true, enumerable: true, configurable: true,
            });
        }
    } else {
        globalThis.addEventListener = _addEventListener;
        globalThis.removeEventListener = _removeEventListener;
        globalThis.dispatchEvent = _dispatchEvent;
    }

    // Export all event classes
    // SecurityPolicyViolationEvent — what real Chrome dispatches on
    // `document` (and propagates to `window`) when a CSP rule blocks
    // a fetch. Sites can listen for `securitypolicyviolation` to log
    // their own violations; we must surface the same shape so that
    // analytics/telemetry code probing the event fires correctly.
    // Spec: https://www.w3.org/TR/CSP3/#securitypolicyviolationevent
    class SecurityPolicyViolationEvent extends Event {
        constructor(type, init) {
            super(type, init || {});
            const i = init || {};
            this.blockedURI = String(i.blockedURI ?? "");
            this.documentURI = String(i.documentURI ?? (typeof location !== 'undefined' ? location.href : ""));
            this.referrer = String(i.referrer ?? (typeof document !== 'undefined' && document.referrer ? document.referrer : ""));
            this.violatedDirective = String(i.violatedDirective ?? "");
            this.effectiveDirective = String(i.effectiveDirective ?? this.violatedDirective);
            this.originalPolicy = String(i.originalPolicy ?? "");
            this.disposition = String(i.disposition ?? "enforce");
            this.sample = String(i.sample ?? "");
            this.sourceFile = String(i.sourceFile ?? "");
            this.statusCode = +i.statusCode || 0;
            this.lineNumber = +i.lineNumber || 0;
            this.columnNumber = +i.columnNumber || 0;
        }
    }
    try {
        Object.defineProperty(SecurityPolicyViolationEvent.prototype, Symbol.toStringTag, {
            value: 'SecurityPolicyViolationEvent',
            writable: false,
            enumerable: false,
            configurable: true,
        });
    } catch (_) {}

    globalThis.Event = Event;
    globalThis.CustomEvent = CustomEvent;
    globalThis.SecurityPolicyViolationEvent = SecurityPolicyViolationEvent;
    globalThis.UIEvent = UIEvent;
    globalThis.MouseEvent = MouseEvent;
    globalThis.KeyboardEvent = KeyboardEvent;
    globalThis.InputEvent = InputEvent;
    globalThis.FocusEvent = FocusEvent;
    globalThis.PointerEvent = PointerEvent;
    globalThis.WheelEvent = WheelEvent;
    globalThis.TouchEvent = TouchEvent;
    globalThis.MessageEvent = MessageEvent;
    globalThis.ErrorEvent = ErrorEvent;
    globalThis.ProgressEvent = ProgressEvent;
    globalThis.AnimationEvent = AnimationEvent;
    globalThis.TransitionEvent = TransitionEvent;
    globalThis.ClipboardEvent = ClipboardEvent;
    globalThis.PopStateEvent = PopStateEvent;
    globalThis.HashChangeEvent = HashChangeEvent;
    globalThis.StorageEvent = StorageEvent;
    globalThis.PageTransitionEvent = PageTransitionEvent;
    globalThis.BeforeUnloadEvent = BeforeUnloadEvent;
    globalThis.DragEvent = DragEvent;
    // EventTarget is already defined in dom_bootstrap.js as the base of
    // the Node prototype chain — do not reassign it here or the
    // `document instanceof EventTarget` check will break.

    // Browser-generated cross-frame postMessage events are trusted. Hand the
    // private WeakSet-backed minter into dom_bootstrap's closure before the
    // temporary bootstrap object is removed; page JS never receives the fn.
    try {
        const bo = globalThis.__browser_oxide;
        if (bo && typeof bo._installFrameMessageTrustMarker === 'function') {
            bo._installFrameMessageTrustMarker(_markTrusted);
        }
        if (bo && typeof bo._installFrameEventStateAccessors === 'function') {
            bo._installFrameEventStateAccessors(
                function getEventState(event) {
                    const state = _stateFor(_eventState, event);
                    return {
                        target: state.target,
                        currentTarget: state.currentTarget,
                        eventPhase: state.eventPhase,
                        bubbles: state.bubbles,
                        defaultPrevented: state.defaultPrevented,
                        stopped: state.stopped,
                        stoppedImmediate: state.stoppedImmediate,
                        dispatching: state.dispatching,
                    };
                },
                function setEventState(event, patch) {
                    const state = _stateFor(_eventState, event);
                    if (!patch || typeof patch !== 'object') return;
                    for (const name of ['target', 'currentTarget', 'eventPhase',
                        'stopped', 'stoppedImmediate', 'dispatching']) {
                        if (Object.prototype.hasOwnProperty.call(patch, name)) {
                            state[name] = patch[name];
                        }
                    }
                    if (Array.isArray(patch.path)) state.path = patch.path.slice();
                },
            );
        }
        if (bo) {
            bo._markTrustedEvent = _markTrusted;
            // Some Web APIs have non-DOM EventTarget parent relationships.
            // IndexedDB request errors, for example, bubble from IDBRequest to
            // IDBTransaction while retaining the original Event identity and
            // target. Keep that hop inside this module so the private Event
            // WeakMap remains unforgeable and no page-visible event fields are
            // introduced.
            bo._dispatchEventToParentTarget = function(event, parentTarget) {
                const state = _stateFor(_eventState, event);
                if (!state.bubbles || state.stopped || !parentTarget) {
                    return !state.defaultPrevented;
                }
                state.currentTarget = parentTarget;
                state.eventPhase = 3;
                _fireListeners(parentTarget, event, false);
                state.currentTarget = null;
                state.eventPhase = 0;
                return !state.defaultPrevented;
            };
            const _lifecycleState = function() {
                const state = globalThis._browser_oxide;
                const lifecycle = state
                    ? (state.__navigationLifecycleTiming ||
                        (state.__navigationLifecycleTiming = {}))
                    : null;
                const stamp = (name) => {
                    if (!lifecycle) return;
                    try { lifecycle[name] = performance.now(); } catch (_) {}
                };
                return { state, stamp };
            };

            bo._markDocumentInteractive = function() {
                const { state, stamp } = _lifecycleState();
                const trusted = (type, options) => _markTrusted(new Event(type, options));

                stamp('domInteractive');
                if (state) state.__documentReadyState = 'interactive';
                document.dispatchEvent(trusted('readystatechange'));
            };

            bo._dispatchDOMContentLoaded = function() {
                const { stamp } = _lifecycleState();
                const trusted = (type, options) => _markTrusted(new Event(type, options));
                // DOM standard: bubbles:true so the event propagates to the
                // window (window-level DOMContentLoaded listeners).
                stamp('domContentLoadedEventStart');
                document.dispatchEvent(trusted('DOMContentLoaded', { bubbles: true }));
                stamp('domContentLoadedEventEnd');
            };

            bo._dispatchLoad = function() {
                const { state, stamp } = _lifecycleState();
                const trusted = (type, options) => _markTrusted(new Event(type, options));
                stamp('domComplete');
                if (state) state.__documentReadyState = 'complete';
                document.dispatchEvent(trusted('readystatechange'));
                stamp('loadEventStart');
                window.dispatchEvent(trusted('load'));
                stamp('loadEventEnd');
                try { globalThis[Symbol.for('__browser_oxide_mark_load__')](); } catch (_) {}
            };

            bo._completeDocumentLifecycle = function() {
                bo._markDocumentInteractive();
                bo._dispatchDOMContentLoaded();
                bo._dispatchLoad();
            };
        }
    } catch (_) { /* ignore */ }

    // Privileged handoff of the trusted-event minter (behavioral E1/E2). Our
    // init scripts (humanize.js) capture this into a closure and `delete` it
    // synchronously at their top — before any page script runs — so page JS
    // never observes it. Non-enumerable to keep it off Object.keys scans even
    // in the brief window before capture.
    try {
        Object.defineProperty(globalThis, '__bo_mark_trusted', {
            value: _markTrusted,
            configurable: true,
            enumerable: false,
            writable: false,
        });
    } catch (_) { /* ignore */ }
})(globalThis);
