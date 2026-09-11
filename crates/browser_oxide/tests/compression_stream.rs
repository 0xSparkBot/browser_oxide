use browser_oxide::{stealth::presets::chrome_148_macos, Page};
use std::time::Duration;

async fn drive_until(page: &mut Page, expression: &str) -> String {
    for _ in 0..40 {
        let _ = page
            .event_loop()
            .run_until_idle(Duration::from_millis(50))
            .await;
        let value = page.evaluate(expression).unwrap_or_default();
        if value != "undefined" && value != "null" && !value.is_empty() {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    page.evaluate(expression).unwrap_or_default()
}

#[tokio::test]
async fn compression_stream_round_trips_standard_formats() {
    let mut page = Page::from_html(
        "<!doctype html><html><body></body></html>",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"
        globalThis.__compressionResult = null;
        (async () => {
            try {
                const encoder = new TextEncoder();
                const decoder = new TextDecoder();
                const input = encoder.encode('compression stream — multiple chunks — 0123456789');
                const formats = ['gzip', 'deflate', 'deflate-raw'];
                const results = [];

                for (const format of formats) {
                    const compressor = new CompressionStream(format);
                    const decompressor = new DecompressionStream(format);
                    const source = new ReadableStream({
                        start(controller) {
                            controller.enqueue(input.slice(0, 13));
                            controller.enqueue(input.slice(13, 29));
                            controller.enqueue(input.slice(29));
                            controller.close();
                        }
                    });
                    const output = source
                        .pipeThrough(compressor)
                        .pipeThrough(decompressor);
                    const reader = output.getReader();
                    const chunks = [];
                    let total = 0;
                    while (true) {
                        const { value, done } = await reader.read();
                        if (done) break;
                        const bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
                        chunks.push(bytes.slice());
                        total += bytes.byteLength;
                    }
                    const merged = new Uint8Array(total);
                    let offset = 0;
                    for (const chunk of chunks) {
                        merged.set(chunk, offset);
                        offset += chunk.byteLength;
                    }
                    results.push({ format, text: decoder.decode(merged) });
                }

                const sample = new CompressionStream('gzip');
                let invalid, invalidMessage, directCall;
                try { new CompressionStream('br'); invalid = 'accepted'; }
                catch (error) { invalid = error.name; invalidMessage = error.message; }
                try { CompressionStream('gzip'); directCall = 'accepted'; }
                catch (error) { directCall = error.name + ':' + error.message; }
                globalThis.__compressionResult = JSON.stringify({
                    results,
                    own: Reflect.ownKeys(sample).map(String),
                    tag: Object.prototype.toString.call(sample),
                    readable: sample.readable instanceof ReadableStream,
                    writable: sample.writable instanceof WritableStream,
                    invalid,
                    invalidMessage,
                    directCall,
                    ctorLength: CompressionStream.length,
                    protoKeys: Reflect.ownKeys(CompressionStream.prototype).map(String),
                    readableGetter: Function.prototype.toString.call(
                        Object.getOwnPropertyDescriptor(CompressionStream.prototype, 'readable').get
                    ),
                });
            } catch (error) {
                globalThis.__compressionResult = JSON.stringify({
                    error: error.name + ':' + error.message,
                });
            }
        })();
        "#,
    )
    .unwrap();

    let result = drive_until(&mut page, "globalThis.__compressionResult").await;
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(value.get("error").is_none(), "compression failed: {value}");
    for result in value["results"].as_array().unwrap() {
        assert_eq!(
            result["text"],
            "compression stream — multiple chunks — 0123456789"
        );
    }
    assert_eq!(value["own"], serde_json::json!([]));
    assert_eq!(value["tag"], "[object CompressionStream]");
    assert_eq!(value["readable"], true);
    assert_eq!(value["writable"], true);
    assert_eq!(value["invalid"], "TypeError");
    assert_eq!(
        value["invalidMessage"],
        "Failed to construct 'CompressionStream': Unsupported compression format: 'br'"
    );
    assert_eq!(
        value["directCall"],
        "TypeError:Failed to construct 'CompressionStream': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
    );
    assert_eq!(value["ctorLength"], 1);
    assert_eq!(
        value["protoKeys"],
        serde_json::json!([
            "readable",
            "writable",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["readableGetter"],
        "function get readable() { [native code] }"
    );
}

#[test]
fn compression_stream_works_in_dedicated_worker() {
    let dom =
        browser_oxide::html_parser::parse_html("<html><body><div id=\"out\"></div></body></html>");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    let output = local.block_on(&rt, async move {
        let mut runtime = browser_oxide::js_runtime::BrowserJsRuntime::new(dom);
        runtime
            .execute_script(
                r#"
                const workerSource = `
                    (async () => {
                        try {
                            const input = new TextEncoder().encode('worker compression round trip');
                            const source = new ReadableStream({
                                start(controller) { controller.enqueue(input); controller.close(); }
                            });
                            const output = source
                                .pipeThrough(new CompressionStream('gzip'))
                                .pipeThrough(new DecompressionStream('gzip'));
                            const reader = output.getReader();
                            const chunks = [];
                            while (true) {
                                const { value, done } = await reader.read();
                                if (done) break;
                                chunks.push(...value);
                            }
                            self.postMessage(JSON.stringify({
                                text: new TextDecoder().decode(new Uint8Array(chunks)),
                                compression: Object.prototype.toString.call(new CompressionStream('gzip')),
                                decompression: Object.prototype.toString.call(new DecompressionStream('gzip')),
                            }));
                        } catch (error) {
                            self.postMessage(JSON.stringify({ error: error.name + ':' + error.message }));
                        }
                    })();
                `;
                const worker = new Worker(URL.createObjectURL(new Blob([workerSource], {type:'text/javascript'})));
                worker.onmessage = event => {
                    document.querySelector('#out').textContent = event.data;
                    worker.terminate();
                };
                "#,
                None,
            )
            .unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let tick = (deadline.saturating_duration_since(std::time::Instant::now()))
                .min(Duration::from_millis(50));
            if tick.is_zero() {
                break String::new();
            }
            let _ = tokio::time::timeout(tick, runtime.run_event_loop()).await;
            let value = runtime
                .execute_script("document.querySelector('#out').textContent || ''", None)
                .unwrap_or_default();
            if !value.is_empty() {
                break value;
            }
        }
    });

    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(
        value.get("error").is_none(),
        "worker compression failed: {value}"
    );
    assert_eq!(value["text"], "worker compression round trip");
    assert_eq!(value["compression"], "[object CompressionStream]");
    assert_eq!(value["decompression"], "[object DecompressionStream]");
}
