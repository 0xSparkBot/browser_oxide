use browser_oxide::Page;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_import_map_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut request = [0_u8; 4096];
                let Ok(n) = socket.read(&mut request).await else {
                    return;
                };
                let text = String::from_utf8_lossy(&request[..n]);
                let path = text
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let (content_type, body) = match path {
                    "/exact/" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><script type="importmap">{"imports":{"pkg":"/modules/exact.js"}}</script><script type="module">import value from 'pkg'; globalThis.__mapped = value;</script>"#,
                    ),
                    "/prefix/" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><script type="importmap">{"imports":{"pkg/":"/modules/pkg/"}}</script><script type="module">import value from 'pkg/tool.js'; globalThis.__mapped = value;</script>"#,
                    ),
                    "/scope/app/" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><script type="importmap">{"imports":{"dep":"/modules/global.js"},"scopes":{"/scope/app/":{"dep":"/modules/scoped.js"}}}</script><script type="module">import value from 'dep'; globalThis.__mapped = value;</script>"#,
                    ),
                    "/warm-one/" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><script type="importmap">{"imports":{"dep":"/modules/one.js"}}</script><script type="module">import value from 'dep'; globalThis.__mapped = value;</script>"#,
                    ),
                    "/warm-two/" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><script type="importmap">{"imports":{"dep":"/modules/two.js"}}</script><script type="module">import value from 'dep'; globalThis.__mapped = value;</script>"#,
                    ),
                    "/base-map/" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><base href="/assets/"><script type="importmap">{"imports":{"dep":"./mapped.js"}}</script><script type="module">import value from 'dep'; globalThis.__mapped = value;</script>"#,
                    ),
                    "/modules/exact.js" => ("text/javascript", "export default 'exact';"),
                    "/modules/pkg/tool.js" => ("text/javascript", "export default 'prefix';"),
                    "/modules/global.js" => ("text/javascript", "export default 'global';"),
                    "/modules/scoped.js" => ("text/javascript", "export default 'scoped';"),
                    "/modules/one.js" => ("text/javascript", "export default 'one';"),
                    "/modules/two.js" => ("text/javascript", "export default 'two';"),
                    "/assets/mapped.js" => ("text/javascript", "export default 'base';"),
                    "/base-map/mapped.js" => ("text/javascript", "export default 'document-url';"),
                    _ => ("text/plain; charset=utf-8", "not found"),
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
    (format!("http://{addr}"), task)
}

#[tokio::test(flavor = "current_thread")]
async fn synthetic_document_import_map_resolves_bare_specifier() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html>
        <script type="importmap">{"imports":{"pkg":"data:text/javascript,export default 41"}}</script>
        <script type="module">import value from 'pkg'; globalThis.__mapped = value + 1;</script>"#;
    let mut page = Page::from_html_with_url(html, "https://example.test/app/", Some(profile))
        .await
        .expect("page");
    assert_eq!(page.evaluate("String(globalThis.__mapped)").unwrap(), "42");
}

#[tokio::test(flavor = "current_thread")]
async fn network_document_import_map_resolves_exact_prefix_and_scope() {
    let (base, server) = spawn_import_map_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();

    for (path, expected) in [
        ("/exact/", "exact"),
        ("/prefix/", "prefix"),
        ("/scope/app/", "scoped"),
    ] {
        let mut page = Page::navigate(&format!("{base}{path}"), profile.clone(), 1)
            .await
            .expect("navigate");
        assert_eq!(
            page.evaluate("String(globalThis.__mapped)").unwrap(),
            expected,
            "import map resolution failed for {path}"
        );
    }

    server.abort();
}

#[tokio::test(flavor = "current_thread")]
async fn warm_navigation_replaces_import_map_state() {
    let (base, server) = spawn_import_map_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::navigate(&format!("{base}/warm-one/"), profile, 1)
        .await
        .expect("first navigate");
    assert_eq!(page.evaluate("String(globalThis.__mapped)").unwrap(), "one");

    page.navigate_warm(&format!("{base}/warm-two/"))
        .await
        .expect("warm navigate");
    assert_eq!(page.evaluate("String(globalThis.__mapped)").unwrap(), "two");

    server.abort();
}

#[tokio::test(flavor = "current_thread")]
async fn import_map_addresses_resolve_against_document_base_url() {
    let (base, server) = spawn_import_map_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::navigate(&format!("{base}/base-map/"), profile, 1)
        .await
        .expect("navigate");
    assert_eq!(
        page.evaluate("String(globalThis.__mapped)").unwrap(),
        "base"
    );
    server.abort();
}
