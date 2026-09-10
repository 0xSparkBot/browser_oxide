use browser_oxide::Page;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

type HeaderMap = HashMap<String, String>;

async fn spawn_module_server() -> (
    String,
    mpsc::UnboundedReceiver<(String, HeaderMap)>,
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
                    .collect::<HeaderMap>();
                let _ = tx.send((path.clone(), headers));

                let (body, content_type) = match path.as_str() {
                    "/" => (
                        r#"<!doctype html><script type="module" src="/entry.js"></script>"#,
                        "text/html",
                    ),
                    "/entry.js" => (
                        "import './dep.js'; globalThis.__moduleEntryLoaded = true;",
                        "text/javascript",
                    ),
                    "/dep.js" => (
                        "globalThis.__moduleDependencyLoaded = true;",
                        "text/javascript",
                    ),
                    _ => ("not found", "text/plain"),
                };
                let status = if path == "/" || path == "/entry.js" || path == "/dep.js" {
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
    (format!("http://{addr}"), rx, task)
}

fn drain_by_path(
    requests: &mut mpsc::UnboundedReceiver<(String, HeaderMap)>,
) -> HashMap<String, HeaderMap> {
    let mut by_path = HashMap::new();
    while let Ok((path, headers)) = requests.try_recv() {
        by_path.insert(path, headers);
    }
    by_path
}

fn assert_module_headers(by_path: &HashMap<String, HeaderMap>, base: &str) {
    let page_url = format!("{base}/");
    let entry_url = format!("{base}/entry.js");

    let entry = by_path.get("/entry.js").expect("module entry request");
    assert_eq!(
        entry.get("sec-fetch-site").map(String::as_str),
        Some("same-origin")
    );
    assert_eq!(
        entry.get("sec-fetch-mode").map(String::as_str),
        Some("cors")
    );
    assert_eq!(
        entry.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(entry.get("origin").map(String::as_str), Some(base));
    assert_eq!(
        entry.get("referer").map(String::as_str),
        Some(page_url.as_str())
    );
    assert!(!entry.contains_key("sec-fetch-user"));
    assert!(!entry.contains_key("upgrade-insecure-requests"));

    let dep = by_path.get("/dep.js").expect("module dependency request");
    assert_eq!(
        dep.get("sec-fetch-site").map(String::as_str),
        Some("same-origin")
    );
    assert_eq!(dep.get("sec-fetch-mode").map(String::as_str), Some("cors"));
    assert_eq!(
        dep.get("sec-fetch-dest").map(String::as_str),
        Some("script")
    );
    assert_eq!(dep.get("origin").map(String::as_str), Some(base));
    assert_eq!(
        dep.get("referer").map(String::as_str),
        Some(entry_url.as_str())
    );
    assert!(!dep.contains_key("sec-fetch-user"));
    assert!(!dep.contains_key("upgrade-insecure-requests"));
}

#[tokio::test(flavor = "current_thread")]
async fn warm_navigation_replaces_module_request_origin_and_referrers() {
    let (first_base, mut first_requests, first_server) = spawn_module_server().await;
    let (second_base, mut second_requests, second_server) = spawn_module_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();

    let mut page = Page::navigate(&format!("{first_base}/"), profile, 1)
        .await
        .expect("first navigation");
    assert_eq!(
        page.evaluate("String(__moduleDependencyLoaded)").unwrap(),
        "true"
    );
    assert_module_headers(&drain_by_path(&mut first_requests), &first_base);

    page.navigate_warm(&format!("{second_base}/"))
        .await
        .expect("warm navigation");
    assert_eq!(
        page.evaluate("String(__moduleDependencyLoaded)").unwrap(),
        "true"
    );
    assert_module_headers(&drain_by_path(&mut second_requests), &second_base);

    first_server.abort();
    second_server.abort();
}
