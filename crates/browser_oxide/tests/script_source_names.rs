//! Script resource-name parity for browser-visible Error stacks.

use browser_oxide::Page;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

fn script_endpoint(source: &'static str) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            source.len(),
            source,
        );
        socket.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}/named-script.js"), handle)
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_external_script_stack_uses_request_url() {
    let (script_url, server) = script_endpoint(
        "globalThis.__dynamicScriptStack = new Error('dynamic source name').stack;",
    );
    let page_url = script_url.replace("/named-script.js", "/page");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><head></head><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate_async(
        &format!(
            r#"new Promise((resolve, reject) => {{
                const script = document.createElement('script');
                script.onload = () => resolve();
                script.onerror = () => reject(new Error('dynamic script failed'));
                script.src = {script_url:?};
                document.head.appendChild(script);
            }})"#,
        ),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    server.join().unwrap();

    let stack = page.evaluate("globalThis.__dynamicScriptStack").unwrap();
    assert!(
        stack.contains(&script_url),
        "external script stack must contain its request URL, got: {stack}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn parser_scripts_use_external_and_document_urls() {
    let (script_url, server) =
        script_endpoint("globalThis.__externalStack = new Error('external source name').stack;");
    let page_url = script_url.replace("/named-script.js", "/document");
    let html = format!(
        r#"<!doctype html><html><head>
        <script>globalThis.__inlineStack = new Error('inline source name').stack;</script>
        <script src={script_url:?}></script>
        </head><body></body></html>"#,
    );
    let mut page = Page::from_html_with_url(
        &html,
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    server.join().unwrap();

    let inline_stack = page.evaluate("globalThis.__inlineStack").unwrap();
    assert!(
        inline_stack.contains(&page_url),
        "inline script stack must contain the document URL, got: {inline_stack}"
    );
    let external_stack = page.evaluate("globalThis.__externalStack").unwrap();
    assert!(
        external_stack.contains(&script_url),
        "external script stack must contain its request URL, got: {external_stack}"
    );
}
