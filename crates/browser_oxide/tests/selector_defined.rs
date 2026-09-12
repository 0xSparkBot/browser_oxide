use browser_oxide::stealth::presets::chrome_148_macos;
use browser_oxide::Page;

#[tokio::test]
async fn defined_pseudo_class_tracks_custom_element_registration() {
    let html = r#"<!doctype html><html><body>
        <div id="host">
            <div id="builtin"></div>
            <foo id="unknown"></foo>
            <annotation-xml id="reserved"></annotation-xml>
            <x-defined-probe id="custom"></x-defined-probe>
            <svg xmlns="http://www.w3.org/2000/svg"><x-svg-probe id="svg-customish"/></svg>
        </div>
    </body></html>"#;

    let mut page = Page::from_html(html, Some(chrome_148_macos()))
        .await
        .expect("page");

    let before = page
        .evaluate(
            r#"JSON.stringify((() => {
                const host = document.getElementById('host');
                const custom = document.getElementById('custom');
                return {
                    builtin: document.getElementById('builtin').matches(':defined'),
                    unknown: document.getElementById('unknown').matches(':defined'),
                    reserved: document.getElementById('reserved').matches(':defined'),
                    custom: custom.matches(':defined'),
                    customNot: custom.matches(':not(:defined)'),
                    svgCustomish: document.getElementById('svg-customish').matches(':defined'),
                    ids: Array.from(host.querySelectorAll(':defined'), e => e.id).filter(Boolean),
                };
            })())"#,
        )
        .expect("before result");
    assert_eq!(
        before,
        r#"{"builtin":true,"unknown":true,"reserved":true,"custom":false,"customNot":true,"svgCustomish":true,"ids":["builtin","unknown","reserved","svg-customish"]}"#
    );

    page.evaluate(
        r#"class DefinedProbe extends HTMLElement {}
           customElements.define('x-defined-probe', DefinedProbe);"#,
    )
    .expect("define custom element");

    let after = page
        .evaluate(
            r#"JSON.stringify((() => {
                const host = document.getElementById('host');
                const custom = document.getElementById('custom');
                return {
                    custom: custom.matches(':defined'),
                    customNot: custom.matches(':not(:defined)'),
                    ctor: custom.constructor.name,
                    ids: Array.from(host.querySelectorAll(':defined'), e => e.id).filter(Boolean),
                };
            })())"#,
        )
        .expect("after result");
    assert_eq!(
        after,
        r#"{"custom":true,"customNot":false,"ctor":"DefinedProbe","ids":["builtin","unknown","reserved","custom","svg-customish"]}"#
    );
}

#[tokio::test]
async fn defined_pseudo_class_state_resets_with_reused_page() {
    let mut page = Page::from_html(
        r#"<!doctype html><html><body><x-reset-defined id="first"></x-reset-defined><script>
            class FirstDefined extends HTMLElement {}
            customElements.define('x-reset-defined', FirstDefined);
        </script></body></html>"#,
        Some(chrome_148_macos()),
    )
    .await
    .expect("first page");

    assert_eq!(
        page.evaluate("document.getElementById('first').matches(':defined')")
            .expect("first defined state"),
        "true"
    );

    assert!(page.reset_for_reuse(), "page should be reusable");
    page.reload_html(
        r#"<!doctype html><html><body><x-reset-defined id="second"></x-reset-defined></body></html>"#,
        "about:blank",
    );

    assert_eq!(
        page.evaluate("document.getElementById('second').matches(':defined')")
            .expect("reset defined state"),
        "false",
        "previous document's custom-element definition leaked into selector state"
    );

    page.evaluate(
        r#"class SecondDefined extends HTMLElement {}
           customElements.define('x-reset-defined', SecondDefined);"#,
    )
    .expect("redefine after reset");
    assert_eq!(
        page.evaluate("document.getElementById('second').matches(':defined')")
            .expect("second defined state"),
        "true"
    );
}

#[test]
fn defined_pseudo_class_parser_shape() {
    use browser_oxide::css_selectors::{parse_selector_list, PseudoClass, SimpleSelector};

    let list = parse_selector_list(":defined").expect(":defined parses");
    assert!(matches!(
        &list[0].components()[0],
        browser_oxide::css_selectors::Component::Simple(SimpleSelector::PseudoClass(
            PseudoClass::Defined
        ))
    ));
    assert!(parse_selector_list(":defined()").is_err());
}
