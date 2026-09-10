use browser_oxide::Page;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

const HTML: &str = r#"<!doctype html><html><head>
<script>
window.__order = ['inline-head:' + document.readyState];
document.addEventListener('DOMContentLoaded', () => __order.push('dcl:' + document.readyState));
window.addEventListener('load', () => __order.push('load:' + document.readyState));
</script>
<script src="/block.js"></script>
<script defer src="/defer1.js"></script>
<script type="module" src="/module1.js"></script>
<script async src="/async-slow.js"></script>
</head><body>
<script>__order.push('inline-body:' + document.readyState)</script>
<script defer src="/defer2.js"></script>
</body></html>"#;

async fn spawn_fixture_server() -> (String, tokio::task::JoinHandle<()>) {
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
                let first_line = String::from_utf8_lossy(&request[..n]);
                let path = first_line
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let (delay_ms, content_type, body) = match path {
                    "/" => (0, "text/html; charset=utf-8", HTML),
                    "/block.js" => (
                        50,
                        "text/javascript; charset=utf-8",
                        "__order.push('block:' + document.readyState);",
                    ),
                    "/defer1.js" => (
                        150,
                        "text/javascript; charset=utf-8",
                        "__order.push('defer1:' + document.readyState);",
                    ),
                    "/defer2.js" => (
                        10,
                        "text/javascript; charset=utf-8",
                        "__order.push('defer2:' + document.readyState);",
                    ),
                    "/module1.js" => (
                        0,
                        "text/javascript; charset=utf-8",
                        "import '/module-dep.js'; __order.push('module1:' + document.readyState);",
                    ),
                    "/module-dep.js" => (
                        80,
                        "text/javascript; charset=utf-8",
                        "__order.push('module-dep:' + document.readyState);",
                    ),
                    "/async-slow.js" => (
                        300,
                        "text/javascript; charset=utf-8",
                        "__order.push('async-slow:' + document.readyState);",
                    ),
                    _ => (0, "text/plain; charset=utf-8", "not found"),
                };
                if delay_ms != 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
                let status = if path == "/" || path.ends_with(".js") {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}/"), task)
}

#[tokio::test(flavor = "current_thread")]
async fn parser_defer_module_async_lifecycle_matches_chrome_148() {
    let (url, server) = spawn_fixture_server().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::navigate(&url, profile, 2).await.expect("navigate");

    let actual = page
        .evaluate("JSON.stringify(globalThis.__order)")
        .expect("order");
    assert_eq!(
        actual,
        r#"["inline-head:loading","block:loading","inline-body:loading","defer1:interactive","module-dep:interactive","module1:interactive","defer2:interactive","dcl:interactive","async-slow:interactive","load:complete"]"#
    );
    assert_eq!(page.evaluate("document.readyState").unwrap(), "complete");

    server.abort();
}
