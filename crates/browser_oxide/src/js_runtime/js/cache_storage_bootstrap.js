((globalThis) => {
    // Functional in-memory CacheStorage/Cache implementation shared by page
    // and dedicated-worker realms. The authoritative backing store is the
    // process-wide Rust registry below; JS Request/Response objects are rebuilt
    // from snapshots in each isolate rather than shared across V8 runtimes.
    // The local Map remains only as a fallback for embedders that omit the host
    // CacheStorage ops.
    const ops = globalThis.Deno && Deno.core && Deno.core.ops;
    const secureContext = globalThis.isSecureContext === true
        || !!(ops && ops.op_is_secure_context && ops.op_is_secure_context());
    if (!secureContext) return;
    if (typeof globalThis.Request !== 'function' || typeof globalThis.Response !== 'function') return;

    const hostBacked = !!(ops
        && typeof ops.op_cache_storage_open === 'function'
        && typeof ops.op_cache_storage_has === 'function'
        && typeof ops.op_cache_storage_names === 'function'
        && typeof ops.op_cache_storage_delete === 'function'
        && typeof ops.op_cache_storage_records === 'function'
        && typeof ops.op_cache_storage_put === 'function'
        && typeof ops.op_cache_storage_delete_record === 'function');

    const storesByOrigin = new Map(); // origin -> Map(cacheName -> Map(url -> record))
    const cacheState = new WeakMap(); // Cache instance -> { origin, name }

    function _origin() {
        try {
            const value = globalThis.location && globalThis.location.origin;
            return value == null || value === '' ? 'null' : String(value);
        } catch (_) {
            return 'null';
        }
    }

    function _originStores(origin, create = false) {
        let stores = storesByOrigin.get(origin);
        if (!stores && create) {
            stores = new Map();
            storesByOrigin.set(origin, stores);
        }
        return stores || null;
    }

    function _cacheRecords(origin, name, create = false) {
        const stores = _originStores(origin, create);
        if (!stores) return null;
        let records = stores.get(name);
        if (!records && create) {
            records = new Map();
            stores.set(name, records);
        }
        return records || null;
    }

    function _absoluteUrl(input) {
        const raw = input instanceof globalThis.Request ? input.url : String(input);
        try {
            return new URL(raw, globalThis.location && globalThis.location.href || undefined).href;
        } catch (_) {
            return raw;
        }
    }

    function _request(input) {
        if (input instanceof globalThis.Request) return input;
        return new globalThis.Request(_absoluteUrl(input));
    }

    function _cloneRequest(input) {
        const req = _request(input);
        try { return req.clone(); } catch (_) {}
        return new globalThis.Request(_absoluteUrl(req), {
            method: req.method,
            headers: req.headers,
        });
    }

    function _cloneResponse(response) {
        if (!(response instanceof globalThis.Response)) {
            throw new TypeError("Failed to execute 'put' on 'Cache': parameter 2 is not of type 'Response'.");
        }
        return response.clone();
    }

    function _urlForCompare(url, ignoreSearch) {
        if (!ignoreSearch) return url;
        try {
            const parsed = new URL(url);
            parsed.search = '';
            return parsed.href;
        } catch (_) {
            return String(url).split('?')[0];
        }
    }

    function _headersEqualForVary(vary, storedRequest, candidateRequest) {
        if (!vary) return true;
        const names = String(vary).split(',').map(v => v.trim().toLowerCase()).filter(Boolean);
        if (names.includes('*')) return false;
        for (const name of names) {
            if (storedRequest.headers.get(name) !== candidateRequest.headers.get(name)) return false;
        }
        return true;
    }

    function _recordMatches(record, input, options = {}) {
        const candidate = _request(input);
        if (!options.ignoreMethod && String(candidate.method || 'GET').toUpperCase() !== 'GET') return false;
        const left = _urlForCompare(record.request.url, !!options.ignoreSearch);
        const right = _urlForCompare(_absoluteUrl(candidate), !!options.ignoreSearch);
        if (left !== right) return false;
        if (!options.ignoreVary) {
            const vary = record.response.headers.get('vary');
            if (!_headersEqualForVary(vary, record.request, candidate)) return false;
        }
        return true;
    }

    function _state(cache) {
        const state = cacheState.get(cache);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    }

    function _makeCache(origin, name) {
        const cache = Object.create(Cache.prototype);
        cacheState.set(cache, { origin, name });
        return cache;
    }

    function _recordFromSnapshot(snapshot) {
        const bytes = new Uint8Array(snapshot && snapshot.body || []);
        const request = new globalThis.Request(String(snapshot && snapshot.url || ''), {
            method: String(snapshot && snapshot.method || 'GET'),
            headers: snapshot && snapshot.requestHeaders || [],
        });
        const response = new globalThis.Response(bytes, {
            _rawBytes: bytes,
            status: Number((snapshot && snapshot.status) ?? 200),
            statusText: String((snapshot && snapshot.statusText) ?? ''),
            headers: snapshot && snapshot.responseHeaders || [],
            url: String(snapshot && snapshot.responseUrl || ''),
        });
        return { request, response };
    }

    function _recordValues(records) {
        return Array.isArray(records) ? records : records.values();
    }

    function _records(cache, create = false) {
        const state = _state(cache);
        if (hostBacked) {
            if (create) ops.op_cache_storage_open(state.origin, state.name);
            return (ops.op_cache_storage_records(state.origin, state.name) || [])
                .map(_recordFromSnapshot);
        }
        return _cacheRecords(state.origin, state.name, create);
    }

    function _validatePut(request, response) {
        const req = _request(request);
        const method = String(req.method || 'GET').toUpperCase();
        if (method !== 'GET') {
            throw new TypeError("Failed to execute 'put' on 'Cache': Request method '" + method + "' is unsupported");
        }
        const url = _absoluteUrl(req);
        let scheme = '';
        try { scheme = new URL(url).protocol; } catch (_) {}
        if (scheme !== 'http:' && scheme !== 'https:') {
            throw new TypeError("Failed to execute 'put' on 'Cache': Request scheme '" + scheme.replace(':', '') + "' is unsupported");
        }
        if (!(response instanceof globalThis.Response)) {
            throw new TypeError("Failed to execute 'put' on 'Cache': parameter 2 is not of type 'Response'.");
        }
        if (response.status === 206) {
            throw new TypeError("Failed to execute 'put' on 'Cache': Partial response (status code 206) is unsupported");
        }
        const vary = response.headers.get('vary');
        if (vary && String(vary).split(',').some(v => v.trim() === '*')) {
            throw new TypeError("Failed to execute 'put' on 'Cache': Vary header contains *");
        }
        if (response.bodyUsed) {
            throw new TypeError("Failed to execute 'put' on 'Cache': Response body is already used");
        }
        return { req, url };
    }

    function Cache() {
        throw new TypeError("Failed to construct 'Cache': Illegal constructor");
    }

    async function cacheMatch(request, options = {}) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'match' on 'Cache': 1 argument required, but only 0 present.");
        const records = _records(this, false);
        if (!records) return undefined;
        for (const record of _recordValues(records)) {
            if (_recordMatches(record, request, options || {})) return _cloneResponse(record.response);
        }
        return undefined;
    }

    async function cacheMatchAll(request = undefined, options = {}) {
        const records = _records(this, false);
        if (!records) return [];
        const out = [];
        if (arguments.length === 0 || request === undefined) {
            for (const record of _recordValues(records)) out.push(_cloneResponse(record.response));
            return out;
        }
        for (const record of _recordValues(records)) {
            if (_recordMatches(record, request, options || {})) out.push(_cloneResponse(record.response));
        }
        return out;
    }

    async function cachePut(request, response) {
        if (arguments.length < 2) throw new TypeError("Failed to execute 'put' on 'Cache': 2 arguments required");
        const { req, url } = _validatePut(request, response);
        if (hostBacked) {
            const state = _state(this);
            const responseClone = _cloneResponse(response);
            const body = new Uint8Array(await responseClone.arrayBuffer());
            ops.op_cache_storage_put(
                state.origin,
                state.name,
                url,
                String(req.method || 'GET'),
                Array.from(req.headers.entries()),
                Number(responseClone.status),
                String(responseClone.statusText || ''),
                String(responseClone.url || ''),
                Array.from(responseClone.headers.entries()),
                body,
            );
            return;
        }
        const records = _records(this, true);
        records.set(url, {
            request: _cloneRequest(req),
            response: _cloneResponse(response),
        });
    }

    async function cacheAdd(request) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'add' on 'Cache': 1 argument required, but only 0 present.");
        const req = _request(request);
        if (String(req.method || 'GET').toUpperCase() !== 'GET') {
            throw new TypeError("Failed to execute 'add' on 'Cache': Request method must be GET");
        }
        const response = await globalThis.fetch(req);
        if (!response.ok) throw new TypeError("Failed to execute 'add' on 'Cache': Request failed");
        await cachePut.call(this, req, response);
    }

    async function cacheAddAll(requests) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'addAll' on 'Cache': 1 argument required, but only 0 present.");
        const list = Array.from(requests || [], value => _request(value));
        for (const req of list) {
            if (String(req.method || 'GET').toUpperCase() !== 'GET') {
                throw new TypeError("Failed to execute 'addAll' on 'Cache': Request method must be GET");
            }
        }
        // Fetch and validate every entry before writing any record so a failed
        // member does not leave a partially-populated cache.
        const responses = await Promise.all(list.map(req => globalThis.fetch(req)));
        for (let i = 0; i < responses.length; i++) {
            if (!responses[i].ok) throw new TypeError("Failed to execute 'addAll' on 'Cache': Request failed");
            _validatePut(list[i], responses[i]);
        }
        for (let i = 0; i < responses.length; i++) await cachePut.call(this, list[i], responses[i]);
    }

    async function cacheDelete(request, options = {}) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'delete' on 'Cache': 1 argument required, but only 0 present.");
        const records = _records(this, false);
        if (!records) return false;
        if (hostBacked) {
            const state = _state(this);
            for (const record of _recordValues(records)) {
                if (_recordMatches(record, request, options || {})) {
                    return !!ops.op_cache_storage_delete_record(
                        state.origin,
                        state.name,
                        record.request.url,
                    );
                }
            }
            return false;
        }
        for (const [key, record] of records) {
            if (_recordMatches(record, request, options || {})) {
                records.delete(key);
                return true;
            }
        }
        return false;
    }

    async function cacheKeys(request = undefined, options = {}) {
        const records = _records(this, false);
        if (!records) return [];
        const out = [];
        if (arguments.length === 0 || request === undefined) {
            for (const record of _recordValues(records)) out.push(_cloneRequest(record.request));
            return out;
        }
        for (const record of _recordValues(records)) {
            if (_recordMatches(record, request, options || {})) out.push(_cloneRequest(record.request));
        }
        return out;
    }

    function CacheStorage() {
        throw new TypeError("Failed to construct 'CacheStorage': Illegal constructor");
    }

    async function storageMatch(request, options = {}) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'match' on 'CacheStorage': 1 argument required, but only 0 present.");
        const origin = _origin();
        if (hostBacked) {
            const names = ops.op_cache_storage_names(origin) || [];
            if (options && options.cacheName !== undefined) {
                const name = String(options.cacheName);
                if (!names.includes(name)) return undefined;
                return cacheMatch.call(_makeCache(origin, name), request, options);
            }
            for (const name of names) {
                const found = await cacheMatch.call(_makeCache(origin, name), request, options || {});
                if (found !== undefined) return found;
            }
            return undefined;
        }
        const stores = _originStores(origin, false);
        if (!stores) return undefined;
        if (options && options.cacheName !== undefined) {
            const name = String(options.cacheName);
            if (!stores.has(name)) return undefined;
            return cacheMatch.call(_makeCache(origin, name), request, options);
        }
        for (const name of stores.keys()) {
            const found = await cacheMatch.call(_makeCache(origin, name), request, options || {});
            if (found !== undefined) return found;
        }
        return undefined;
    }

    async function storageHas(cacheName) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'has' on 'CacheStorage': 1 argument required, but only 0 present.");
        const origin = _origin();
        if (hostBacked) return !!ops.op_cache_storage_has(origin, String(cacheName));
        return !!_originStores(origin, false)?.has(String(cacheName));
    }

    async function storageOpen(cacheName) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'open' on 'CacheStorage': 1 argument required, but only 0 present.");
        const origin = _origin();
        const name = String(cacheName);
        if (hostBacked) {
            ops.op_cache_storage_open(origin, name);
            return _makeCache(origin, name);
        }
        _cacheRecords(origin, name, true);
        return _makeCache(origin, name);
    }

    async function storageDelete(cacheName) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'delete' on 'CacheStorage': 1 argument required, but only 0 present.");
        const origin = _origin();
        if (hostBacked) return !!ops.op_cache_storage_delete(origin, String(cacheName));
        const stores = _originStores(origin, false);
        if (!stores) return false;
        const deleted = stores.delete(String(cacheName));
        if (stores.size === 0) storesByOrigin.delete(origin);
        return deleted;
    }

    async function storageKeys() {
        const origin = _origin();
        if (hostBacked) return ops.op_cache_storage_names(origin) || [];
        const stores = _originStores(origin, false);
        return stores ? Array.from(stores.keys()) : [];
    }

    function _method(proto, name, value) {
        try { Object.defineProperty(value, 'name', { value: name, configurable: true }); } catch (_) {}
        Object.defineProperty(proto, name, {
            value, writable: true, configurable: true, enumerable: true,
        });
    }

    Object.defineProperty(Cache, 'prototype', { writable: false });
    try { delete Cache.prototype.constructor; } catch (_) {}
    // Blink/WebIDL insertion order is observable through Reflect.ownKeys().
    _method(Cache.prototype, 'add', cacheAdd);
    _method(Cache.prototype, 'addAll', cacheAddAll);
    _method(Cache.prototype, 'delete', cacheDelete);
    _method(Cache.prototype, 'keys', cacheKeys);
    _method(Cache.prototype, 'match', cacheMatch);
    _method(Cache.prototype, 'matchAll', cacheMatchAll);
    _method(Cache.prototype, 'put', cachePut);
    Object.defineProperty(Cache.prototype, 'constructor', {
        value: Cache, writable: true, configurable: true, enumerable: false,
    });
    Object.defineProperty(Cache.prototype, Symbol.toStringTag, {
        value: 'Cache', configurable: true,
    });

    Object.defineProperty(CacheStorage, 'prototype', { writable: false });
    try { delete CacheStorage.prototype.constructor; } catch (_) {}
    _method(CacheStorage.prototype, 'delete', storageDelete);
    _method(CacheStorage.prototype, 'has', storageHas);
    _method(CacheStorage.prototype, 'keys', storageKeys);
    _method(CacheStorage.prototype, 'match', storageMatch);
    _method(CacheStorage.prototype, 'open', storageOpen);
    Object.defineProperty(CacheStorage.prototype, 'constructor', {
        value: CacheStorage, writable: true, configurable: true, enumerable: false,
    });
    Object.defineProperty(CacheStorage.prototype, Symbol.toStringTag, {
        value: 'CacheStorage', configurable: true,
    });

    const cacheStorage = Object.create(CacheStorage.prototype);
    Object.defineProperty(globalThis, 'Cache', {
        value: Cache, writable: true, configurable: true, enumerable: false,
    });
    Object.defineProperty(globalThis, 'CacheStorage', {
        value: CacheStorage, writable: true, configurable: true, enumerable: false,
    });
    Object.defineProperty(globalThis, 'caches', {
        value: cacheStorage, writable: false, configurable: true, enumerable: true,
    });

    try {
        if (typeof globalThis._maskFunction === 'function') {
            globalThis._maskFunction(Cache, 'Cache');
            globalThis._maskFunction(CacheStorage, 'CacheStorage');
        }
        if (typeof globalThis._maskAsNative === 'function') {
            globalThis._maskAsNative(Cache.prototype,
                'match', 'matchAll', 'add', 'addAll', 'put', 'delete', 'keys');
            globalThis._maskAsNative(CacheStorage.prototype,
                'match', 'has', 'open', 'delete', 'keys');
        }
    } catch (_) {}
})(globalThis);
