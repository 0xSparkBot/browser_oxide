//! Fetch CORS response-gate parity.

use browser_oxide::Page;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn cors_endpoint(extra_headers: &str) -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let headers = extra_headers.replace("{PAGE_ORIGIN}", &format!("http://localhost:{port}"));
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

async fn run_window_fetch_headers(extra_headers: &str, init: &str) -> String {
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
        fetch({target:?}, {init}).then(response => {{
            globalThis.__corsResult = JSON.stringify({{
                names: Array.from(response.headers.keys()),
                cacheControl: response.headers.get('cache-control'),
                contentType: response.headers.get('content-type'),
                peek: response.headers.get('cf-chl-peek'),
                hidden: response.headers.get('x-hidden'),
                allowOrigin: response.headers.get('access-control-allow-origin'),
                exposeHeaders: response.headers.get('access-control-expose-headers'),
            }});
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
async fn cors_response_exposes_only_safelisted_and_named_headers() {
    let result = run_window_fetch_headers(
        concat!(
            "Access-Control-Allow-Origin: *\r\n",
            "Access-Control-Expose-Headers: Cf-Chl-Peek\r\n",
            "Cache-Control: private, no-store\r\n",
            "Cf-Chl-Peek: challenge-state\r\n",
            "X-Hidden: secret\r\n",
        ),
        r#"{ mode: "cors", credentials: "omit" }"#,
    )
    .await;

    assert_eq!(
        result,
        r#"{"names":["cache-control","cf-chl-peek","content-length","content-type"],"cacheControl":"private, no-store","contentType":"text/plain","peek":"challenge-state","hidden":null,"allowOrigin":null,"exposeHeaders":null}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cors_expose_wildcard_without_credentials_exposes_non_forbidden_headers() {
    let result = run_window_fetch_headers(
        concat!(
            "Access-Control-Allow-Origin: *\r\n",
            "Access-Control-Expose-Headers: *\r\n",
            "X-Hidden: visible-with-wildcard\r\n",
        ),
        r#"{ mode: "cors", credentials: "omit" }"#,
    )
    .await;

    assert!(result.contains(r#""hidden":"visible-with-wildcard""#));
    assert!(result.contains(r#""allowOrigin":"*""#));
    assert!(result.contains(r#""exposeHeaders":"*""#));
}

#[tokio::test(flavor = "current_thread")]
async fn cors_expose_wildcard_with_credentials_is_not_a_wildcard() {
    let result = run_window_fetch_headers(
        concat!(
            "Access-Control-Allow-Origin: {PAGE_ORIGIN}\r\n",
            "Access-Control-Allow-Credentials: true\r\n",
            "Access-Control-Expose-Headers: *\r\n",
            "X-Hidden: remains-hidden\r\n",
        ),
        r#"{ mode: "cors", credentials: "include" }"#,
    )
    .await;

    assert_eq!(
        result,
        r#"{"names":["content-length","content-type"],"cacheControl":null,"contentType":"text/plain","peek":null,"hidden":null,"allowOrigin":null,"exposeHeaders":null}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn headers_iterate_in_lowercase_lexicographic_order() {
    let mut page = Page::from_html(
        "<!doctype html><html><body></body></html>",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate(
            r#"Array.from(new Headers({ Zeta: '3', alpha: '1', Middle: '2' }))
                .map(([name, value]) => name + '=' + value).join('|')"#,
        )
        .unwrap(),
        "alpha=1|middle=2|zeta=3"
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

#[tokio::test(flavor = "current_thread")]
async fn fetched_image_array_buffer_preserves_exact_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).await;
        let body = [0_u8, 0xff, 0x7f, 0x80];
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(headers.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
        let _ = socket.shutdown().await;
    });

    let target = format!("http://127.0.0.1:{port}/challenge-image");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &format!("http://127.0.0.1:{port}/page"),
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    page.evaluate(&format!(
        r#"globalThis.__binaryFetchResult = 'pending';
        fetch({target:?}).then(async response => {{
            const bytes = new Uint8Array(await response.arrayBuffer());
            globalThis.__binaryFetchResult = bytes.length + ':' + Array.from(bytes).join(',');
        }}, error => {{ globalThis.__binaryFetchResult = error.name + ':' + error.message; }});"#,
    ))
    .unwrap();
    for _ in 0..30 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(100))
            .await;
        if !matches!(
            page.evaluate("globalThis.__binaryFetchResult").as_deref(),
            Ok("pending")
        ) {
            break;
        }
    }
    assert_eq!(
        page.evaluate("globalThis.__binaryFetchResult").unwrap(),
        "4:0,255,127,128"
    );
}
