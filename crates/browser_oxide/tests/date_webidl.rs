use browser_oxide::Page;

#[tokio::test]
async fn patched_date_methods_keep_native_callable_shape_and_profile_timezone() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/",
        Some(profile),
    )
    .await
    .unwrap();

    let shape = page
        .evaluate(
            r#"
            JSON.stringify([
                'getTimezoneOffset', 'toString', 'toDateString',
                'toTimeString', 'toLocaleString'
            ].map(name => {
                const fn = Date.prototype[name];
                let constructable = true;
                try { Reflect.construct(function(){}, [], fn); }
                catch (_) { constructable = false; }
                let inheritedReceiver = 'NO_THROW';
                try { Object.create(fn).toString(); }
                catch (error) { inheritedReceiver = error.name; }
                return {
                    name: fn.name,
                    own: Object.getOwnPropertyNames(fn).sort(),
                    hasPrototype: 'prototype' in fn,
                    constructable,
                    source: Function.prototype.toString.call(fn),
                    inheritedReceiver,
                };
            }))
            "#,
        )
        .unwrap();

    let expected = r#"[{"name":"getTimezoneOffset","own":["length","name"],"hasPrototype":false,"constructable":false,"source":"function getTimezoneOffset() { [native code] }","inheritedReceiver":"TypeError"},{"name":"toString","own":["length","name"],"hasPrototype":false,"constructable":false,"source":"function toString() { [native code] }","inheritedReceiver":"TypeError"},{"name":"toDateString","own":["length","name"],"hasPrototype":false,"constructable":false,"source":"function toDateString() { [native code] }","inheritedReceiver":"TypeError"},{"name":"toTimeString","own":["length","name"],"hasPrototype":false,"constructable":false,"source":"function toTimeString() { [native code] }","inheritedReceiver":"TypeError"},{"name":"toLocaleString","own":["length","name"],"hasPrototype":false,"constructable":false,"source":"function toLocaleString() { [native code] }","inheritedReceiver":"TypeError"}]"#;
    assert_eq!(shape, expected);

    let offset = page
        .evaluate("String(new Date('2026-07-01T12:00:00Z').getTimezoneOffset())")
        .unwrap();
    assert_eq!(offset, "420", "Los Angeles summer offset must be UTC-7");

    let rendered = page
        .evaluate("new Date('2026-07-01T12:00:00Z').toString()")
        .unwrap();
    assert!(rendered.contains("GMT-0700"));
    assert!(rendered.contains("Pacific Daylight Time"));
}
