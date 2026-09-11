//! Verify that Workers are functional in the full Page bootstrap (not just
//! the bare BrowserJsRuntime). Catches interference from later bootstrap
//! scripts that might overwrite globalThis.Worker, URL.createObjectURL, etc.
//!
//! Run: cargo test -p browser --test worker_page_integration -- --test-threads=1 --nocapture

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
