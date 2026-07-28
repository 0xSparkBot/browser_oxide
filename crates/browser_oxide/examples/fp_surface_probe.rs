//! Read-only fingerprint-surface probe used for the FINGERPRINT_SURFACE.md audit.
//!
//! Prints, for a fixed profile:
//!   - the canvas `toDataURL` length + FNV-1a digest,
//!   - the `getImageData` digest for the SAME drawing,
//!   - the same pair for a profile with a DIFFERENT canvas_seed.
//!
//! The audit ran this to demonstrate the defect: encode-time seeded noise
//! moved the `toDataURL` digest with the seed while leaving `getImageData`
//! byte-identical, so the defence covered one API and not the other. Since
//! the noise was removed, all four digests are expected to be **stable and
//! seed-independent** — that is now the passing result, and a `toDataURL`
//! digest that moves with `canvas_seed` would be the regression.
//!
//! Alters no engine behaviour; it only evaluates script against a page.

use browser_oxide::Page;

const DRAW: &str = r#"
    const c = document.createElement('canvas');
    c.width = 200; c.height = 60;
    const ctx = c.getContext('2d');
    if (!ctx) throw new Error('NO_CTX');
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
"#;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

async fn probe(label: &str, profile: browser_oxide::stealth::StealthProfile) {
    let seed = profile.canvas_seed;
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        Some(profile),
    )
    .await
    .expect("page");

    let data_url = page
        .evaluate(&format!("(() => {{ {DRAW} return c.toDataURL(); }})()"))
        .expect("eval toDataURL");

    // Hash the raw pixels via getImageData, joined as a plain string so the
    // example needs no extra dependency.
    let pixels = page
        .evaluate(&format!(
            "(() => {{ {DRAW} const d = ctx.getImageData(0, 0, 200, 60).data; \
             let s = ''; for (let i = 0; i < d.length; i++) s += d[i] + ','; return s; }})()"
        ))
        .expect("eval getImageData");

    println!(
        "{label}: seed={seed:#018x}\n  toDataURL   len={} fnv1a={:016x}\n  getImageData len={} fnv1a={:016x}",
        data_url.len(),
        fnv1a(data_url.as_bytes()),
        pixels.len(),
        fnv1a(pixels.as_bytes()),
    );
}

/// Does the profile timezone actually reach V8's `Date`, or only the JS veneer
/// over `getTimezoneOffset` / `Date.prototype.toString`?
async fn probe_timezone(profile: browser_oxide::stealth::StealthProfile) {
    let tz = profile.timezone.clone();
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        Some(profile),
    )
    .await
    .expect("page");

    let out = page
        .evaluate(
            r#"(() => {
                const d = new Date();
                // Hours as V8 computes them from the *process* timezone.
                const localHours = d.getHours();
                // Hours as the profile timezone would give, read back out of
                // the patched Date.prototype.toString.
                const shimHours = parseInt(d.toString().split(' ')[4].split(':')[0], 10);
                return JSON.stringify({
                    intlTz: Intl.DateTimeFormat().resolvedOptions().timeZone,
                    getTimezoneOffset: d.getTimezoneOffset(),
                    dateToString: d.toString(),
                    getHours: localHours,
                    toStringHours: shimHours,
                    agree: localHours === shimHours,
                    toLocaleTimeString: d.toLocaleTimeString(),
                    ownToStringOnDateToString:
                        Object.getOwnPropertyNames(Date.prototype.toString).join(','),
                    intlName: Intl.DateTimeFormat.name,
                });
            })()"#,
        )
        .expect("eval tz");
    println!("profile timezone = {tz}\n  {out}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    probe("baseline  ", browser_oxide::stealth::chrome_148_macos()).await;

    let mut alt = browser_oxide::stealth::chrome_148_macos();
    alt.canvas_seed = 0x0123_4567_89ab_cdef;
    probe("alt-seed  ", alt).await;

    // The macOS preset claims America/Los_Angeles. If the host happens to sit in
    // that zone the comparison proves nothing, so also probe a profile whose
    // timezone cannot match the host: chrome_148_ru claims Europe/Moscow.
    probe_timezone(browser_oxide::stealth::chrome_148_macos()).await;
    probe_timezone(browser_oxide::stealth::chrome_148_ru()).await;
}
