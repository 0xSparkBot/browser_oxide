// Scheduling/abort WebIDL normalization shared by Window and Worker realms.
//
// The Window realm already has a functional AbortController/AbortSignal pair;
// this layer adds AbortSignal.any() and normalizes WebIDL key order.  Older
// worker bootstrap paths still expose an own-field AbortSignal; detect that
// shape and replace it with the same EventTarget-backed semantics first.
(() => {
    const _mask = typeof globalThis._maskFunction === 'function'
        ? globalThis._maskFunction
        : (fn) => fn;
    const _validPriorities = new Set(['user-blocking', 'user-visible', 'background']);

    const _nativeMethod = (prototype, name, fn) => {
        Object.defineProperty(prototype, name, {
            value: _mask(fn, name),
            writable: true,
            enumerable: true,
            configurable: true,
        });
    };
    const _nativeGetter = (prototype, name, getter, setter = undefined) => {
        Object.defineProperty(prototype, name, {
            get: _mask(getter, `get ${name}`),
            set: setter ? _mask(setter, `set ${name}`) : undefined,
            enumerable: true,
            configurable: true,
        });
    };
    const _reorderPrototype = (prototype, names) => {
        const descriptors = new Map();
        for (const name of names) {
            const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
            if (descriptor) descriptors.set(name, descriptor);
        }
        for (const name of names) {
            try { delete prototype[name]; } catch (_) {}
        }
        for (const name of names) {
            const descriptor = descriptors.get(name);
            if (descriptor) Object.defineProperty(prototype, name, descriptor);
        }
    };
    const _illegalConstructor = (name) => {
        throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
    };
    const _validatePriority = (value, context) => {
        const priority = String(value);
        if (!_validPriorities.has(priority)) {
            throw new TypeError(
                `${context}: The provided value '${priority}' is not a valid enum value of type TaskPriority.`
            );
        }
        return priority;
    };

    // -----------------------------------------------------------------
    // AbortSignal / AbortController
    // -----------------------------------------------------------------
    let AbortSignalCtor = globalThis.AbortSignal;
    let AbortControllerCtor = globalThis.AbortController;
    let needsAbortRebuild = false;
    try {
        const controller = new AbortControllerCtor();
        const signal = controller.signal;
        needsAbortRebuild = !(signal instanceof globalThis.EventTarget)
            || Reflect.ownKeys(signal).length !== 0;
    } catch (_) {
        needsAbortRebuild = true;
    }

    if (needsAbortRebuild) {
        const internalToken = {};
        const signalState = new WeakMap();
        class AbortSignal extends EventTarget {
            constructor(token = undefined) {
                super();
                if (token !== internalToken) _illegalConstructor('AbortSignal');
                signalState.set(this, { aborted: false, reason: undefined, onabort: null });
            }
        }
        const createSignal = () => new AbortSignal(internalToken);
        const fireSignal = (signal, reason) => {
            const state = signalState.get(signal);
            if (!state || state.aborted) return;
            state.aborted = true;
            state.reason = reason === undefined
                ? new DOMException('The operation was aborted.', 'AbortError')
                : reason;
            signal.dispatchEvent(new Event('abort'));
        };
        _nativeGetter(AbortSignal.prototype, 'aborted', function aborted() {
            const state = signalState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.aborted;
        });
        _nativeGetter(AbortSignal.prototype, 'reason', function reason() {
            const state = signalState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.reason;
        });
        _nativeGetter(
            AbortSignal.prototype,
            'onabort',
            function onabort() {
                const state = signalState.get(this);
                if (!state) throw new TypeError('Illegal invocation');
                return state.onabort;
            },
            function onabort(value) {
                const state = signalState.get(this);
                if (!state) throw new TypeError('Illegal invocation');
                state.onabort = typeof value === 'function' ? value : null;
            },
        );
        _nativeMethod(AbortSignal.prototype, 'throwIfAborted', function throwIfAborted() {
            const state = signalState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            if (state.aborted) throw state.reason;
        });
        Object.defineProperty(AbortSignal.prototype, Symbol.toStringTag, {
            value: 'AbortSignal', writable: false, enumerable: false, configurable: true,
        });
        Object.defineProperty(AbortSignal, 'abort', {
            value: _mask(function abort(reason = undefined) {
                const signal = createSignal();
                fireSignal(signal, reason);
                return signal;
            }, 'abort'),
            writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(AbortSignal, 'timeout', {
            value: _mask(function timeout(milliseconds) {
                const signal = createSignal();
                setTimeout(() => fireSignal(
                    signal,
                    new DOMException('The operation timed out.', 'TimeoutError'),
                ), Number(milliseconds));
                return signal;
            }, 'timeout'),
            writable: true, enumerable: true, configurable: true,
        });
        _mask(AbortSignal, 'AbortSignal');

        const controllerState = new WeakMap();
        class AbortController {
            constructor() { controllerState.set(this, createSignal()); }
        }
        _nativeGetter(AbortController.prototype, 'signal', function signal() {
            const value = controllerState.get(this);
            if (!value) throw new TypeError('Illegal invocation');
            return value;
        });
        _nativeMethod(AbortController.prototype, 'abort', function abort(reason = undefined) {
            const signal = controllerState.get(this);
            if (!signal) throw new TypeError('Illegal invocation');
            fireSignal(signal, reason);
        });
        Object.defineProperty(AbortController.prototype, Symbol.toStringTag, {
            value: 'AbortController', writable: false, enumerable: false, configurable: true,
        });
        _mask(AbortController, 'AbortController');
        Object.defineProperty(globalThis, 'AbortSignal', {
            value: AbortSignal, writable: true, enumerable: false, configurable: true,
        });
        Object.defineProperty(globalThis, 'AbortController', {
            value: AbortController, writable: true, enumerable: false, configurable: true,
        });
        AbortSignalCtor = AbortSignal;
        AbortControllerCtor = AbortController;
    }

    // Static order in Blink is abort, any, timeout.
    const timeoutDescriptor = Object.getOwnPropertyDescriptor(AbortSignalCtor, 'timeout');
    if (timeoutDescriptor?.configurable) delete AbortSignalCtor.timeout;
    Object.defineProperty(AbortSignalCtor, 'any', {
        value: _mask(function any(signals) {
            const list = Array.from(signals);
            const controller = new AbortControllerCtor();
            const cleanups = [];
            const cleanup = () => {
                for (const [signal, listener] of cleanups) {
                    try { signal.removeEventListener('abort', listener); } catch (_) {}
                }
                cleanups.length = 0;
            };
            for (const signal of list) {
                if (!(signal instanceof AbortSignalCtor)) {
                    throw new TypeError("Failed to execute 'any' on 'AbortSignal': The provided value is not of type 'AbortSignal'.");
                }
                if (signal.aborted) {
                    controller.abort(signal.reason);
                    return controller.signal;
                }
            }
            for (const signal of list) {
                const listener = () => {
                    if (!controller.signal.aborted) controller.abort(signal.reason);
                    cleanup();
                };
                cleanups.push([signal, listener]);
                signal.addEventListener('abort', listener, { once: true });
            }
            return controller.signal;
        }, 'any'),
        writable: true, enumerable: true, configurable: true,
    });
    if (timeoutDescriptor) Object.defineProperty(AbortSignalCtor, 'timeout', timeoutDescriptor);
    _reorderPrototype(AbortSignalCtor.prototype,
        ['aborted', 'reason', 'onabort', 'throwIfAborted', 'constructor', Symbol.toStringTag]);
    _reorderPrototype(AbortControllerCtor.prototype,
        ['signal', 'abort', 'constructor', Symbol.toStringTag]);

    // -----------------------------------------------------------------
    // TaskSignal / TaskController / TaskPriorityChangeEvent
    // -----------------------------------------------------------------
    const taskSignalState = new WeakMap();
    class TaskSignal {
        constructor() { _illegalConstructor('TaskSignal'); }
    }
    Object.setPrototypeOf(TaskSignal, AbortSignalCtor);
    Object.setPrototypeOf(TaskSignal.prototype, AbortSignalCtor.prototype);
    delete TaskSignal.prototype.constructor;
    _nativeGetter(TaskSignal.prototype, 'priority', function priority() {
        const state = taskSignalState.get(this);
        if (!state) throw new TypeError('Illegal invocation');
        return state.priority;
    });
    _nativeGetter(
        TaskSignal.prototype,
        'onprioritychange',
        function onprioritychange() {
            const state = taskSignalState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.onprioritychange;
        },
        function onprioritychange(value) {
            const state = taskSignalState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            state.onprioritychange = typeof value === 'function' ? value : null;
        },
    );
    Object.defineProperty(TaskSignal.prototype, 'constructor', {
        value: TaskSignal, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(TaskSignal.prototype, Symbol.toStringTag, {
        value: 'TaskSignal', writable: false, enumerable: false, configurable: true,
    });
    const promoteTaskSignal = (signal, priority = 'user-visible') => {
        Object.setPrototypeOf(signal, TaskSignal.prototype);
        taskSignalState.set(signal, { priority, onprioritychange: null });
        return signal;
    };
    Object.defineProperty(TaskSignal, 'any', {
        value: _mask(function any(signals) {
            return promoteTaskSignal(AbortSignalCtor.any(signals), 'user-visible');
        }, 'any'),
        writable: true, enumerable: true, configurable: true,
    });
    _mask(TaskSignal, 'TaskSignal');

    const previousPriorityState = new WeakMap();
    class TaskPriorityChangeEvent extends Event {
        constructor(type, init) {
            super(type, init || {});
            const previousPriority = init?.previousPriority === undefined
                ? 'user-visible'
                : _validatePriority(
                    init.previousPriority,
                    "Failed to construct 'TaskPriorityChangeEvent'",
                );
            previousPriorityState.set(this, previousPriority);
        }
    }
    delete TaskPriorityChangeEvent.prototype.constructor;
    _nativeGetter(TaskPriorityChangeEvent.prototype, 'previousPriority', function previousPriority() {
        if (!previousPriorityState.has(this)) throw new TypeError('Illegal invocation');
        return previousPriorityState.get(this);
    });
    Object.defineProperty(TaskPriorityChangeEvent.prototype, 'constructor', {
        value: TaskPriorityChangeEvent, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(TaskPriorityChangeEvent.prototype, Symbol.toStringTag, {
        value: 'TaskPriorityChangeEvent', writable: false, enumerable: false, configurable: true,
    });
    _mask(TaskPriorityChangeEvent, 'TaskPriorityChangeEvent');

    class TaskController extends AbortControllerCtor {
        constructor(options = {}) {
            super();
            const priority = options?.priority === undefined
                ? 'user-visible'
                : _validatePriority(
                    options.priority,
                    "Failed to construct 'TaskController': Failed to read the 'priority' property from 'TaskControllerInit'",
                );
            promoteTaskSignal(this.signal, priority);
        }
    }
    delete TaskController.prototype.constructor;
    _nativeMethod(TaskController.prototype, 'setPriority', function setPriority(value) {
        const priority = _validatePriority(
            value,
            "Failed to execute 'setPriority' on 'TaskController'",
        );
        const signal = this.signal;
        const state = taskSignalState.get(signal);
        if (!state) throw new TypeError('Illegal invocation');
        if (state.priority === priority) return;
        const previousPriority = state.priority;
        state.priority = priority;
        signal.dispatchEvent(new TaskPriorityChangeEvent('prioritychange', { previousPriority }));
    });
    Object.defineProperty(TaskController.prototype, 'constructor', {
        value: TaskController, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(TaskController.prototype, Symbol.toStringTag, {
        value: 'TaskController', writable: false, enumerable: false, configurable: true,
    });
    _mask(TaskController, 'TaskController');

    Object.defineProperty(globalThis, 'TaskSignal', {
        value: TaskSignal, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(globalThis, 'TaskController', {
        value: TaskController, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(globalThis, 'TaskPriorityChangeEvent', {
        value: TaskPriorityChangeEvent, writable: true, enumerable: false, configurable: true,
    });

    // -----------------------------------------------------------------
    // Scheduler singleton + WebIDL prototype
    // -----------------------------------------------------------------
    class Scheduler {
        constructor() { _illegalConstructor('Scheduler'); }
    }
    delete Scheduler.prototype.constructor;
    const schedulerInstances = new WeakSet();
    _nativeMethod(Scheduler.prototype, 'postTask', function postTask(callback, options = {}) {
        if (!schedulerInstances.has(this)) throw new TypeError('Illegal invocation');
        if (arguments.length < 1) {
            throw new TypeError("Failed to execute 'postTask' on 'Scheduler': 1 argument required, but only 0 present.");
        }
        if (typeof callback !== 'function') {
            throw new TypeError("Failed to execute 'postTask' on 'Scheduler': parameter 1 is not of type 'Function'.");
        }
        if (options?.priority !== undefined) {
            _validatePriority(
                options.priority,
                "Failed to execute 'postTask' on 'Scheduler': Failed to read the 'priority' property from 'SchedulerPostTaskOptions'",
            );
        }
        const signal = options?.signal;
        const delay = Math.max(0, Number(options?.delay) || 0);
        return new Promise((resolve, reject) => {
            if (signal?.aborted) {
                reject(signal.reason);
                return;
            }
            let timer = 0;
            const onAbort = () => {
                try { clearTimeout(timer); } catch (_) {}
                try { signal.removeEventListener('abort', onAbort); } catch (_) {}
                reject(signal.reason);
            };
            if (signal && typeof signal.addEventListener === 'function') {
                signal.addEventListener('abort', onAbort, { once: true });
            }
            timer = setTimeout(() => {
                try { signal?.removeEventListener?.('abort', onAbort); } catch (_) {}
                try { resolve(callback()); } catch (error) { reject(error); }
            }, delay);
        });
    });
    _nativeMethod(Scheduler.prototype, 'yield', function yieldTask() {
        if (!schedulerInstances.has(this)) throw new TypeError('Illegal invocation');
        return new Promise(resolve => setTimeout(resolve, 0));
    });
    // Native name is `yield`, not the source-level helper name.
    try { Object.defineProperty(Scheduler.prototype.yield, 'name', { value: 'yield', configurable: true }); } catch (_) {}
    _mask(Scheduler.prototype.yield, 'yield');
    Object.defineProperty(Scheduler.prototype, 'constructor', {
        value: Scheduler, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(Scheduler.prototype, Symbol.toStringTag, {
        value: 'Scheduler', writable: false, enumerable: false, configurable: true,
    });
    _mask(Scheduler, 'Scheduler');
    Object.defineProperty(globalThis, 'Scheduler', {
        value: Scheduler, writable: true, enumerable: false, configurable: true,
    });

    let scheduler = globalThis.scheduler;
    if (!scheduler || (typeof scheduler !== 'object' && typeof scheduler !== 'function')) {
        scheduler = Object.create(Scheduler.prototype);
    } else {
        try { Object.setPrototypeOf(scheduler, Scheduler.prototype); } catch (_) {}
        for (const name of ['postTask', 'yield', Symbol.toStringTag]) {
            try { delete scheduler[name]; } catch (_) {}
        }
    }
    schedulerInstances.add(scheduler);
    try { globalThis.scheduler = scheduler; } catch (_) {
        try {
            Object.defineProperty(globalThis, 'scheduler', {
                value: scheduler, writable: true, enumerable: true, configurable: true,
            });
        } catch (_) {}
    }
})();
