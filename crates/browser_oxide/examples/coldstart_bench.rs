//! Cold-start cost: a fresh V8 isolate plus full JS bootstrap. Run with or
//! without the startup snapshot:
//!
//!   cargo run --release --example coldstart_bench -p browser_oxide
//!   BROWSER_OXIDE_USE_SNAPSHOT=1 cargo run --release --example coldstart_bench -p browser_oxide

use browser_oxide::stealth::presets::chrome_148_windows;
use browser_oxide::Page;
use std::time::Instant;

fn main() {
    browser_oxide::js_runtime::block_on_v8_thread("coldstart-bench", async_main);
}

async fn async_main() {
    let snap = std::env::var_os("BROWSER_OXIDE_USE_SNAPSHOT").is_some();
    let n = 8;
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        let page = Page::from_html_fast(
            "<html><head></head><body></body></html>",
            "about:blank",
            chrome_148_windows(),
        )
        .await
        .unwrap();
        let e = t.elapsed();
        std::hint::black_box(&page);
        drop(page);
        times.push(e);
    }
    times.sort_unstable();
    println!(
        "cold isolate+bootstrap (snapshot={}): first={:?}  median={:?}  min={:?}  (n={})",
        snap,
        times[n - 1], // sorted max, not the first run
        times[n / 2],
        times[0],
        n
    );
    for (i, t) in times.iter().enumerate() {
        println!("  run[{i}] = {t:?}");
    }
}
