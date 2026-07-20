// Debug-only: navigate with the V8 inspector enabled and keep driving the event
// loop so a CDP client can attach. Set BROWSER_OXIDE_INSPECT=1.
use std::time::Duration;
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let url = std::env::args().nth(1).expect("url");
    let secs: u64 = std::env::var("DRIVE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let pool = browser_oxide::PagePool::new(2);
            let mut page = pool.acquire(Some(profile.clone())).await.expect("acquire");
            page.navigate_warm(&url).await.expect("warm nav");
            eprintln!("[inspect_probe] navigated; driving for {secs}s (attach CDP now)");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
            while tokio::time::Instant::now() < deadline {
                let _ = page.event_loop().run_until_idle().await;
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            let r = page
                .evaluate("String((document.getElementById('root')||{}).childElementCount||0)")
                .unwrap_or_default();
            eprintln!("[inspect_probe] #root children at end: {r}");
        })
        .await;
}
