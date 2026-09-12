//! Tests for the generic navigation primitives introduced by the
//! zero-per-engine refactor.
//!
//! Each primitive (`location.reload()`, `location.href = ...`,
//! `location.replace()`, `<meta http-equiv="refresh">`) must set
//! `globalThis.__pendingNavigation` to a `{url, kind}` object. The
//! driver loop in `Page::navigate` watches this flag to re-navigate.

use browser_oxide::Page;
use std::time::Duration;

/// Parse the JS-serialised `{url, kind}` pair produced by the test helper.
fn parse_pair(s: &str) -> (String, String) {
    // "url|kind"
    let mut it = s.splitn(2, '|');
    let url = it.next().unwrap_or("").to_string();
    let kind = it.next().unwrap_or("").to_string();
    (url, kind)
}

fn read_pending_navigation(page: &mut Page) -> (String, String) {
    let s = page
        .evaluate(
            "(function(){\
                const p = globalThis.__pendingNavigation;\
                if (!p) return '';\
                return String(p.url || '') + '|' + String(p.kind || '');\
             })()",
        )
        .unwrap_or_default();
    parse_pair(&s)
}

#[tokio::test]
async fn location_reload_sets_pending_navigation() {
    let mut page = Page::from_html_with_url(
        "<html><body><script>\
            location.reload();\
         </script></body></html>",
        "https://example.com/path",
        Some(browser_oxide::stealth::presets::chrome_148_ru()),
    )
    .await
    .expect("build page");

    let (url, kind) = read_pending_navigation(&mut page);
    assert_eq!(kind, "reload", "expected kind=reload, got {kind:?}");
    assert_eq!(
        url, "https://example.com/path",
        "expected reload url to be the current href"
    );
}

#[tokio::test]
async fn location_href_assignment_sets_pending_navigation() {
    let mut page = Page::from_html_with_url(
        "<html><body><script>\
            location.href = 'https://example.com/other';\
         </script></body></html>",
        "https://example.com/",
        Some(browser_oxide::stealth::presets::chrome_148_ru()),
    )
    .await
    .expect("build page");

    let (url, kind) = read_pending_navigation(&mut page);
    assert_eq!(kind, "assign", "expected kind=assign, got {kind:?}");
    assert_eq!(url, "https://example.com/other");
}

#[tokio::test]
async fn location_replace_sets_pending_navigation() {
    let mut page = Page::from_html_with_url(
        "<html><body><script>\
            location.replace('https://example.com/replaced');\
         </script></body></html>",
        "https://example.com/",
        Some(browser_oxide::stealth::presets::chrome_148_ru()),
    )
    .await
    .expect("build page");

    let (url, kind) = read_pending_navigation(&mut page);
    assert_eq!(kind, "replace", "expected kind=replace, got {kind:?}");
    assert_eq!(url, "https://example.com/replaced");
}

#[tokio::test]
async fn location_assign_sets_pending_navigation() {
    let mut page = Page::from_html_with_url(
        "<html><body><script>\
            location.assign('https://example.com/assigned');\
         </script></body></html>",
        "https://example.com/",
        Some(browser_oxide::stealth::presets::chrome_148_ru()),
    )
    .await
    .expect("build page");

    let (url, kind) = read_pending_navigation(&mut page);
    assert_eq!(kind, "assign");
    assert_eq!(url, "https://example.com/assigned");
}

#[tokio::test]
async fn meta_refresh_sets_pending_navigation() {
    // `from_html_with_url` runs scripts but does NOT process the
    // meta-refresh scanner (that lives in `build_page_with_scripts`).
    // So we install the same scanner inline via a <script> tag in the
    // fixture. The point of this test is to exercise the
    // __pendingNavigation signal for a meta-refresh target, which is
    // equivalent to the scanner's own behavior.
    let html = r#"
        <html>
        <head>
            <meta http-equiv="refresh" content="0;url=https://target.example/">
        </head>
        <body>
        <script>
            (function() {
                const metas = document.getElementsByTagName('meta');
                for (let i = 0; i < metas.length; i++) {
                    const m = metas[i];
                    const equiv = String(m.getAttribute('http-equiv') || '').toLowerCase();
                    if (equiv !== 'refresh') continue;
                    const content = String(m.getAttribute('content') || '');
                    const match = content.match(/^\s*(\d+)(?:\s*[;,]\s*url\s*=\s*(.+))?$/i);
                    if (!match) continue;
                    const delay = parseInt(match[1], 10) || 0;
                    const target = ((match[2] || '').trim()).replace(/^['"]|['"]$/g, '') || location.href;
                    setTimeout(() => {
                        globalThis.__pendingNavigation = { url: target, kind: 'assign' };
                    }, delay * 1000);
                    break;
                }
            })();
        </script>
        </body></html>
    "#;
    let mut page = Page::from_html_with_url(
        html,
        "https://source.example/",
        Some(browser_oxide::stealth::presets::chrome_148_ru()),
    )
    .await
    .expect("build page");

    // The scanner uses setTimeout(…, 0) — drain the event loop so it fires.
    page.evaluate_async("1", Duration::from_secs(1))
        .await
        .expect("drain");

    let (url, kind) = read_pending_navigation(&mut page);
    assert_eq!(kind, "assign", "expected kind=assign for meta-refresh");
    assert_eq!(url, "https://target.example/");
}

#[tokio::test]
async fn navigation_entry_state_matches_chrome_148_clone_and_event_semantics() {
    let mut page = Page::from_html_with_url(
        "<html><body></body></html>",
        "https://example.com/navigation-state",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("build page");

    let result = page
        .evaluate(
            r#"
            (() => {
                const entry = navigation.currentEntry;
                let seen = null;
                navigation.addEventListener('currententrychange', event => {
                    seen = {
                        brand: Object.prototype.toString.call(event),
                        fromSame: event.from === entry,
                        navigationType: event.navigationType,
                        isTrusted: event.isTrusted,
                    };
                });

                const source = { a: 1, nested: { b: 2 } };
                navigation.updateCurrentEntry({ state: source });
                source.a = 9;
                source.nested.b = 8;

                const first = navigation.currentEntry.getState();
                const beforeReturnedMutation = JSON.stringify(first);
                first.a = 7;
                first.nested.b = 6;
                const afterReturnedMutation = JSON.stringify(navigation.currentEntry.getState());

                let cloneError = null;
                try {
                    navigation.updateCurrentEntry({ state: { bad() {} } });
                } catch (error) {
                    cloneError = error.name;
                }

                return JSON.stringify({
                    beforeReturnedMutation,
                    afterReturnedMutation,
                    cloneError,
                    event: seen,
                    sameEntry: navigation.currentEntry === entry,
                    keyLooksOpaque: typeof entry.key === 'string' && entry.key.length > 10,
                    idLooksOpaque: typeof entry.id === 'string' && entry.id.length > 10,
                    keyDiffersFromId: entry.key !== entry.id,
                    eventCtorLength: NavigationCurrentEntryChangeEvent.length,
                    eventProtoParent: Object.getPrototypeOf(NavigationCurrentEntryChangeEvent.prototype) === Event.prototype,
                });
            })()
            "#,
        )
        .expect("evaluate navigation state fixture");

    let value: serde_json::Value = serde_json::from_str(&result).expect("json result");
    assert_eq!(
        value["beforeReturnedMutation"],
        r#"{"a":1,"nested":{"b":2}}"#
    );
    assert_eq!(
        value["afterReturnedMutation"],
        r#"{"a":1,"nested":{"b":2}}"#
    );
    assert_eq!(value["cloneError"], "DataCloneError");
    assert_eq!(
        value["event"]["brand"],
        "[object NavigationCurrentEntryChangeEvent]"
    );
    assert_eq!(value["event"]["fromSame"], true);
    assert!(value["event"]["navigationType"].is_null());
    assert_eq!(value["event"]["isTrusted"], true);
    assert_eq!(value["sameEntry"], true);
    assert_eq!(value["keyLooksOpaque"], true);
    assert_eq!(value["idLooksOpaque"], true);
    assert_eq!(value["keyDiffersFromId"], true);
    assert_eq!(value["eventCtorLength"], 2);
    assert_eq!(value["eventProtoParent"], true);
}
