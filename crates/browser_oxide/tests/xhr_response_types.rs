use browser_oxide::{stealth::presets, Page};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn read_path(socket: &mut TcpStream) -> String {
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
    let head = String::from_utf8_lossy(&data);
    head.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string()
}

async fn write_response(socket: &mut TcpStream, content_type: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    socket.write_all(head.as_bytes()).await.expect("write head");
    socket.write_all(body).await.expect("write body");
    let _ = socket.shutdown().await;
}

#[tokio::test]
async fn xhr_response_types_and_fetch_binary_bytes_match_chromium() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind XHR fixture");
    let addr = listener.local_addr().expect("fixture addr");
    let server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut socket, _) = listener.accept().await.expect("accept request");
            let path = read_path(&mut socket).await;
            match path.as_str() {
                "/json" => {
                    write_response(&mut socket, "application/json", br#"{"value":42}"#).await
                }
                "/bytes" | "/fetch-bytes" => {
                    write_response(&mut socket, "application/octet-stream", &[1, 2, 255, 0]).await
                }
                "/text" => write_response(&mut socket, "text/plain; charset=utf-8", b"hello").await,
                other => panic!("unexpected fixture path: {other}"),
            }
        }
    });

    let html = r#"<!doctype html><html><body><script>
        function runXhr(type, url) {
            return new Promise(resolve => {
                const xhr = new XMLHttpRequest();
                const states = [];
                const events = [];
                xhr.onreadystatechange = () => states.push(xhr.readyState);
                for (const name of ['loadstart','progress','load','error','abort','timeout','loadend']) {
                    xhr.addEventListener(name, event => events.push({
                        type: name,
                        loaded: event.loaded ?? null,
                        total: event.total ?? null,
                        lengthComputable: event.lengthComputable ?? null,
                        trusted: event.isTrusted,
                    }));
                }
                xhr.open('GET', url, true);
                xhr.responseType = type;
                xhr.onloadend = () => {
                    let response;
                    if (type === 'arraybuffer') {
                        response = { tag: Object.prototype.toString.call(xhr.response), bytes: Array.from(new Uint8Array(xhr.response)) };
                    } else if (type === 'blob') {
                        response = { tag: Object.prototype.toString.call(xhr.response), size: xhr.response.size, type: xhr.response.type };
                    } else {
                        response = { tag: Object.prototype.toString.call(xhr.response), value: xhr.response };
                    }
                    let responseText;
                    try { responseText = { ok: true, value: xhr.responseText }; }
                    catch (error) { responseText = { ok: false, name: error.name }; }
                    resolve({ type, readyState: xhr.readyState, status: xhr.status, responseType: xhr.responseType, response, responseText, states, events });
                };
                xhr.send();
            });
        }
        globalThis.__xhrDone = (async () => {
            const xhr = [];
            xhr.push(await runXhr('json', '/json'));
            xhr.push(await runXhr('arraybuffer', '/bytes'));
            xhr.push(await runXhr('blob', '/bytes'));
            xhr.push(await runXhr('text', '/text'));
            const fetchResponse = await fetch('/fetch-bytes');
            const fetchBytes = Array.from(new Uint8Array(await fetchResponse.arrayBuffer()));
            globalThis.__xhrResult = { xhr, fetchBytes };
            return true;
        })();
    </script></body></html>"#;

    let page_url = format!("http://{addr}/page");
    let mut page = Page::from_html_with_url(html, &page_url, Some(presets::chrome_148_macos()))
        .await
        .expect("page");
    page.evaluate_async("globalThis.__xhrDone", std::time::Duration::from_secs(3))
        .await
        .expect("XHR fixture completed");

    let raw = page
        .evaluate("JSON.stringify(globalThis.__xhrResult)")
        .expect("XHR result");
    let value: Value = serde_json::from_str(&raw).expect("valid XHR JSON");
    let xhr = value["xhr"].as_array().expect("XHR array");
    assert_eq!(xhr.len(), 4);

    for item in xhr {
        assert_eq!(item["readyState"], 4);
        assert_eq!(item["status"], 200);
        assert_eq!(item["states"], serde_json::json!([1, 2, 3, 4]));
        let events = item["events"].as_array().expect("events");
        assert_eq!(
            events
                .iter()
                .map(|event| event["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["loadstart", "progress", "load", "loadend"]
        );
        assert!(events.iter().all(|event| event["trusted"] == true));
        assert_eq!(events[0]["lengthComputable"], false);
        assert_eq!(events[0]["loaded"], 0);
        assert_eq!(events[0]["total"], 0);
    }

    assert_eq!(xhr[0]["responseType"], "json");
    assert_eq!(xhr[0]["response"]["tag"], "[object Object]");
    assert_eq!(
        xhr[0]["response"]["value"],
        serde_json::json!({"value": 42})
    );
    assert_eq!(
        xhr[0]["responseText"],
        serde_json::json!({"ok": false, "name": "InvalidStateError"})
    );
    assert_eq!(xhr[0]["events"][1]["loaded"], 12);
    assert_eq!(xhr[0]["events"][1]["total"], 12);

    assert_eq!(xhr[1]["responseType"], "arraybuffer");
    assert_eq!(xhr[1]["response"]["tag"], "[object ArrayBuffer]");
    assert_eq!(
        xhr[1]["response"]["bytes"],
        serde_json::json!([1, 2, 255, 0])
    );
    assert_eq!(
        xhr[1]["responseText"],
        serde_json::json!({"ok": false, "name": "InvalidStateError"})
    );
    assert_eq!(xhr[1]["events"][1]["loaded"], 4);
    assert_eq!(xhr[1]["events"][1]["total"], 4);

    assert_eq!(xhr[2]["responseType"], "blob");
    assert_eq!(xhr[2]["response"]["tag"], "[object Blob]");
    assert_eq!(xhr[2]["response"]["size"], 4);
    assert_eq!(xhr[2]["response"]["type"], "application/octet-stream");
    assert_eq!(
        xhr[2]["responseText"],
        serde_json::json!({"ok": false, "name": "InvalidStateError"})
    );

    assert_eq!(xhr[3]["responseType"], "text");
    assert_eq!(xhr[3]["response"]["tag"], "[object String]");
    assert_eq!(xhr[3]["response"]["value"], "hello");
    assert_eq!(
        xhr[3]["responseText"],
        serde_json::json!({"ok": true, "value": "hello"})
    );

    assert_eq!(value["fetchBytes"], serde_json::json!([1, 2, 255, 0]));

    server.await.expect("fixture server");
}
