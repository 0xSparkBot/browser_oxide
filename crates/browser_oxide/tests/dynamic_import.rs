use browser_oxide::Page;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_blob_data_scheme_and_cache_match_chrome_148() {
    let html = r#"<!doctype html><html><body><script type="module">
    const r = {};
    const attempt = async (k, f) => {
      try { const m = await f(); r[k] = ['ok', m.default ?? null]; }
      catch (e) { r[k] = ['err', e.name]; }
    };

    await attempt('dataPlain', () => import('data:text/plain,export%20default%201'));
    await attempt('dataJs', () => import('data:application/javascript,export%20default%202'));

    const plainBlob = URL.createObjectURL(new Blob(['export default 3'], {type:'text/plain'}));
    await attempt('blobPlain', () => import(plainBlob));
    URL.revokeObjectURL(plainBlob);

    const emptyBlob = URL.createObjectURL(new Blob(['export default 4']));
    await attempt('blobEmpty', () => import(emptyBlob));
    URL.revokeObjectURL(emptyBlob);

    const jsBlob = URL.createObjectURL(new Blob([
      'globalThis.__blobEval=(globalThis.__blobEval||0)+1; export default 5'
    ], {type:'application/javascript'}));
    await attempt('blobJs', () => import(jsBlob));
    r.blobEval = globalThis.__blobEval;
    URL.revokeObjectURL(jsBlob);

    const revokedBlob = URL.createObjectURL(
      new Blob(['export default 6'], {type:'text/javascript'})
    );
    URL.revokeObjectURL(revokedBlob);
    await attempt('blobRevoked', () => import(revokedBlob));

    await attempt('ftp', () => import('ftp://example.test/nope.js'));

    const duplicate =
      'data:text/javascript,globalThis.__dupEval=(globalThis.__dupEval||0)+1%3Bexport%20default%201';
    await import(duplicate);
    await import(duplicate);
    r.dup = globalThis.__dupEval;

    globalThis.__dynamicImportResult = r;
    </script></body></html>"#;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page =
        Page::from_html_with_url(html, "https://example.test/dynamic-import", Some(profile))
            .await
            .expect("page");

    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__dynamicImportResult)")
            .unwrap(),
        r#"{"dataPlain":["err","TypeError"],"dataJs":["ok",2],"blobPlain":["err","TypeError"],"blobEmpty":["err","TypeError"],"blobJs":["ok",5],"blobEval":1,"blobRevoked":["err","TypeError"],"ftp":["err","TypeError"],"dup":1}"#
    );
}

async fn spawn_module_server() -> (String, tokio::task::JoinHandle<()>) {
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
                let request = String::from_utf8_lossy(&request[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let (content_type, body) = match path {
                    "/good.js" => ("application/javascript", "export default 11"),
                    "/bad.js" => ("text/plain", "export default 12"),
                    _ => ("text/plain", "not found"),
                };
                let status = if path == "/good.js" || path == "/bad.js" {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), task)
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_http_uses_strict_javascript_mime_checking() {
    let (origin, server) = spawn_module_server().await;
    let html = format!(
        r#"<!doctype html><html><body><script type="module">
        const r = {{}};
        try {{ r.good = (await import('{origin}/good.js')).default; }}
        catch (e) {{ r.good = 'ERR:' + e.name; }}
        try {{ await import('{origin}/bad.js'); r.bad = 'resolved'; }}
        catch (e) {{ r.bad = 'ERR:' + e.name; }}
        globalThis.__httpModuleMime = r;
        </script></body></html>"#
    );
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(&html, &format!("{origin}/page"), Some(profile))
        .await
        .expect("page");

    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__httpModuleMime)")
            .unwrap(),
        r#"{"good":11,"bad":"ERR:TypeError"}"#
    );
    server.abort();
}
