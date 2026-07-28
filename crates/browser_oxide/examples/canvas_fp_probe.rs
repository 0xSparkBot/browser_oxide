//! Prints the canvas data-URL length + digest for a fixed profile and a fixed
//! drawing sequence.
//!
//! **This is a renderer change detector, not a privacy claim.** The digest is
//! an A/B harness: run it on two checkouts and compare, and any difference
//! means something in the render or encode path moved — a `png` crate bump
//! that perturbs the IDAT bytes, a font-database change, a Skia upgrade, an
//! antialiasing tweak. Those are all changes we want to notice deliberately
//! rather than discover from a downstream diff.
//!
//! It is emphatically *not* evidence that the canvas surface is defended. It
//! once was read that way — the value was documented as a fingerprinting
//! invariant — but a stable hash is what a fingerprinter wants, not what
//! frustrates one. Its worth here is purely as a build-over-build tripwire.
//! What the value should be is "whatever a clean, deterministic render
//! produces today"; when it changes, the question to ask is *what moved in
//! the renderer*, not *is our privacy still intact*.

use browser_oxide::Page;

const CANVAS_FP_SEQUENCE_JS: &str = r#"(() => {
    const c = document.createElement('canvas');
    c.width = 200; c.height = 60;
    const ctx = c.getContext('2d');
    if (!ctx) return 'NO_CTX';
    ctx.textBaseline = 'top';
    ctx.font = '14px Arial';
    ctx.fillStyle = '#f60';
    ctx.fillRect(0, 0, 100, 30);
    ctx.fillStyle = '#069';
    ctx.fillText('browser_oxide', 2, 15);
    ctx.fillStyle = 'rgba(102, 204, 0, 0.7)';
    ctx.fillText('parity-test', 4, 17);
    ctx.beginPath();
    ctx.arc(150, 30, 12, 0, Math.PI * 2);
    ctx.fill();
    return c.toDataURL();
})()"#;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        Some(profile),
    )
    .await
    .expect("page");

    let data_url = page.evaluate(CANVAS_FP_SEQUENCE_JS).expect("eval");

    // Simple FNV-1a so this example carries no extra dependency and behaves
    // identically on both checkouts regardless of their sha2 version.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data_url.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    println!("len={} fnv1a={:016x}", data_url.len(), h);
}
