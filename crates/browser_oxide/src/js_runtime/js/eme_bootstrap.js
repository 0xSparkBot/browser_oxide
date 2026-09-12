// Encrypted Media Extensions WebIDL/state layer.
//
// The capability/support policy is still owned by window_bootstrap.js. This
// layer converts its capability result into Chromium-shaped EME objects while
// intentionally leaving real CDM/license exchange out of scope.
((globalThis) => {
    if (!globalThis.navigator || !globalThis.Navigator) return;
    const navProto = globalThis.Navigator.prototype;
    const legacyRequest = navProto.requestMediaKeySystemAccess;
    if (typeof legacyRequest !== 'function') return;

    const Access = globalThis.MediaKeySystemAccess;
    const Keys = globalThis.MediaKeys;
    const Session = globalThis.MediaKeySession;
    const StatusMap = globalThis.MediaKeyStatusMap;
    if (![Access, Keys, Session, StatusMap].every(value => typeof value === 'function')) return;

    const accessState = new WeakMap();
    const keysState = new WeakMap();
    const sessionState = new WeakMap();
    const statusMapState = new WeakMap();
    const handlerState = new WeakMap();

    const requireState = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const cloneValue = value => {
        if (value == null || typeof value !== 'object') return value;
        if (Array.isArray(value)) return value.map(cloneValue);
        const out = {};
        for (const [key, entry] of Object.entries(value)) out[key] = cloneValue(entry);
        return out;
    };
    const normalizeCapability = capability => ({
        contentType: String(capability?.contentType ?? ''),
        encryptionScheme: capability?.encryptionScheme ?? null,
        robustness: String(capability?.robustness ?? ''),
    });
    const normalizeConfiguration = (input, keySystem) => {
        const config = input && typeof input === 'object' ? input : {};
        const clearKey = keySystem === 'org.w3.clearkey';
        return {
            audioCapabilities: Array.isArray(config.audioCapabilities)
                ? config.audioCapabilities.map(normalizeCapability) : [],
            distinctiveIdentifier: String(config.distinctiveIdentifier ?? (clearKey ? 'not-allowed' : 'optional')),
            initDataTypes: Array.isArray(config.initDataTypes)
                ? config.initDataTypes.map(String) : [],
            label: String(config.label ?? ''),
            persistentState: String(config.persistentState ?? (clearKey ? 'not-allowed' : 'optional')),
            sessionTypes: Array.isArray(config.sessionTypes) && config.sessionTypes.length
                ? config.sessionTypes.map(String) : ['temporary'],
            videoCapabilities: Array.isArray(config.videoCapabilities)
                ? config.videoCapabilities.map(normalizeCapability) : [],
        };
    };
    const bytes = (value, operation, argumentName) => {
        if (value instanceof ArrayBuffer) return new Uint8Array(value);
        if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
        throw new TypeError(`Failed to execute '${operation}' on '${argumentName}': parameter is not of type 'ArrayBuffer or ArrayBufferView'.`);
    };
    const named = (fn, name, length) => {
        try { Object.defineProperty(fn, 'name', { value: name, configurable: true }); } catch (_) {}
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        return fn;
    };
    const method = (proto, name, length, fn) => {
        Object.defineProperty(proto, name, {
            value: named(fn, name, length), writable: true, enumerable: true, configurable: true,
        });
    };
    const getter = (proto, name, fn) => {
        const get = named(fn, `get ${name}`, 0);
        Object.defineProperty(proto, name, { get, enumerable: true, configurable: true });
    };
    const handler = (proto, name) => {
        getter(proto, name, function() {
            requireState(sessionState, this);
            const state = handlerState.get(this);
            return state?.[name] ?? null;
        });
        const descriptor = Object.getOwnPropertyDescriptor(proto, name);
        const set = named(function(value) {
            requireState(sessionState, this);
            let state = handlerState.get(this);
            if (!state) { state = Object.create(null); handlerState.set(this, state); }
            state[name] = typeof value === 'function' ? value : null;
        }, `set ${name}`, 1);
        Object.defineProperty(proto, name, {
            get: descriptor.get, set, enumerable: true, configurable: true,
        });
    };
    const resetNamedProperties = (proto, names) => {
        const constructor = Object.getOwnPropertyDescriptor(proto, 'constructor');
        for (const name of Object.getOwnPropertyNames(proto)) {
            if (name === 'constructor') continue;
            const d = Object.getOwnPropertyDescriptor(proto, name);
            if (d?.configurable) { try { delete proto[name]; } catch (_) {} }
        }
        try { delete proto.constructor; } catch (_) {}
        return () => Object.defineProperty(proto, 'constructor', constructor || {
            value: proto.constructor, writable: true, enumerable: false, configurable: true,
        });
    };

    // MediaKeySystemAccess
    let restoreConstructor = resetNamedProperties(Access.prototype);
    getter(Access.prototype, 'keySystem', function() { return requireState(accessState, this).keySystem; });
    method(Access.prototype, 'createMediaKeys', 0, function() {
        const access = requireState(accessState, this);
        return Promise.resolve(makeKeys(access));
    });
    method(Access.prototype, 'getConfiguration', 0, function() {
        return cloneValue(requireState(accessState, this).configuration);
    });
    restoreConstructor();

    // MediaKeys
    restoreConstructor = resetNamedProperties(Keys.prototype);
    method(Keys.prototype, 'createSession', 0, function(sessionType = 'temporary') {
        const state = requireState(keysState, this);
        const type = String(sessionType);
        if (type !== 'temporary' && type !== 'persistent-license') {
            throw new TypeError(
                "Failed to execute 'createSession' on 'MediaKeys': The provided value '" +
                type + "' is not a valid enum value of type MediaKeySessionType."
            );
        }
        if (!state.configuration.sessionTypes.includes(type)) {
            throw new DOMException('The requested session type is not supported by this configuration.', 'NotSupportedError');
        }
        return makeSession(type);
    });
    method(Keys.prototype, 'setServerCertificate', 1, function(serverCertificate) {
        requireState(keysState, this);
        if (arguments.length < 1) {
            return Promise.reject(new TypeError(
                "Failed to execute 'setServerCertificate' on 'MediaKeys': 1 argument required, but only 0 present."
            ));
        }
        let data;
        try { data = bytes(serverCertificate, 'setServerCertificate', 'MediaKeys'); }
        catch (error) { return Promise.reject(error); }
        if (data.byteLength === 0) {
            return Promise.reject(new TypeError(
                "Failed to execute 'setServerCertificate' on 'MediaKeys': The serverCertificate parameter is empty."
            ));
        }
        // No CDM is attached; ClearKey does not consume a server certificate.
        return Promise.resolve(false);
    });
    restoreConstructor();
    method(Keys.prototype, 'getStatusForPolicy', 0, function(_policy = {}) {
        requireState(keysState, this);
        return Promise.resolve('usable');
    });

    // MediaKeyStatusMap
    restoreConstructor = resetNamedProperties(StatusMap.prototype);
    getter(StatusMap.prototype, 'size', function() { return requireState(statusMapState, this).size; });
    method(StatusMap.prototype, 'get', 1, function(key) { return requireState(statusMapState, this).get(key); });
    method(StatusMap.prototype, 'has', 1, function(key) { return requireState(statusMapState, this).has(key); });
    method(StatusMap.prototype, 'entries', 0, function() { return requireState(statusMapState, this).entries(); });
    method(StatusMap.prototype, 'forEach', 1, function(callback) {
        const map = requireState(statusMapState, this);
        if (typeof callback !== 'function') throw new TypeError('Callback must be a function');
        const thisArg = arguments.length > 1 ? arguments[1] : undefined;
        map.forEach((value, key) => callback.call(thisArg, value, key, this));
    });
    method(StatusMap.prototype, 'keys', 0, function() { return requireState(statusMapState, this).keys(); });
    method(StatusMap.prototype, 'values', 0, function() { return requireState(statusMapState, this).values(); });
    restoreConstructor();
    Object.defineProperty(StatusMap.prototype, Symbol.iterator, {
        value: StatusMap.prototype.entries, writable: true, enumerable: false, configurable: true,
    });

    // MediaKeySession is an EventTarget.
    try { Object.setPrototypeOf(Session.prototype, globalThis.EventTarget.prototype); } catch (_) {}
    try { Object.setPrototypeOf(Session, globalThis.EventTarget); } catch (_) {}
    restoreConstructor = resetNamedProperties(Session.prototype);
    getter(Session.prototype, 'sessionId', function() { return requireState(sessionState, this).sessionId; });
    getter(Session.prototype, 'expiration', function() { return requireState(sessionState, this).expiration; });
    getter(Session.prototype, 'closed', function() { return requireState(sessionState, this).closed; });
    getter(Session.prototype, 'keyStatuses', function() { return requireState(sessionState, this).keyStatuses; });
    handler(Session.prototype, 'onkeystatuseschange');
    handler(Session.prototype, 'onmessage');
    method(Session.prototype, 'close', 0, function() {
        const state = requireState(sessionState, this);
        if (!state.closedSettled) {
            state.closedSettled = true;
            state.resolveClosed();
        }
        return Promise.resolve();
    });
    method(Session.prototype, 'generateRequest', 2, function(initDataType, initData) {
        requireState(sessionState, this);
        if (arguments.length < 2) {
            return Promise.reject(new TypeError(
                "Failed to execute 'generateRequest' on 'MediaKeySession': 2 arguments required, but only " +
                arguments.length + ' present.'
            ));
        }
        const type = String(initDataType);
        if (!['cenc', 'keyids', 'webm'].includes(type)) {
            return Promise.reject(new DOMException(
                `The initialization data type '${type}' is not supported.`, 'NotSupportedError'
            ));
        }
        try { bytes(initData, 'generateRequest', 'MediaKeySession'); }
        catch (error) { return Promise.reject(error); }
        // No CDM/license exchange is synthesized. The operation is accepted so
        // capability/state-machine code can proceed without fake key material.
        return Promise.resolve();
    });
    method(Session.prototype, 'load', 1, function(sessionId) {
        requireState(sessionState, this);
        if (arguments.length < 1) {
            return Promise.reject(new TypeError(
                "Failed to execute 'load' on 'MediaKeySession': 1 argument required, but only 0 present."
            ));
        }
        if (String(sessionId).length === 0) return Promise.resolve(false);
        return Promise.resolve(false);
    });
    method(Session.prototype, 'remove', 0, function() {
        requireState(sessionState, this);
        return Promise.resolve();
    });
    method(Session.prototype, 'update', 1, function(response) {
        requireState(sessionState, this);
        if (arguments.length < 1) {
            return Promise.reject(new TypeError(
                "Failed to execute 'update' on 'MediaKeySession': 1 argument required, but only 0 present."
            ));
        }
        try { bytes(response, 'update', 'MediaKeySession'); }
        catch (error) { return Promise.reject(error); }
        return Promise.resolve();
    });
    restoreConstructor();

    function makeStatusMap() {
        const value = Object.create(StatusMap.prototype);
        statusMapState.set(value, new Map());
        return value;
    }
    function makeSession(sessionType) {
        const value = Object.create(Session.prototype);
        let resolveClosed;
        const closed = new Promise(resolve => { resolveClosed = resolve; });
        sessionState.set(value, {
            sessionType,
            sessionId: '',
            expiration: NaN,
            closed,
            resolveClosed,
            closedSettled: false,
            keyStatuses: makeStatusMap(),
        });
        handlerState.set(value, Object.create(null));
        return value;
    }
    function makeKeys(access) {
        const value = Object.create(Keys.prototype);
        keysState.set(value, {
            keySystem: access.keySystem,
            configuration: cloneValue(access.configuration),
        });
        return value;
    }
    function makeAccess(keySystem, configuration) {
        const value = Object.create(Access.prototype);
        accessState.set(value, {
            keySystem,
            configuration: normalizeConfiguration(configuration, keySystem),
        });
        return value;
    }

    const request = named(function requestMediaKeySystemAccess(keySystem, supportedConfigurations) {
        // Delegate support policy and its rejection behavior to the existing
        // implementation, then replace only the returned object model.
        let result;
        try { result = legacyRequest.call(this, keySystem, supportedConfigurations); }
        catch (error) { return Promise.reject(error); }
        return Promise.resolve(result).then(access => {
            const system = String(access?.keySystem ?? keySystem);
            let config = {};
            try { config = access?.getConfiguration?.() ?? supportedConfigurations?.[0] ?? {}; }
            catch (_) { config = supportedConfigurations?.[0] ?? {}; }
            return makeAccess(system, config);
        });
    }, 'requestMediaKeySystemAccess', 2);
    Object.defineProperty(navProto, 'requestMediaKeySystemAccess', {
        value: request, writable: true, enumerable: true, configurable: true,
    });
})(globalThis);
