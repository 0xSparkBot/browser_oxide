//! Warm per-navigation CPU cost: reuse one isolate and time reloading a
//! realistic SPA-ish page (parse + inline scripts).
//!
//!   cargo run --release --example warm_bench -p browser_oxide

use browser_oxide::stealth::presets::chrome_148_windows;
use browser_oxide::Page;
use std::time::{Duration, Instant};

fn report(label: &str, mut v: Vec<Duration>) {
    v.sort_unstable();
    let median = v[v.len() / 2];
    let mean = v.iter().sum::<Duration>() / v.len() as u32;
    println!(
        "{label:<28} median={median:>10.3?}  mean={mean:>10.3?}  min={:>10.3?}  (n={})",
        v[0],
        v.len()
    );
}

// Renders into #root over a microtask chain, then runs a React-like setInterval.
fn spa_html() -> &'static str {
    r#"<!DOCTYPE html><html><head><title>SPA</title></head><body>
<div id="root"></div>
<script>
  (function(){
    var root = document.getElementById('root');
    Promise.resolve().then(function(){
      for (var i=0;i<40;i++){ var d=document.createElement('div'); d.className='row'; d.textContent='item '+i; root.appendChild(d); }
    });
    setInterval(function(){ /* scheduler tick, no DOM change */ }, 250);
  })();
</script>
</body></html>"#
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let mut page =
                Page::from_html_fast("<html></html>", "about:blank", chrome_148_windows())
                    .await
                    .unwrap();

            // warm up
            for _ in 0..10 {
                page.reload_html(spa_html(), "http://bench/p");
            }

            // reload_html parses + runs inline scripts synchronously (no `load`
            // dispatch), so this is the per-nav CPU cost.
            let n = 200;
            let mut reload = Vec::with_capacity(n);
            for _ in 0..n {
                let t0 = Instant::now();
                page.reload_html(spa_html(), "http://bench/p");
                reload.push(t0.elapsed());
            }
            let rows = page
                .evaluate("String(document.querySelectorAll('#root .row').length)")
                .unwrap_or_default();

            println!("\n=== warm per-nav CPU (reload SPA parse+scripts), rendered rows={rows} ===");
            report("reload_html", reload);
        })
        .await;
}
