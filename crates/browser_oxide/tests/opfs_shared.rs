use browser_oxide::stealth::presets::chrome_148_macos;
use browser_oxide::Page;
use serde_json::Value;
use std::time::Duration;

async fn pump_until(page: &mut Page, expression: &str, timeout: Duration) -> String {
    let started = std::time::Instant::now();
    loop {
        let value = page.evaluate(expression).unwrap_or_default();
        if !value.is_empty() && value != "undefined" && value != "null" {
            return value;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out waiting for {expression}"
        );
        let _ = page
            .event_loop()
            .run_until_idle(Duration::from_millis(50))
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn opfs_is_shared_across_window_and_workers_but_isolated_by_origin() {
    let html = r#"<!doctype html><html><body><div id="out"></div></body></html>"#;
    let mut page = Page::from_html_with_url(
        html,
        "https://opfs-shared.example/page",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"
        globalThis.__opfsResult = '';
        const writerSource = `
            self.onmessage = async () => {
                try {
                    const root = await navigator.storage.getDirectory();
                    const file = await root.getFileHandle('shared.bin', { create: true });
                    const access = await file.createSyncAccessHandle();
                    access.write(new Uint8Array([11, 22, 33, 44]), { at: 0 });
                    access.flush();
                    access.close();
                    self.postMessage('written');
                } catch (error) {
                    self.postMessage('writer-error:' + String(error && error.stack || error));
                }
            };
        `;
        const writer = new Worker(URL.createObjectURL(new Blob([writerSource], { type: 'text/javascript' })));
        writer.onmessage = async event => {
            if (String(event.data).startsWith('writer-error:')) {
                globalThis.__opfsResult = String(event.data);
                writer.terminate();
                return;
            }
            try {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle('shared.bin');
                const parentFile = await file.getFile();
                const parentBytes = Array.from(new Uint8Array(await parentFile.arrayBuffer()));
                const names = [];
                for await (const [name] of root.entries()) names.push(name);

                const readerSource = `
                    self.onmessage = async () => {
                        try {
                            const root = await navigator.storage.getDirectory();
                            const file = await root.getFileHandle('shared.bin');
                            const access = await file.createSyncAccessHandle();
                            const bytes = new Uint8Array(4);
                            const count = access.read(bytes, { at: 0 });
                            const sameSize = access.getSize();
                            access.close();
                            self.postMessage(JSON.stringify({ count, sameSize, bytes: Array.from(bytes) }));
                        } catch (error) {
                            self.postMessage(JSON.stringify({ error: String(error && error.stack || error) }));
                        }
                    };
                `;
                const reader = new Worker(URL.createObjectURL(new Blob([readerSource], { type: 'text/javascript' })));
                reader.onmessage = readerEvent => {
                    const worker = JSON.parse(readerEvent.data);
                    globalThis.__opfsResult = JSON.stringify({ parentBytes, names, worker });
                    reader.terminate();
                    writer.terminate();
                };
                reader.postMessage('read');
            } catch (error) {
                globalThis.__opfsResult = 'parent-error:' + String(error && error.stack || error);
                writer.terminate();
            }
        };
        writer.postMessage('write');
        "#,
    )
    .unwrap();

    let raw = pump_until(&mut page, "globalThis.__opfsResult", Duration::from_secs(5)).await;
    assert!(!raw.starts_with("writer-error:"), "{raw}");
    assert!(!raw.starts_with("parent-error:"), "{raw}");
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid shared OPFS JSON: {error}; raw={raw}"));
    assert_eq!(value["parentBytes"], serde_json::json!([11, 22, 33, 44]));
    assert_eq!(value["names"], serde_json::json!(["shared.bin"]));
    assert_eq!(value["worker"]["error"], Value::Null, "{raw}");
    assert_eq!(value["worker"]["count"], 4, "{raw}");
    assert_eq!(value["worker"]["sameSize"], 4, "{raw}");
    assert_eq!(
        value["worker"]["bytes"],
        serde_json::json!([11, 22, 33, 44])
    );

    drop(page);

    let mut other = Page::from_html_with_url(
        html,
        "https://opfs-other.example/page",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();
    other
        .evaluate(
            r#"
            globalThis.__otherResult = '';
            navigator.storage.getDirectory().then(root =>
                root.getFileHandle('shared.bin')
            ).then(() => {
                globalThis.__otherResult = 'unexpected-found';
            }).catch(error => {
                globalThis.__otherResult = error.name;
            });
            "#,
        )
        .unwrap();
    let other_result = pump_until(
        &mut other,
        "globalThis.__otherResult",
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(other_result, "NotFoundError");
}
