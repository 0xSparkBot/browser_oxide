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

#[tokio::test]
async fn eventsource_dispatches_message_before_stream_eof() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind streaming SSE server");
    let addr = listener.local_addr().expect("local addr");
    let (first_chunk_sent_tx, first_chunk_sent_rx) = tokio::sync::oneshot::channel();
    let (allow_close_tx, allow_close_rx) = tokio::sync::oneshot::channel();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept SSE connection");
        let request = read_request(&mut socket).await;
        assert!(
            request.starts_with("GET /stream HTTP/1.1\r\n"),
            "request={request:?}"
        );

        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n",
            )
            .await
            .expect("write SSE headers");
        let payload = b"id: early-id\ndata: before-eof\n\n";
        socket
            .write_all(format!("{:x}\r\n", payload.len()).as_bytes())
            .await
            .expect("write chunk size");
        socket
            .write_all(payload)
            .await
            .expect("write first SSE chunk");
        socket
            .write_all(b"\r\n")
            .await
            .expect("write chunk trailer");
        socket.flush().await.expect("flush first SSE chunk");
        let _ = first_chunk_sent_tx.send(());

        let _ = allow_close_rx.await;
        socket
            .write_all(b"0\r\n\r\n")
            .await
            .expect("finish chunked SSE response");
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

    page.evaluate(&format!(
        r#"
        globalThis.__earlySse = [];
        globalThis.__earlySource = new EventSource('http://{addr}/stream');
        __earlySource.onmessage = event => __earlySse.push({{
            data: event.data,
            id: event.lastEventId,
            trusted: event.isTrusted,
        }});
        "#
    ))
    .expect("create streaming EventSource");

    tokio::time::timeout(Duration::from_secs(2), first_chunk_sent_rx)
        .await
        .expect("server sent first SSE chunk")
        .expect("first chunk signal");

    let _ = page
        .evaluate_async(
            "new Promise(resolve => setTimeout(resolve, 75))",
            Duration::from_secs(2),
        )
        .await
        .expect("drive SSE event loop while response stays open");

    assert_eq!(
        page.evaluate("JSON.stringify(__earlySse)").unwrap(),
        r#"[{"data":"before-eof","id":"early-id","trusted":true}]"#,
        "EventSource must dispatch complete SSE records before the HTTP response reaches EOF"
    );

    page.evaluate("__earlySource.close()").unwrap();
    let _ = allow_close_tx.send(());
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("streaming SSE server completed")
        .expect("streaming SSE server task");
}

#[tokio::test]
async fn eventsource_reconnects_with_last_event_id_and_retry_delay() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind reconnect SSE server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        for attempt in 0..2 {
            let (mut socket, _) = listener.accept().await.expect("accept SSE connection");
            let request = read_request(&mut socket).await;
            assert!(request.starts_with("GET /reconnect HTTP/1.1\r\n"));
            if attempt == 0 {
                assert!(
                    !request.to_ascii_lowercase().contains("last-event-id:"),
                    "first EventSource request must not send Last-Event-ID: {request:?}"
                );
            } else {
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("last-event-id: first-id\r\n"),
                    "reconnect must carry the previous event id: {request:?}"
                );
            }

            let body = if attempt == 0 {
                "retry: 20\nid: first-id\ndata: one\n\n"
            } else {
                "id: second-id\ndata: two\n\n"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write reconnect SSE response");
            let _ = socket.shutdown().await;
        }
    });

    let page_url = format!("http://{addr}/page");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("page");
    page.evaluate(&format!(
        r#"
        globalThis.__reconnectEvents = [];
        globalThis.__reconnectOpen = 0;
        globalThis.__reconnectErrors = 0;
        globalThis.__reconnectSource = new EventSource('http://{addr}/reconnect');
        __reconnectSource.onopen = () => __reconnectOpen++;
        __reconnectSource.onerror = () => __reconnectErrors++;
        __reconnectSource.onmessage = event => {{
            __reconnectEvents.push([event.data, event.lastEventId, event.isTrusted]);
            if (event.data === 'two') __reconnectSource.close();
        }};
        "#
    ))
    .expect("create reconnect EventSource");

    let _ = page
        .evaluate_async(
            "new Promise(resolve => setTimeout(resolve, 250))",
            Duration::from_secs(2),
        )
        .await
        .expect("drive reconnect event loop");
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("reconnect SSE server completed")
        .expect("reconnect SSE server task");

    let result = page
        .evaluate(
            r#"JSON.stringify({
                events: __reconnectEvents,
                opens: __reconnectOpen,
                errors: __reconnectErrors,
                state: __reconnectSource.readyState,
            })"#,
        )
        .unwrap();
    let value: Value = serde_json::from_str(&result).expect("reconnect result json");
    assert_eq!(
        value["events"],
        serde_json::json!([["one", "first-id", true], ["two", "second-id", true],])
    );
    assert_eq!(value["opens"], 2);
    assert!(value["errors"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(
        value["state"], 2,
        "explicit close after second event must win"
    );
}

#[tokio::test]
async fn eventsource_cross_origin_credentials_and_cors_match_fetch_mode() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind credential SSE server");
    let addr = listener.local_addr().expect("local addr");
    let page_origin = "http://127.0.0.1:1";
    let server = tokio::spawn(async move {
        for attempt in 0..2 {
            let (mut socket, _) = listener.accept().await.expect("accept SSE connection");
            let request = read_request(&mut socket).await;
            let lower = request.to_ascii_lowercase();
            if attempt == 0 {
                assert!(
                    !lower.contains("cookie:"),
                    "default cross-origin EventSource must use same-origin credentials mode: {request:?}"
                );
            } else {
                assert!(
                    lower.contains("cookie: sse_cookie=value"),
                    "withCredentials EventSource must include matching cookies: {request:?}"
                );
            }
            let body = if attempt == 0 {
                "data: anonymous\n\n"
            } else {
                "data: credentialed\n\n"
            };
            let credentials = if attempt == 0 {
                ""
            } else {
                "Access-Control-Allow-Credentials: true\r\n"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nAccess-Control-Allow-Origin: {page_origin}\r\n{credentials}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write SSE response");
            let _ = socket.shutdown().await;
        }
    });

    let page_url = format!("{page_origin}/page");
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &page_url,
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("page");
    page.evaluate("document.cookie = 'sse_cookie=value; Path=/'")
        .expect("set page cookie");
    page.evaluate(&format!(
        r#"
        globalThis.__corsSse = [];
        globalThis.__anonSse = new EventSource('http://{addr}/anonymous');
        __anonSse.onmessage = event => {{
            __corsSse.push(event.data);
            __anonSse.close();
            globalThis.__credSse = new EventSource('http://{addr}/credentialed', {{ withCredentials: true }});
            __credSse.onmessage = event => {{
                __corsSse.push(event.data);
                __credSse.close();
            }};
        }};
        "#
    ))
    .expect("create cross-origin EventSources");
    let _ = page
        .evaluate_async(
            "new Promise(resolve => setTimeout(resolve, 250))",
            Duration::from_secs(2),
        )
        .await
        .expect("drive cross-origin SSE");
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("credential SSE server completed")
        .expect("credential SSE server task");
    assert_eq!(
        page.evaluate("JSON.stringify(__corsSse)").unwrap(),
        r#"["anonymous","credentialed"]"#
    );
}
