((globalThis) => {
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
    const _messageEventState = new WeakMap();

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
            super(type, { bubbles: true, cancelable: true, ...options });
            this.key = options.key || "";
            this.code = options.code || "";
            this.keyCode = options.keyCode || 0;
            this.charCode = options.charCode || 0;
            this.which = options.which || options.keyCode || 0;
            this.ctrlKey = !!options.ctrlKey;
            this.shiftKey = !!options.shiftKey;
            this.altKey = !!options.altKey;
            this.metaKey = !!options.metaKey;
            this.repeat = !!options.repeat;
            this.isComposing = !!options.isComposing;
            this.location = options.location || 0;
        }
        getModifierState(key) { return false; }
    }

    class InputEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, { bubbles: true, cancelable: false, ...options });
            this.data = options.data || null;
            this.inputType = options.inputType || "";
            this.isComposing = !!options.isComposing;
        }
    }

    class FocusEvent extends UIEvent {
        constructor(type, options = {}) {
            super(type, options);
            this.relatedTarget = options.relatedTarget || null;
        }
    }

    class PointerEvent extends MouseEvent {
        constructor(type, options = {}) {
            super(type, options);
            this.pointerId = options.pointerId || 0;
            this.width = options.width || 1;
            this.height = options.height || 1;
            this.pressure = options.pressure || 0;
            this.tangentialPressure = options.tangentialPressure || 0;
            this.tiltX = options.tiltX || 0;
            this.tiltY = options.tiltY || 0;
            this.twist = options.twist || 0;
            this.pointerType = options.pointerType || "mouse";
            this.isPrimary = options.isPrimary !== undefined ? options.isPrimary : true;
        }
    }

    class WheelEvent extends MouseEvent {
        constructor(type, options = {}) {
            super(type, options);
            this.deltaX = options.deltaX || 0;
            this.deltaY = options.deltaY || 0;
            this.deltaZ = options.deltaZ || 0;
            this.deltaMode = options.deltaMode || 0;
        }
        static DOM_DELTA_PIXEL = 0;
        static DOM_DELTA_LINE = 1;
        static DOM_DELTA_PAGE = 2;
    }

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
            _fireListeners(this, event, false);
            _fireListeners(this, event, true);
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
        return !eventState.defaultPrevented;
    };

    function _fireListeners(target, event, capturePhase) {
        // --- 1. Fire on* handler (Target phase only, not capture phase) ---
        const eventState = _stateFor(_eventState, event);
        if (!capturePhase && !eventState.stoppedImmediate) {
            const handlerName = `on${event.type}`;
            const handler = target[handlerName];
            if (typeof handler === "function") {
                try {
                    handler.call(target, event);
                } catch (e) {
                    console.error(e);
                }
            }
        }

        // --- 2. Fire registered listeners ---
        const listeners = _getListeners(target, event.type);
        const toRemove = [];
        for (let i = 0; i < listeners.length; i++) {
            const l = listeners[i];
            if (l.capture !== capturePhase) continue;
            if (eventState.stoppedImmediate) break;
            if (typeof l.callback === "function") {
                l.callback.call(target, event);
            } else if (l.callback && typeof l.callback.handleEvent === "function") {
                l.callback.handleEvent(event);
            }
            if (l.once) toRemove.push(i);
        }
        for (let i = toRemove.length - 1; i >= 0; i--) {
            listeners.splice(toRemove[i], 1);
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
            bo._completeDocumentLifecycle = function() {
                const state = globalThis._browser_oxide;
                const trusted = (type, options) => _markTrusted(new Event(type, options));

                if (state) state.__documentReadyState = 'interactive';
                document.dispatchEvent(trusted('readystatechange'));
                // DOM standard: bubbles:true so the event propagates to the
                // window (window-level DOMContentLoaded listeners).
                document.dispatchEvent(trusted('DOMContentLoaded', { bubbles: true }));

                if (state) state.__documentReadyState = 'complete';
                document.dispatchEvent(trusted('readystatechange'));
                window.dispatchEvent(trusted('load'));
                try { globalThis[Symbol.for('__browser_oxide_mark_load__')](); } catch (_) {}
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
