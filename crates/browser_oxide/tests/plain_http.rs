//! Regression coverage for cleartext HTTP transport selection.

use browser_oxide::net::HttpClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(header_end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let header_end = header_end + 4;
            let headers = String::from_utf8_lossy(&data[..header_end]).to_ascii_lowercase();
            let content_len = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if data.len() >= header_end + content_len {
                break;
            }
        }
    }
    data
}

async fn spawn_server(requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_request(&mut socket).await;
            let text = String::from_utf8_lossy(&request);
            let first_line = text.lines().next().unwrap_or("");
            let body = text
                .split_once("\r\n\r\n")
                .map(|(_, body)| body)
                .unwrap_or("");
            let response_body = format!("{first_line}|{body}");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            let _ = socket.shutdown().await;
        }
    });
    format!("http://{addr}/echo?q=1")
}

#[tokio::test]
async fn all_http_get_and_post_paths_use_cleartext_h1() {
    let url = spawn_server(5).await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = HttpClient::shared(&profile).unwrap();

    let get = client.get(&url).await.unwrap();
    assert_eq!(get.status, 200);
    assert!(get.text().starts_with("GET /echo?q=1 HTTP/1.1|"));

    let exact_get = client
        .get_with_exact_headers(&url, &[("accept".into(), "text/plain".into())])
        .await
        .unwrap();
    assert!(exact_get.text().starts_with("GET /echo?q=1 HTTP/1.1|"));

    let post = client.post(&url, "normal-post").await.unwrap();
    assert!(post.text().ends_with("|normal-post"));

    let direct = client
        .post_bytes_with_exact_headers_direct(
            &url,
            b"direct-post",
            &[("content-type".into(), "text/plain".into())],
        )
        .await
        .unwrap();
    assert!(direct.text().ends_with("|direct-post"));

    let exact = client
        .post_bytes_with_exact_headers(
            &url,
            b"exact-post",
            &[("content-type".into(), "text/plain".into())],
        )
        .await
        .unwrap();
    assert!(exact.text().ends_with("|exact-post"));
}
