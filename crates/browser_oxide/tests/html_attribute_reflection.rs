use browser_oxide::Page;

#[tokio::test]
async fn hreflang_reflects_on_anchor_area_and_link_elements() {
    let html = r#"<!doctype html><html><head>
        <link id="link" rel="alternate" href="/fr" hreflang="FR-ca">
    </head><body>
        <a id="anchor" href="/en" hreflang="EN-us">English</a>
        <map name="m"><area id="area" href="/de" hreflang="de-DE"></map>
        <a id="missing" href="/missing">Missing</a>
    </body></html>"#;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile))
        .await
        .expect("page");

    let result = page
        .evaluate(
            r#"
            (() => {
                const a = document.getElementById('anchor');
                const area = document.getElementById('area');
                const link = document.getElementById('link');
                const missing = document.getElementById('missing');

                const shape = C => {
                    const d = Object.getOwnPropertyDescriptor(C.prototype, 'hreflang');
                    return d ? {
                        enumerable: d.enumerable,
                        configurable: d.configurable,
                        getter: typeof d.get,
                        setter: typeof d.set,
                        getLength: d.get.length,
                        setLength: d.set.length,
                    } : null;
                };

                const before = [a.hreflang, area.hreflang, link.hreflang];

                a.hreflang = 'fr-CA';
                area.hreflang = 'es-MX';
                link.hreflang = 'ja-JP';

                return JSON.stringify({
                    before,
                    after: [a.hreflang, area.hreflang, link.hreflang],
                    attrs: [
                        a.getAttribute('hreflang'),
                        area.getAttribute('hreflang'),
                        link.getAttribute('hreflang'),
                    ],
                    missing: missing.hreflang,
                    anchorShape: shape(HTMLAnchorElement),
                    areaShape: shape(HTMLAreaElement),
                    linkShape: shape(HTMLLinkElement),
                    misplaced: {
                        element: Object.prototype.hasOwnProperty.call(Element.prototype, 'hreflang'),
                        htmlElement: Object.prototype.hasOwnProperty.call(HTMLElement.prototype, 'hreflang'),
                    },
                });
            })()
            "#,
        )
        .expect("probe");

    assert_eq!(
        result,
        r#"{"before":["EN-us","de-DE","FR-ca"],"after":["fr-CA","es-MX","ja-JP"],"attrs":["fr-CA","es-MX","ja-JP"],"missing":"","anchorShape":{"enumerable":true,"configurable":true,"getter":"function","setter":"function","getLength":0,"setLength":1},"areaShape":{"enumerable":true,"configurable":true,"getter":"function","setter":"function","getLength":0,"setLength":1},"linkShape":{"enumerable":true,"configurable":true,"getter":"function","setter":"function","getLength":0,"setLength":1},"misplaced":{"element":false,"htmlElement":false}}"#
    );
}
