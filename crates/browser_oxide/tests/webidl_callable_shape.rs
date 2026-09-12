use browser_oxide::Page;

#[tokio::test]
async fn interface_constructor_prototypes_are_read_only_like_chrome_148() {
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
                const names = [
                    'AbsoluteOrientationSensor', 'AbstractRange', 'Blob',
                    'CSSStyleSheet', 'HTMLDivElement', 'PerformanceObserver',
                    'WebSocket', 'BatteryManager',
                ];
                const rows = names.map(name => {
                    const ctor = globalThis[name];
                    const desc = Object.getOwnPropertyDescriptor(ctor, 'prototype');
                    return [name, typeof ctor, !!desc, desc && desc.writable,
                        desc && desc.enumerable, desc && desc.configurable,
                        desc && typeof desc.value];
                });
                function UserFunction() {}
                const user = Object.getOwnPropertyDescriptor(UserFunction, 'prototype');
                return JSON.stringify({ rows, userWritable: user.writable });
            })()
            "#,
        )
        .unwrap();

    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    for row in value["rows"].as_array().unwrap() {
        assert_eq!(
            row[1], "function",
            "{} must be a constructor object",
            row[0]
        );
        assert_eq!(row[2], true, "{} must own .prototype", row[0]);
        assert_eq!(row[3], false, "{}.prototype must be read-only", row[0]);
        assert_eq!(row[4], false, "{}.prototype must be non-enumerable", row[0]);
        assert_eq!(
            row[5], false,
            "{}.prototype must be non-configurable",
            row[0]
        );
        assert_eq!(row[6], "object", "{}.prototype must be an object", row[0]);
    }
    assert_eq!(
        value["userWritable"], true,
        "page-created ordinary functions must retain normal JS prototype writability"
    );
}

#[tokio::test]
async fn existing_webidl_operations_and_accessors_are_non_constructable() {
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
                const samples = [
                    ['AbstractRange.collapsed', Object.getOwnPropertyDescriptor(AbstractRange.prototype, 'collapsed').get],
                    ['Blob.arrayBuffer', Blob.prototype.arrayBuffer],
                    ['BroadcastChannel.postMessage', BroadcastChannel.prototype.postMessage],
                    ['CharacterData.after', CharacterData.prototype.after],
                    ['Element.querySelector', Element.prototype.querySelector],
                    ['Request.signal', Object.getOwnPropertyDescriptor(Request.prototype, 'signal').get],
                    ['WebSocket.send', WebSocket.prototype.send],
                    ['XMLSerializer.serializeToString', XMLSerializer.prototype.serializeToString],
                ];
                const rows = samples.map(([label, fn]) => {
                    let constructable = true;
                    try { Reflect.construct(function(){}, [], fn); }
                    catch (_) { constructable = false; }
                    return {
                        label,
                        own: Reflect.ownKeys(fn).map(String).sort(),
                        constructable,
                        source: Function.prototype.toString.call(fn),
                    };
                });
                let blobCall = 'THREW';
                try { blobCall = Object.prototype.toString.call(Blob.prototype.text.call({})); }
                catch (_) {}
                let requestBrand = 'NO_THROW';
                try { Request.prototype.clone.call({}); }
                catch (error) { requestBrand = error.name; }
                return JSON.stringify({
                    rows,
                    blobCall,
                    requestBrand,
                    urlSearchParamsAlias:
                        URLSearchParams.prototype.entries === URLSearchParams.prototype[Symbol.iterator],
                    formDataAlias:
                        FormData.prototype.entries === FormData.prototype[Symbol.iterator],
                });
            })()
            "#,
        )
        .unwrap();

    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    for row in value["rows"].as_array().unwrap() {
        assert_eq!(row["constructable"], false, "{}", row["label"]);
        assert_eq!(
            row["own"],
            serde_json::json!(["length", "name"]),
            "{}",
            row["label"]
        );
        assert!(
            row["source"]
                .as_str()
                .unwrap_or_default()
                .contains("[native code]"),
            "{}",
            row["label"]
        );
    }
    assert_eq!(value["blobCall"], "[object Promise]");
    assert_eq!(value["requestBrand"], "TypeError");
    assert_eq!(value["urlSearchParamsAlias"], true);
    assert_eq!(value["formDataAlias"], true);
}

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
