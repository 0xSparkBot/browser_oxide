//! Micro-benchmark: per-script drain cost of `drain_microtasks()` vs a
//! 50ms-bounded `run_until_idle()`, on one warm isolate with no network.
//! Run: cargo run --release --example drain_bench -p browser_oxide

use browser_oxide::stealth::presets::chrome_148_windows;
use browser_oxide::Page;
use std::time::{Duration, Instant};

fn report(label: &str, mut v: Vec<Duration>) {
    v.sort_unstable();
    let median = v[v.len() / 2];
    let mean = v.iter().sum::<Duration>() / v.len() as u32;
    let p99 = v[v.len() * 99 / 100];
    println!(
        "{label:<44} median={median:>10.3?}  mean={mean:>10.3?}  p99={p99:>10.3?}  (n={})",
        v.len()
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let mut page = Page::from_html_fast(
                "<html><body><div id=x>a</div></body></html>",
                "about:blank",
                chrome_148_windows(),
            )
            .await
            .unwrap();
            let el = page.event_loop();

            let micro =
                "Promise.resolve().then(()=>{document.getElementById('x').textContent='y';})";
            let timer = "setTimeout(()=>{document.getElementById('x').textContent='z';}, 5)";

            // Warm up JIT / caches.
            for _ in 0..200 {
                el.execute_script(micro).ok();
                el.drain_microtasks();
            }

            let n = 3000;

            let mut new_micro = Vec::with_capacity(n);
            for _ in 0..n {
                el.execute_script(micro).ok();
                let t = Instant::now();
                el.drain_microtasks();
                new_micro.push(t.elapsed());
            }
            let mut old_micro = Vec::with_capacity(n);
            for _ in 0..n {
                el.execute_script(micro).ok();
                let t = Instant::now();
                let _ = tokio::time::timeout(Duration::from_millis(50), el.run_until_idle()).await;
                old_micro.push(t.elapsed());
            }

            let mut new_timer = Vec::with_capacity(n);
            for _ in 0..n {
                el.execute_script(timer).ok();
                let t = Instant::now();
                el.drain_microtasks();
                new_timer.push(t.elapsed());
            }
            let mut old_timer = Vec::with_capacity(n);
            for _ in 0..n {
                el.execute_script(timer).ok();
                let t = Instant::now();
                let _ = tokio::time::timeout(Duration::from_millis(50), el.run_until_idle()).await;
                old_timer.push(t.elapsed());
            }

            println!("\n=== per-script drain cost (lower = faster) ===");
            report("microtask render : drain_microtasks (NEW)", new_micro);
            report("microtask render : run_until_idle(50ms) (OLD)", old_micro);
            report("timer-start      : drain_microtasks (NEW)", new_timer);
            report("timer-start      : run_until_idle(50ms) (OLD)", old_timer);
        })
        .await;
}
