use browser_oxide::Page;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn blank_page() -> Page {
    Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("page")
}

#[tokio::test]
async fn websocket_sync_webidl_matches_chrome_148() {
    let mut page = blank_page().await;
    let result = page
        .evaluate(
            r#"JSON.stringify((() => {
                const capture = fn => {
                    try { return { ok: true, value: fn() }; }
                    catch (e) { return { ok: false, name: e.name, message: e.message }; }
                };
                const out = {};
                out.httpUrl = new WebSocket('http://127.0.0.1:9/path').url;
                out.httpsUrl = new WebSocket('https://127.0.0.1:9/a').url;
                out.relative = capture(() => new WebSocket('/socket'));
                out.fragment = capture(() => new WebSocket('ws://example.com/x#frag'));
                out.duplicateProtocol = capture(() => new WebSocket('ws://127.0.0.1:9/', ['chat', 'chat']));
                out.invalidProtocol = capture(() => new WebSocket('ws://127.0.0.1:9/', ['bad proto']));

                const connecting = new WebSocket('ws://127.0.0.1:9/');
                out.sendConnecting = capture(() => connecting.send('x'));

                const invalidCode = new WebSocket('ws://127.0.0.1:9/');
                out.close999 = capture(() => invalidCode.close(999));

                const longReason = new WebSocket('ws://127.0.0.1:9/');
                out.closeLongReason = capture(() => longReason.close(1000, 'a'.repeat(124)));

                const binary = new WebSocket('ws://127.0.0.1:9/');
                out.binaryBefore = binary.binaryType;
                binary.binaryType = 'arraybuffer';
                out.binaryArray = binary.binaryType;
                binary.binaryType = 'invalid';
                out.binaryInvalid = binary.binaryType;
                return out;
            })())"#,
        )
        .expect("websocket shape");
    let value: Value = serde_json::from_str(&result).expect("valid json");

    assert_eq!(value["httpUrl"], "ws://127.0.0.1:9/path");
    assert_eq!(value["httpsUrl"], "wss://127.0.0.1:9/a");
    assert_eq!(value["relative"]["name"], "SyntaxError");
    assert_eq!(
        value["relative"]["message"],
        "Failed to construct 'WebSocket': The URL '/socket' is invalid."
    );
    assert_eq!(value["fragment"]["name"], "SyntaxError");
    assert_eq!(value["duplicateProtocol"]["name"], "SyntaxError");
    assert_eq!(value["invalidProtocol"]["name"], "SyntaxError");
    assert_eq!(value["sendConnecting"]["name"], "InvalidStateError");
    assert_eq!(value["close999"]["name"], "InvalidAccessError");
    assert_eq!(value["closeLongReason"]["name"], "SyntaxError");
    assert_eq!(value["binaryBefore"], "blob");
    assert_eq!(value["binaryArray"], "arraybuffer");
    assert_eq!(value["binaryInvalid"], "arraybuffer");
}

#[tokio::test]
async fn websocket_local_events_use_eventtarget_and_are_trusted() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut websocket = accept_async(stream).await.expect("websocket handshake");
        while let Some(message) = websocket.next().await {
            match message.expect("client websocket message") {
                Message::Text(text) if text.as_str() == "hello" => {
                    websocket
                        .send(Message::Text("".into()))
                        .await
                        .expect("send empty text");
                    websocket
                        .send(Message::Text("hello".into()))
                        .await
                        .expect("send echo");
                    break;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    let mut page = blank_page().await;
    let script = format!(
        r#"
        globalThis.__wsEvents = {{
            openHandler: 0, openListener: 0,
            messageHandler: [], messageListener: [],
            closeHandler: 0, closeListener: 0,
            trusted: []
        }};
        globalThis.__ws = new WebSocket('ws://{addr}/echo');
        __ws.onopen = event => {{
            __wsEvents.openHandler++;
            __wsEvents.trusted.push(['open-handler', event.isTrusted]);
            __ws.send('hello');
        }};
        __ws.addEventListener('open', event => {{
            __wsEvents.openListener++;
            __wsEvents.trusted.push(['open-listener', event.isTrusted]);
        }});
        __ws.onmessage = event => {{
            __wsEvents.messageHandler.push(event.data);
            __wsEvents.trusted.push(['message-handler', event.isTrusted]);
            if (event.data === 'hello') __ws.close(1000, 'done');
        }};
        __ws.addEventListener('message', event => {{
            __wsEvents.messageListener.push(event.data);
            __wsEvents.trusted.push(['message-listener', event.isTrusted]);
        }});
        __ws.onclose = event => {{
            __wsEvents.closeHandler++;
            __wsEvents.trusted.push(['close-handler', event.isTrusted]);
        }};
        __ws.addEventListener('close', event => {{
            __wsEvents.closeListener++;
            __wsEvents.trusted.push(['close-listener', event.isTrusted]);
        }});
        "#
    );
    page.evaluate(&script).expect("create local websocket");
    let _ = page
        .evaluate_async("void 0", Duration::from_secs(3))
        .await
        .expect("drive websocket event loop");

    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("server completed in time")
        .expect("server task");

    let result = page
        .evaluate("JSON.stringify(__wsEvents)")
        .expect("event results");
    let value: Value = serde_json::from_str(&result).expect("valid json");

    assert_eq!(value["openHandler"], 1);
    assert_eq!(value["openListener"], 1);
    assert_eq!(value["messageHandler"], serde_json::json!(["", "hello"]));
    assert_eq!(value["messageListener"], serde_json::json!(["", "hello"]));
    assert_eq!(value["closeHandler"], 1);
    assert_eq!(value["closeListener"], 1);
    assert!(
        value["trusted"]
            .as_array()
            .expect("trusted rows")
            .iter()
            .all(|row| row[1] == true),
        "browser-delivered WebSocket events must be trusted: {value}"
    );
}
