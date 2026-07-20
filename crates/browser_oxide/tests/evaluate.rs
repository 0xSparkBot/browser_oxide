//! evaluate() result fidelity — values, undefined, empty string, and thrown
//! exceptions must each be distinguishable.

use browser_oxide::Page;

fn html(body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head></head><body>{}</body></html>",
        body
    )
}

async fn page() -> Page {
    Page::from_html(
        &html(r#"<div id="root"></div>"#),
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn simple_value() {
    let mut p = page().await;
    assert_eq!(p.evaluate("String(42)").unwrap(), "42");
}

#[tokio::test]
async fn iife_touching_dom_returns_value() {
    let mut p = page().await;
    let yes = p
        .evaluate(
            "(function(){ var r = document.getElementById('root'); return r ? 'yes' : 'no'; })()",
        )
        .unwrap();
    assert_eq!(yes, "yes");
    let no = p
        .evaluate("(() => document.getElementById('nope') ? 'yes' : 'no')()")
        .unwrap();
    assert_eq!(no, "no");
}

#[tokio::test]
async fn thrown_exception_is_error_not_empty() {
    let mut p = page().await;
    let r = p.evaluate("(() => { throw new Error('boom'); })()");
    assert!(
        r.is_err(),
        "a throwing IIFE must surface as Err, not Ok(\"\")"
    );
    assert!(r.unwrap_err().to_string().contains("boom"));
}

#[tokio::test]
async fn undefined_and_empty_string_are_distinct() {
    let mut p = page().await;
    assert_eq!(p.evaluate("(() => {})()").unwrap(), "undefined");
    assert_eq!(p.evaluate("''").unwrap(), "");
}
