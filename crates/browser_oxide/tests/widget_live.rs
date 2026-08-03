//! Live Cloudflare Turnstile end-to-end validation.
//!
//! Network-dependent and intentionally ignored by default. Run with:
//! `BROWSER_OXIDE_FRAME_TREE=1 cargo test -p browser_oxide --test widget_live -- --ignored --test-threads=1 --nocapture`

use std::time::Duration;

const DEFAULT_DEMO_URL: &str = "https://turnstiletestingkeyformprotection.pages.dev/";
const ALWAYS_PASS_SITEKEY: &str = "1x00000000000000000000AA";
const DUMMY_TOKEN: &str = "XXXX.DUMMY.TOKEN.XXXX";

#[tokio::test]
#[ignore = "live network test against Cloudflare Turnstile"]
async fn cloudflare_turnstile_always_passes() {
    let url =
        std::env::var("BROWSER_OXIDE_TARGET").unwrap_or_else(|_| DEFAULT_DEMO_URL.to_string());
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).expect("http client");
    let mut page = browser_oxide::Page::navigate(&url, profile.clone(), 2)
        .await
        .unwrap_or_else(|error| panic!("navigation failed: {error}"));

    for _ in 0..16 {
        if matches!(
            page.evaluate("typeof globalThis.turnstile").as_deref(),
            Ok("object")
        ) {
            break;
        }
        page.drive_frame_tree(&client, &profile).await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(400))
            .await;
    }
    assert_eq!(
        page.evaluate("typeof globalThis.turnstile")
            .expect("evaluate Turnstile API"),
        "object",
        "Turnstile api.js did not initialize"
    );

    let render = page
        .evaluate(&format!(
            r#"(function(){{
                globalThis.__oxTsResult={{callback:false,token:'',error:''}};
                var host=document.createElement('div');
                host.id='browser-oxide-always-pass';
                document.body.appendChild(host);
                globalThis.__oxTsWidgetId=turnstile.render(host,{{
                    sitekey:'{ALWAYS_PASS_SITEKEY}',
                    callback:function(token){{
                        __oxTsResult.callback=true;
                        __oxTsResult.token=String(token||'');
                    }},
                    'error-callback':function(code){{
                        __oxTsResult.error=String(code||'unknown');
                    }}
                }});
                return String(globalThis.__oxTsWidgetId);
            }})()"#
        ))
        .expect("render always-pass widget");
    assert!(
        render.starts_with("cf-chl-widget-"),
        "Turnstile did not return a widget id: {render}"
    );

    let mut completed = false;
    for _ in 0..48 {
        page.drive_frame_tree(&client, &profile).await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(500))
            .await;
        if matches!(
            page.evaluate("String(!!(globalThis.__oxTsResult&&globalThis.__oxTsResult.callback))")
                .as_deref(),
            Ok("true")
        ) {
            completed = true;
            break;
        }
    }
    assert!(completed, "always-pass Turnstile callback did not fire");

    let verification = page
        .evaluate(
            r#"JSON.stringify({
                callback:!!__oxTsResult.callback,
                token:__oxTsResult.token,
                error:__oxTsResult.error,
                response:turnstile.getResponse(__oxTsWidgetId)||'',
                hidden:Array.from(document.querySelectorAll('input[name="cf-turnstile-response"]'))
                    .some(function(input){return input.value==='XXXX.DUMMY.TOKEN.XXXX';})
            })"#,
        )
        .expect("read Turnstile result");
    assert_eq!(
        verification,
        format!(
            r#"{{"callback":true,"token":"{DUMMY_TOKEN}","error":"","response":"{DUMMY_TOKEN}","hidden":true}}"#
        )
    );

    println!(
        "TURNSTILE_E2E_PASS url={url} frames={} token={DUMMY_TOKEN}",
        page.frame_tree_count()
    );
}
