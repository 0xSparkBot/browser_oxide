//! Fetch CORS response-gate parity.

use browser_oxide::Page;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn cors_endpoint(extra_headers: &str) -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let headers = extra_headers.to_string();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).await;
        let body = "cors-ok";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });
    (
        format!("http://127.0.0.1:{port}/resource"),
        format!("http://localhost:{port}/page"),
    )
}

async fn run_window_fetch(extra_headers: &str, init: &str) -> String {
    let (target, page_url) = cors_endpoint(extra_headers).await;
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    page.evaluate(&format!(
        r#"globalThis.__corsResult = 'pending';
        fetch({target:?}, {init}).then(async response => {{
            globalThis.__corsResult = response.status + ':' + await response.text();
        }}, error => {{
            globalThis.__corsResult = error.name + ':' + error.message;
        }});"#,
    ))
    .unwrap();
    for _ in 0..30 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(100))
            .await;
        let result = page.evaluate("globalThis.__corsResult").unwrap();
        if result != "pending" {
            return result;
        }
    }
    page.evaluate("globalThis.__corsResult").unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn cross_origin_cors_fetch_rejects_without_allow_origin() {
    assert_eq!(
        run_window_fetch("", r#"{ mode: "cors", credentials: "omit" }"#).await,
        "TypeError:Failed to fetch"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cross_origin_cors_fetch_accepts_explicit_allow_origin() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).await;
        let body = "cors-ok";
        let response = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: http://localhost:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });
    let target = format!("http://127.0.0.1:{port}/resource");
    let page_url = format!("http://localhost:{port}/page");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    page.evaluate(&format!(
        r#"globalThis.__corsResult = 'pending';
        fetch({target:?}, {{ mode: 'cors', credentials: 'omit' }}).then(async response => {{
            globalThis.__corsResult = response.status + ':' + await response.text();
        }}, error => {{ globalThis.__corsResult = error.name + ':' + error.message; }});"#,
    ))
    .unwrap();
    for _ in 0..30 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(100))
            .await;
        if !matches!(
            page.evaluate("globalThis.__corsResult").as_deref(),
            Ok("pending")
        ) {
            break;
        }
    }
    assert_eq!(
        page.evaluate("globalThis.__corsResult").unwrap(),
        "200:cors-ok"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cors_wildcard_rejects_credentials_include() {
    assert_eq!(
        run_window_fetch(
            "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\n",
            r#"{ mode: "cors", credentials: "include" }"#,
        )
        .await,
        "TypeError:Failed to fetch"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn challenge_worker_observes_cors_network_error() {
    let (target, page_url) = cors_endpoint("").await;
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    page.evaluate(&format!(
        r#"globalThis.__workerCorsResult = 'pending';
        const source = `fetch({target:?}, {{ mode: 'cors', credentials: 'omit' }})
            .then(response => postMessage('ok:' + response.status))
            .catch(error => postMessage(error.name + ':' + error.message));`;
        const worker = new Worker(URL.createObjectURL(new Blob([source])));
        worker.onmessage = event => {{ globalThis.__workerCorsResult = event.data; }};"#,
    ))
    .unwrap();
    for _ in 0..40 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(100))
            .await;
        if !matches!(
            page.evaluate("globalThis.__workerCorsResult").as_deref(),
            Ok("pending")
        ) {
            break;
        }
    }
    assert_eq!(
        page.evaluate("globalThis.__workerCorsResult").unwrap(),
        "TypeError:Failed to fetch"
    );
}
