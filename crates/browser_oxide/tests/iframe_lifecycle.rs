use browser_oxide::Page;

#[tokio::test]
async fn child_iframe_completes_document_lifecycle() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"
        <html><head><script>
            globalThis.__dclSeen = false;
            globalThis.__loadSeen = false;
            globalThis.__lifecycle = [['script', document.readyState, false]];
            document.addEventListener('readystatechange', function(e){
                __lifecycle.push(['readystatechange', document.readyState, e.isTrusted]);
            });
            document.addEventListener('DOMContentLoaded', function(e){
                __dclSeen = true;
                __lifecycle.push(['DOMContentLoaded', document.readyState, e.isTrusted]);
            });
            window.addEventListener('load', function(e){
                __loadSeen = true;
                __lifecycle.push(['load', document.readyState, e.isTrusted]);
            });
        </script></head><body></body></html>
    "#;

    let mut page = Page::from_html("<!doctype html><html><body></body></html>", Some(profile))
        .await
        .expect("page");
    let srcdoc = serde_json::to_string(html).unwrap();
    page.evaluate(&format!(
        "const f=document.createElement('iframe');f.srcdoc={srcdoc};document.body.appendChild(f);void f.contentWindow;"
    ))
    .unwrap();
    let mut child = page.child_iframe(0).expect("frame context");

    assert_eq!(child.evaluate("document.readyState").unwrap(), "complete");
    assert_eq!(child.evaluate("String(__dclSeen)").unwrap(), "true");
    assert_eq!(child.evaluate("String(__loadSeen)").unwrap(), "true");
    assert_eq!(
        child.evaluate("JSON.stringify(__lifecycle)").unwrap(),
        r#"[["script","loading",false],["readystatechange","interactive",true],["DOMContentLoaded","interactive",true],["readystatechange","complete",true],["load","complete",true]]"#
    );
}
