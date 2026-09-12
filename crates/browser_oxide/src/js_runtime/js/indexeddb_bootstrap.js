// IndexedDB in-memory browser semantics.
//
// The storage backend is intentionally process-local for now, but the public
// object model follows Blink closely: browser-created objects carry no JS
// implementation fields, requests/databases/transactions inherit EventTarget,
// constructors that are browser-only are illegal, and transactions remain
// active until all requests (including requests queued from callbacks) settle.
((globalThis) => {
    'use strict';

    if (typeof globalThis.Event !== 'function' ||
        typeof globalThis.EventTarget !== 'function') return;

    const _markTrusted =
        (globalThis.__browser_oxide && globalThis.__browser_oxide._markTrustedEvent) ||
        globalThis.__bo_mark_trusted || ((event) => event);

    const _dispatchEventToParentTarget =
        globalThis.__browser_oxide &&
        typeof globalThis.__browser_oxide._dispatchEventToParentTarget === 'function'
            ? globalThis.__browser_oxide._dispatchEventToParentTarget
            : null;

    const _requestState = new WeakMap();
    const _dbState = new WeakMap();
    const _txState = new WeakMap();
    const _storeState = new WeakMap();
    const _rangeState = new WeakMap();
    const _cursorState = new WeakMap();
    const _indexState = new WeakMap();
    const _versionEventState = new WeakMap();
    const _nameListState = new WeakMap();
    const _dbRegistry = new Map();

    const _native = (fn, name) => {
        try { Object.defineProperty(fn, 'name', { value: name, configurable: true }); } catch (_) {}
        if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        else if (typeof _maskFunction === 'function') _maskFunction(fn, name);
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
    const _stateFor = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const _defineGetter = (proto, name, read) => {
        Object.defineProperty(proto, name, {
            enumerable: true, configurable: true,
            get: _getter(name, read),
        });
    };
    const _defineHandler = (proto, name, map) => {
        Object.defineProperty(proto, name, {
            enumerable: true, configurable: true,
            get: _getter(name, value => _stateFor(map, value).handlers[name] || null),
            set: _setter(name, (value, next) => {
                _stateFor(map, value).handlers[name] =
                    (typeof next === 'function' || (next && typeof next.handleEvent === 'function'))
                        ? next : null;
            }),
        });
    };
    const _defineMethod = (proto, name, fn) => {
        Object.defineProperty(proto, name, {
            enumerable: true, configurable: true, writable: true,
            value: _native(fn, name),
        });
    };
    const _reorderProto = (proto, names) => {
        const descriptors = names.map(name => [name, Object.getOwnPropertyDescriptor(proto, name)]);
        for (const [name, descriptor] of descriptors) {
            if (descriptor && descriptor.configurable) delete proto[name];
        }
        for (const [name, descriptor] of descriptors) {
            if (descriptor) Object.defineProperty(proto, name, descriptor);
        }
    };
    const _finishProto = (Ctor, proto, tag) => {
        Object.defineProperty(proto, 'constructor', {
            value: Ctor, writable: true, enumerable: false, configurable: true,
        });
        Object.defineProperty(proto, Symbol.toStringTag, {
            value: tag, writable: false, enumerable: false, configurable: true,
        });
        Ctor.prototype = proto;
    };
    const _illegal = (name, parentCtor, parentProto) => {
        const Ctor = ({ [name]: function () {
            throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
        } })[name];
        _native(Ctor, name);
        if (parentCtor) Object.setPrototypeOf(Ctor, parentCtor);
        const proto = Object.create(parentProto || Object.prototype);
        return { Ctor, proto };
    };
    const _install = (name, value) => {
        Object.defineProperty(globalThis, name, {
            value, writable: true, configurable: true, enumerable: false,
        });
    };
    const _domError = (name, message) => {
        try { return new DOMException(message || name, name); }
        catch (_) { const e = new Error(message || name); e.name = name; return e; }
    };
    const _clone = (value) => {
        if (typeof globalThis.structuredClone === 'function') {
            try { return globalThis.structuredClone(value); } catch (_) {}
        }
        try { return JSON.parse(JSON.stringify(value)); } catch (_) { return value; }
    };
    const _emit = (target, event) => target.dispatchEvent(_markTrusted(event));

    // ---------------------------------------------------------------------
    // DOMStringList (used by objectStoreNames / indexNames).
    // ---------------------------------------------------------------------
    const dsl = _illegal('DOMStringList', null, Object.prototype);
    const DOMStringList = dsl.Ctor;
    const DOMStringListProto = dsl.proto;
    _defineGetter(DOMStringListProto, 'length', value => _stateFor(_nameListState, value).names().length);
    _defineMethod(DOMStringListProto, 'contains', function contains(name) {
        return _stateFor(_nameListState, this).names().includes(String(name));
    });
    _defineMethod(DOMStringListProto, 'item', function item(index) {
        const values = _stateFor(_nameListState, this).names();
        const i = Number(index) >>> 0;
        return i < values.length ? values[i] : null;
    });
    const _dslValues = _native(function values() {
        const list = this;
        let i = 0;
        return {
            next() {
                const values = _stateFor(_nameListState, list).names();
                if (i >= values.length) return { value: undefined, done: true };
                return { value: values[i++], done: false };
            },
            [Symbol.iterator]() { return this; },
        };
    }, 'values');
    Object.defineProperty(DOMStringListProto, Symbol.iterator, {
        value: _dslValues, writable: true, enumerable: false, configurable: true,
    });
    _finishProto(DOMStringList, DOMStringListProto, 'DOMStringList');
    _reorderProto(DOMStringListProto, [
        'length', 'contains', 'item', 'constructor', Symbol.toStringTag, Symbol.iterator,
    ]);

    function _makeNameList(getNames) {
        const target = Object.create(DOMStringListProto);
        const handler = {
            ownKeys() {
                return getNames().map((_, i) => String(i));
            },
            getOwnPropertyDescriptor(_target, prop) {
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    const values = getNames();
                    const i = Number(prop);
                    if (i < values.length) {
                        return { value: values[i], writable: false, enumerable: true, configurable: true };
                    }
                }
                return undefined;
            },
            has(target, prop) {
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    return Number(prop) < getNames().length;
                }
                return Reflect.has(target, prop);
            },
            get(target, prop, receiver) {
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    const values = getNames();
                    const i = Number(prop);
                    return i < values.length ? values[i] : undefined;
                }
                return Reflect.get(target, prop, receiver);
            },
        };
        const proxy = new Proxy(target, handler);
        const state = { names: () => getNames().map(String).sort() };
        _nameListState.set(target, state);
        _nameListState.set(proxy, state);
        return proxy;
    }

    // ---------------------------------------------------------------------
    // Public interface constructors / prototype graph.
    // ---------------------------------------------------------------------
    const reqI = _illegal('IDBRequest', globalThis.EventTarget, globalThis.EventTarget.prototype);
    const IDBRequest = reqI.Ctor, IDBRequestProto = reqI.proto;
    const openI = _illegal('IDBOpenDBRequest', IDBRequest, IDBRequestProto);
    const IDBOpenDBRequest = openI.Ctor, IDBOpenDBRequestProto = openI.proto;
    const dbI = _illegal('IDBDatabase', globalThis.EventTarget, globalThis.EventTarget.prototype);
    const IDBDatabase = dbI.Ctor, IDBDatabaseProto = dbI.proto;
    const txI = _illegal('IDBTransaction', globalThis.EventTarget, globalThis.EventTarget.prototype);
    const IDBTransaction = txI.Ctor, IDBTransactionProto = txI.proto;
    const storeI = _illegal('IDBObjectStore', null, Object.prototype);
    const IDBObjectStore = storeI.Ctor, IDBObjectStoreProto = storeI.proto;
    const factoryI = _illegal('IDBFactory', null, Object.prototype);
    const IDBFactory = factoryI.Ctor, IDBFactoryProto = factoryI.proto;
    const rangeI = _illegal('IDBKeyRange', null, Object.prototype);
    const IDBKeyRange = rangeI.Ctor, IDBKeyRangeProto = rangeI.proto;
    const cursorI = _illegal('IDBCursor', null, Object.prototype);
    const IDBCursor = cursorI.Ctor, IDBCursorProto = cursorI.proto;
    const cursorValueI = _illegal('IDBCursorWithValue', IDBCursor, IDBCursorProto);
    const IDBCursorWithValue = cursorValueI.Ctor, IDBCursorWithValueProto = cursorValueI.proto;
    const indexI = _illegal('IDBIndex', null, Object.prototype);
    const IDBIndex = indexI.Ctor, IDBIndexProto = indexI.proto;

    // IDBVersionChangeEvent is the one constructible interface in this set.
    class IDBVersionChangeEvent extends globalThis.Event {
        constructor(type, init = {}) {
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'IDBVersionChangeEvent': 1 argument required, but only 0 present.");
            }
            super(type, init || {});
            const i = init || {};
            _versionEventState.set(this, {
                oldVersion: Number(i.oldVersion || 0),
                newVersion: i.newVersion == null ? null : Number(i.newVersion),
                dataLoss: String(i.dataLoss || 'none'),
                dataLossMessage: String(i.dataLossMessage || ''),
            });
        }
    }
    _native(IDBVersionChangeEvent, 'IDBVersionChangeEvent');
    for (const name of ['oldVersion', 'newVersion', 'dataLoss', 'dataLossMessage']) {
        _defineGetter(IDBVersionChangeEvent.prototype, name,
            value => _stateFor(_versionEventState, value)[name]);
    }
    const _vceCtor = Object.getOwnPropertyDescriptor(IDBVersionChangeEvent.prototype, 'constructor');
    delete IDBVersionChangeEvent.prototype.constructor;
    Object.defineProperty(IDBVersionChangeEvent.prototype, 'constructor', _vceCtor);
    Object.defineProperty(IDBVersionChangeEvent.prototype, Symbol.toStringTag, {
        value: 'IDBVersionChangeEvent', writable: false, enumerable: false, configurable: true,
    });

    // ---------------------------------------------------------------------
    // Request state / event dispatch.
    // ---------------------------------------------------------------------
    function _makeRequest(source, transaction, open = false) {
        const proto = open ? IDBOpenDBRequestProto : IDBRequestProto;
        const request = Object.create(proto);
        _requestState.set(request, {
            result: undefined,
            error: null,
            source: source || null,
            transaction: transaction || null,
            readyState: 'pending',
            handlers: Object.create(null),
        });
        return request;
    }
    function _setRequestDone(request, result, error = null) {
        const state = _stateFor(_requestState, request);
        state.result = result;
        state.error = error;
        state.readyState = 'done';
    }
    function _queueRequestSuccess(request, result) {
        const state = _stateFor(_requestState, request);
        const tx = state.transaction;
        if (tx) {
            _txBegin(tx);
            _stateFor(_txState, tx).requests.add(request);
        }
        queueMicrotask(() => {
            if (tx) {
                const txState = _stateFor(_txState, tx);
                if (txState.aborted || state.readyState !== 'pending') return;
                txState.requests.delete(request);
            }
            _setRequestDone(request, result, null);
            _emit(request, new Event('success'));
            if (tx) _txEnd(tx);
        });
        return request;
    }
    function _queueRequestError(request, error) {
        const state = _stateFor(_requestState, request);
        const tx = state.transaction;
        if (tx) {
            _txBegin(tx);
            _stateFor(_txState, tx).requests.add(request);
        }
        queueMicrotask(() => {
            if (tx) {
                const txState = _stateFor(_txState, tx);
                if (txState.aborted || state.readyState !== 'pending') return;
                txState.requests.delete(request);
            }
            _setRequestDone(request, undefined, error);
            const event = new Event('error', { bubbles: true, cancelable: true });
            const requestUncanceled = _emit(request, event);
            if (tx) {
                // IDB error events bubble from request to transaction as the
                // very same Event object. The event layer exposes a private
                // parent-target hop so target/currentTarget/eventPhase and
                // preventDefault() match Blink without teaching the general
                // DOM tree walker about IndexedDB ownership.
                const transactionUncanceled = _dispatchEventToParentTarget
                    ? _dispatchEventToParentTarget(event, tx)
                    : _emit(tx, new Event('error', { bubbles: true, cancelable: true }));
                if (requestUncanceled && transactionUncanceled) {
                    _abortTransaction(tx, error);
                } else {
                    _txEnd(tx);
                }
            }
        });
        return request;
    }
    for (const name of ['result', 'error']) {
        _defineGetter(IDBRequestProto, name, value => {
            const state = _stateFor(_requestState, value);
            if (state.readyState !== 'done') {
                throw _domError('InvalidStateError', 'The request has not finished.');
            }
            return state[name];
        });
    }
    for (const name of ['source', 'transaction', 'readyState']) {
        _defineGetter(IDBRequestProto, name, value => _stateFor(_requestState, value)[name]);
    }
    _defineHandler(IDBRequestProto, 'onsuccess', _requestState);
    _defineHandler(IDBRequestProto, 'onerror', _requestState);
    _finishProto(IDBRequest, IDBRequestProto, 'IDBRequest');
    _defineHandler(IDBOpenDBRequestProto, 'onblocked', _requestState);
    _defineHandler(IDBOpenDBRequestProto, 'onupgradeneeded', _requestState);
    _finishProto(IDBOpenDBRequest, IDBOpenDBRequestProto, 'IDBOpenDBRequest');

    // ---------------------------------------------------------------------
    // Key ranges and key ordering.
    // ---------------------------------------------------------------------
    function _rankKey(value) {
        if (typeof value === 'number') return 0;
        if (value instanceof Date) return 1;
        if (typeof value === 'string') return 2;
        if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) return 3;
        if (Array.isArray(value)) return 4;
        return 99;
    }
    function _keyCmp(a, b) {
        const ra = _rankKey(a), rb = _rankKey(b);
        if (ra !== rb) return ra < rb ? -1 : 1;
        if (ra === 0) return a === b ? 0 : (a < b ? -1 : 1);
        if (ra === 1) return a.getTime() === b.getTime() ? 0 : (a < b ? -1 : 1);
        if (ra === 2) return a === b ? 0 : (a < b ? -1 : 1);
        if (ra === 4) {
            const n = Math.min(a.length, b.length);
            for (let i = 0; i < n; i++) {
                const c = _keyCmp(a[i], b[i]);
                if (c) return c;
            }
            return a.length === b.length ? 0 : (a.length < b.length ? -1 : 1);
        }
        const aa = ra === 3 ? new Uint8Array(a.buffer || a, a.byteOffset || 0, a.byteLength) : null;
        const bb = rb === 3 ? new Uint8Array(b.buffer || b, b.byteOffset || 0, b.byteLength) : null;
        if (aa && bb) {
            const n = Math.min(aa.length, bb.length);
            for (let i = 0; i < n; i++) if (aa[i] !== bb[i]) return aa[i] < bb[i] ? -1 : 1;
            return aa.length === bb.length ? 0 : (aa.length < bb.length ? -1 : 1);
        }
        if (Object.is(a, b)) return 0;
        throw _domError('DataError', 'The parameter is not a valid key.');
    }
    function _makeRange(lower, upper, lowerOpen, upperOpen) {
        const range = Object.create(IDBKeyRangeProto);
        _rangeState.set(range, { lower, upper, lowerOpen: !!lowerOpen, upperOpen: !!upperOpen });
        return range;
    }
    for (const name of ['lower', 'upper', 'lowerOpen', 'upperOpen']) {
        _defineGetter(IDBKeyRangeProto, name, value => _stateFor(_rangeState, value)[name]);
    }
    _defineMethod(IDBKeyRangeProto, 'includes', function includes(key) {
        const state = _stateFor(_rangeState, this);
        if (state.lower !== undefined) {
            const c = _keyCmp(key, state.lower);
            if (c < 0 || (c === 0 && state.lowerOpen)) return false;
        }
        if (state.upper !== undefined) {
            const c = _keyCmp(key, state.upper);
            if (c > 0 || (c === 0 && state.upperOpen)) return false;
        }
        return true;
    });
    _finishProto(IDBKeyRange, IDBKeyRangeProto, 'IDBKeyRange');
    const _rangeStatic = (name, fn) => Object.defineProperty(IDBKeyRange, name, {
        value: _native(fn, name), writable: true, enumerable: true, configurable: true,
    });
    _rangeStatic('bound', function bound(lower, upper, ...rest) {
        if (_keyCmp(lower, upper) > 0 || (_keyCmp(lower, upper) === 0 && (rest[0] || rest[1]))) {
            throw _domError('DataError', 'The lower bound is greater than the upper bound.');
        }
        return _makeRange(lower, upper, !!rest[0], !!rest[1]);
    });
    _rangeStatic('lowerBound', function lowerBound(lower, ...rest) {
        return _makeRange(lower, undefined, !!rest[0], false);
    });
    _rangeStatic('only', function only(value) { return _makeRange(value, value, false, false); });
    _rangeStatic('upperBound', function upperBound(upper, ...rest) {
        return _makeRange(undefined, upper, false, !!rest[0]);
    });

    // ---------------------------------------------------------------------
    // Database/store records and live name lists.
    // ---------------------------------------------------------------------
    function _extractKey(value, keyPath) {
        if (keyPath == null) return undefined;
        if (Array.isArray(keyPath)) return keyPath.map(path => _extractKey(value, path));
        let current = value;
        for (const part of String(keyPath).split('.')) {
            if (current == null) return undefined;
            current = current[part];
        }
        return current;
    }
    function _assignKey(value, keyPath, key) {
        if (!value || typeof value !== 'object' || Array.isArray(keyPath)) return;
        const parts = String(keyPath).split('.');
        let current = value;
        for (let i = 0; i < parts.length - 1; i++) {
            const part = parts[i];
            if (!current[part] || typeof current[part] !== 'object') current[part] = {};
            current = current[part];
        }
        current[parts[parts.length - 1]] = key;
    }
    function _findEntry(record, key) {
        for (const [storedKey, value] of record.data) {
            if (_keyCmp(storedKey, key) === 0) return [storedKey, value];
        }
        return null;
    }
    function _deleteEntry(record, key) {
        const found = _findEntry(record, key);
        if (found) record.data.delete(found[0]);
    }
    function _sortedEntries(record) {
        return [...record.data.entries()].sort((a, b) => _keyCmp(a[0], b[0]));
    }
    function _resolveStoreKey(record, value, explicitKey, addGeneratedToValue) {
        let key = record.keyPath == null ? explicitKey : _extractKey(value, record.keyPath);
        if (key === undefined && record.autoIncrement) {
            key = record.nextKey++;
            if (record.keyPath != null && addGeneratedToValue) _assignKey(value, record.keyPath, key);
        }
        if (key === undefined) throw _domError('DataError', 'A key could not be derived from the value.');
        if (record.autoIncrement && typeof key === 'number' && Number.isFinite(key) && key >= record.nextKey) {
            record.nextKey = Math.floor(key) + 1;
        }
        _keyCmp(key, key); // validate
        return key;
    }
    function _makeStore(record, transaction) {
        const store = Object.create(IDBObjectStoreProto);
        _storeState.set(store, { record, transaction });
        return store;
    }

    function _cloneStoreRecord(record) {
        const data = new Map();
        for (const [key, value] of record.data.entries()) {
            data.set(_clone(key), _clone(value));
        }
        const indexes = new Map();
        for (const [name, index] of record.indexes.entries()) {
            indexes.set(name, {
                name: index.name,
                keyPath: _clone(index.keyPath),
                multiEntry: !!index.multiEntry,
                unique: !!index.unique,
            });
        }
        return {
            name: record.name,
            keyPath: _clone(record.keyPath),
            autoIncrement: !!record.autoIncrement,
            nextKey: record.nextKey,
            data,
            indexes,
        };
    }

    function _snapshotDatabaseRecord(record) {
        const stores = new Map();
        for (const [name, store] of record.stores.entries()) {
            stores.set(name, _cloneStoreRecord(store));
        }
        return { version: record.version, stores };
    }

    function _restoreDatabaseRecord(record, snapshot) {
        if (!snapshot) return;
        record.version = snapshot.version;
        record.stores = snapshot.stores;
    }

    // ---------------------------------------------------------------------
    // Transactions.
    // ---------------------------------------------------------------------
    function _assertTxAccepting(transaction) {
        const state = _stateFor(_txState, transaction);
        if (!state.active || state.committing) {
            throw _domError('TransactionInactiveError', 'The transaction is not active.');
        }
        return state;
    }
    function _txBegin(transaction) {
        const state = _assertTxAccepting(transaction);
        state.pending++;
    }
    function _scheduleTxCheck(transaction) {
        const state = _stateFor(_txState, transaction);
        if (!state.active || state.checkScheduled) return;
        state.checkScheduled = true;
        // IndexedDB transactions remain active through the microtask checkpoint
        // of the task that created/used them. Blink therefore allows a request
        // first issued from Promise.resolve().then(...) in the same task, while
        // a request from the next task observes TransactionInactiveError.
        // Scheduling auto-commit itself as a microtask races ahead of page-authored
        // Promise jobs, so defer the idle check to the next task instead.
        setTimeout(() => {
            state.checkScheduled = false;
            if (!state.active || state.pending !== 0) return;
            state.active = false;
            state.snapshot = null;
            state.requests.clear();
            _emit(transaction, new Event('complete'));
            for (const callback of state.completeCallbacks.splice(0)) {
                try { callback(); } catch (_) {}
            }
        }, 0);
    }
    function _txEnd(transaction) {
        const state = _stateFor(_txState, transaction);
        if (state.pending > 0) state.pending--;
        _scheduleTxCheck(transaction);
    }
    function _abortTransaction(transaction, error) {
        const state = _stateFor(_txState, transaction);
        if (!state.active) return;
        state.active = false;
        state.aborted = true;
        state.error = error || null;
        _restoreDatabaseRecord(_stateFor(_dbState, state.db).record, state.snapshot);
        state.snapshot = null;
        state.pending = 0;
        state.checkScheduled = false;

        // Requests that had been queued by this transaction but had not yet
        // dispatched success/error are aborted before the transaction's abort
        // event. Blink exposes AbortError on those requests and never lets a
        // queued success callback run after IDBTransaction.abort().
        const abortError = _domError('AbortError', 'The transaction was aborted.');
        const pendingRequests = [...state.requests];
        state.requests.clear();
        for (const request of pendingRequests) {
            const requestState = _stateFor(_requestState, request);
            if (requestState.readyState !== 'pending') continue;
            _setRequestDone(request, undefined, abortError);
            const requestEvent = new Event('error', { bubbles: true, cancelable: true });
            _emit(request, requestEvent);
            if (_dispatchEventToParentTarget) {
                _dispatchEventToParentTarget(requestEvent, transaction);
            }
        }
        _emit(transaction, new Event('abort'));
        for (const callback of state.abortCallbacks.splice(0)) {
            try { callback(state.error || abortError); } catch (_) {}
        }
        state.completeCallbacks.length = 0;
    }
    function _makeTransaction(database, names, mode = 'readonly', durability = 'default') {
        const transaction = Object.create(IDBTransactionProto);
        const dbRecord = _stateFor(_dbState, database).record;
        const normalized = [...new Set(names.map(String))].sort();
        for (const name of normalized) {
            if (!dbRecord.stores.has(name)) throw _domError('NotFoundError', `The object store '${name}' was not found.`);
        }
        _txState.set(transaction, {
            db: database,
            names: normalized,
            mode,
            durability,
            error: null,
            handlers: Object.create(null),
            active: true,
            committing: false,
            aborted: false,
            pending: 0,
            checkScheduled: false,
            completeCallbacks: [],
            abortCallbacks: [],
            requests: new Set(),
            snapshot: mode === 'readonly' ? null : _snapshotDatabaseRecord(dbRecord),
        });
        _scheduleTxCheck(transaction);
        return transaction;
    }
    for (const name of ['mode', 'durability', 'db', 'error']) {
        _defineGetter(IDBTransactionProto, name, value => _stateFor(_txState, value)[name]);
    }
    _defineGetter(IDBTransactionProto, 'objectStoreNames', value => {
        const state = _stateFor(_txState, value);
        return _makeNameList(() => state.names.slice());
    });
    for (const name of ['onabort', 'oncomplete', 'onerror']) _defineHandler(IDBTransactionProto, name, _txState);
    _defineMethod(IDBTransactionProto, 'abort', function abort() {
        const state = _stateFor(_txState, this);
        if (!state.active || state.committing) throw _domError('InvalidStateError', 'The transaction has finished.');
        _abortTransaction(this, null);
    });
    _defineMethod(IDBTransactionProto, 'commit', function commit() {
        const state = _stateFor(_txState, this);
        if (!state.active || state.committing) throw _domError('InvalidStateError', 'The transaction has finished.');
        // Explicit commit stops the transaction from accepting new requests
        // immediately, but already-queued requests are still allowed to drain
        // before the asynchronous complete event.
        state.committing = true;
        _scheduleTxCheck(this);
    });
    _defineMethod(IDBTransactionProto, 'objectStore', function objectStore(name) {
        const state = _stateFor(_txState, this);
        if (!state.active || state.committing) throw _domError('InvalidStateError', 'The transaction has finished.');
        name = String(name);
        if (!state.names.includes(name) && state.mode !== 'versionchange') {
            throw _domError('NotFoundError', `The object store '${name}' is not in this transaction.`);
        }
        const record = _stateFor(_dbState, state.db).record.stores.get(name);
        if (!record) throw _domError('NotFoundError', `The object store '${name}' was not found.`);
        return _makeStore(record, this);
    });
    _reorderProto(IDBTransactionProto, [
        'objectStoreNames', 'mode', 'durability', 'db', 'error',
        'onabort', 'oncomplete', 'onerror', 'abort', 'commit', 'objectStore',
    ]);
    _finishProto(IDBTransaction, IDBTransactionProto, 'IDBTransaction');

    // ---------------------------------------------------------------------
    // Object stores / indexes.
    // ---------------------------------------------------------------------
    Object.defineProperty(IDBObjectStoreProto, 'name', {
        enumerable: true, configurable: true,
        get: _getter('name', value => _stateFor(_storeState, value).record.name),
        set: _setter('name', (value, next) => {
            const state = _stateFor(_storeState, value);
            const tx = _stateFor(_txState, state.transaction);
            if (tx.mode !== 'versionchange' || !tx.active) {
                throw _domError('InvalidStateError', 'The object store can only be renamed during a versionchange transaction.');
            }
            const name = String(next);
            const dbRecord = _stateFor(_dbState, tx.db).record;
            if (name !== state.record.name && dbRecord.stores.has(name)) {
                throw _domError('ConstraintError', 'An object store with the requested name already exists.');
            }
            const oldName = state.record.name;
            dbRecord.stores.delete(oldName);
            state.record.name = name;
            dbRecord.stores.set(name, state.record);
            tx.names = tx.names.filter(existing => existing !== oldName);
            if (!tx.names.includes(name)) tx.names.push(name), tx.names.sort();
        }),
    });
    for (const name of ['keyPath', 'autoIncrement']) {
        _defineGetter(IDBObjectStoreProto, name, value => _stateFor(_storeState, value).record[name]);
    }
    _defineGetter(IDBObjectStoreProto, 'transaction', value => _stateFor(_storeState, value).transaction);
    _defineGetter(IDBObjectStoreProto, 'indexNames', value => {
        const record = _stateFor(_storeState, value).record;
        return _makeNameList(() => [...record.indexes.keys()].sort());
    });
    function _storeWrite(store, value, explicitKey, overwrite) {
        const state = _stateFor(_storeState, store);
        const tx = state.transaction;
        const txs = _assertTxAccepting(tx);
        if (txs.mode === 'readonly') throw _domError('ReadOnlyError', 'The transaction is read-only.');
        const cloned = _clone(value);
        let key;
        try { key = _resolveStoreKey(state.record, cloned, explicitKey, true); }
        catch (error) { return _queueRequestError(_makeRequest(store, tx), error); }
        const existing = _findEntry(state.record, key);
        if (!overwrite && existing) {
            return _queueRequestError(_makeRequest(store, tx), _domError('ConstraintError', 'The key already exists.'));
        }
        if (existing) state.record.data.delete(existing[0]);
        state.record.data.set(_clone(key), cloned);
        return _queueRequestSuccess(_makeRequest(store, tx), _clone(key));
    }
    _defineMethod(IDBObjectStoreProto, 'add', function add(value, ...rest) {
        return _storeWrite(this, value, rest[0], false);
    });
    _defineMethod(IDBObjectStoreProto, 'put', function put(value, ...rest) {
        return _storeWrite(this, value, rest[0], true);
    });
    _defineMethod(IDBObjectStoreProto, 'clear', function clear() {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        const txs = _assertTxAccepting(tx);
        if (txs.mode === 'readonly') throw _domError('ReadOnlyError', 'The transaction is read-only.');
        state.record.data.clear();
        return _queueRequestSuccess(_makeRequest(this, tx), undefined);
    });
    function _queryEntries(record, query, count) {
        const limit = count === undefined ? Infinity : Math.max(0, Number(count) >>> 0);
        const out = [];
        for (const entry of _sortedEntries(record)) {
            if (out.length >= limit) break;
            const match = query == null ? true :
                (query instanceof IDBKeyRange ? query.includes(entry[0]) : _keyCmp(entry[0], query) === 0);
            if (match) out.push(entry);
        }
        return out;
    }
    _defineMethod(IDBObjectStoreProto, 'count', function count(...args) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        const values = _queryEntries(state.record, args[0], undefined);
        return _queueRequestSuccess(_makeRequest(this, tx), values.length);
    });
    _defineMethod(IDBObjectStoreProto, 'delete', function del(query) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        const txs = _assertTxAccepting(tx);
        if (txs.mode === 'readonly') throw _domError('ReadOnlyError', 'The transaction is read-only.');
        if (query instanceof IDBKeyRange) {
            for (const [key] of _queryEntries(state.record, query)) _deleteEntry(state.record, key);
        } else _deleteEntry(state.record, query);
        return _queueRequestSuccess(_makeRequest(this, tx), undefined);
    });
    _defineMethod(IDBObjectStoreProto, 'get', function get(query) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        const found = _queryEntries(state.record, query, 1)[0];
        return _queueRequestSuccess(_makeRequest(this, tx), found ? _clone(found[1]) : undefined);
    });
    _defineMethod(IDBObjectStoreProto, 'getKey', function getKey(query) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        const found = _queryEntries(state.record, query, 1)[0];
        return _queueRequestSuccess(_makeRequest(this, tx), found ? _clone(found[0]) : undefined);
    });
    _defineMethod(IDBObjectStoreProto, 'getAll', function getAll(...args) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        return _queueRequestSuccess(_makeRequest(this, tx),
            _queryEntries(state.record, args[0], args[1]).map(entry => _clone(entry[1])));
    });
    _defineMethod(IDBObjectStoreProto, 'getAllKeys', function getAllKeys(...args) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        return _queueRequestSuccess(_makeRequest(this, tx),
            _queryEntries(state.record, args[0], args[1]).map(entry => _clone(entry[0])));
    });
    _defineMethod(IDBObjectStoreProto, 'getAllRecords', function getAllRecords(...args) {
        const state = _stateFor(_storeState, this), tx = state.transaction;
        const options = args[0] || {};
        const entries = _queryEntries(state.record, options.query, options.count);
        return _queueRequestSuccess(_makeRequest(this, tx), entries.map(([key, value]) => ({
            key: _clone(key), primaryKey: _clone(key), value: _clone(value),
        })));
    });
    _defineMethod(IDBObjectStoreProto, 'createIndex', function createIndex(name, keyPath, ...rest) {
        const state = _stateFor(_storeState, this);
        const tx = _stateFor(_txState, state.transaction);
        if (tx.mode !== 'versionchange') throw _domError('InvalidStateError', 'Indexes can only be created during a versionchange transaction.');
        name = String(name);
        if (state.record.indexes.has(name)) throw _domError('ConstraintError', 'The index already exists.');
        const options = rest[0] || {};
        const record = { name, keyPath, multiEntry: !!options.multiEntry, unique: !!options.unique };
        state.record.indexes.set(name, record);
        return _makeIndex(this, record);
    });
    _defineMethod(IDBObjectStoreProto, 'deleteIndex', function deleteIndex(name) {
        const state = _stateFor(_storeState, this);
        const tx = _stateFor(_txState, state.transaction);
        if (tx.mode !== 'versionchange') throw _domError('InvalidStateError', 'Indexes can only be deleted during a versionchange transaction.');
        if (!state.record.indexes.delete(String(name))) throw _domError('NotFoundError', 'The index was not found.');
    });
    _defineMethod(IDBObjectStoreProto, 'index', function index(name) {
        const state = _stateFor(_storeState, this), record = state.record.indexes.get(String(name));
        if (!record) throw _domError('NotFoundError', 'The index was not found.');
        return _makeIndex(this, record);
    });

    function _makeCursor(source, store, transaction, entries, direction, keyOnly, request) {
        const CtorProto = keyOnly ? IDBCursorProto : IDBCursorWithValueProto;
        const cursor = Object.create(CtorProto);
        const ordered = entries.slice();
        if (direction === 'prev' || direction === 'prevunique') ordered.reverse();
        _cursorState.set(cursor, {
            source, store, direction, request, transaction,
            entries: ordered, index: -1, key: undefined, primaryKey: undefined,
            value: undefined, keyOnly,
        });
        return cursor;
    }
    function _cursorStep(cursor, amount = 1) {
        const state = _stateFor(_cursorState, cursor);
        const reqState = _stateFor(_requestState, state.request);
        const tx = state.transaction;
        _txBegin(tx);
        reqState.readyState = 'pending';
        queueMicrotask(() => {
            state.index += Math.max(1, amount);
            if (state.index >= state.entries.length) {
                state.key = state.primaryKey = state.value = undefined;
                _setRequestDone(state.request, null, null);
            } else {
                const entry = state.entries[state.index];
                state.key = _clone(entry.key);
                state.primaryKey = _clone(entry.primaryKey);
                state.value = _clone(entry.value);
                _setRequestDone(state.request, cursor, null);
            }
            _emit(state.request, new Event('success'));
            _txEnd(tx);
        });
    }
    function _openStoreCursor(store, query, direction, keyOnly) {
        const state = _stateFor(_storeState, store);
        const request = _makeRequest(store, state.transaction);
        const range = query == null ? null : (query instanceof IDBKeyRange ? query : IDBKeyRange.only(query));
        const entries = _queryEntries(state.record, range, undefined).map(([key, value]) => ({
            key, primaryKey: key, value,
        }));
        const cursor = _makeCursor(
            store, store, state.transaction, entries, direction || 'next', keyOnly, request,
        );
        _cursorStep(cursor, 1);
        return request;
    }
    _defineMethod(IDBObjectStoreProto, 'openCursor', function openCursor(...args) {
        return _openStoreCursor(this, args[0], args[1], false);
    });
    _defineMethod(IDBObjectStoreProto, 'openKeyCursor', function openKeyCursor(...args) {
        return _openStoreCursor(this, args[0], args[1], true);
    });
    _reorderProto(IDBObjectStoreProto, [
        'name', 'keyPath', 'indexNames', 'transaction', 'autoIncrement',
        'add', 'clear', 'count', 'createIndex', 'delete', 'deleteIndex',
        'get', 'getAll', 'getAllKeys', 'getAllRecords', 'getKey', 'index',
        'openCursor', 'openKeyCursor', 'put',
    ]);
    _finishProto(IDBObjectStore, IDBObjectStoreProto, 'IDBObjectStore');

    for (const name of ['source', 'direction', 'key', 'primaryKey', 'request']) {
        _defineGetter(IDBCursorProto, name, value => _stateFor(_cursorState, value)[name]);
    }
    _defineMethod(IDBCursorProto, 'advance', function advance(count) {
        count = Number(count) >>> 0;
        if (!count) throw new TypeError("Failed to execute 'advance' on 'IDBCursor': The value provided must be greater than 0.");
        _cursorStep(this, count);
    });
    _defineMethod(IDBCursorProto, 'continue', function continueCursor(...args) {
        if (args.length && args[0] !== undefined) {
            const state = _stateFor(_cursorState, this), target = args[0];
            if (state.key === undefined) {
                throw _domError('InvalidStateError', 'The cursor has no current value.');
            }
            _keyCmp(target, target);
            const targetVsCurrent = _keyCmp(target, state.key);
            const forward = state.direction === 'next' || state.direction === 'nextunique';
            if ((forward && targetVsCurrent <= 0) || (!forward && targetVsCurrent >= 0)) {
                throw _domError('DataError', 'The requested key does not advance the cursor.');
            }

            let next = state.index + 1;
            while (next < state.entries.length) {
                const entryVsTarget = _keyCmp(state.entries[next].key, target);
                if ((forward && entryVsTarget >= 0) || (!forward && entryVsTarget <= 0)) break;
                next++;
            }
            _cursorStep(this, Math.max(1, next - state.index));
            return;
        }
        _cursorStep(this, 1);
    });
    function _cursorTupleCmp(aKey, aPrimaryKey, bKey, bPrimaryKey) {
        const keyCmp = _keyCmp(aKey, bKey);
        return keyCmp || _keyCmp(aPrimaryKey, bPrimaryKey);
    }
    _defineMethod(IDBCursorProto, 'continuePrimaryKey', function continuePrimaryKey(key, primaryKey) {
        const state = _stateFor(_cursorState, this);
        if (!_indexState.has(state.source) || state.direction === 'nextunique' || state.direction === 'prevunique') {
            throw _domError('InvalidAccessError', 'continuePrimaryKey is only valid for non-unique index cursors.');
        }
        if (state.key === undefined || state.primaryKey === undefined) {
            throw _domError('InvalidStateError', 'The cursor has no current value.');
        }

        // Validate both target keys using the same IndexedDB key ordering used
        // by stores/indexes, then require the requested tuple to move strictly
        // forward in the cursor's direction.
        _keyCmp(key, key);
        _keyCmp(primaryKey, primaryKey);
        const targetVsCurrent = _cursorTupleCmp(key, primaryKey, state.key, state.primaryKey);
        const forward = state.direction === 'next';
        if ((forward && targetVsCurrent <= 0) || (!forward && targetVsCurrent >= 0)) {
            throw _domError('DataError', 'The requested key and primary key do not advance the cursor.');
        }

        let next = state.index + 1;
        while (next < state.entries.length) {
            const entry = state.entries[next];
            const entryVsTarget = _cursorTupleCmp(entry.key, entry.primaryKey, key, primaryKey);
            if ((forward && entryVsTarget >= 0) || (!forward && entryVsTarget <= 0)) break;
            next++;
        }
        _cursorStep(this, Math.max(1, next - state.index));
    });
    _defineMethod(IDBCursorProto, 'delete', function deleteCursor() {
        const state = _stateFor(_cursorState, this);
        if (state.key === undefined) throw _domError('InvalidStateError', 'The cursor has no value.');
        return IDBObjectStoreProto.delete.call(state.store, state.primaryKey);
    });
    _defineMethod(IDBCursorProto, 'update', function update(value) {
        const state = _stateFor(_cursorState, this);
        if (state.key === undefined) throw _domError('InvalidStateError', 'The cursor has no value.');
        return IDBObjectStoreProto.put.call(state.store, value, state.primaryKey);
    });
    _finishProto(IDBCursor, IDBCursorProto, 'IDBCursor');
    _defineGetter(IDBCursorWithValueProto, 'value', value => _stateFor(_cursorState, value).value);
    _finishProto(IDBCursorWithValue, IDBCursorWithValueProto, 'IDBCursorWithValue');

    function _makeIndex(store, record) {
        const index = Object.create(IDBIndexProto);
        _indexState.set(index, { store, record });
        return index;
    }
    Object.defineProperty(IDBIndexProto, 'name', {
        enumerable: true, configurable: true,
        get: _getter('name', value => _stateFor(_indexState, value).record.name),
        set: _setter('name', (value, next) => {
            const state = _stateFor(_indexState, value);
            const storeState = _stateFor(_storeState, state.store);
            const tx = _stateFor(_txState, storeState.transaction);
            if (tx.mode !== 'versionchange' || !tx.active) {
                throw _domError('InvalidStateError', 'The index can only be renamed during a versionchange transaction.');
            }
            const name = String(next);
            if (name !== state.record.name && storeState.record.indexes.has(name)) {
                throw _domError('ConstraintError', 'An index with the requested name already exists.');
            }
            const oldName = state.record.name;
            storeState.record.indexes.delete(oldName);
            state.record.name = name;
            storeState.record.indexes.set(name, state.record);
        }),
    });
    for (const name of ['keyPath', 'multiEntry', 'unique']) {
        _defineGetter(IDBIndexProto, name, value => _stateFor(_indexState, value).record[name]);
    }
    _defineGetter(IDBIndexProto, 'objectStore', value => _stateFor(_indexState, value).store);
    function _indexMatches(index, query) {
        const state = _stateFor(_indexState, index);
        const storeState = _stateFor(_storeState, state.store);
        const out = [];
        for (const [primaryKey, value] of _sortedEntries(storeState.record)) {
            let key = _extractKey(value, state.record.keyPath);
            if (key === undefined) continue;
            const keys = state.record.multiEntry && Array.isArray(key) ? key : [key];
            for (const candidate of keys) {
                const match = query == null ? true :
                    (query instanceof IDBKeyRange ? query.includes(candidate) : _keyCmp(candidate, query) === 0);
                if (match) { out.push({ key: candidate, primaryKey, value }); break; }
            }
        }
        out.sort((a, b) => _keyCmp(a.key, b.key) || _keyCmp(a.primaryKey, b.primaryKey));
        return out;
    }
    _defineMethod(IDBIndexProto, 'count', function count(...args) {
        const state = _stateFor(_indexState, this), storeState = _stateFor(_storeState, state.store);
        return _queueRequestSuccess(_makeRequest(this, storeState.transaction), _indexMatches(this, args[0]).length);
    });
    _defineMethod(IDBIndexProto, 'get', function get(query) {
        const state = _stateFor(_indexState, this), storeState = _stateFor(_storeState, state.store);
        const item = _indexMatches(this, query)[0];
        return _queueRequestSuccess(_makeRequest(this, storeState.transaction), item ? _clone(item.value) : undefined);
    });
    _defineMethod(IDBIndexProto, 'getKey', function getKey(query) {
        const state = _stateFor(_indexState, this), storeState = _stateFor(_storeState, state.store);
        const item = _indexMatches(this, query)[0];
        return _queueRequestSuccess(_makeRequest(this, storeState.transaction), item ? _clone(item.primaryKey) : undefined);
    });
    _defineMethod(IDBIndexProto, 'getAll', function getAll(...args) {
        const state = _stateFor(_indexState, this), storeState = _stateFor(_storeState, state.store);
        const count = args[1] === undefined ? Infinity : Number(args[1]) >>> 0;
        return _queueRequestSuccess(_makeRequest(this, storeState.transaction), _indexMatches(this, args[0]).slice(0, count).map(x => _clone(x.value)));
    });
    _defineMethod(IDBIndexProto, 'getAllKeys', function getAllKeys(...args) {
        const state = _stateFor(_indexState, this), storeState = _stateFor(_storeState, state.store);
        const count = args[1] === undefined ? Infinity : Number(args[1]) >>> 0;
        return _queueRequestSuccess(_makeRequest(this, storeState.transaction), _indexMatches(this, args[0]).slice(0, count).map(x => _clone(x.primaryKey)));
    });
    _defineMethod(IDBIndexProto, 'getAllRecords', function getAllRecords(...args) {
        const state = _stateFor(_indexState, this), storeState = _stateFor(_storeState, state.store);
        const options = args[0] || {}, count = options.count === undefined ? Infinity : Number(options.count) >>> 0;
        return _queueRequestSuccess(_makeRequest(this, storeState.transaction), _indexMatches(this, options.query).slice(0, count).map(x => ({ key: _clone(x.key), primaryKey: _clone(x.primaryKey), value: _clone(x.value) })));
    });
    function _openIndexCursor(index, query, direction, keyOnly) {
        const indexState = _stateFor(_indexState, index);
        const store = indexState.store;
        const storeState = _stateFor(_storeState, store);
        const request = _makeRequest(index, storeState.transaction);
        const entries = _indexMatches(index, query).map(item => ({
            key: item.key, primaryKey: item.primaryKey, value: item.value,
        }));
        const cursor = _makeCursor(
            index, store, storeState.transaction, entries,
            direction || 'next', keyOnly, request,
        );
        _cursorStep(cursor, 1);
        return request;
    }
    _defineMethod(IDBIndexProto, 'openCursor', function openCursor(...args) {
        return _openIndexCursor(this, args[0], args[1], false);
    });
    _defineMethod(IDBIndexProto, 'openKeyCursor', function openKeyCursor(...args) {
        return _openIndexCursor(this, args[0], args[1], true);
    });
    _reorderProto(IDBIndexProto, [
        'name', 'objectStore', 'keyPath', 'multiEntry', 'unique',
        'count', 'get', 'getAll', 'getAllKeys', 'getAllRecords', 'getKey',
        'openCursor', 'openKeyCursor',
    ]);
    _finishProto(IDBIndex, IDBIndexProto, 'IDBIndex');

    // ---------------------------------------------------------------------
    // Database.
    // ---------------------------------------------------------------------
    function _ensureDatabaseRuntime(record) {
        if (!record.connections) record.connections = new Set();
        if (!record.pendingOperations) record.pendingOperations = [];
        if (record.operationRunning === undefined) record.operationRunning = false;
        return record;
    }
    function _liveDatabaseConnections(record) {
        _ensureDatabaseRuntime(record);
        const live = [];
        for (const database of record.connections) {
            const state = _dbState.get(database);
            if (state && !state.closed) live.push(database);
        }
        return live;
    }
    function _finishDatabaseOperation(record) {
        _ensureDatabaseRuntime(record).operationRunning = false;
        queueMicrotask(() => _processDatabaseOperations(record));
    }
    function _processDatabaseOperations(record) {
        _ensureDatabaseRuntime(record);
        if (record.operationRunning || record.pendingOperations.length === 0) return;
        const operation = record.pendingOperations[0];

        if (!operation.prepared) {
            try {
                if (typeof operation.prepare === 'function') operation.prepare();
                operation.prepared = true;
            } catch (error) {
                record.pendingOperations.shift();
                _setRequestDone(operation.request, undefined, error);
                _emit(operation.request, new Event('error', { cancelable: true }));
                queueMicrotask(() => _processDatabaseOperations(record));
                return;
            }
        }

        if (operation.requeueRecord && operation.requeueRecord !== record) {
            record.pendingOperations.shift();
            const nextRecord = operation.requeueRecord;
            delete operation.requeueRecord;
            _queueDatabaseOperation(nextRecord, operation);
            queueMicrotask(() => _processDatabaseOperations(record));
            return;
        }

        if (operation.waitForConnections && !operation.versionchangeDispatched) {
            operation.versionchangeDispatched = true;
            // Per IndexedDB, existing connections get versionchange first and
            // are allowed to close synchronously from that handler. `blocked`
            // is emitted only if at least one connection remains afterwards.
            for (const database of _liveDatabaseConnections(record)) {
                _emit(database, new IDBVersionChangeEvent('versionchange', {
                    oldVersion: operation.oldVersion,
                    newVersion: operation.newVersion,
                }));
            }
        }

        if (operation.waitForConnections && _liveDatabaseConnections(record).length !== 0) {
            if (!operation.blockedDispatched) {
                operation.blockedDispatched = true;
                _emit(operation.request, new IDBVersionChangeEvent('blocked', {
                    oldVersion: operation.oldVersion,
                    newVersion: operation.newVersion,
                }));
            }
            return;
        }

        record.pendingOperations.shift();
        record.operationRunning = true;
        try {
            operation.begin(() => _finishDatabaseOperation(record));
        } catch (error) {
            _setRequestDone(operation.request, undefined, error);
            _emit(operation.request, new Event('error', { bubbles: true, cancelable: true }));
            _finishDatabaseOperation(record);
        }
    }
    function _queueDatabaseOperation(record, operation) {
        operation.prepared = false;
        operation.versionchangeDispatched = false;
        operation.blockedDispatched = false;
        _ensureDatabaseRuntime(record).pendingOperations.push(operation);
        _processDatabaseOperations(record);
    }
    function _makeDatabase(record) {
        _ensureDatabaseRuntime(record);
        const db = Object.create(IDBDatabaseProto);
        _dbState.set(db, { record, handlers: Object.create(null), upgradeTransaction: null, closed: false });
        record.connections.add(db);
        return db;
    }
    _defineGetter(IDBDatabaseProto, 'name', value => _stateFor(_dbState, value).record.name);
    _defineGetter(IDBDatabaseProto, 'version', value => _stateFor(_dbState, value).record.version);
    _defineGetter(IDBDatabaseProto, 'objectStoreNames', value => {
        const record = _stateFor(_dbState, value).record;
        return _makeNameList(() => [...record.stores.keys()].sort());
    });
    for (const name of ['onabort', 'onclose', 'onerror', 'onversionchange']) _defineHandler(IDBDatabaseProto, name, _dbState);
    _defineMethod(IDBDatabaseProto, 'close', function close() {
        const state = _stateFor(_dbState, this);
        if (state.closed) return;
        state.closed = true;
        _ensureDatabaseRuntime(state.record).connections.delete(this);
        // Resuming a blocked upgrade/delete from inside close() itself is too
        // re-entrant. Blink resumes the queued operation asynchronously after
        // the closing handler yields, so schedule the queue pump as a microtask.
        queueMicrotask(() => _processDatabaseOperations(state.record));
    });
    _defineMethod(IDBDatabaseProto, 'createObjectStore', function createObjectStore(name, ...rest) {
        const state = _stateFor(_dbState, this), tx = state.upgradeTransaction;
        if (!tx || !_stateFor(_txState, tx).active) throw _domError('InvalidStateError', 'The database is not running a versionchange transaction.');
        name = String(name);
        if (state.record.stores.has(name)) throw _domError('ConstraintError', 'The object store already exists.');
        const options = rest[0] || {};
        const record = {
            name,
            keyPath: options.keyPath === undefined ? null : options.keyPath,
            autoIncrement: !!options.autoIncrement,
            nextKey: 1,
            data: new Map(),
            indexes: new Map(),
        };
        state.record.stores.set(name, record);
        const txState = _stateFor(_txState, tx);
        if (!txState.names.includes(name)) txState.names.push(name), txState.names.sort();
        return _makeStore(record, tx);
    });
    _defineMethod(IDBDatabaseProto, 'deleteObjectStore', function deleteObjectStore(name) {
        const state = _stateFor(_dbState, this), tx = state.upgradeTransaction;
        if (!tx || !_stateFor(_txState, tx).active) throw _domError('InvalidStateError', 'The database is not running a versionchange transaction.');
        name = String(name);
        if (!state.record.stores.delete(name)) throw _domError('NotFoundError', 'The object store was not found.');
        const txState = _stateFor(_txState, tx);
        txState.names = txState.names.filter(x => x !== name);
    });
    _defineMethod(IDBDatabaseProto, 'transaction', function transaction(storeNames, ...rest) {
        const state = _stateFor(_dbState, this);
        if (state.closed) throw _domError('InvalidStateError', 'The database connection is closed.');
        const names = Array.isArray(storeNames) ? storeNames : [storeNames];
        const mode = rest[0] === undefined ? 'readonly' : String(rest[0]);
        const options = rest[1] || {};
        if (mode !== 'readonly' && mode !== 'readwrite') throw new TypeError(`The provided value '${mode}' is not a valid enum value of type IDBTransactionMode.`);
        return _makeTransaction(this, names, mode, String(options.durability || 'default'));
    });
    _finishProto(IDBDatabase, IDBDatabaseProto, 'IDBDatabase');

    // ---------------------------------------------------------------------
    // Factory/open/delete lifecycle.
    // ---------------------------------------------------------------------
    function _makeFactory() { return Object.create(IDBFactoryProto); }
    _defineMethod(IDBFactoryProto, 'cmp', function cmp(first, second) { return _keyCmp(first, second); });
    _defineMethod(IDBFactoryProto, 'databases', function databases() {
        return Promise.resolve([..._dbRegistry.values()].map(record => ({ name: record.name, version: record.version })));
    });
    _defineMethod(IDBFactoryProto, 'deleteDatabase', function deleteDatabase(name) {
        const request = _makeRequest(null, null, true);
        name = String(name);
        queueMicrotask(() => {
            const record = _dbRegistry.get(name);
            if (!record) {
                _setRequestDone(request, undefined, null);
                _emit(request, new Event('success'));
                return;
            }
            const operation = {
                request,
                waitForConnections: true,
                prepare() {
                    operation.oldVersion = record.version;
                    operation.newVersion = null;
                },
                begin(done) {
                    _dbRegistry.delete(name);
                    _setRequestDone(request, undefined, null);
                    // Queue any already-pending operations before success
                    // handlers get a chance to enqueue new opens for this name.
                    // The success event still fires synchronously in this task;
                    // only the queue pump itself is deferred by `done()`.
                    done();
                    _emit(request, new Event('success'));
                },
            };
            _queueDatabaseOperation(record, operation);
        });
        return request;
    });
    _defineMethod(IDBFactoryProto, 'open', function open(name, ...rest) {
        name = String(name);
        const request = _makeRequest(null, null, true);
        let requested = rest[0];
        if (requested !== undefined) {
            requested = Number(requested);
            if (!Number.isInteger(requested) || requested <= 0) throw new TypeError('The version provided must be a positive integer.');
        }
        queueMicrotask(() => {
            let record = _dbRegistry.get(name);
            if (!record) {
                // Keep the not-yet-created database at version 0 until the
                // versionchange transaction has captured its rollback
                // snapshot. If that transaction aborts, the registry entry is
                // removed entirely below, matching Blink's failed initial-open
                // semantics.
                record = { name, version: 0, stores: new Map() };
                _ensureDatabaseRuntime(record);
                _dbRegistry.set(name, record);
            }
            _ensureDatabaseRuntime(record);
            const operation = {
                request,
                waitForConnections: false,
                prepare() {
                    const registered = _dbRegistry.get(name);
                    if (registered !== record) {
                        if (registered) {
                            // A preceding delete completed and a success handler
                            // already recreated the database before this older
                            // queued open resumed. Move the request to the live
                            // record's queue so operations remain serialized by
                            // database name rather than by a stale record object.
                            record = registered;
                            operation.requeueRecord = registered;
                            return;
                        }

                        // A preceding delete removed this record. Reuse the
                        // queue container, but reset all persistent database
                        // state so this open observes a fresh database at
                        // oldVersion=0 and runs the normal initial upgrade.
                        record.version = 0;
                        record.stores = new Map();
                        _ensureDatabaseRuntime(record).connections.clear();
                        _dbRegistry.set(name, record);
                    }
                    const oldVersion = record.version;
                    const targetVersion = requested === undefined
                        ? (oldVersion === 0 ? 1 : oldVersion)
                        : requested;
                    if (targetVersion < oldVersion) {
                        throw _domError('VersionError', 'The requested version is less than the existing version.');
                    }
                    operation.oldVersion = oldVersion;
                    operation.newVersion = targetVersion;
                    operation.targetVersion = targetVersion;
                    operation.waitForConnections = oldVersion < targetVersion && oldVersion !== 0;
                },
                begin(done) {
                    const oldVersion = operation.oldVersion;
                    const targetVersion = operation.targetVersion;
                    const database = _makeDatabase(record);
                    const reqState = _stateFor(_requestState, request);
                    if (oldVersion < targetVersion) {
                        const transaction = _makeTransaction(database, [...record.stores.keys()], 'versionchange', 'default');
                        record.version = targetVersion;
                        _stateFor(_dbState, database).upgradeTransaction = transaction;
                        reqState.result = database;
                        reqState.readyState = 'done';
                        reqState.transaction = transaction;
                        const txState = _stateFor(_txState, transaction);
                        txState.completeCallbacks.push(() => {
                            _stateFor(_dbState, database).upgradeTransaction = null;
                            reqState.transaction = null;
                            _setRequestDone(request, database, null);
                            queueMicrotask(() => {
                                _emit(request, new Event('success'));
                                done();
                            });
                        });
                        txState.abortCallbacks.push((abortError) => {
                            const dbState = _stateFor(_dbState, database);
                            dbState.upgradeTransaction = null;
                            dbState.closed = true;
                            _ensureDatabaseRuntime(record).connections.delete(database);
                            reqState.transaction = null;
                            if (oldVersion === 0) {
                                _dbRegistry.delete(name);
                            }
                            _setRequestDone(request, undefined, abortError);
                            queueMicrotask(() => {
                                _emit(request, new Event('error', { bubbles: true, cancelable: true }));
                                done();
                            });
                        });
                        _emit(request, new IDBVersionChangeEvent('upgradeneeded', {
                            oldVersion,
                            newVersion: targetVersion,
                        }));
                        _scheduleTxCheck(transaction);
                    } else {
                        _setRequestDone(request, database, null);
                        _emit(request, new Event('success'));
                        done();
                    }
                },
            };
            _queueDatabaseOperation(record, operation);
        });
        return request;
    });
    _finishProto(IDBFactory, IDBFactoryProto, 'IDBFactory');

    // Public globals. Replacing an existing value does not change the Window
    // own-property order established earlier in bootstrap.
    _install('DOMStringList', DOMStringList);
    _install('IDBFactory', IDBFactory);
    _install('IDBDatabase', IDBDatabase);
    _install('IDBTransaction', IDBTransaction);
    _install('IDBObjectStore', IDBObjectStore);
    _install('IDBRequest', IDBRequest);
    _install('IDBOpenDBRequest', IDBOpenDBRequest);
    _install('IDBKeyRange', IDBKeyRange);
    _install('IDBCursor', IDBCursor);
    _install('IDBCursorWithValue', IDBCursorWithValue);
    _install('IDBIndex', IDBIndex);
    _install('IDBVersionChangeEvent', IDBVersionChangeEvent);
    globalThis.indexedDB = _makeFactory();
})(globalThis);
