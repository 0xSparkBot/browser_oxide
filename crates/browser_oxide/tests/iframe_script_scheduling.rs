use browser_oxide::Page;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

#[derive(Debug)]
struct SeenRequest {
    path: String,
    headers: HashMap<String, String>,
}

async fn spawn_fixture_server() -> (
    String,
    mpsc::UnboundedReceiver<SeenRequest>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16384];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let mut lines = req.lines();
                let path = lines
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let mut headers = HashMap::new();
                for line in lines {
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
                    }
                }
                let _ = tx.send(SeenRequest {
                    path: path.clone(),
                    headers,
                });

                let (content_type, body) = match path.as_str() {
                    "/child" => (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><html><body>
<script>
  globalThis.__frameOrder = ['parser-before'];
  globalThis.__classicCurrentScript = !!document.currentScript && document.currentScript.tagName === 'SCRIPT';
</script>
<script defer src="defer.js"></script>
<script type="module" src="mods/entry.js"></script>
<script>globalThis.__frameOrder.push('parser-after');</script>
</body></html>"#,
                    ),
                    "/defer.js" => (
                        "text/javascript; charset=utf-8",
                        "globalThis.__frameOrder.push('defer');",
                    ),
                    "/mods/entry.js" => (
                        "text/javascript; charset=utf-8",
                        r#"import { marker } from './dep.js';
globalThis.__moduleCurrentScript = document.currentScript === null;
if (marker === 7) globalThis.__frameOrder.push('module');"#,
                    ),
                    "/mods/dep.js" => {
                        ("text/javascript; charset=utf-8", "export const marker = 7;")
                    }
                    _ => ("text/plain", "not found"),
                };
                let status = if matches!(
                    path.as_str(),
                    "/child" | "/defer.js" | "/mods/entry.js" | "/mods/dep.js"
                ) {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (format!("http://{addr}"), rx, task)
}

#[tokio::test]
async fn remote_iframe_uses_browser_script_scheduling_and_module_semantics() {
    let (base, mut requests, server) = spawn_fixture_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let parent_url = format!("{base}/parent");
    let child_url = format!("{base}/child");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &parent_url,
        Some(profile.clone()),
    )
    .await
    .unwrap();

    page.evaluate(&format!(
        "const f=document.createElement('iframe');f.src={};document.body.appendChild(f);",
        serde_json::to_string(&child_url).unwrap()
    ))
    .unwrap();
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);

    let state = page
        .frame_tree_evaluate(
            0,
            "JSON.stringify({order:globalThis.__frameOrder,classicCurrent:globalThis.__classicCurrentScript,moduleCurrent:globalThis.__moduleCurrentScript})",
        )
        .unwrap_or_default();
    assert_eq!(
        state,
        r#"{"order":["parser-before","parser-after","defer","module"],"classicCurrent":true,"moduleCurrent":true}"#
    );

    let mut by_path = HashMap::new();
    while let Ok(request) = requests.try_recv() {
        by_path.insert(request.path.clone(), request);
    }
    let defer = by_path.get("/defer.js").expect("defer request");
    assert_eq!(
        defer.headers.get("sec-fetch-mode").map(String::as_str),
        Some("no-cors")
    );
    assert_eq!(
        defer.headers.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(
        defer.headers.get("referer").map(String::as_str),
        Some(child_url.as_str())
    );
    assert!(!defer.headers.contains_key("origin"));

    let entry = by_path.get("/mods/entry.js").expect("module entry request");
    assert_eq!(
        entry.headers.get("sec-fetch-mode").map(String::as_str),
        Some("cors")
    );
    assert_eq!(
        entry.headers.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(
        entry.headers.get("referer").map(String::as_str),
        Some(child_url.as_str())
    );
    assert_eq!(
        entry.headers.get("origin").map(String::as_str),
        Some(base.as_str())
    );

    let dep = by_path
        .get("/mods/dep.js")
        .expect("module dependency request");
    assert_eq!(
        dep.headers.get("sec-fetch-mode").map(String::as_str),
        Some("cors")
    );
    assert_eq!(
        dep.headers.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(
        dep.headers.get("referer").map(String::as_str),
        Some(format!("{base}/mods/entry.js").as_str())
    );
    assert_eq!(
        dep.headers.get("origin").map(String::as_str),
        Some(base.as_str())
    );

    server.abort();
}
