use browser_oxide::Page;

#[tokio::test]
async fn abort_and_battery_wrong_this_match_chrome() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.com/",
        Some(profile),
    )
    .await
    .unwrap();

    let result = page
        .evaluate(
            r#"
            JSON.stringify((() => {
              const throwsTypeError = fn => {
                try { fn(); return false; } catch (e) { return e && e.name === 'TypeError'; }
              };
              const abortSignal = Object.getOwnPropertyDescriptor(AbortSignal.prototype, 'aborted');
              const abortReason = Object.getOwnPropertyDescriptor(AbortSignal.prototype, 'reason');
              const abortHandler = Object.getOwnPropertyDescriptor(AbortSignal.prototype, 'onabort');
              const controllerSignal = Object.getOwnPropertyDescriptor(AbortController.prototype, 'signal');
              const batteryLevel = Object.getOwnPropertyDescriptor(BatteryManager.prototype, 'level');
              const batteryHandler = Object.getOwnPropertyDescriptor(BatteryManager.prototype, 'onlevelchange');

              const frame = document.createElement('iframe');
              document.body.appendChild(frame);
              const child = frame.contentWindow;
              const childSignal = child.AbortSignal.abort('child-reason');

              return {
                signalGet: throwsTypeError(() => abortSignal.get.call({})),
                reasonGet: throwsTypeError(() => abortReason.get.call({})),
                onabortGet: throwsTypeError(() => abortHandler.get.call({})),
                onabortSet: throwsTypeError(() => abortHandler.set.call({}, null)),
                throwIfAborted: throwsTypeError(() => AbortSignal.prototype.throwIfAborted.call({})),
                controllerSignal: throwsTypeError(() => controllerSignal.get.call({})),
                controllerAbort: throwsTypeError(() => AbortController.prototype.abort.call({})),
                batteryCtor: throwsTypeError(() => new BatteryManager()),
                batteryGet: throwsTypeError(() => batteryLevel.get.call({})),
                batteryHandlerGet: throwsTypeError(() => batteryHandler.get.call({})),
                batteryHandlerSet: throwsTypeError(() => batteryHandler.set.call({}, null)),
                childBatteryType: typeof child.BatteryManager,
                childBatteryCtor: throwsTypeError(() => new child.BatteryManager()),
                crossRealmAbort: abortSignal.get.call(childSignal),
              };
            })())
            "#,
        )
        .unwrap();

    assert_eq!(
        result,
        r#"{"signalGet":true,"reasonGet":true,"onabortGet":true,"onabortSet":true,"throwIfAborted":true,"controllerSignal":true,"controllerAbort":true,"batteryCtor":true,"batteryGet":true,"batteryHandlerGet":true,"batteryHandlerSet":true,"childBatteryType":"function","childBatteryCtor":true,"crossRealmAbort":true}"#
    );
}
