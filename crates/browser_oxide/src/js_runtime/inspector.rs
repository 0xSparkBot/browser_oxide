//! V8 inspector bridge — exposes the deno_core inspector over a WebSocket so a
//! Chrome DevTools Protocol frontend can attach for breakpoint/step debugging.
//! The runtime is `!Send` and runs on its own thread; the WebSocket accept loop
//! runs on another thread and bridges through the session-proxy channels (all
//! `Send`). The runtime pumps sessions whenever its event loop is driven, so no
//! extra per-tick call is needed. Child-realm (iframe) scripts still fire
//! `Debugger.scriptParsed` and bind breakpoints by URL because the `Debugger`
//! domain is isolate-wide, so frame scripts are debuggable too.

use deno_core::futures::channel::mpsc;
use deno_core::{
    InspectorMsg, InspectorSessionChannels, InspectorSessionKind, InspectorSessionProxy,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

pub type SessionSender = mpsc::UnboundedSender<InspectorSessionProxy>;

/// A running V8-inspector WebSocket server. Stops when dropped.
pub struct InspectorServer {
    port: u16,
    shutdown: Arc<tokio::sync::Notify>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl InspectorServer {
    /// Bind and serve CDP over WebSocket; port 0 lets the OS pick a free port.
    pub fn start(session_sender: SessionSender, port: u16) -> std::io::Result<Self> {
        let std_listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
        std_listener.set_nonblocking(true)?;
        let actual_port = std_listener.local_addr()?.port();
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let sd = shutdown.clone();
        let thread = std::thread::Builder::new()
            .name("browser-oxide-inspector".into())
            .spawn(move || {
                let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let local = tokio::task::LocalSet::new();
                local.block_on(&rt, accept_loop(std_listener, session_sender, sd));
            })?;
        Ok(Self {
            port: actual_port,
            shutdown,
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/", self.port)
    }
}

impl Drop for InspectorServer {
    fn drop(&mut self) {
        self.shutdown.notify_one();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

async fn accept_loop(
    std_listener: std::net::TcpListener,
    session_sender: SessionSender,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let Ok(listener) = TcpListener::from_std(std_listener) else {
        return;
    };
    loop {
        // `biased` checks shutdown first so a pending stop wins over a new connection.
        tokio::select! {
            biased;
            _ = shutdown.notified() => break,
            r = listener.accept() => {
                if let Ok((stream, _addr)) = r {
                    let sender = session_sender.clone();
                    tokio::task::spawn_local(async move {
                        let _ = handle_connection(stream, sender).await;
                    });
                }
            }
        }
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    session_sender: SessionSender,
) -> Result<(), Box<dyn std::error::Error>> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut ws_sink, mut ws_stream) = ws.split();

    // deno_core reads commands from `rx` and pushes responses/events on `tx`;
    // we keep the opposite ends: out_rx for V8->DevTools, in_tx for DevTools->V8.
    let (out_tx, mut out_rx) = mpsc::unbounded::<InspectorMsg>();
    let (in_tx, in_rx) = mpsc::unbounded::<String>();
    let proxy = InspectorSessionProxy {
        channels: InspectorSessionChannels::Regular {
            tx: out_tx,
            rx: in_rx,
        },
        kind: InspectorSessionKind::NonBlocking {
            wait_for_disconnect: false,
        },
    };
    session_sender
        .unbounded_send(proxy)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let out_task = tokio::task::spawn_local(async move {
        while let Some(msg) = out_rx.next().await {
            if ws_sink
                .send(Message::Text(msg.content.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    while let Some(frame) = ws_stream.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                if in_tx.unbounded_send(text.to_string()).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    out_task.abort();
    Ok(())
}
