//! Minimal child-isolate timer drive repro: does a bare setInterval in a
//! same-isolate srcdoc frame keep firing across repeated drive cycles?
//! Run: cargo run -p browser_oxide --example timer_repro --release

use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).expect("http client");
    let mut page = browser_oxide::Page::navigate("https://example.com/", profile.clone(), 2)
        .await
        .expect("navigation");

    // Same-origin iframe with a real URL — goes through the ChildIframe
    // (frame_tree) path, which is where the challenge isolate lives.
    page.evaluate(
        r#"(function(){
        var f = document.createElement('iframe');
        f.id = 'repro'; f.src = '/#repro';
        document.body.appendChild(f);
    })()"#,
    )
    .expect("insert iframe");

    // Materialize + seed on the first drive.
    page.drive_frame_tree(&client, &profile).await;
    let _ = page
        .event_loop()
        .run_until_settled(Duration::from_millis(300))
        .await;

    let seed_js = r#"(function(){
        if (globalThis.__B) return 'already';
        globalThis.__B = []; globalThis.__T = [];
        var now = function(){ try { return Math.round(performance.now()); } catch(e){ return -1; } };
        setTimeout(function(){ __B.push('t100'); __T.push(now()); }, 100);
        var iv = setInterval(function(){ __B.push('I'); __T.push(now()); }, 500);
        setTimeout(function(){ clearInterval(iv); }, 30000);
        return 'seeded';
    })()"#;
    let mut seeded = false;
    for poll in 0..30 {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        page.drive_frame_tree(&client, &profile).await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(500))
            .await;
        if !seeded && page.frame_tree_count() > 0 {
            seeded = page.frame_tree_evaluate(0, seed_js).is_some();
            println!("SEED@{poll}={seeded}");
            continue;
        }
        if poll % 3 == 0 {
            let s = page.frame_tree_evaluate(
                0,
                "JSON.stringify({n:globalThis.__B ? globalThis.__B.length : 'gone', t:(globalThis.__T||[]).slice(-3), fr:globalThis.__FR})",
            );
            println!("P{poll}={s:?}");
        }
    }
    println!("FRAMES={}", page.frame_tree_count());
}
