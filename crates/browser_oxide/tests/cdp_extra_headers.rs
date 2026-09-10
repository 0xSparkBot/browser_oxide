use browser_oxide::protocol::CdpServer;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

async fn send_cmd<S>(
    tx: &mut S,
    rx: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    id: u64,
    method: &str,
    params: serde_json::Value,
) where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    tx.send(Message::Text(
        serde_json::json!({"id": id, "method": method, "params": params})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    loop {
        let msg = rx.next().await.unwrap().unwrap();
        if let Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                assert!(value.get("error").is_none(), "CDP error: {value}");
                return;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extra_headers_replace_and_reach_navigation_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = oneshot::channel::<HashMap<String, String>>();
    let http_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0_u8; 16_384];
        let n = socket.read(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf[..n]);
        let headers = text
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect::<HashMap<_, _>>();
        let _ = seen_tx.send(headers);

        let body = "<!doctype html><title>ok</title><p>ok</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let server = CdpServer::start_navigable(0).unwrap();
    let (ws, _) = tokio_tungstenite::connect_async(server.ws_url())
        .await
        .unwrap();
    let (mut tx, mut rx) = ws.split();

    send_cmd(
        &mut tx,
        &mut rx,
        1,
        "Network.setExtraHTTPHeaders",
        serde_json::json!({"headers":{"X-First":"one","X-Keep":"old"}}),
    )
    .await;
    send_cmd(
        &mut tx,
        &mut rx,
        2,
        "Network.setExtraHTTPHeaders",
        serde_json::json!({"headers":{"X-Keep":"new","X-Second":"two"}}),
    )
    .await;
    send_cmd(
        &mut tx,
        &mut rx,
        3,
        "Page.navigate",
        serde_json::json!({"url":format!("http://{addr}/target")}),
    )
    .await;

    let headers = tokio::time::timeout(std::time::Duration::from_secs(3), seen_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !headers.contains_key("x-first"),
        "old header leaked: {headers:?}"
    );
    assert_eq!(headers.get("x-keep").map(String::as_str), Some("new"));
    assert_eq!(headers.get("x-second").map(String::as_str), Some("two"));

    tx.send(Message::Close(None)).await.ok();
    drop(server);
    http_task.await.unwrap();
}
