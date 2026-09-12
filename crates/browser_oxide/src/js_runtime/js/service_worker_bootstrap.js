// ServiceWorker registration lifecycle for Window realms.
//
// The host owns origin-scoped registration records. This layer provides
// Chrome-shaped WebIDL wrappers and stable per-realm object identity. It does
// not execute Service Worker scripts or intercept fetches; `controller`
// therefore remains null unless a future execution layer takes ownership.
((globalThis) => {
    const ops = Deno.core.ops;
    const mask = globalThis._maskFunction || ((fn) => fn);
    const EventTargetCtor = globalThis.EventTarget;
    const ContainerCtor = globalThis.ServiceWorkerContainer;
    const RegistrationCtor = globalThis.ServiceWorkerRegistration;
    const WorkerCtor = globalThis.ServiceWorker;
    if (!ContainerCtor || !RegistrationCtor || !WorkerCtor || !EventTargetCtor) return;

    const containerProto = ContainerCtor.prototype;
    const registrationProto = RegistrationCtor.prototype;
    const workerProto = WorkerCtor.prototype;
    try { Object.setPrototypeOf(containerProto, EventTargetCtor.prototype); } catch (_) {}
    try { Object.setPrototypeOf(registrationProto, EventTargetCtor.prototype); } catch (_) {}
    try { Object.setPrototypeOf(workerProto, EventTargetCtor.prototype); } catch (_) {}
    try { Object.setPrototypeOf(ContainerCtor, EventTargetCtor); } catch (_) {}
    try { Object.setPrototypeOf(RegistrationCtor, EventTargetCtor); } catch (_) {}
    try { Object.setPrototypeOf(WorkerCtor, EventTargetCtor); } catch (_) {}

    const resetPrototype = (prototype) => {
        for (const key of Reflect.ownKeys(prototype)) {
            const descriptor = Object.getOwnPropertyDescriptor(prototype, key);
            if (descriptor && descriptor.configurable) {
                try { delete prototype[key]; } catch (_) {}
            }
        }
    };
    resetPrototype(containerProto);
    resetPrototype(registrationProto);
    resetPrototype(workerProto);

    const method = (prototype, name, fn) => {
        mask(fn, name);
        Object.defineProperty(prototype, name, {
            value: fn,
            enumerable: true,
            configurable: true,
            writable: true,
        });
    };
    const getter = (prototype, name, get) => {
        mask(get, `get ${name}`);
        Object.defineProperty(prototype, name, {
            get,
            enumerable: true,
            configurable: true,
        });
    };
    const accessor = (prototype, name, get, set) => {
        mask(get, `get ${name}`);
        mask(set, `set ${name}`);
        Object.defineProperty(prototype, name, {
            get,
            set,
            enumerable: true,
            configurable: true,
        });
    };
    const ctorAndTag = (prototype, ctor, tag) => {
        Object.defineProperty(prototype, 'constructor', {
            value: ctor,
            enumerable: false,
            configurable: true,
            writable: true,
        });
        Object.defineProperty(prototype, Symbol.toStringTag, {
            value: tag,
            enumerable: false,
            configurable: true,
            writable: false,
        });
    };

    const workerState = new WeakMap();
    const registrationState = new WeakMap();
    const containerState = new WeakMap();
    const registrationCache = new Map();
    const workerCache = new Map();

    const origin = () => String(globalThis.location?.origin || 'null');
    const currentURL = () => String(globalThis.location?.href || 'about:blank');
    const toURL = (value, base = currentURL()) => new URL(String(value), base);
    const workerKey = (snapshot) => `${origin()}\n${snapshot.id}`;

    const workerFor = (snapshot, initialState = 'activated') => {
        const key = workerKey(snapshot);
        let worker = workerCache.get(key);
        if (!worker) {
            worker = Object.create(workerProto);
            workerState.set(worker, {
                scriptURL: snapshot.scriptUrl,
                state: initialState,
                onstatechange: null,
                onerror: null,
            });
            workerCache.set(key, worker);
        } else {
            const state = workerState.get(worker);
            state.scriptURL = snapshot.scriptUrl;
            if (initialState) state.state = initialState;
        }
        return worker;
    };

    const registrationFor = (snapshot, activation = 'active') => {
        let registration = registrationCache.get(snapshot.id);
        if (!registration) {
            registration = Object.create(registrationProto);
            const active = activation === 'active' ? workerFor(snapshot, 'activated') : null;
            const installing = activation === 'installing' ? workerFor(snapshot, 'installing') : null;
            registrationState.set(registration, {
                snapshot: { ...snapshot },
                active,
                installing,
                waiting: null,
                onupdatefound: null,
                navigationPreload: null,
                paymentManager: null,
                backgroundFetch: null,
                periodicSync: null,
                sync: null,
                cookies: null,
                pushManager: null,
            });
            registrationCache.set(snapshot.id, registration);
        } else {
            registrationState.get(registration).snapshot = { ...snapshot };
        }
        return registration;
    };

    const refreshRegistration = (registration) => {
        const state = registrationState.get(registration);
        if (!state) return null;
        const snapshot = ops.op_service_worker_get(state.snapshot.origin || origin(), state.snapshot.id);
        if (snapshot) state.snapshot = { ...snapshot, origin: state.snapshot.origin || origin() };
        return state;
    };

    const activateRegistration = (registration) => {
        const state = registrationState.get(registration);
        if (!state || !state.installing) return;
        const worker = state.installing;
        const workerInfo = workerState.get(worker);
        workerInfo.state = 'activated';
        state.installing = null;
        state.active = worker;
        try { worker.dispatchEvent(new Event('statechange')); } catch (_) {}
        for (const container of [containerInstance]) {
            if (container) maybeResolveReady(container);
        }
    };

    const normalizeRegistration = (snapshot, activation = 'active') => {
        if (!snapshot) return undefined;
        const registration = registrationFor({ ...snapshot, origin: origin() }, activation);
        return registration;
    };

    const containerInstance = (() => {
        try { return globalThis.navigator?.serviceWorker || null; } catch (_) { return null; }
    })();
    if (containerInstance) {
        for (const key of Reflect.ownKeys(containerInstance)) {
            try { delete containerInstance[key]; } catch (_) {}
        }
        containerState.set(containerInstance, {
            oncontrollerchange: null,
            onmessage: null,
            onmessageerror: null,
            readyPromise: null,
            resolveReady: null,
            readyRegistration: null,
        });
    }

    const matchingSnapshot = (url = currentURL()) => {
        try {
            const target = toURL(url);
            if (target.origin !== origin()) return null;
            return ops.op_service_worker_match(origin(), target.href);
        } catch (_) {
            return null;
        }
    };
    function maybeResolveReady(container) {
        const state = containerState.get(container);
        if (!state || state.readyRegistration) return state?.readyRegistration || null;
        const snapshot = matchingSnapshot();
        if (!snapshot) return null;
        const registration = normalizeRegistration(snapshot, 'active');
        state.readyRegistration = registration;
        if (state.resolveReady) {
            const resolve = state.resolveReady;
            state.resolveReady = null;
            resolve(registration);
        }
        return registration;
    }

    // ServiceWorkerContainer.prototype — Chrome 148 property order.
    getter(containerProto, 'controller', function controller() { return null; });
    getter(containerProto, 'ready', function ready() {
        let state = containerState.get(this);
        if (!state) {
            state = {
                oncontrollerchange: null,
                onmessage: null,
                onmessageerror: null,
                readyPromise: null,
                resolveReady: null,
                readyRegistration: null,
            };
            containerState.set(this, state);
        }
        if (!state.readyPromise) {
            state.readyPromise = new Promise(resolve => { state.resolveReady = resolve; });
            maybeResolveReady(this);
        }
        return state.readyPromise;
    });
    accessor(containerProto, 'oncontrollerchange', function oncontrollerchange() {
        return containerState.get(this)?.oncontrollerchange ?? null;
    }, function oncontrollerchange(value) {
        const state = containerState.get(this); if (state) state.oncontrollerchange = value;
    });
    accessor(containerProto, 'onmessage', function onmessage() {
        return containerState.get(this)?.onmessage ?? null;
    }, function onmessage(value) {
        const state = containerState.get(this); if (state) state.onmessage = value;
    });
    accessor(containerProto, 'onmessageerror', function onmessageerror() {
        return containerState.get(this)?.onmessageerror ?? null;
    }, function onmessageerror(value) {
        const state = containerState.get(this); if (state) state.onmessageerror = value;
    });
    method(containerProto, 'getRegistration', function getRegistration(...args) {
        const clientURL = args[0];
        const target = clientURL === undefined ? currentURL() : toURL(clientURL).href;
        const parsed = toURL(target);
        if (parsed.origin !== origin()) {
            return Promise.reject(new DOMException(
                "Failed to execute 'getRegistration' on 'ServiceWorkerContainer': The origin of the provided URL does not match the current origin.",
                'SecurityError',
            ));
        }
        return Promise.resolve(normalizeRegistration(ops.op_service_worker_match(origin(), parsed.href), 'active'));
    });
    method(containerProto, 'getRegistrations', function getRegistrations() {
        return Promise.resolve(
            ops.op_service_worker_list(origin()).map(snapshot => normalizeRegistration(snapshot, 'active')),
        );
    });
    method(containerProto, 'register', function register(scriptURL, options = {}) {
        if (arguments.length < 1) {
            return Promise.reject(new TypeError(
                "Failed to execute 'register' on 'ServiceWorkerContainer': 1 argument required, but only 0 present.",
            ));
        }
        let script;
        let scope;
        try {
            script = toURL(scriptURL);
            const scopeValue = options && options.scope !== undefined
                ? options.scope
                : new URL('.', script.href).href;
            scope = toURL(scopeValue);
        } catch (error) {
            return Promise.reject(error);
        }
        if (script.origin !== origin() || scope.origin !== origin()) {
            return Promise.reject(new DOMException(
                "Failed to register a ServiceWorker: The origin of the provided scriptURL or scope does not match the current origin.",
                'SecurityError',
            ));
        }
        const updateViaCache = String(options?.updateViaCache ?? 'imports');
        if (!['imports', 'all', 'none'].includes(updateViaCache)) {
            return Promise.reject(new TypeError(
                "Failed to execute 'register' on 'ServiceWorkerContainer': The provided value is not a valid enum value of type ServiceWorkerUpdateViaCache.",
            ));
        }
        const workerType = String(options?.type ?? 'classic');
        if (!['classic', 'module'].includes(workerType)) {
            return Promise.reject(new TypeError(
                "Failed to execute 'register' on 'ServiceWorkerContainer': The provided value is not a valid enum value of type WorkerType.",
            ));
        }
        const snapshot = ops.op_service_worker_register(
            origin(), scope.href, script.href, updateViaCache,
        );
        const registration = normalizeRegistration(snapshot, 'installing');
        const state = registrationState.get(registration);
        if (!state.installing && !state.active) state.installing = workerFor(snapshot, 'installing');
        setTimeout(() => {
            activateRegistration(registration);
            maybeResolveReady(this);
        }, 0);
        return Promise.resolve(registration);
    });
    method(containerProto, 'startMessages', function startMessages() {});
    ctorAndTag(containerProto, ContainerCtor, 'ServiceWorkerContainer');

    // ServiceWorkerRegistration.prototype — Chrome 148 property order.
    getter(registrationProto, 'installing', function installing() {
        return registrationState.get(this)?.installing ?? null;
    });
    getter(registrationProto, 'waiting', function waiting() {
        return registrationState.get(this)?.waiting ?? null;
    });
    getter(registrationProto, 'active', function active() {
        return registrationState.get(this)?.active ?? null;
    });
    getter(registrationProto, 'navigationPreload', function navigationPreload() {
        const state = registrationState.get(this);
        if (!state) return undefined;
        if (!state.navigationPreload) {
            const proto = globalThis.NavigationPreloadManager?.prototype || Object.prototype;
            state.navigationPreload = Object.create(proto);
        }
        return state.navigationPreload;
    });
    getter(registrationProto, 'scope', function scope() {
        return registrationState.get(this)?.snapshot?.scope || '';
    });
    getter(registrationProto, 'updateViaCache', function updateViaCache() {
        return registrationState.get(this)?.snapshot?.updateViaCache || 'imports';
    });
    accessor(registrationProto, 'onupdatefound', function onupdatefound() {
        return registrationState.get(this)?.onupdatefound ?? null;
    }, function onupdatefound(value) {
        const state = registrationState.get(this); if (state) state.onupdatefound = value;
    });
    method(registrationProto, 'unregister', function unregister() {
        const state = registrationState.get(this);
        if (!state) return Promise.resolve(false);
        return Promise.resolve(ops.op_service_worker_unregister(origin(), state.snapshot.id));
    });
    method(registrationProto, 'update', function update() {
        const state = refreshRegistration(this);
        return Promise.resolve(state ? this : undefined);
    });
    const registrationManagerGetter = (name, globalName) => {
        getter(registrationProto, name, function registrationManager() {
            const state = registrationState.get(this);
            if (!state) return undefined;
            if (!state[name]) state[name] = Object.create(globalThis[globalName]?.prototype || Object.prototype);
            return state[name];
        });
    };
    registrationManagerGetter('paymentManager', 'PaymentManager');
    Object.defineProperty(registrationProto, 'constructor', {
        value: RegistrationCtor,
        enumerable: false,
        configurable: true,
        writable: true,
    });
    registrationManagerGetter('backgroundFetch', 'BackgroundFetchManager');
    registrationManagerGetter('periodicSync', 'PeriodicSyncManager');
    registrationManagerGetter('sync', 'SyncManager');
    registrationManagerGetter('cookies', 'CookieStoreManager');
    registrationManagerGetter('pushManager', 'PushManager');
    method(registrationProto, 'getNotifications', function getNotifications() {
        return Promise.resolve([]);
    });
    method(registrationProto, 'showNotification', function showNotification(title) {
        void title;
        return Promise.resolve();
    });
    Object.defineProperty(registrationProto, Symbol.toStringTag, {
        value: 'ServiceWorkerRegistration',
        enumerable: false,
        configurable: true,
        writable: false,
    });

    // ServiceWorker.prototype — Chrome 148 property order.
    getter(workerProto, 'scriptURL', function scriptURL() {
        return workerState.get(this)?.scriptURL || '';
    });
    getter(workerProto, 'state', function state() {
        return workerState.get(this)?.state || 'redundant';
    });
    accessor(workerProto, 'onstatechange', function onstatechange() {
        return workerState.get(this)?.onstatechange ?? null;
    }, function onstatechange(value) {
        const state = workerState.get(this); if (state) state.onstatechange = value;
    });
    method(workerProto, 'postMessage', function postMessage(message) { void message; });
    ctorAndTag(workerProto, WorkerCtor, 'ServiceWorker');
    // Chrome installs `onerror` after constructor on this interface.
    accessor(workerProto, 'onerror', function onerror() {
        return workerState.get(this)?.onerror ?? null;
    }, function onerror(value) {
        const state = workerState.get(this); if (state) state.onerror = value;
    });
    // Reinsert the tag after onerror to match Chrome's observable own-key order.
    const tag = Object.getOwnPropertyDescriptor(workerProto, Symbol.toStringTag);
    delete workerProto[Symbol.toStringTag];
    Object.defineProperty(workerProto, Symbol.toStringTag, tag);

    // Existing registration state may predate this realm (another same-origin
    // Page). Resolve `ready` immediately if the current URL is already covered.
    if (containerInstance) maybeResolveReady(containerInstance);
})(globalThis);
