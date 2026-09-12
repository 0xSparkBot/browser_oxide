use browser_oxide::Page;

#[tokio::test]
async fn navigator_and_screen_accessors_use_native_webidl_callable_shape() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/",
        Some(profile),
    )
    .await
    .unwrap();

    let result = page
        .evaluate(
            r#"
            (() => {
                const probes = [
                    [Navigator.prototype, 'userAgent'],
                    [Navigator.prototype, 'hardwareConcurrency'],
                    [Screen.prototype, 'width'],
                    [Screen.prototype, 'orientation'],
                    [Plugin.prototype, 'name'],
                ];
                return JSON.stringify(probes.map(([proto, name]) => {
                    const fn = Object.getOwnPropertyDescriptor(proto, name).get;
                    let constructable = true;
                    try { Reflect.construct(function(){}, [], fn); }
                    catch (_) { constructable = false; }
                    return {
                        name: fn.name,
                        own: Object.getOwnPropertyNames(fn).sort(),
                        hasPrototype: 'prototype' in fn,
                        constructable,
                        source: Function.prototype.toString.call(fn),
                    };
                }));
            })()
            "#,
        )
        .unwrap();

    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    for item in value.as_array().unwrap() {
        assert_eq!(item["own"], serde_json::json!(["length", "name"]));
        assert_eq!(item["hasPrototype"], false);
        assert_eq!(item["constructable"], false);
        assert!(item["name"].as_str().unwrap().starts_with("get "));
        assert!(item["source"].as_str().unwrap().contains("[native code]"));
    }

    let receiver_checks = page
        .evaluate(
            r#"
            JSON.stringify([
                [Navigator.prototype, 'connection'],
                [Navigator.prototype, 'plugins'],
                [Navigator.prototype, 'mimeTypes'],
                [Navigator.prototype, 'storage'],
                [Navigator.prototype, 'serviceWorker'],
                [Navigator.prototype, 'webdriver'],
                [Screen.prototype, 'availLeft'],
                [Screen.prototype, 'availTop'],
                [Screen.prototype, 'colorDepth'],
                [Screen.prototype, 'pixelDepth'],
                [Screen.prototype, 'onchange'],
            ].map(([proto, name]) => {
                let result = 'NO_THROW';
                try { void proto[name]; }
                catch (error) { result = error instanceof TypeError ? 'TypeError' : error.name; }
                const desc = Object.getOwnPropertyDescriptor(proto, name);
                return [name, result, desc.enumerable, desc.configurable];
            }))
            "#,
        )
        .unwrap();
    let rows: serde_json::Value = serde_json::from_str(&receiver_checks).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            row[1], "TypeError",
            "{} must brand-check its receiver",
            row[0]
        );
        assert_eq!(row[2], true, "{} must be enumerable WebIDL", row[0]);
        assert_eq!(row[3], true, "{} must remain configurable", row[0]);
    }

    assert_ne!(
        page.evaluate("typeof navigator.connection").unwrap(),
        "undefined"
    );
    assert_eq!(page.evaluate("navigator.webdriver").unwrap(), "false");
    assert_eq!(page.evaluate("typeof screen.colorDepth").unwrap(), "number");
}
