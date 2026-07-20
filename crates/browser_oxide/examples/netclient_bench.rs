//! Per-navigation HTTP-client acquisition cost (no network): building a fresh
//! client (BoringSSL TLS connector + QUIC + empty pools) vs cloning a cached
//! one. The clone shares the pool (Arc), so keep-alive survives across navs.
//!
//!   cargo run --release --example netclient_bench -p browser_oxide

use browser_oxide::net::HttpClient;
use browser_oxide::stealth::presets::chrome_148_windows;
use std::time::{Duration, Instant};

fn report(label: &str, mut v: Vec<Duration>) {
    v.sort_unstable();
    let median = v[v.len() / 2];
    println!(
        "{label:<36} median={median:>10.3?}  min={:>10.3?}  (n={})",
        v[0],
        v.len()
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let profile = chrome_148_windows();
            for _ in 0..3 {
                let _ = HttpClient::shared(&profile).unwrap();
            }
            let base = HttpClient::shared(&profile).unwrap();

            let n = 50;
            let mut build = Vec::with_capacity(n);
            let mut clone = Vec::with_capacity(n);
            for _ in 0..n {
                let t = Instant::now();
                let c = HttpClient::shared(&profile).unwrap();
                build.push(t.elapsed());
                std::hint::black_box(&c);
            }
            for _ in 0..n {
                let t = Instant::now();
                let c = base.clone();
                clone.push(t.elapsed());
                std::hint::black_box(&c);
            }

            println!("\n=== per-nav HTTP client acquisition (no network) ===");
            report("HttpClient::shared (fresh build)", build);
            report("cached client.clone", clone);
        })
        .await;
}
