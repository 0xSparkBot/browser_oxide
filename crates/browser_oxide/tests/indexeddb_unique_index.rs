use browser_oxide::Page;

async fn page(html: &str) -> Page {
    Page::from_html_with_url(
        html,
        "https://example.test/indexeddb-unique-index",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("page")
}

#[tokio::test]
async fn unique_index_conflict_errors_without_forcing_abort_when_canceled() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__result = null;
        const name = 'browser-oxide-idb-unique-index-standalone';
        const out = { seq: [] };
        const open = indexedDB.open(name, 1);
        open.onupgradeneeded = () => {
            const store = open.result.createObjectStore('items', { keyPath: 'id' });
            store.createIndex('email', 'email', { unique: true });
        };
        open.onsuccess = () => {
            const db = open.result;
            const tx = db.transaction('items', 'readwrite');
            const store = tx.objectStore('items');
            store.add({ id: 1, email: 'a@example.test' });
            const duplicate = store.add({ id: 2, email: 'a@example.test' });
            duplicate.onerror = event => {
                out.error = duplicate.error && duplicate.error.name;
                out.seq.push('duplicate-error');
                event.preventDefault();
                event.stopPropagation();
            };
            tx.oncomplete = () => {
                const verifyTx = db.transaction('items');
                const verify = verifyTx.objectStore('items').getAll();
                verify.onsuccess = () => { out.rows = verify.result; };
                verifyTx.oncomplete = () => {
                    out.seq.push('complete');
                    globalThis.__result = out;
                    db.close();
                    indexedDB.deleteDatabase(name);
                };
            };
            tx.onabort = () => {
                out.seq.push('abort');
                globalThis.__result = out;
            };
        };
    </script></body></html>"#;

    let mut page = page(html).await;
    let value: serde_json::Value = serde_json::from_str(
        &page
            .evaluate("JSON.stringify(globalThis.__result)")
            .expect("result"),
    )
    .expect("json");

    assert_eq!(value["error"], "ConstraintError");
    assert_eq!(
        value["seq"],
        serde_json::json!(["duplicate-error", "complete"])
    );
    assert_eq!(
        value["rows"],
        serde_json::json!([{ "id": 1, "email": "a@example.test" }])
    );
}

#[tokio::test]
async fn unique_index_allows_put_to_replace_the_same_primary_key() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__result = null;
        const name = 'browser-oxide-idb-unique-index-self-update';
        const open = indexedDB.open(name, 1);
        open.onupgradeneeded = () => {
            const store = open.result.createObjectStore('items', { keyPath: 'id' });
            store.createIndex('email', 'email', { unique: true });
        };
        open.onsuccess = () => {
            const db = open.result;
            const tx = db.transaction('items', 'readwrite');
            const store = tx.objectStore('items');
            store.add({ id: 1, email: 'a@example.test', version: 1 });
            const update = store.put({ id: 1, email: 'a@example.test', version: 2 });
            update.onerror = () => {
                globalThis.__result = { error: update.error && update.error.name };
            };
            tx.oncomplete = () => {
                const verify = db.transaction('items').objectStore('items').get(1);
                verify.onsuccess = () => {
                    globalThis.__result = { row: verify.result };
                    db.close();
                    indexedDB.deleteDatabase(name);
                };
            };
        };
    </script></body></html>"#;

    let mut page = page(html).await;
    let value: serde_json::Value = serde_json::from_str(
        &page
            .evaluate("JSON.stringify(globalThis.__result)")
            .expect("result"),
    )
    .expect("json");

    assert!(value.get("error").is_none(), "unexpected error: {value}");
    assert_eq!(
        value["row"],
        serde_json::json!({ "id": 1, "email": "a@example.test", "version": 2 })
    );
}

#[tokio::test]
async fn invalid_index_keys_are_ignored_instead_of_rejecting_the_store_write() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__result = null;
        const name = 'browser-oxide-idb-invalid-index-key';
        const open = indexedDB.open(name, 1);
        open.onupgradeneeded = () => {
            const store = open.result.createObjectStore('items', { keyPath: 'id' });
            store.createIndex('email', 'email', { unique: true });
        };
        open.onsuccess = () => {
            const db = open.result;
            const tx = db.transaction('items', 'readwrite');
            const store = tx.objectStore('items');
            const first = store.add({ id: 1, email: { invalid: true } });
            const second = store.add({ id: 2, email: { invalid: true } });
            store.add({ id: 3, email: 'valid@example.test' });
            const out = { errors: [] };
            first.onerror = () => out.errors.push(first.error && first.error.name);
            second.onerror = () => out.errors.push(second.error && second.error.name);
            tx.oncomplete = () => {
                const verifyTx = db.transaction('items');
                const verifyStore = verifyTx.objectStore('items');
                const rows = verifyStore.getAll();
                const indexed = verifyStore.index('email').count();
                verifyTx.oncomplete = () => {
                    out.rows = rows.result;
                    out.indexCount = indexed.result;
                    globalThis.__result = out;
                    db.close();
                    indexedDB.deleteDatabase(name);
                };
            };
            tx.onabort = () => {
                out.aborted = true;
                globalThis.__result = out;
            };
        };
    </script></body></html>"#;

    let mut page = page(html).await;
    let value: serde_json::Value = serde_json::from_str(
        &page
            .evaluate("JSON.stringify(globalThis.__result)")
            .expect("result"),
    )
    .expect("json");

    assert_eq!(value["errors"], serde_json::json!([]));
    assert_ne!(value["aborted"], true);
    assert_eq!(value["rows"].as_array().map(Vec::len), Some(3));
    assert_eq!(value["indexCount"], 1);
}

#[tokio::test]
async fn multientry_index_deduplicates_one_record_and_rejects_cross_record_conflicts() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__result = null;
        const name = 'browser-oxide-idb-unique-multientry';
        const out = { seq: [] };
        const open = indexedDB.open(name, 1);
        open.onupgradeneeded = () => {
            const store = open.result.createObjectStore('items', { keyPath: 'id' });
            store.createIndex('tags', 'tags', { unique: true, multiEntry: true });
        };
        open.onsuccess = () => {
            const db = open.result;
            const tx = db.transaction('items', 'readwrite');
            const store = tx.objectStore('items');
            const first = store.add({ id: 1, tags: ['a', 'a', 'b'] });
            const duplicate = store.add({ id: 2, tags: ['c', 'a'] });
            first.onsuccess = () => out.seq.push('first-ok');
            duplicate.onerror = event => {
                out.error = duplicate.error && duplicate.error.name;
                out.seq.push('duplicate-error');
                event.preventDefault();
                event.stopPropagation();
            };
            tx.oncomplete = () => {
                const verifyTx = db.transaction('items');
                const index = verifyTx.objectStore('items').index('tags');
                const count = index.count();
                verifyTx.oncomplete = () => {
                    out.count = count.result;
                    out.seq.push('complete');
                    globalThis.__result = out;
                    db.close();
                    indexedDB.deleteDatabase(name);
                };
            };
        };
    </script></body></html>"#;

    let mut page = page(html).await;
    let value: serde_json::Value = serde_json::from_str(
        &page
            .evaluate("JSON.stringify(globalThis.__result)")
            .expect("result"),
    )
    .expect("json");

    assert_eq!(value["error"], "ConstraintError");
    assert_eq!(value["count"], 2);
    assert_eq!(
        value["seq"],
        serde_json::json!(["first-ok", "duplicate-error", "complete"])
    );
}

#[tokio::test]
async fn multientry_rejects_compound_key_path() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__result = null;
        const name = 'browser-oxide-idb-multientry-compound';
        const open = indexedDB.open(name, 1);
        open.onupgradeneeded = () => {
            const store = open.result.createObjectStore('items', { keyPath: 'id' });
            try {
                store.createIndex('bad', ['first', 'last'], { multiEntry: true });
                globalThis.__result = 'no-throw';
            } catch (error) {
                globalThis.__result = error.name;
            }
        };
        open.onsuccess = () => {
            open.result.close();
            indexedDB.deleteDatabase(name);
        };
    </script></body></html>"#;

    let mut page = page(html).await;
    assert_eq!(
        page.evaluate("String(globalThis.__result)")
            .expect("result"),
        "InvalidAccessError"
    );
}
