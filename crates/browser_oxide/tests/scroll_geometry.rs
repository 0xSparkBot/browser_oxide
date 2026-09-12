use browser_oxide::stealth::presets::chrome_148_macos;
use browser_oxide::Page;

const HTML: &str = r#"<!doctype html><html><body style="margin:0;padding:0">
<div id="spacer" style="height:1200px"></div>
<div id="target" style="height:100px;width:200px"></div>
<div style="height:1200px"></div>
</body></html>"#;

#[tokio::test]
async fn bounding_rect_tracks_viewport_scroll_and_scroll_into_view() {
    let mut page = Page::from_html(HTML, Some(chrome_148_macos()))
        .await
        .expect("page");

    let result = page
        .evaluate(
            r#"(() => {
                const t = document.getElementById('target');
                const a = t.getBoundingClientRect();
                window.scrollTo(0, 200);
                const b = t.getBoundingClientRect();
                const afterManual = { y: scrollY, top: b.top };

                t.scrollIntoView();
                const c = t.getBoundingClientRect();
                const start = { y: scrollY, top: c.top };

                window.scrollTo(0, 0);
                t.scrollIntoView({ block: 'center' });
                const d = t.getBoundingClientRect();
                const center = { y: scrollY, mid: d.top + d.height / 2, viewportMid: innerHeight / 2 };

                return JSON.stringify({
                    initialTop: a.top,
                    offsetTop: t.offsetTop,
                    afterManual,
                    start,
                    center,
                });
            })()"#,
        )
        .expect("evaluate");

    let v: serde_json::Value = serde_json::from_str(&result).expect("json");
    let initial = v["initialTop"].as_f64().unwrap();
    let manual_top = v["afterManual"]["top"].as_f64().unwrap();
    assert!(
        initial > 1000.0,
        "fixture target should be below the fold: {v}"
    );
    assert_eq!(v["afterManual"]["y"].as_f64().unwrap(), 200.0);
    assert!(
        (manual_top - (initial - 200.0)).abs() < 0.01,
        "getBoundingClientRect must be viewport-relative: {v}"
    );
    assert!(
        v["start"]["top"].as_f64().unwrap().abs() < 0.01,
        "default scrollIntoView must align the target to the viewport start: {v}"
    );
    assert!(
        (v["center"]["mid"].as_f64().unwrap() - v["center"]["viewportMid"].as_f64().unwrap()).abs()
            < 0.01,
        "block:center must center the target in the viewport: {v}"
    );
    assert_eq!(
        v["offsetTop"].as_f64().unwrap(),
        initial,
        "offsetTop stays document-relative when viewport scrolling changes"
    );
}

#[tokio::test]
async fn scroll_into_view_options_and_child_realm_geometry() {
    let mut page = Page::from_html(HTML, Some(chrome_148_macos()))
        .await
        .expect("page");

    let options = page
        .evaluate(
            r#"(() => {
                const t = document.getElementById('target');
                window.scrollTo(0, 0);
                t.scrollIntoView(false);
                const end = {
                    y: scrollY,
                    bottom: t.getBoundingClientRect().bottom,
                    viewport: innerHeight,
                };

                const beforeNearest = scrollY;
                t.scrollIntoView({ block: 'nearest' });
                const afterNearest = scrollY;

                let invalidBlock = '';
                let invalidBehavior = '';
                try { t.scrollIntoView({ block: 'sideways' }); } catch (e) { invalidBlock = e.name; }
                try { t.scrollIntoView({ behavior: 'teleport' }); } catch (e) { invalidBehavior = e.name; }

                return JSON.stringify({ end, beforeNearest, afterNearest, invalidBlock, invalidBehavior });
            })()"#,
        )
        .expect("evaluate");
    let v: serde_json::Value = serde_json::from_str(&options).expect("json");
    assert!(
        (v["end"]["bottom"].as_f64().unwrap() - v["end"]["viewport"].as_f64().unwrap()).abs()
            < 0.01,
        "boolean false must align the target to the viewport end: {v}"
    );
    assert_eq!(
        v["beforeNearest"], v["afterNearest"],
        "nearest must not move an already-visible target: {v}"
    );
    assert_eq!(v["invalidBlock"], "TypeError");
    assert_eq!(v["invalidBehavior"], "TypeError");

    page.evaluate(
        r#"(() => {
            const f = document.createElement('iframe');
            f.srcdoc = '<body style="margin:0"><div style="height:900px"></div><div id="ct" style="height:40px"></div><div style="height:900px"></div></body>';
            document.body.appendChild(f);
            void f.contentWindow;
        })()"#,
    )
    .expect("create frame");

    let child = page
        .evaluate(
            r#"(() => {
                const w = document.querySelector('iframe').contentWindow;
                const t = w.document.getElementById('ct');
                const initial = t.getBoundingClientRect().top;
                w.scrollY = 100;
                w.pageYOffset = 100;
                const after = t.getBoundingClientRect().top;
                t.scrollIntoView();
                return JSON.stringify({ initial, after, y: w.scrollY, top: t.getBoundingClientRect().top });
            })()"#,
        )
        .expect("child evaluate");
    let c: serde_json::Value = serde_json::from_str(&child).expect("child json");
    assert!(
        (c["after"].as_f64().unwrap() - (c["initial"].as_f64().unwrap() - 100.0)).abs() < 0.01,
        "child getBoundingClientRect must be viewport-relative: {c}"
    );
    assert_eq!(
        c["y"].as_f64().unwrap(),
        c["initial"].as_f64().unwrap(),
        "child scrollIntoView must update the child realm's own scroll position: {c}"
    );
    assert!(
        c["top"].as_f64().unwrap().abs() < 0.01,
        "child scrollIntoView must use the child viewport: {c}"
    );
}
