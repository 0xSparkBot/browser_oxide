//! Fetch BodyInit request serialization parity for binary/browser body types.

use browser_oxide::Page;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[derive(Debug)]
struct CapturedRequest {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

async fn start_capture() -> (String, String, oneshot::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        let header_end = loop {
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0, "request closed before headers completed");
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(offset) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break offset + 4;
            }
        };

        let head = String::from_utf8_lossy(&bytes[..header_end]);
        let headers = head
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
            .collect::<Vec<_>>();
        let content_length = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.parse::<usize>().ok())
            .unwrap_or(0);

        while bytes.len() < header_end + content_length {
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0, "request closed before body completed");
            bytes.extend_from_slice(&chunk[..count]);
        }
        let body = bytes[header_end..header_end + content_length].to_vec();
        let _ = tx.send(CapturedRequest { headers, body });

        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .unwrap();
        let _ = socket.shutdown().await;
    });

    (
        format!("http://127.0.0.1:{port}/echo"),
        format!("http://127.0.0.1:{port}/page"),
        rx,
    )
}

async fn capture_body(body_expression: &str) -> CapturedRequest {
    let (target, page_url, request_rx) = start_capture().await;
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(&format!(
        r#"globalThis.__fetchBodyDone = 'pending';
        fetch({target:?}, {{ method: 'POST', body: ({body_expression}) }}).then(
            async response => globalThis.__fetchBodyDone = response.status + ':' + await response.text(),
            error => globalThis.__fetchBodyDone = error.name + ':' + error.message
        );"#,
    ))
    .unwrap();

    for _ in 0..40 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(100))
            .await;
        if page.evaluate("globalThis.__fetchBodyDone").unwrap() != "pending" {
            break;
        }
    }
    assert_eq!(
        page.evaluate("globalThis.__fetchBodyDone").unwrap(),
        "200:ok"
    );

    tokio::time::timeout(Duration::from_secs(2), request_rx)
        .await
        .expect("server capture timed out")
        .expect("server capture dropped")
}

#[tokio::test(flavor = "current_thread")]
async fn fetch_bodyinit_preserves_binary_bytes_and_browser_content_types() {
    let blob =
        capture_body("new Blob([new Uint8Array([0, 255, 65])], { type: 'application/x-test' })")
            .await;
    assert_eq!(blob.body, vec![0, 255, 65]);
    assert_eq!(blob.header("content-type"), Some("application/x-test"));

    let empty_type_blob = capture_body("new Blob([new Uint8Array([1, 2, 3])])").await;
    assert_eq!(empty_type_blob.body, vec![1, 2, 3]);
    assert_eq!(empty_type_blob.header("content-type"), None);

    let bytes = capture_body("new Uint8Array([9, 0, 254])").await;
    assert_eq!(bytes.body, vec![9, 0, 254]);
    assert_eq!(bytes.header("content-type"), None);

    let text = capture_body("'hello'").await;
    assert_eq!(text.body, b"hello");
    assert_eq!(
        text.header("content-type"),
        Some("text/plain;charset=UTF-8")
    );

    let params = capture_body("new URLSearchParams('a=1&b=two')").await;
    assert_eq!(params.body, b"a=1&b=two");
    assert_eq!(
        params.header("content-type"),
        Some("application/x-www-form-urlencoded;charset=UTF-8")
    );

    let multipart = capture_body(
        "(() => { const fd = new FormData(); fd.append('field', 'hello'); fd.append('file', new File([new Uint8Array([0, 255, 65])], 'x.bin', { type: 'application/octet-stream' })); return fd; })()",
    )
    .await;
    let content_type = multipart
        .header("content-type")
        .expect("multipart content-type");
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    assert!(multipart
        .body
        .windows(b"filename=\"x.bin\"".len())
        .any(|window| window == b"filename=\"x.bin\""));
    assert!(multipart
        .body
        .windows(b"Content-Type: application/octet-stream".len())
        .any(|window| window == b"Content-Type: application/octet-stream"));
    assert!(multipart
        .body
        .windows(3)
        .any(|window| window == [0, 255, 65]));
    assert!(!multipart
        .body
        .windows(b"[object File]".len())
        .any(|window| window == b"[object File]"));
}
