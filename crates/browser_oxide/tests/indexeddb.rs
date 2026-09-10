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
