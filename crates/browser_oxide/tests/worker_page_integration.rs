//! Verify Worker delivery in both the bare runtime and the real Page/frame tree.
//! Catches bootstrap interference and missing worker-to-frame-to-parent delivery.
//!
//! Run: cargo test -p browser_oxide --test worker_page_integration -- --test-threads=1 --nocapture

#[tokio::test]
async fn worker_works_in_page_bootstrap() {
    use browser_oxide::event_loop::BrowserEventLoop;
    use browser_oxide::js_runtime::BrowserJsRuntime;

    let dom = browser_oxide::html_parser::parse_html(
        "<html><head></head><body><div id=\"out\"></div></body></html>",
    );
    let mut evloop = BrowserEventLoop::new(BrowserJsRuntime::new(dom));

    // 1) Capability probes: verify the real Worker and blob URL functions exist.
    evloop
        .execute_script(
            r#"window.__probe = JSON.stringify({
                workerType: typeof Worker,
                createUrlType: typeof URL.createObjectURL,
                blobType: typeof Blob,
                workerProto: (Worker && Worker.prototype && Worker.prototype[Symbol.toStringTag]) || null,
            });"#,
        )
        .unwrap();
    let probe = evloop.execute_script("window.__probe").unwrap();
    println!("[probe] {probe}");
    assert!(
        probe.contains("\"workerType\":\"function\""),
        "Worker should be a function, got {probe}"
    );
    assert!(
        probe.contains("\"createUrlType\":\"function\""),
        "URL.createObjectURL should be a function, got {probe}"
    );

    // 2) Functional test: spawn a worker, send a message, expect a reply.
    evloop
        .execute_script(
            r#"
            (function(){
                const src = `
                    self.onmessage = function(e) {
                        self.postMessage('page:' + e.data);
                    };
                `;
                const blob = new Blob([src], { type: 'text/javascript' });
                const url = URL.createObjectURL(blob);
                const w = new Worker(url);
                w.onmessage = function(e) {
                    document.querySelector('#out').textContent = e.data;
                    w.terminate();
                };
                setTimeout(() => w.postMessage('ping'), 20);
            })();
            "#,
        )
        .unwrap();

    evloop
        .run_until_idle(std::time::Duration::from_secs(5))
        .await
        .unwrap();

    let out = evloop
        .execute_script("document.querySelector('#out').textContent")
        .unwrap();
    assert_eq!(
        out, "page:ping",
        "worker should echo in full page bootstrap"
    );
}

/// Repeatedly cross the owner-idle boundary with a real dedicated Worker.
///
/// A single echo used to pass in isolation but could lose its MessageEvent
/// near the end of the full integration suite: deno_core reported the owner
/// runtime idle in the same scheduler turn that the worker wake became ready.
/// Multiple sequential exchanges make that transition part of the regression
/// contract without adding sleeps or a polling loop.
#[tokio::test]
async fn worker_reply_is_not_lost_at_owner_idle_boundary() {
    use browser_oxide::event_loop::{BrowserEventLoop, IdleReason};
    use browser_oxide::js_runtime::BrowserJsRuntime;

    let dom = browser_oxide::html_parser::parse_html(
        "<html><head></head><body><div id=\"out\"></div></body></html>",
    );
    let mut evloop = BrowserEventLoop::new(BrowserJsRuntime::new(dom));

    for round in 0..16 {
        evloop
            .execute_script(&format!(
                r#"
                (function(){{
                    document.querySelector('#out').textContent = '';
                    const blob = new Blob([
                        "self.onmessage = e => self.postMessage('page:' + e.data);"
                    ], {{ type: 'text/javascript' }});
                    const url = URL.createObjectURL(blob);
                    const w = new Worker(url);
                    URL.revokeObjectURL(url);
                    w.onmessage = function(e) {{
                        document.querySelector('#out').textContent = e.data;
                        w.terminate();
                    }};
                    setTimeout(() => w.postMessage('{round}'), 0);
                }})();
                "#
            ))
            .unwrap();

        let reason = evloop
            .run_until_idle(std::time::Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(reason, IdleReason::AllWorkDone, "round {round} timed out");
        assert_eq!(
            evloop
                .execute_script("document.querySelector('#out').textContent")
                .unwrap(),
            format!("page:{round}"),
            "worker reply was lost at the owner-idle boundary on round {round}"
        );
    }
}

/// Exercise the real Page driver with an independently materialized iframe,
/// rather than treating a bare BrowserJsRuntime as a full Page integration.
#[tokio::test]
async fn iframe_worker_replies_reach_parent_through_frame_tree() {
    use browser_oxide::Page;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const CHILD: &str = r#"<!doctype html><html><body><div id="out"></div><script>
        let worker;
        addEventListener('message', event => {
            if (!event.data || event.data.kind !== 'start') return;
            if (!worker) {
                const url = URL.createObjectURL(new Blob([
                    "self.onmessage = event => self.postMessage({round:event.data.round,value:'worker:'+event.data.round});"
                ], {type:'text/javascript'}));
                worker = new Worker(url);
                URL.revokeObjectURL(url);
                worker.onerror = error => parent.postMessage({kind:'error',message:error.message}, '*');
                worker.onmessage = reply => {
                    const data = reply.data;
                    document.getElementById('out').textContent = data.value;
                    if (data.round === 15) worker.terminate();
                    Promise.resolve().then(() => parent.postMessage({
                        kind:'relay',round:data.round,value:data.value,workerTrusted:reply.isTrusted
                    }, '*'));
                };
            }
            worker.postMessage({round:event.data.round});
        });
    </script></body></html>"#;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        // Only the parent document and child document need HTTP; the Worker
        // source is a local Blob, and no external resources are referenced.
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let n = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            let body = if request.starts_with("GET /child ") {
                CHILD
            } else {
                "<!doctype html><html><body><div id=\"out\"></div></body></html>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });

    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let mut page = tokio::time::timeout(
        Duration::from_secs(5),
        Page::navigate_pure(&format!("{origin}/parent"), profile.clone(), 1),
    )
    .await
    .expect("local parent navigation timed out")
    .expect("local parent navigation failed");
    page.evaluate(&format!(
        r#"globalThis.__relays=[];
        globalThis.__errors=[];
        globalThis.__done=false;
        const frame=document.createElement('iframe');
        frame.src={};
        addEventListener('message', event => {{
            if (!event.data) return;
            if (event.data.kind === 'error') {{
                __errors.push(event.data.message);
                return;
            }}
            if (event.data.kind !== 'relay') return;
            const data=event.data;
            __relays.push({{
                round:data.round,value:data.value,workerTrusted:data.workerTrusted,
                trusted:event.isTrusted,source:event.source===frame.contentWindow,origin:event.origin
            }});
            document.getElementById('out').textContent=data.value;
            if (data.round === 15) __done=true;
            else frame.contentWindow.postMessage({{kind:'start',round:data.round+1}}, '*');
        }});
        document.body.appendChild(frame);"#,
        serde_json::to_string(&format!("{origin}/child")).unwrap()
    ))
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        page.drive_frame_tree(&client, &profile),
    )
    .await
    .expect("local iframe materialization timed out");
    assert_eq!(page.frame_tree_count(), 1);
    server.await.unwrap();

    page.evaluate("frame.contentWindow.postMessage({kind:'start',round:0}, '*');")
        .unwrap();
    // Render-settled is not application-complete: drive the unchanged public
    // Page API with a bounded cadence until the actual final callback arrives.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            page.drive_frame_tree(&client, &profile).await;
            assert_eq!(page.evaluate("JSON.stringify(__errors)").unwrap(), "[]");
            if page.evaluate("String(__done)").unwrap() == "true" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("iframe Worker callbacks did not reach the parent before the deadline");

    let actual: serde_json::Value =
        serde_json::from_str(&page.evaluate("JSON.stringify(__relays)").unwrap()).unwrap();
    let expected: Vec<_> = (0..16)
        .map(|round| {
            serde_json::json!({
                "round": round,
                "value": format!("worker:{round}"),
                "workerTrusted": true,
                "trusted": true,
                "source": true,
                "origin": origin,
            })
        })
        .collect();
    assert_eq!(actual, serde_json::json!(expected));
    assert_eq!(
        page.evaluate("document.getElementById('out').textContent")
            .unwrap(),
        "worker:15"
    );
    assert_eq!(
        page.frame_tree_evaluate(0, "document.getElementById('out').textContent")
            .unwrap(),
        "worker:15"
    );
}
