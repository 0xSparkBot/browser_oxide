use browser_oxide::Page;

#[tokio::test]
async fn offline_audio_completion_dispatches_event_and_resolves_same_buffer() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/audio-completion",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"(() => {
            const context = new OfflineAudioContext(1, 64, 44100);
            globalThis.__audioCompletion = { calls: 0 };
            const promise = context.startRendering();
            context.addEventListener('complete', event => {
                __audioCompletion.calls++;
                __audioCompletion.listener = {
                    tag: Object.prototype.toString.call(event),
                    target: event.target === context,
                    currentTarget: event.currentTarget === context,
                    bufferTag: Object.prototype.toString.call(event.renderedBuffer),
                    bufferLength: event.renderedBuffer.length,
                    sameAsHandler: event.renderedBuffer === __audioCompletion.handlerBuffer,
                };
                __audioCompletion.eventBuffer = event.renderedBuffer;
            });
            context.oncomplete = event => {
                __audioCompletion.calls++;
                __audioCompletion.handlerBuffer = event.renderedBuffer;
                __audioCompletion.handler = {
                    bufferLength: event.renderedBuffer.length,
                };
            };
            promise.then(buffer => {
                __audioCompletion.promise = {
                    sameBuffer: buffer === __audioCompletion.eventBuffer,
                    length: buffer.length,
                };
                __audioCompletion.done = true;
            });
        })()"#,
    )
    .unwrap();

    page.evaluate_async("void 0", std::time::Duration::from_millis(500))
        .await
        .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__audioCompletion)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["done"], true);
    assert_eq!(value["calls"], 2);
    assert_eq!(
        value["listener"]["tag"],
        "[object OfflineAudioCompletionEvent]"
    );
    assert_eq!(value["listener"]["target"], true);
    assert_eq!(value["listener"]["currentTarget"], true);
    assert_eq!(value["listener"]["bufferTag"], "[object AudioBuffer]");
    assert_eq!(value["listener"]["bufferLength"], 64);
    assert_eq!(value["listener"]["sameAsHandler"], true);
    assert_eq!(value["handler"]["bufferLength"], 64);
    assert_eq!(value["promise"]["sameBuffer"], true);
    assert_eq!(value["promise"]["length"], 64);
}
