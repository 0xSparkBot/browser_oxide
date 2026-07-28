//! Render a page to a PNG.
//!
//! ```text
//! cargo run --release -p browser_oxide --example screenshot -- https://example.com out.png
//! cargo run --release -p browser_oxide --example screenshot -- page.html out.png
//! ```
//!
//! The engine could not do this at all until the `render` module existed: Skia
//! was a dependency used only by `<canvas>`, and the CDP surface implemented 48
//! methods of which `Page.captureScreenshot` was not one.

use browser_oxide::Page;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let target = args.next().unwrap_or_else(|| {
        eprintln!("usage: screenshot <url|file.html> [out.png] [width] [height]");
        std::process::exit(2);
    });
    let out = args.next().unwrap_or_else(|| "screenshot.png".to_string());
    let width: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1280);
    let height: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(800);

    let mut page = if target.starts_with("http://") || target.starts_with("https://") {
        let profile = browser_oxide::stealth::presets::chrome_148_linux();
        let client = browser_oxide::net::HttpClient::new(&profile)?;
        Page::navigate_simple(&target, &client, profile).await?
    } else {
        let html = std::fs::read_to_string(&target)?;
        Page::from_html(&html, None).await?
    };

    let started = std::time::Instant::now();
    let png = page
        .screenshot_png(width, height)
        .ok_or("nothing to capture: the page has no document")?;
    let elapsed = started.elapsed();

    std::fs::write(&out, &png)?;
    println!(
        "wrote {out} — {width}x{height}, {} KiB, {:.1} ms",
        png.len() / 1024,
        elapsed.as_secs_f64() * 1000.0
    );
    Ok(())
}
