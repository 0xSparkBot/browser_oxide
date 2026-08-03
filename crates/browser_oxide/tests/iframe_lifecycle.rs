use browser_oxide::dom::node::NodeId;
use browser_oxide::iframe::ChildIframe;

#[tokio::test]
async fn child_iframe_completes_document_lifecycle() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"
        <html><head><script>
            globalThis.__dclSeen = false;
            globalThis.__loadSeen = false;
            document.addEventListener('DOMContentLoaded', function(){ __dclSeen = true; });
            window.addEventListener('load', function(){ __loadSeen = true; });
        </script></head><body></body></html>
    "#;

    let mut child = ChildIframe::from_srcdoc(NodeId::DOCUMENT, html, &profile)
        .await
        .expect("child iframe");

    assert_eq!(child.evaluate("document.readyState").unwrap(), "complete");
    assert_eq!(child.evaluate("String(__dclSeen)").unwrap(), "true");
    assert_eq!(child.evaluate("String(__loadSeen)").unwrap(), "true");
}
