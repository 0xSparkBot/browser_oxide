//! Fetch redirect-mode and Response redirect-state parity.

use browser_oxide::Page;
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
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    data
}

async fn spawn_redirect_server(requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_request(&mut socket).await;
            let first_line = String::from_utf8_lossy(&request)
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            let path = first_line.split_whitespace().nth(1).unwrap_or("/");
            let response = if path == "/start" {
                "HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            } else if path == "/final" {
                let body = "final-body";
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            } else {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            };
            socket.write_all(response.as_bytes()).await.unwrap();
            let _ = socket.shutdown().await;
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn fetch_redirect_modes_match_chromium() {
    // Requests: follow uses /start + /final, then error, manual, Request-manual.
    let origin = spawn_redirect_server(5).await;
    let html = r#"<!doctype html><script>
        globalThis.__redirectResult = 'pending';
        (async () => {
            const out = {};
            for (const mode of ['follow', 'error', 'manual']) {
                try {
                    const r = await fetch('/start', { redirect: mode });
                    out[mode] = {
                        status: r.status,
                        statusText: r.statusText,
                        url: r.url,
                        redirected: r.redirected,
                        type: r.type,
                        body: await r.text(),
                        headers: [...r.headers],
                    };
                } catch (e) {
                    out[mode] = { error: e.name + ':' + e.message };
                }
            }
            const req = new Request('/start', { redirect: 'manual' });
            const manual = await fetch(req);
            out.requestManual = {
                status: manual.status,
                url: manual.url,
                redirected: manual.redirected,
                type: manual.type,
            };
            out.constructed = {
                type: new Response('x').type,
                redirected: new Response('x').redirected,
            };
            globalThis.__redirectResult = JSON.stringify(out);
        })();
    </script>"#;

    let mut page = Page::from_html_with_url(
        html,
        &format!("{origin}/page"),
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page.evaluate("globalThis.__redirectResult").unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["follow"]["status"], 200);
    assert_eq!(value["follow"]["statusText"], "OK");
    assert_eq!(value["follow"]["url"], format!("{origin}/final"));
    assert_eq!(value["follow"]["redirected"], true);
    assert_eq!(value["follow"]["type"], "basic");
    assert_eq!(value["follow"]["body"], "final-body");
    assert!(value["follow"]["headers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry[0] == "content-type" && entry[1] == "text/plain"));

    assert_eq!(value["error"]["error"], "TypeError:Failed to fetch");

    assert_eq!(value["manual"]["status"], 0);
    assert_eq!(value["manual"]["statusText"], "");
    assert_eq!(value["manual"]["url"], format!("{origin}/start"));
    assert_eq!(value["manual"]["redirected"], false);
    assert_eq!(value["manual"]["type"], "opaqueredirect");
    assert_eq!(value["manual"]["body"], "");
    assert_eq!(value["manual"]["headers"], serde_json::json!([]));

    assert_eq!(value["requestManual"]["status"], 0);
    assert_eq!(value["requestManual"]["url"], format!("{origin}/start"));
    assert_eq!(value["requestManual"]["redirected"], false);
    assert_eq!(value["requestManual"]["type"], "opaqueredirect");

    assert_eq!(value["constructed"]["type"], "default");
    assert_eq!(value["constructed"]["redirected"], false);
}
