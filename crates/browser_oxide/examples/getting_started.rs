//! Minimal "hello, stealth browser" example.
//!
//! Run:
//!   cargo run --release -p browser --example getting_started -- https://example.com
//!
//! Demonstrates the canonical usage pattern: `block_on_v8_thread` provides a
//! large-stack current-thread Tokio runtime for the `!Send` V8 engine, then a cold
//! `Page::navigate`, content/title extraction, an `evaluate()` call, and the
//! honest challenge verdict.

use browser_oxide::{ChallengeVerdict, Page};

fn main() {
    browser_oxide::js_runtime::block_on_v8_thread("getting-started", async_main);
}

async fn async_main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://example.com".to_string());

    // Pick a browser identity. See docs/guides/PROFILES.md for the full set.
    let profile = browser_oxide::stealth::presets::chrome_148_macos();

    // Cold navigation. `max_iterations` bounds the redirect/challenge-retry
    // loop (5 is a sane default).
    let mut page = Page::navigate(&url, profile, 5)
        .await
        .expect("navigation failed");

    println!("url     : {}", page.url());
    println!("title   : {}", page.title());
    println!("bytes   : {}", page.content().len());
    println!(
        "h1      : {}",
        page.text_of("h1").unwrap_or_else(|| "(none)".into())
    );

    // Run arbitrary JS in the page realm.
    if let Ok(ua) = page.evaluate("navigator.userAgent") {
        println!("ua      : {ua}");
    }

    // The honest outcome: did we get real content, a thin shell, or a wall?
    let verdict = page.challenge_verdict();
    println!("verdict : {}", verdict.as_str());
    match verdict {
        ChallengeVerdict::Pass => println!("=> real content rendered"),
        ChallengeVerdict::ThinShell | ChallengeVerdict::RenderIncomplete => {
            println!("=> rendered, but body is a thin/SPA shell")
        }
        v if v.is_challenge() => println!("=> blocked by an anti-bot challenge"),
        _ => {}
    }
}
