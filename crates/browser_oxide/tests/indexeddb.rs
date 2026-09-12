//! IndexedDB browser-semantics regression tests.
//!
//! These fixtures are derived from Chrome for Testing 148.0.7778.167 and
//! deliberately use only an in-memory page origin. They lock the observable
//! WebIDL shape and request/transaction ordering without depending on durable
//! on-disk storage.

use browser_oxide::Page;

async fn page(html: &str) -> Page {
    Page::from_html_with_url(
        html,
        "https://example.test/indexeddb",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("indexeddb page")
}

#[tokio::test]
async fn indexeddb_webidl_surface_matches_chrome_148() {
    let mut page = page("<!doctype html><html><body></body></html>").await;
    let result = page
        .evaluate(
            r#"
            JSON.stringify((() => {
                const own = C => Reflect.ownKeys(C.prototype).map(String);
                const illegal = C => {
                    try { new C(); return false; }
                    catch (e) { return e instanceof TypeError && e.message.includes('Illegal constructor'); }
                };
                const descriptor = (C, name) => {
                    const d = Object.getOwnPropertyDescriptor(C.prototype, name);
                    return [!!d.enumerable, !!d.configurable,
                        'writable' in d ? !!d.writable : null,
                        typeof d.get === 'function', typeof d.set === 'function',
                        typeof d.value === 'function' ? d.value.length : null];
                };
                return {
                    factory: {
                        tag: Object.prototype.toString.call(indexedDB),
                        own: Reflect.ownKeys(indexedDB).map(String),
                        illegal: illegal(IDBFactory),
                        keys: own(IDBFactory),
                        open: descriptor(IDBFactory, 'open'),
                    },
                    database: {
                        illegal: illegal(IDBDatabase),
                        parent: Object.getPrototypeOf(IDBDatabase.prototype).constructor.name,
                        keys: own(IDBDatabase),
                    },
                    transaction: {
                        illegal: illegal(IDBTransaction),
                        parent: Object.getPrototypeOf(IDBTransaction.prototype).constructor.name,
                        keys: own(IDBTransaction),
                    },
                    store: {
                        illegal: illegal(IDBObjectStore),
                        keys: own(IDBObjectStore),
                        name: descriptor(IDBObjectStore, 'name'),
                        put: descriptor(IDBObjectStore, 'put'),
                    },
                    request: {
                        illegal: illegal(IDBRequest),
                        parent: Object.getPrototypeOf(IDBRequest.prototype).constructor.name,
                        keys: own(IDBRequest),
                    },
                    openRequest: {
                        illegal: illegal(IDBOpenDBRequest),
                        parent: Object.getPrototypeOf(IDBOpenDBRequest.prototype).constructor.name,
                        keys: own(IDBOpenDBRequest),
                    },
                    range: {
                        illegal: illegal(IDBKeyRange),
                        keys: own(IDBKeyRange),
                        onlyLength: IDBKeyRange.only.length,
                        boundLength: IDBKeyRange.bound.length,
                    },
                    cursor: {
                        illegal: illegal(IDBCursor),
                        keys: own(IDBCursor),
                    },
                    cursorValue: {
                        illegal: illegal(IDBCursorWithValue),
                        parent: Object.getPrototypeOf(IDBCursorWithValue.prototype).constructor.name,
                        keys: own(IDBCursorWithValue),
                    },
                    index: {
                        illegal: illegal(IDBIndex),
                        keys: own(IDBIndex),
                        name: descriptor(IDBIndex, 'name'),
                    },
                    versionEvent: {
                        length: IDBVersionChangeEvent.length,
                        parent: Object.getPrototypeOf(IDBVersionChangeEvent.prototype).constructor.name,
                        tag: Object.prototype.toString.call(new IDBVersionChangeEvent('x', {
                            oldVersion: 1, newVersion: 2
                        })),
                    },
                    stringList: {
                        illegal: illegal(DOMStringList),
                        keys: own(DOMStringList),
                    },
                };
            })())
            "#,
        )
        .expect("surface probe");

    let value: serde_json::Value = serde_json::from_str(&result).expect("surface json");
    assert_eq!(value["factory"]["tag"], "[object IDBFactory]");
    assert_eq!(value["factory"]["own"], serde_json::json!([]));
    assert_eq!(value["factory"]["illegal"], true);
    assert_eq!(
        value["factory"]["keys"],
        serde_json::json!([
            "cmp",
            "databases",
            "deleteDatabase",
            "open",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["factory"]["open"],
        serde_json::json!([true, true, true, false, false, 1])
    );

    assert_eq!(value["database"]["illegal"], true);
    assert_eq!(value["database"]["parent"], "EventTarget");
    assert_eq!(
        value["database"]["keys"],
        serde_json::json!([
            "name",
            "version",
            "objectStoreNames",
            "onabort",
            "onclose",
            "onerror",
            "onversionchange",
            "close",
            "createObjectStore",
            "deleteObjectStore",
            "transaction",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["transaction"]["illegal"], true);
    assert_eq!(value["transaction"]["parent"], "EventTarget");
    assert_eq!(
        value["transaction"]["keys"],
        serde_json::json!([
            "objectStoreNames",
            "mode",
            "durability",
            "db",
            "error",
            "onabort",
            "oncomplete",
            "onerror",
            "abort",
            "commit",
            "objectStore",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["store"]["illegal"], true);
    assert_eq!(
        value["store"]["name"],
        serde_json::json!([true, true, null, true, true, null])
    );
    assert_eq!(
        value["store"]["put"],
        serde_json::json!([true, true, true, false, false, 1])
    );
    assert_eq!(value["request"]["illegal"], true);
    assert_eq!(value["request"]["parent"], "EventTarget");
    assert_eq!(value["openRequest"]["illegal"], true);
    assert_eq!(value["openRequest"]["parent"], "IDBRequest");
    assert_eq!(value["range"]["illegal"], true);
    assert_eq!(value["range"]["onlyLength"], 1);
    assert_eq!(value["range"]["boundLength"], 2);
    assert_eq!(value["cursor"]["illegal"], true);
    assert_eq!(value["cursorValue"]["illegal"], true);
    assert_eq!(value["cursorValue"]["parent"], "IDBCursor");
    assert_eq!(value["index"]["illegal"], true);
    assert_eq!(
        value["index"]["name"],
        serde_json::json!([true, true, null, true, true, null])
    );
    assert_eq!(value["versionEvent"]["length"], 1);
    assert_eq!(value["versionEvent"]["parent"], "Event");
    assert_eq!(
        value["versionEvent"]["tag"],
        "[object IDBVersionChangeEvent]"
    );
    assert_eq!(value["stringList"]["illegal"], true);
    assert_eq!(
        value["stringList"]["keys"],
        serde_json::json!([
            "length",
            "contains",
            "item",
            "constructor",
            "Symbol(Symbol.toStringTag)",
            "Symbol(Symbol.iterator)"
        ])
    );
}

#[tokio::test]
async fn indexeddb_transaction_cursor_and_error_lifecycle_matches_chrome_148() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbResult = null;
        (() => {
            const out = { seq: [], errors: {} };
            const finish = () => { globalThis.__idbResult = out; };
            const request = indexedDB.open('browser-oxide-idb-regression', 1);
            try { void request.result; out.errors.pendingResult = 'no-throw'; }
            catch (error) { out.errors.pendingResult = error.name; }
            request.onupgradeneeded = () => {
                out.seq.push('upgrade');
                const db = request.result;
                const first = db.createObjectStore('b');
                db.createObjectStore('a');
                first.name = 'c';
                const store = db.createObjectStore('items', { keyPath: 'id' });
                const index = store.createIndex('z', 'v');
                store.createIndex('y', 'v');
                index.name = 'x';
                out.upgrade = {
                    stores: Array.from(db.objectStoreNames),
                    indexes: Array.from(store.indexNames),
                    namesOwn: Reflect.ownKeys(db.objectStoreNames).map(String),
                    namesTag: Object.prototype.toString.call(db.objectStoreNames),
                };
                store.put({ id: 2, v: 'b', payload: 'two' });
                store.put({ id: 1, v: 'a', payload: 'one' });
            };
            request.onsuccess = () => {
                out.seq.push('open-success');
                const db = request.result;
                const tx = db.transaction('items');
                const store = tx.objectStore('items');
                const index = store.index('x');
                try { store.put({ id: 9, v: 'x' }); out.errors.readonlyPut = 'no-throw'; }
                catch (error) { out.errors.readonlyPut = error.name; }

                const get = index.get('a');
                get.onsuccess = () => { out.seq.push('index-get'); out.indexGet = get.result; };

                const cursorRequest = index.openCursor();
                const rows = [];
                cursorRequest.onsuccess = () => {
                    const cursor = cursorRequest.result;
                    if (!cursor) {
                        out.seq.push('index-cursor-done');
                        out.rows = rows;
                        return;
                    }
                    rows.push({
                        tag: Object.prototype.toString.call(cursor),
                        own: Reflect.ownKeys(cursor).map(String),
                        sourceIsIndex: cursor.source === index,
                        requestSame: cursor.request === cursorRequest,
                        key: cursor.key,
                        primaryKey: cursor.primaryKey,
                        value: cursor.value,
                    });
                    cursor.continue();
                };

                tx.oncomplete = () => {
                    out.seq.push('readonly-complete');
                    const rw = db.transaction('items', 'readwrite');
                    const duplicate = rw.objectStore('items').add({ id: 1, v: 'duplicate' });
                    let requestErrorEvent = null;
                    duplicate.onerror = event => {
                        requestErrorEvent = event;
                        out.seq.push('dup-error');
                        out.errors.duplicate = event.target.error.name;
                        out.requestError = {
                            trusted: event.isTrusted,
                            bubbles: event.bubbles,
                            cancelable: event.cancelable,
                            targetRequest: event.target === duplicate,
                            currentRequest: event.currentTarget === duplicate,
                            phase: event.eventPhase,
                        };
                        event.preventDefault();
                    };
                    rw.onerror = event => {
                        out.seq.push('rw-error');
                        out.transactionError = {
                            sameEvent: event === requestErrorEvent,
                            trusted: event.isTrusted,
                            targetRequest: event.target === duplicate,
                            currentTransaction: event.currentTarget === rw,
                            phase: event.eventPhase,
                            defaultPrevented: event.defaultPrevented,
                        };
                    };
                    rw.oncomplete = () => { out.seq.push('rw-complete'); finish(); };
                    rw.onabort = () => { out.seq.push('rw-abort'); finish(); };
                };
            };
            request.onerror = () => { out.errors.open = request.error.name; finish(); };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbResult)")
        .expect("lifecycle result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("lifecycle json");

    assert_eq!(
        value["seq"],
        serde_json::json!([
            "upgrade",
            "open-success",
            "index-get",
            "index-cursor-done",
            "readonly-complete",
            "dup-error",
            "rw-error",
            "rw-complete"
        ])
    );
    assert_eq!(value["errors"]["pendingResult"], "InvalidStateError");
    assert_eq!(value["errors"]["readonlyPut"], "ReadOnlyError");
    assert_eq!(value["errors"]["duplicate"], "ConstraintError");
    assert_eq!(
        value["requestError"],
        serde_json::json!({
            "trusted": true,
            "bubbles": true,
            "cancelable": true,
            "targetRequest": true,
            "currentRequest": true,
            "phase": 2
        })
    );
    assert_eq!(
        value["transactionError"],
        serde_json::json!({
            "sameEvent": true,
            "trusted": true,
            "targetRequest": true,
            "currentTransaction": true,
            "phase": 3,
            "defaultPrevented": true
        })
    );
    assert_eq!(
        value["upgrade"]["stores"],
        serde_json::json!(["a", "c", "items"])
    );
    assert_eq!(value["upgrade"]["indexes"], serde_json::json!(["x", "y"]));
    assert_eq!(
        value["upgrade"]["namesOwn"],
        serde_json::json!(["0", "1", "2"])
    );
    assert_eq!(value["upgrade"]["namesTag"], "[object DOMStringList]");
    assert_eq!(
        value["indexGet"],
        serde_json::json!({"id":1,"v":"a","payload":"one"})
    );
    assert_eq!(value["rows"].as_array().map(Vec::len), Some(2));
    assert_eq!(value["rows"][0]["tag"], "[object IDBCursorWithValue]");
    assert_eq!(value["rows"][0]["own"], serde_json::json!([]));
    assert_eq!(value["rows"][0]["sourceIsIndex"], true);
    assert_eq!(value["rows"][0]["requestSame"], true);
    assert_eq!(value["rows"][0]["key"], "a");
    assert_eq!(value["rows"][0]["primaryKey"], 1);
    assert_eq!(value["rows"][1]["key"], "b");
    assert_eq!(value["rows"][1]["primaryKey"], 2);
}

#[tokio::test]
async fn indexeddb_aborted_readwrite_rolls_back_and_aborts_pending_request() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbAbortResult = null;
        (() => {
            const request = indexedDB.open('browser-oxide-idb-abort', 1);
            request.onupgradeneeded = () => {
                request.result.createObjectStore('items', { keyPath: 'id' });
            };
            request.onsuccess = () => {
                const db = request.result;
                const tx = db.transaction('items', 'readwrite');
                const put = tx.objectStore('items').put({ id: 1, value: 'temporary' });
                const out = {
                    putSuccess: false,
                    putError: null,
                    txAbort: false,
                    txComplete: false,
                    storedAfterAbort: 'not-read',
                };

                put.onsuccess = () => { out.putSuccess = true; };
                put.onerror = () => {
                    out.putError = put.error && put.error.name;
                };
                tx.oncomplete = () => {
                    out.txComplete = true;
                    globalThis.__idbAbortResult = out;
                };
                tx.onabort = () => {
                    out.txAbort = true;
                    const verify = db.transaction('items').objectStore('items').get(1);
                    verify.onsuccess = () => {
                        out.storedAfterAbort = verify.result === undefined
                            ? null
                            : verify.result;
                        // Let the already-queued put request callback run, if
                        // the implementation incorrectly leaves it alive.
                        queueMicrotask(() => { globalThis.__idbAbortResult = out; });
                    };
                };
                tx.abort();
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbAbortResult)")
        .expect("abort result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("abort json");

    assert_eq!(value["txAbort"], true);
    assert_eq!(value["txComplete"], false);
    assert_eq!(value["putSuccess"], false);
    assert_eq!(value["putError"], "AbortError");
    assert_eq!(value["storedAfterAbort"], serde_json::Value::Null);
}

#[tokio::test]
async fn indexeddb_uncanceled_request_error_aborts_and_rolls_back_entire_transaction() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbAutoAbortResult = null;
        (() => {
            const open = indexedDB.open('browser-oxide-idb-auto-abort', 1);
            open.onupgradeneeded = () => {
                const store = open.result.createObjectStore('items', { keyPath: 'id' });
                store.put({ id: 1, value: 'original' });
            };
            open.onsuccess = () => {
                const db = open.result;
                const tx = db.transaction('items', 'readwrite');
                const store = tx.objectStore('items');
                const first = store.put({ id: 2, value: 'must-roll-back' });
                const duplicate = store.add({ id: 1, value: 'duplicate' });
                const out = {
                    seq: [], duplicateError: null,
                    txErrorDuringError: null, txErrorDuringAbort: null,
                    id1: null, id2: 'not-read'
                };
                first.onsuccess = () => out.seq.push('first-success');
                duplicate.onerror = () => {
                    out.seq.push('duplicate-error');
                    out.duplicateError = duplicate.error && duplicate.error.name;
                    // Deliberately do not preventDefault(): IndexedDB must
                    // abort the whole transaction.
                };
                tx.onerror = () => {
                    out.seq.push('tx-error');
                    out.txErrorDuringError = tx.error && tx.error.name;
                };
                tx.onabort = () => {
                    out.seq.push('tx-abort');
                    out.txErrorDuringAbort = tx.error && tx.error.name;
                    const verify = db.transaction('items');
                    const s = verify.objectStore('items');
                    const r1 = s.get(1);
                    const r2 = s.get(2);
                    let remaining = 2;
                    const done = () => {
                        if (--remaining) return;
                        globalThis.__idbAutoAbortResult = out;
                    };
                    r1.onsuccess = () => { out.id1 = r1.result; done(); };
                    r2.onsuccess = () => {
                        out.id2 = r2.result === undefined ? null : r2.result;
                        done();
                    };
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbAutoAbortResult)")
        .expect("automatic abort result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("automatic abort json");

    assert_eq!(value["duplicateError"], "ConstraintError");
    assert_eq!(value["txErrorDuringError"], serde_json::Value::Null);
    assert_eq!(value["txErrorDuringAbort"], "ConstraintError");
    assert_eq!(
        value["seq"],
        serde_json::json!(["first-success", "duplicate-error", "tx-error", "tx-abort"])
    );
    assert_eq!(value["id1"], serde_json::json!({"id":1,"value":"original"}));
    assert_eq!(value["id2"], serde_json::Value::Null);
}

#[tokio::test]
async fn indexeddb_aborted_versionchange_restores_previous_schema_and_version() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbUpgradeAbortResult = null;
        (() => {
            const first = indexedDB.open('browser-oxide-idb-upgrade-abort', 1);
            first.onupgradeneeded = () => first.result.createObjectStore('stable');
            first.onsuccess = () => {
                first.result.close();
                const upgrade = indexedDB.open('browser-oxide-idb-upgrade-abort', 2);
                const out = { seq: [], openError: null, version: null, stores: null };
                upgrade.onupgradeneeded = () => {
                    out.seq.push('upgrade');
                    upgrade.result.deleteObjectStore('stable');
                    upgrade.result.createObjectStore('temporary');
                    upgrade.transaction.onabort = () => out.seq.push('tx-abort');
                    upgrade.transaction.abort();
                };
                upgrade.onerror = () => {
                    out.seq.push('open-error');
                    out.openError = upgrade.error && upgrade.error.name;
                    const verify = indexedDB.open('browser-oxide-idb-upgrade-abort');
                    verify.onsuccess = () => {
                        out.version = verify.result.version;
                        out.stores = Array.from(verify.result.objectStoreNames);
                        globalThis.__idbUpgradeAbortResult = out;
                    };
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbUpgradeAbortResult)")
        .expect("upgrade abort result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("upgrade abort json");

    assert_eq!(
        value["seq"],
        serde_json::json!(["upgrade", "tx-abort", "open-error"])
    );
    assert_eq!(value["openError"], "AbortError");
    assert_eq!(value["version"], 1);
    assert_eq!(value["stores"], serde_json::json!(["stable"]));
}

#[tokio::test]
async fn indexeddb_aborted_initial_upgrade_does_not_leave_database_behind() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbInitialAbortResult = null;
        (() => {
            const name = 'browser-oxide-idb-initial-abort';
            const first = indexedDB.open(name, 4);
            const out = { firstError: null, oldVersion: null, newVersion: null, version: null, stores: null };
            first.onupgradeneeded = () => {
                first.result.createObjectStore('must-not-survive');
                first.transaction.abort();
            };
            first.onerror = () => {
                out.firstError = first.error && first.error.name;
                const verify = indexedDB.open(name);
                verify.onupgradeneeded = (event) => {
                    out.oldVersion = event.oldVersion;
                    out.newVersion = event.newVersion;
                    verify.result.createObjectStore('fresh');
                };
                verify.onsuccess = () => {
                    out.version = verify.result.version;
                    out.stores = Array.from(verify.result.objectStoreNames);
                    globalThis.__idbInitialAbortResult = out;
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbInitialAbortResult)")
        .expect("initial upgrade abort result");
    let value: serde_json::Value =
        serde_json::from_str(&result).expect("initial upgrade abort json");

    assert_eq!(value["firstError"], "AbortError");
    assert_eq!(value["oldVersion"], 0);
    assert_eq!(value["newVersion"], 1);
    assert_eq!(value["version"], 1);
    assert_eq!(value["stores"], serde_json::json!(["fresh"]));
}

#[tokio::test]
async fn indexeddb_upgrade_waits_for_open_connection_and_emits_blocked() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbBlockedUpgradeResult = null;
        (() => {
            const name = 'browser-oxide-idb-blocked-upgrade';
            const out = { seq: [], versionchange: null, blocked: null, pending: null, version: null, stores: null };
            const first = indexedDB.open(name, 1);
            first.onupgradeneeded = () => first.result.createObjectStore('v1');
            first.onsuccess = () => {
                const oldConnection = first.result;
                oldConnection.onversionchange = event => {
                    out.seq.push('versionchange');
                    out.versionchange = { old: event.oldVersion, new_: event.newVersion };
                };

                const upgrade = indexedDB.open(name, 2);
                upgrade.onblocked = event => {
                    out.seq.push('blocked');
                    out.blocked = { old: event.oldVersion, new_: event.newVersion };
                    out.pending = upgrade.readyState;
                    queueMicrotask(() => oldConnection.close());
                };
                upgrade.onupgradeneeded = () => {
                    out.seq.push('upgradeneeded');
                    upgrade.result.createObjectStore('v2');
                };
                upgrade.onsuccess = () => {
                    out.seq.push('success');
                    out.version = upgrade.result.version;
                    out.stores = Array.from(upgrade.result.objectStoreNames);
                    globalThis.__idbBlockedUpgradeResult = out;
                };
                upgrade.onerror = () => {
                    out.seq.push('error');
                    globalThis.__idbBlockedUpgradeResult = out;
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbBlockedUpgradeResult)")
        .expect("blocked upgrade result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("blocked upgrade json");

    assert_eq!(
        value["seq"],
        serde_json::json!(["versionchange", "blocked", "upgradeneeded", "success"])
    );
    assert_eq!(
        value["versionchange"],
        serde_json::json!({"old":1,"new_":2})
    );
    assert_eq!(value["blocked"], serde_json::json!({"old":1,"new_":2}));
    assert_eq!(value["pending"], "pending");
    assert_eq!(value["version"], 2);
    assert_eq!(value["stores"], serde_json::json!(["v1", "v2"]));
}

#[tokio::test]
async fn indexeddb_delete_database_waits_for_open_connection_and_emits_blocked() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbBlockedDeleteResult = null;
        (() => {
            const name = 'browser-oxide-idb-blocked-delete';
            const out = { seq: [], versionchange: null, blocked: null, pending: null, databases: null, reopenOldVersion: null };
            const first = indexedDB.open(name, 1);
            first.onupgradeneeded = () => first.result.createObjectStore('items');
            first.onsuccess = () => {
                const oldConnection = first.result;
                oldConnection.onversionchange = event => {
                    out.seq.push('versionchange');
                    out.versionchange = { old: event.oldVersion, new_: event.newVersion };
                };

                const deletion = indexedDB.deleteDatabase(name);
                deletion.onblocked = event => {
                    out.seq.push('blocked');
                    out.blocked = { old: event.oldVersion, new_: event.newVersion };
                    out.pending = deletion.readyState;
                    queueMicrotask(() => oldConnection.close());
                };
                deletion.onsuccess = async () => {
                    out.seq.push('success');
                    out.databases = (await indexedDB.databases()).map(entry => entry.name);
                    const reopen = indexedDB.open(name);
                    reopen.onupgradeneeded = event => {
                        out.reopenOldVersion = event.oldVersion;
                    };
                    reopen.onsuccess = () => {
                        globalThis.__idbBlockedDeleteResult = out;
                    };
                };
                deletion.onerror = () => {
                    out.seq.push('error');
                    globalThis.__idbBlockedDeleteResult = out;
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbBlockedDeleteResult)")
        .expect("blocked delete result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("blocked delete json");

    assert_eq!(
        value["seq"],
        serde_json::json!(["versionchange", "blocked", "success"])
    );
    assert_eq!(
        value["versionchange"],
        serde_json::json!({"old":1,"new_":null})
    );
    assert_eq!(value["blocked"], serde_json::json!({"old":1,"new_":null}));
    assert_eq!(value["pending"], "pending");
    assert_eq!(value["databases"], serde_json::json!([]));
    assert_eq!(value["reopenOldVersion"], 0);
}

#[tokio::test]
async fn indexeddb_versionchange_handler_can_close_without_blocked_event() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbImmediateCloseResult = null;
        (() => {
            const name = 'browser-oxide-idb-immediate-close';
            const out = { seq: [], blocked: false, version: null };
            const first = indexedDB.open(name, 1);
            first.onupgradeneeded = () => first.result.createObjectStore('v1');
            first.onsuccess = () => {
                const oldConnection = first.result;
                oldConnection.onversionchange = () => {
                    out.seq.push('versionchange');
                    oldConnection.close();
                };
                const upgrade = indexedDB.open(name, 2);
                upgrade.onblocked = () => {
                    out.blocked = true;
                    out.seq.push('blocked');
                };
                upgrade.onupgradeneeded = () => out.seq.push('upgradeneeded');
                upgrade.onsuccess = () => {
                    out.seq.push('success');
                    out.version = upgrade.result.version;
                    globalThis.__idbImmediateCloseResult = out;
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbImmediateCloseResult)")
        .expect("immediate close result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("immediate close json");

    assert_eq!(
        value["seq"],
        serde_json::json!(["versionchange", "upgradeneeded", "success"])
    );
    assert_eq!(value["blocked"], false);
    assert_eq!(value["version"], 2);
}

#[tokio::test]
async fn indexeddb_queued_upgrade_uses_version_after_previous_upgrade() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbQueuedUpgradeResult = null;
        (() => {
            const name = 'browser-oxide-idb-queued-upgrade';
            const out = { seq: [], secondVersionchange: null, thirdUpgrade: null, finalVersion: null };
            const first = indexedDB.open(name, 1);
            first.onupgradeneeded = () => first.result.createObjectStore('v1');
            first.onsuccess = () => {
                const db1 = first.result;
                db1.onversionchange = () => db1.close();

                const second = indexedDB.open(name, 2);
                const third = indexedDB.open(name, 3);

                second.onupgradeneeded = event => {
                    out.seq.push('upgrade-2');
                    if (event.oldVersion !== 1 || event.newVersion !== 2) {
                        out.seq.push(`bad-upgrade-2:${event.oldVersion}:${event.newVersion}`);
                    }
                };
                second.onsuccess = () => {
                    out.seq.push('success-2');
                    second.result.onversionchange = event => {
                        out.seq.push('versionchange-2');
                        out.secondVersionchange = [event.oldVersion, event.newVersion];
                        second.result.close();
                    };
                };

                third.onupgradeneeded = event => {
                    out.seq.push('upgrade-3');
                    out.thirdUpgrade = [event.oldVersion, event.newVersion];
                };
                third.onsuccess = () => {
                    out.seq.push('success-3');
                    out.finalVersion = third.result.version;
                    globalThis.__idbQueuedUpgradeResult = out;
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbQueuedUpgradeResult)")
        .expect("queued upgrade result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("queued upgrade json");

    assert_eq!(
        value["seq"],
        serde_json::json!([
            "upgrade-2",
            "success-2",
            "versionchange-2",
            "upgrade-3",
            "success-3"
        ])
    );
    assert_eq!(value["secondVersionchange"], serde_json::json!([2, 3]));
    assert_eq!(value["thirdUpgrade"], serde_json::json!([2, 3]));
    assert_eq!(value["finalVersion"], 3);
}

#[tokio::test]
async fn indexeddb_open_queued_behind_blocked_delete_recreates_database() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__idbDeleteThenOpenResult = null;
        (() => {
            const name = 'browser-oxide-idb-delete-then-open';
            const out = { seq: [], reopenOldVersion: null, reopenNewVersion: null, stores: null };
            const first = indexedDB.open(name, 1);
            first.onupgradeneeded = () => first.result.createObjectStore('old-store');
            first.onsuccess = () => {
                const oldConnection = first.result;
                oldConnection.onversionchange = () => out.seq.push('versionchange');

                const deletion = indexedDB.deleteDatabase(name);
                const reopen = indexedDB.open(name);

                deletion.onblocked = () => {
                    out.seq.push('delete-blocked');
                    queueMicrotask(() => oldConnection.close());
                };
                deletion.onsuccess = () => out.seq.push('delete-success');

                reopen.onupgradeneeded = event => {
                    out.seq.push('reopen-upgrade');
                    out.reopenOldVersion = event.oldVersion;
                    out.reopenNewVersion = event.newVersion;
                    reopen.result.createObjectStore('new-store');
                };
                reopen.onsuccess = () => {
                    out.seq.push('reopen-success');
                    out.stores = Array.from(reopen.result.objectStoreNames);
                    globalThis.__idbDeleteThenOpenResult = out;
                };
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__idbDeleteThenOpenResult)")
        .expect("delete then open result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("delete then open json");

    assert_eq!(
        value["seq"],
        serde_json::json!([
            "versionchange",
            "delete-blocked",
            "delete-success",
            "reopen-upgrade",
            "reopen-success"
        ])
    );
    assert_eq!(value["reopenOldVersion"], 0);
    assert_eq!(value["reopenNewVersion"], 1);
    assert_eq!(value["stores"], serde_json::json!(["new-store"]));
}
