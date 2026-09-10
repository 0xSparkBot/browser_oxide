use browser_oxide::Page;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

async fn spawn_module_header_server() -> (
    String,
    mpsc::UnboundedReceiver<(String, HashMap<String, String>)>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut request = [0_u8; 8192];
                let Ok(n) = socket.read(&mut request).await else {
                    return;
                };
                let text = String::from_utf8_lossy(&request[..n]);
                let mut lines = text.lines();
                let path = lines
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let headers = lines
                    .take_while(|line| !line.is_empty())
                    .filter_map(|line| line.split_once(':'))
                    .map(|(name, value)| {
                        (name.trim().to_ascii_lowercase(), value.trim().to_string())
                    })
                    .collect::<HashMap<_, _>>();
                let _ = tx.send((path.clone(), headers));

                let (body, content_type) = match path.as_str() {
                    "/" => (
                        r#"<!doctype html><script src="/classic.js"></script><script type="module" src="/entry.js"></script>"#,
                        "text/html",
                    ),
                    "/classic.js" => ("globalThis.__classicLoaded = true;", "text/javascript"),
                    "/entry.js" => (
                        "import './dep.js'; globalThis.__entryLoaded = true;",
                        "text/javascript",
                    ),
                    "/dep.js" => ("export const value = 1;", "text/javascript"),
                    _ => ("not found", "text/plain"),
                };
                let status = if body == "not found" {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), rx, task)
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_executes_blob_url_module_source() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="module">
            const source = new Blob(['export default 42;'], { type: 'text/javascript' });
            const url = URL.createObjectURL(source);
            try {
                const namespace = await import(url);
                globalThis.__blobModuleResult = String(namespace.default);
            } catch (error) {
                globalThis.__blobModuleResult = 'ERR:' + error.name + ':' + error.message;
            } finally {
                URL.revokeObjectURL(url);
            }
        </script>
    </body></html>"#;

    let mut page =
        Page::from_html_with_url(html, "https://example.test/blob-module/", Some(profile))
            .await
            .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__blobModuleResult")
            .expect("blob module result"),
        "42"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_rejects_revoked_blob_url() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="module">
            const source = new Blob(['export default 7;'], { type: 'text/javascript' });
            const url = URL.createObjectURL(source);
            URL.revokeObjectURL(url);
            try {
                await import(url);
                globalThis.__revokedBlobImport = 'resolved';
            } catch (_) {
                globalThis.__revokedBlobImport = 'rejected';
            }
        </script>
    </body></html>"#;

    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/revoked-blob-module/",
        Some(profile),
    )
    .await
    .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__revokedBlobImport")
            .expect("revoked blob module result"),
        "rejected"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_uses_document_import_map() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="importmap">
            {"imports":{"mapped-dynamic":"data:text/javascript,export default 99"}}
        </script>
        <script type="module">
            const namespace = await import('mapped-dynamic');
            globalThis.__mappedDynamicImport = String(namespace.default);
        </script>
    </body></html>"#;

    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/dynamic-import-map/",
        Some(profile),
    )
    .await
    .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__mappedDynamicImport")
            .expect("mapped dynamic import result"),
        "99"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_rejects_unsupported_module_scheme() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="module">
            try {
                await import('about:blank');
                globalThis.__unsupportedModuleImport = 'resolved';
            } catch (_) {
                globalThis.__unsupportedModuleImport = 'rejected';
            }
        </script>
    </body></html>"#;

    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/unsupported-module/",
        Some(profile),
    )
    .await
    .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__unsupportedModuleImport")
            .expect("unsupported module result"),
        "rejected"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn script_fetch_metadata_matches_browser_subresource_semantics() {
    let (base, mut requests, server) = spawn_module_header_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::navigate(&format!("{base}/"), profile, 1)
        .await
        .expect("navigate");
    assert_eq!(page.evaluate("String(__classicLoaded)").unwrap(), "true");
    assert_eq!(page.evaluate("String(__entryLoaded)").unwrap(), "true");

    let mut by_path = HashMap::new();
    while let Ok((path, headers)) = requests.try_recv() {
        by_path.insert(path, headers);
    }

    let page_url = format!("{base}/");
    let entry_url = format!("{base}/entry.js");

    let classic = by_path.get("/classic.js").expect("classic request");
    assert_eq!(
        classic.get("sec-fetch-mode").map(String::as_str),
        Some("no-cors")
    );
    assert_eq!(
        classic.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(
        classic.get("sec-fetch-site").map(String::as_str),
        Some("same-origin")
    );
    assert_eq!(
        classic.get("referer").map(String::as_str),
        Some(page_url.as_str())
    );
    assert!(!classic.contains_key("origin"));
    assert!(!classic.contains_key("sec-fetch-user"));
    assert!(!classic.contains_key("upgrade-insecure-requests"));

    let entry = by_path.get("/entry.js").expect("module entry request");
    assert_eq!(
        entry.get("sec-fetch-mode").map(String::as_str),
        Some("cors")
    );
    assert_eq!(
        entry.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(
        entry.get("sec-fetch-site").map(String::as_str),
        Some("same-origin")
    );
    assert_eq!(entry.get("origin").map(String::as_str), Some(base.as_str()));
    assert_eq!(
        entry.get("referer").map(String::as_str),
        Some(page_url.as_str())
    );
    assert!(!entry.contains_key("sec-fetch-user"));
    assert!(!entry.contains_key("upgrade-insecure-requests"));

    let dep = by_path.get("/dep.js").expect("module dependency request");
    assert_eq!(dep.get("sec-fetch-mode").map(String::as_str), Some("cors"));
    assert_eq!(
        dep.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(
        dep.get("sec-fetch-site").map(String::as_str),
        Some("same-origin")
    );
    assert_eq!(dep.get("origin").map(String::as_str), Some(base.as_str()));
    assert_eq!(
        dep.get("referer").map(String::as_str),
        Some(entry_url.as_str())
    );
    assert!(!dep.contains_key("sec-fetch-user"));
    assert!(!dep.contains_key("upgrade-insecure-requests"));

    server.abort();
}
