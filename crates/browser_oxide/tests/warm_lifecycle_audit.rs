//! Deterministic warm-navigation lifecycle audit.

use browser_oxide::Page;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve_once(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 4096];
        let _ = socket.read(&mut request).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });
    format!("http://{addr}/warm")
}

#[tokio::test]
async fn warm_navigation_scripts_see_loading_and_current_script() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html(
        "<!doctype html><html><body><script>globalThis.__oldPage=true</script></body></html>",
        Some(profile),
    )
    .await
    .unwrap();
    let url = serve_once(
        r#"<!doctype html><html><body>
        <script id="warm-audit" data-probe="yes">
          globalThis.__warmAudit = JSON.stringify({
            state: document.readyState,
            current: document.currentScript && document.currentScript.id,
            attr: document.currentScript && document.currentScript.getAttribute('data-probe'),
            oldPage: typeof globalThis.__oldPage
          });
        </script>
        </body></html>"#,
    )
    .await;

    page.navigate_warm(&url).await.unwrap();
    assert_eq!(
        page.evaluate("globalThis.__warmAudit").unwrap(),
        r#"{"state":"loading","current":"warm-audit","attr":"yes","oldPage":"undefined"}"#
    );
    assert_eq!(page.evaluate("document.readyState").unwrap(), "complete");
    assert_eq!(
        page.evaluate("String(document.currentScript)").unwrap(),
        "null"
    );
}
