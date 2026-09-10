//! Safe live regression against Cloudflare's documented Turnstile test-key page.
//!
//! This test is network-dependent and ignored by default. It intentionally uses
//! only Cloudflare's public always-pass test configuration and never prints the
//! response value.

use std::time::{Duration, Instant};

const TEST_PAGE_URL: &str = "https://turnstiletestingkeyformprotection.pages.dev/";
const ALWAYS_PASS_SITEKEY: &str = "1x00000000000000000000AA";
const RESPONSE_NAME: &str = "cf-turnstile-response";

#[tokio::test]
#[ignore = "live network test using Cloudflare's public always-pass test key"]
async fn official_always_pass_populates_response_without_exposing_value() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).expect("http client");
    let mut page = browser_oxide::Page::navigate(TEST_PAGE_URL, profile.clone(), 2)
        .await
        .expect("navigate official Turnstile test page");

    let api_deadline = Instant::now() + Duration::from_secs(20);
    while !matches!(
        page.evaluate("typeof globalThis.turnstile").as_deref(),
        Ok("object")
    ) && Instant::now() < api_deadline
    {
        page.drive_frame_tree(&client, &profile).await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(400))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        page.evaluate("typeof globalThis.turnstile")
            .expect("evaluate Turnstile API"),
        "object",
        "Turnstile api.js did not initialize"
    );

    let widget_id = page
        .evaluate(&format!(
            r#"(function(){{
                globalThis.__officialTestResult={{callback:false,tokenLength:0,error:''}};
                var host=document.createElement('div');
                host.id='browser-oxide-official-testkey';
                document.body.appendChild(host);
                return String(turnstile.render(host,{{
                    sitekey:'{ALWAYS_PASS_SITEKEY}',
                    callback:function(token){{
                        __officialTestResult.callback=true;
                        __officialTestResult.tokenLength=String(token||'').length;
                    }},
                    'error-callback':function(code){{
                        __officialTestResult.error=String(code||'unknown');
                    }}
                }}));
            }})()"#
        ))
        .expect("render official always-pass widget");
    assert!(
        widget_id.starts_with("cf-chl-widget-"),
        "Turnstile did not return a widget id"
    );

    let mut completed = false;
    let callback_deadline = Instant::now() + Duration::from_secs(130);
    while Instant::now() < callback_deadline {
        page.drive_frame_tree(&client, &profile).await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(500))
            .await;
        if matches!(
            page.evaluate("String(!!globalThis.__officialTestResult.callback)")
                .as_deref(),
            Ok("true")
        ) {
            completed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(completed, "official always-pass callback did not fire");

    let verification = page
        .evaluate(&format!(
            r#"JSON.stringify((function(){{
                var result=globalThis.__officialTestResult||{{}};
                var responseLength=String(turnstile.getResponse({widget})||'').length;
                var hiddenLengths=Array.from(document.querySelectorAll('input[name="{response_name}"]'))
                    .map(function(input){{return String(input.value||'').length;}});
                return {{
                    callback:!!result.callback,
                    error:String(result.error||''),
                    tokenLength:Number(result.tokenLength||0),
                    responseLength:responseLength,
                    hiddenHasMatch:hiddenLengths.indexOf(responseLength)>=0
                }};
            }})())"#,
            widget = serde_json::to_string(&widget_id).expect("serialize widget id"),
            response_name = RESPONSE_NAME
        ))
        .expect("verify official always-pass result");
    let state: serde_json::Value =
        serde_json::from_str(&verification).expect("parse verification state");
    let token_len = state["tokenLength"].as_u64().unwrap_or(0);
    let response_len = state["responseLength"].as_u64().unwrap_or(0);

    assert_eq!(state["callback"], true, "{verification}");
    assert_eq!(state["error"], "", "{verification}");
    assert!(token_len > 0, "{verification}");
    assert_eq!(response_len, token_len, "{verification}");
    assert_eq!(state["hiddenHasMatch"], true, "{verification}");
    println!(
        "TURNSTILE_OFFICIAL_TESTKEY_PASS frames={} response_len={response_len}",
        page.frame_tree_count()
    );
}
