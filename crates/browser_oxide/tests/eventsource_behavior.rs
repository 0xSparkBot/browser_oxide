use browser_oxide::Page;
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn read_request(socket: &mut TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await.expect("read request");
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

#[tokio::test]
async fn eventsource_local_sse_dispatches_trusted_events() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind SSE server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept SSE connection");
        let request = read_request(&mut socket).await;
        assert!(
            request.starts_with("GET /events HTTP/1.1\r\n"),
            "request={request:?}"
        );

        let body = concat!(
            "id: first-id\n",
            "data: hello\n",
            "data: world\n",
            "\n",
            "event: custom\n",
            "id: second-id\n",
            "data: custom-data\n",
            "\n",
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write SSE response");
        let _ = socket.shutdown().await;
    });

    let page_url = format!("http://{addr}/page");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("page");

    let script = format!(
        r#"
        globalThis.__sseEvents = [];
        globalThis.__sse = new EventSource('http://{addr}/events', {{ withCredentials: true }});
        __sse.onopen = event => {{
            __sseEvents.push({{
                kind: 'open-handler',
                trusted: event.isTrusted,
                eventTag: Object.prototype.toString.call(event),
                state: __sse.readyState,
            }});
        }};
        __sse.addEventListener('open', event => {{
            __sseEvents.push({{ kind: 'open-listener', trusted: event.isTrusted }});
        }});
        __sse.onmessage = event => {{
            __sseEvents.push({{
                kind: 'message-handler',
                trusted: event.isTrusted,
                message: event instanceof MessageEvent,
                data: event.data,
                id: event.lastEventId,
                origin: event.origin,
            }});
        }};
        __sse.addEventListener('message', event => {{
            __sseEvents.push({{ kind: 'message-listener', trusted: event.isTrusted, data: event.data }});
        }});
        __sse.addEventListener('custom', event => {{
            __sseEvents.push({{
                kind: 'custom',
                trusted: event.isTrusted,
                message: event instanceof MessageEvent,
                data: event.data,
                id: event.lastEventId,
                origin: event.origin,
            }});
            __sse.close();
        }});
        "#,
    );
    page.evaluate(&script).expect("create EventSource");
    let _ = page
        .evaluate_async(
            "new Promise(resolve => setTimeout(resolve, 50))",
            Duration::from_secs(2),
        )
        .await
        .expect("drive SSE event loop");

    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("SSE request reached server")
        .expect("SSE server task");

    let result = page
        .evaluate(
            r#"JSON.stringify({
                url: __sse.url,
                withCredentials: __sse.withCredentials,
                readyState: __sse.readyState,
                events: __sseEvents,
            })"#,
        )
        .expect("SSE result");
    let value: Value = serde_json::from_str(&result).expect("valid SSE JSON");

    assert_eq!(value["url"], format!("http://{addr}/events"));
    assert_eq!(value["withCredentials"], true);
    assert_eq!(value["readyState"], 2);
    let events = value["events"].as_array().expect("events array");
    assert_eq!(events.len(), 5, "events={events:?}");
    assert_eq!(events[0]["kind"], "open-handler");
    assert_eq!(events[0]["trusted"], true);
    assert_eq!(events[0]["eventTag"], "[object Event]");
    assert_eq!(events[0]["state"], 1);
    assert_eq!(events[1]["kind"], "open-listener");
    assert_eq!(events[1]["trusted"], true);
    assert_eq!(events[2]["kind"], "message-handler");
    assert_eq!(events[2]["trusted"], true);
    assert_eq!(events[2]["message"], true);
    assert_eq!(events[2]["data"], "hello\nworld");
    assert_eq!(events[2]["id"], "first-id");
    assert_eq!(events[2]["origin"], format!("http://{addr}"));
    assert_eq!(events[3]["kind"], "message-listener");
    assert_eq!(events[3]["trusted"], true);
    assert_eq!(events[3]["data"], "hello\nworld");
    assert_eq!(events[4]["kind"], "custom");
    assert_eq!(events[4]["trusted"], true);
    assert_eq!(events[4]["message"], true);
    assert_eq!(events[4]["data"], "custom-data");
    assert_eq!(events[4]["id"], "second-id");
    assert_eq!(events[4]["origin"], format!("http://{addr}"));
}
