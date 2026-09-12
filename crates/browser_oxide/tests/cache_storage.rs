use browser_oxide::Page;
use std::io::{Read, Write};
use std::net::TcpListener as StdTcpListener;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn drive_until(page: &mut Page, expression: &str) {
    for _ in 0..60 {
        let _ = page.evaluate_async("void 0", Duration::from_millis(50)).await;
        if page.evaluate(expression).unwrap_or_default() != "pending" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {expression}");
}

#[tokio::test(flavor = "current_thread")]
async fn cache_storage_put_match_keys_delete_round_trip() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://cache.example.test/page",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"
        globalThis.__cacheRoundTrip = 'pending';
        (async () => {
            try {
                let cacheCtor = '', storageCtor = '';
                try { new Cache(); } catch (e) { cacheCtor = e.name + ':' + e.message; }
                try { new CacheStorage(); } catch (e) { storageCtor = e.name + ':' + e.message; }

                const first = await caches.open('alpha');
                const second = await caches.open('alpha');
                await first.put('/item?q=1', new Response('hello-cache', {
                    status: 201,
                    statusText: 'Created',
                    headers: { 'content-type': 'text/plain', 'x-cache-test': 'yes' },
                }));

                const exact = await second.match('/item?q=1');
                const ignoreSearch = await second.match('/item?q=999', { ignoreSearch: true });
                const storageHit = await caches.match('/item?q=1');
                const keys = await second.keys();
                const all = await second.matchAll();
                const names = await caches.keys();
                const beforeDelete = await caches.has('alpha');
                const deletedEntry = await second.delete('/item?q=1');
                const afterEntry = await second.match('/item?q=1');
                const deletedCache = await caches.delete('alpha');
                const afterCache = await caches.has('alpha');

                globalThis.__cacheRoundTrip = JSON.stringify({
                    cacheCtor,
                    storageCtor,
                    cacheTag: Object.prototype.toString.call(first),
                    storageTag: Object.prototype.toString.call(caches),
                    cacheProtoKeys: Reflect.ownKeys(Cache.prototype).map(String),
                    storageProtoKeys: Reflect.ownKeys(CacheStorage.prototype).map(String),
                    methodLengths: {
                        add: Cache.prototype.add.length,
                        addAll: Cache.prototype.addAll.length,
                        delete: Cache.prototype.delete.length,
                        keys: Cache.prototype.keys.length,
                        match: Cache.prototype.match.length,
                        matchAll: Cache.prototype.matchAll.length,
                        put: Cache.prototype.put.length,
                        storageDelete: CacheStorage.prototype.delete.length,
                        storageHas: CacheStorage.prototype.has.length,
                        storageKeys: CacheStorage.prototype.keys.length,
                        storageMatch: CacheStorage.prototype.match.length,
                        storageOpen: CacheStorage.prototype.open.length,
                    },
                    exactStatus: exact && exact.status,
                    exactStatusText: exact && exact.statusText,
                    exactHeader: exact && exact.headers.get('x-cache-test'),
                    exactText: exact && await exact.text(),
                    ignoreSearchText: ignoreSearch && await ignoreSearch.text(),
                    storageText: storageHit && await storageHit.text(),
                    keyUrls: keys.map(r => r.url),
                    allCount: all.length,
                    names,
                    beforeDelete,
                    deletedEntry,
                    afterEntry: afterEntry === undefined,
                    deletedCache,
                    afterCache,
                });
            } catch (e) {
                globalThis.__cacheRoundTrip = JSON.stringify({ error: String(e && e.stack || e) });
            }
        })();
        "#,
    )
    .unwrap();
    drive_until(&mut page, "globalThis.__cacheRoundTrip").await;

    let raw = page.evaluate("globalThis.__cacheRoundTrip").unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("invalid cache result: {e}; raw={raw}");
    });
    assert_eq!(value["error"], serde_json::Value::Null, "{raw}");
    assert!(value["cacheCtor"].as_str().unwrap().contains("Illegal constructor"));
    assert!(value["storageCtor"].as_str().unwrap().contains("Illegal constructor"));
    assert_eq!(value["cacheTag"], "[object Cache]");
    assert_eq!(value["storageTag"], "[object CacheStorage]");
    assert_eq!(
        value["cacheProtoKeys"],
        serde_json::json!([
            "add", "addAll", "delete", "keys", "match", "matchAll", "put",
            "constructor", "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["storageProtoKeys"],
        serde_json::json!([
            "delete", "has", "keys", "match", "open", "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["methodLengths"],
        serde_json::json!({
            "add": 1, "addAll": 1, "delete": 1, "keys": 0,
            "match": 1, "matchAll": 0, "put": 2,
            "storageDelete": 1, "storageHas": 1, "storageKeys": 0,
            "storageMatch": 1, "storageOpen": 1
        })
    );
    assert_eq!(value["exactStatus"], 201);
    assert_eq!(value["exactStatusText"], "Created");
    assert_eq!(value["exactHeader"], "yes");
    assert_eq!(value["exactText"], "hello-cache");
    assert_eq!(value["ignoreSearchText"], "hello-cache");
    assert_eq!(value["storageText"], "hello-cache");
    assert_eq!(value["allCount"], 1);
    assert_eq!(value["names"], serde_json::json!(["alpha"]));
    assert_eq!(value["beforeDelete"], true);
    assert_eq!(value["deletedEntry"], true);
    assert_eq!(value["afterEntry"], true);
    assert_eq!(value["deletedCache"], true);
    assert_eq!(value["afterCache"], false);
    let key_urls = value["keyUrls"].as_array().unwrap();
    assert_eq!(key_urls.len(), 1);
    assert_eq!(key_urls[0], "https://cache.example.test/item?q=1");
}

async fn spawn_cache_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let handle = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = Vec::new();
            let mut buf = [0_u8; 2048];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 { break; }
                data.extend_from_slice(&buf[..n]);
                if data.windows(4).any(|w| w == b"\r\n\r\n") { break; }
            }
            let request = String::from_utf8_lossy(&data);
            let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");
            let body = match path {
                "/one" => "one-body",
                "/two" => "two-body",
                "/three" => "three-body",
                _ => "unknown",
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-Path: {path}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            let _ = socket.shutdown().await;
        }
    });
    (base, handle)
}

#[tokio::test(flavor = "current_thread")]
async fn cache_add_and_add_all_fetch_then_store_atomically() {
    let (base, server) = spawn_cache_server().await;
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &format!("{base}/page"),
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(&format!(
        r#"
        globalThis.__cacheAddResult = 'pending';
        (async () => {{
            try {{
                const cache = await caches.open('network');
                await cache.add({base:?} + '/one');
                await cache.addAll([{base:?} + '/two', {base:?} + '/three']);
                const one = await cache.match({base:?} + '/one');
                const two = await cache.match({base:?} + '/two');
                const three = await cache.match({base:?} + '/three');
                globalThis.__cacheAddResult = JSON.stringify({{
                    texts: [await one.text(), await two.text(), await three.text()],
                    headers: [one.headers.get('x-path'), two.headers.get('x-path'), three.headers.get('x-path')],
                    keys: (await cache.keys()).map(r => r.url),
                }});
            }} catch (e) {{
                globalThis.__cacheAddResult = JSON.stringify({{ error: String(e && e.stack || e) }});
            }}
        }})();
        "#,
    ))
    .unwrap();
    drive_until(&mut page, "globalThis.__cacheAddResult").await;
    let raw = page.evaluate("globalThis.__cacheAddResult").unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["error"], serde_json::Value::Null, "{raw}");
    assert_eq!(value["texts"], serde_json::json!(["one-body", "two-body", "three-body"]));
    assert_eq!(value["headers"], serde_json::json!(["/one", "/two", "/three"]));
    assert_eq!(value["keys"].as_array().unwrap().len(), 3);

    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("cache test server did not finish")
        .expect("cache test server task failed");
}

#[tokio::test(flavor = "current_thread")]
async fn dedicated_worker_cache_put_and_match_round_trip() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body><div id=out></div></body></html>",
        "https://worker-cache.example.test/page",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"
        const workerCode = `
            self.onmessage = async () => {
                try {
                    const cache = await caches.open('worker-cache');
                    await cache.put('https://worker-cache.example.test/data',
                        new Response('from-worker', { headers: { 'x-worker': 'yes' } }));
                    const hit = await cache.match('https://worker-cache.example.test/data');
                    self.postMessage(JSON.stringify({
                        tag: Object.prototype.toString.call(cache),
                        text: await hit.text(),
                        header: hit.headers.get('x-worker'),
                        keys: (await cache.keys()).map(r => r.url),
                    }));
                } catch (e) {
                    self.postMessage(JSON.stringify({ error: String(e && e.stack || e) }));
                }
            };
        `;
        const url = URL.createObjectURL(new Blob([workerCode], { type: 'text/javascript' }));
        const worker = new Worker(url);
        globalThis.__workerCacheResult = 'pending';
        worker.onmessage = e => { globalThis.__workerCacheResult = e.data; worker.terminate(); URL.revokeObjectURL(url); };
        worker.postMessage('go');
        "#,
    )
    .unwrap();
    drive_until(&mut page, "globalThis.__workerCacheResult").await;
    let raw = page.evaluate("globalThis.__workerCacheResult").unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["error"], serde_json::Value::Null, "{raw}");
    assert_eq!(value["tag"], "[object Cache]");
    assert_eq!(value["text"], "from-worker");
    assert_eq!(value["header"], "yes");
    assert_eq!(
        value["keys"],
        serde_json::json!(["https://worker-cache.example.test/data"])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn window_and_dedicated_worker_share_same_origin_cache_storage() {
    // Worker script resolution is synchronous from the page's JS thread.
    // Serve this fixture from an OS thread rather than a Tokio task on the
    // same current-thread runtime, otherwise the test deadlocks itself while
    // new Worker() waits for the script response.
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let worker_source = r#"
        self.onmessage = async () => {
            try {
                const cache = await caches.open('shared-cache');
                const fromPage = await cache.match('/from-page');
                await cache.put('/from-worker', new Response('worker-value', {
                    headers: { 'x-owner': 'worker' },
                }));
                self.postMessage(JSON.stringify({
                    pageText: fromPage ? await fromPage.text() : null,
                    names: await caches.keys(),
                }));
            } catch (e) {
                self.postMessage(JSON.stringify({ error: String(e && e.stack || e) }));
            }
        };
    "#
    .to_string();
    let worker_server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buf = [0_u8; 2048];
        loop {
            let n = socket.read(&mut buf).unwrap();
            if n == 0 { break; }
            request.extend_from_slice(&buf[..n]);
            if request.windows(4).any(|w| w == b"\r\n\r\n") { break; }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            worker_source.len(), worker_source
        );
        socket.write_all(response.as_bytes()).unwrap();
        let _ = socket.shutdown(std::net::Shutdown::Both);
    });

    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &format!("{base}/page"),
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(&format!(
        r#"
        globalThis.__crossRealmCache = 'pending';
        (async () => {{
            try {{
                const cache = await caches.open('shared-cache');
                await cache.put('/from-page', new Response('page-value', {{
                    headers: {{ 'x-owner': 'page' }},
                }}));
                const worker = new Worker({worker_url:?});
                worker.onmessage = async (event) => {{
                    try {{
                        const workerResult = JSON.parse(event.data);
                        const fromWorker = await cache.match('/from-worker');
                        globalThis.__crossRealmCache = JSON.stringify({{
                            workerResult,
                            workerText: fromWorker ? await fromWorker.text() : null,
                            workerHeader: fromWorker ? fromWorker.headers.get('x-owner') : null,
                            pageNames: await caches.keys(),
                        }});
                    }} catch (e) {{
                        globalThis.__crossRealmCache = JSON.stringify({{ error: String(e && e.stack || e) }});
                    }} finally {{ worker.terminate(); }}
                }};
                worker.onerror = (event) => {{
                    globalThis.__crossRealmCache = JSON.stringify({{ error: String(event && event.message || 'worker error') }});
                }};
                worker.postMessage('go');
            }} catch (e) {{
                globalThis.__crossRealmCache = JSON.stringify({{ error: String(e && e.stack || e) }});
            }}
        }})();
        "#,
        worker_url = format!("{base}/worker.js"),
    ))
    .unwrap();
    drive_until(&mut page, "globalThis.__crossRealmCache").await;
    let raw = page.evaluate("globalThis.__crossRealmCache").unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["error"], serde_json::Value::Null, "{raw}");
    assert_eq!(value["workerResult"]["error"], serde_json::Value::Null, "{raw}");
    assert_eq!(value["workerResult"]["pageText"], "page-value", "{raw}");
    assert_eq!(value["workerResult"]["names"], serde_json::json!(["shared-cache"]), "{raw}");
    assert_eq!(value["workerText"], "worker-value", "{raw}");
    assert_eq!(value["workerHeader"], "worker", "{raw}");
    assert_eq!(value["pageNames"], serde_json::json!(["shared-cache"]), "{raw}");

    worker_server.join().expect("worker script server task failed");
}
