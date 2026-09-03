//! Live Cloudflare Turnstile end-to-end validation.
//!
//! Network-dependent and intentionally ignored by default. Run with:
//! `BROWSER_OXIDE_FRAME_TREE=1 cargo test -p browser_oxide --test widget_live -- --ignored --test-threads=1 --nocapture`
//!
//! Production-mode check against any real sitekey (the engine's clean path —
//! no seed wrappers, no probe hooks):
//! `BROWSER_OXIDE_TARGET='https://accounts.x.ai/sign-up' BROWSER_OXIDE_SK='0x4AAAAAAAhr9JGVDZbrZOo0' cargo test -p browser_oxide --test widget_live -- --ignored --nocapture`

use std::time::Duration;

const DEFAULT_DEMO_URL: &str = "https://turnstiletestingkeyformprotection.pages.dev/";
const ALWAYS_PASS_SITEKEY: &str = "1x00000000000000000000AA";
const DUMMY_TOKEN: &str = "XXXX.DUMMY.TOKEN.XXXX";

#[tokio::test]
#[ignore = "live network test against Cloudflare Turnstile"]
async fn cloudflare_turnstile_always_passes() {
    let url =
        std::env::var("BROWSER_OXIDE_TARGET").unwrap_or_else(|_| DEFAULT_DEMO_URL.to_string());
    let sitekey =
        std::env::var("BROWSER_OXIDE_SK").unwrap_or_else(|_| ALWAYS_PASS_SITEKEY.to_string());
    let always_pass = sitekey == ALWAYS_PASS_SITEKEY;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let widget_debug = std::env::var_os("BROWSER_OXIDE_WIDGET_DEBUG").is_some();
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
                globalThis.__browser_oxide_debug={widget_debug};
                globalThis.__oxTsResult={{callback:false,token:'',error:''}};
                window.addEventListener('message',function(ev){{
                    try{{
                        if(ev.data&&typeof ev.data==='object'&&ev.data.__oxEvalSrc){{
                            var arr=globalThis.__oxParentEvalSrc=(globalThis.__oxParentEvalSrc||[]);
                            var d=ev.data.__oxEvalSrc,dup=false;
                            for(var i=0;i<arr.length;i++){{if(arr[i]&&arr[i].code&&arr[i].code.length===d.code.length){{dup=true;break;}}}}
                            if(!dup&&arr.length<6)arr.push(d);
                            return;
                        }}
                        var log=globalThis.__oxParentMsgLog=(globalThis.__oxParentMsgLog||[]);
                        var entry=String(ev.origin||'')+'|'+String(
                            typeof ev.data==='string'?ev.data:(JSON.stringify(ev.data)||'typeof'+typeof ev.data)
                        ).slice(0,240);
                        var shape='?';
                        try{{shape=(typeof ev.data==='object'&&ev.data)?Object.keys(ev.data).slice(0,6).join(','):String(typeof ev.data);}}catch(_x){{}}
                        var last=log.length?log[log.length-1]:'';
                        if(last&&shape==='event,seq'&&entry.indexOf('food')>=0&&last.indexOf('food')>=0){{
                            var at=last.lastIndexOf(' x');var base=at>0?last.slice(0,at):last;var cnt=at>0?(parseInt(last.slice(at+2),10)||1):1;
                            log[log.length-1]=base+' x'+(cnt+1);
                        }} else if(log.length<40){{log.push(entry);}}
                    }}catch(_e){{
                    }}
                }});
                window.addEventListener('error',function(ev){{
                    try{{
                        var log=globalThis.__oxParentErr=(globalThis.__oxParentErr||[]);
                        if(log.length<10)log.push(String((ev&&ev.message)||'non-uncaught'));
                    }}catch(_e){{
                    }}
                }});
                var host=document.createElement('div');
                host.id='browser-oxide-always-pass';
                document.body.appendChild(host);
                globalThis.__oxTsWidgetId=turnstile.render(host,{{
                    sitekey:'{sitekey}',
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
    // The managed-challenge flow used by production keys runs notably longer
    // than the always-pass demo key; give it the same budget the probe uses.
    let attempts = if always_pass { 48 } else { 240 };
    for _ in 0..attempts {
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
    if !completed {
        let top_state = page
            .evaluate(
                r#"JSON.stringify({
                    api:typeof globalThis.turnstile,
                    result:globalThis.__oxTsResult||null,
                    widget:globalThis.__oxTsWidgetId||'',
                    response:(globalThis.turnstile&&globalThis.__oxTsWidgetId)
                        ? (turnstile.getResponse(__oxTsWidgetId)||'') : '',
                    hidden:Array.from(document.querySelectorAll('input[name="cf-turnstile-response"]'))
                        .map(function(input){return input.value||'';}),
                    iframes:Array.from(document.querySelectorAll('iframe')).map(function(frame){
                        return {id:frame.id||'',name:frame.name||'',src:frame.src||frame.getAttribute('src')||'',connected:frame.isConnected};
                    }),
                    shadows:Array.from(globalThis.__oxideShadowDebug||[]).map(function(root){
                        var frames=[];
                        try { frames=Array.from(root.querySelectorAll('iframe')).map(function(frame){
                            return {src:frame.src||frame.getAttribute('src')||'',connected:frame.isConnected,hasWindow:!!frame.contentWindow};
                        }); } catch (_) {}
                        return {mode:root.mode||'',html:String(root.innerHTML||'').slice(0,1200),iframes:frames};
                    }),
                    frameDebug:(globalThis.__oxideFrameDebug||[]).slice(-80),
                    frameMappings:Object.assign({},globalThis.__frameIdForNode||{}),
                    hostOuter:(function(){var h=document.getElementById('browser-oxide-always-pass');return h?h.outerHTML.slice(0,500):'MISSING';})(),
                    hostShadow:(function(){var h=document.getElementById('browser-oxide-always-pass');return (h&&h.shadowRoot)?String(h.shadowRoot.innerHTML).slice(0,600):'none';})(),
                    scriptErrors:(globalThis.__scriptErrors||[]).slice(0,8),
                    rejections:(globalThis.__oxRejLog||[]).slice(0,8),
                    parentMsgLog:(globalThis.__oxParentMsgLog||[]).slice(-14),
                    parentErr:(globalThis.__oxParentErr||[]).slice(0,6)
                })"#,
            )
            .unwrap_or_else(|error| format!("EVAL_ERROR:{error}"));
        eprintln!("TURNSTILE_TIMEOUT_TOP={top_state}");
        let eval_src = page.evaluate("JSON.stringify(globalThis.__oxParentEvalSrc||[])").unwrap_or_default();
        if eval_src.len() > 4 {
            eprintln!("TURNSTILE_EVAL_SRC={eval_src}");
        }
        eprintln!("TURNSTILE_TIMEOUT_FRAMES={}", page.frame_tree_count());
        for index in 0..page.frame_tree_count() {
            let state = page
                .frame_tree_evaluate(
                    index,
                    r#"JSON.stringify({
                        href:String(location.href),
                        ready:document.readyState,
                        bodyLength:(document.body&&document.body.innerHTML||'').length,
                        big:(function(){var s=document.querySelectorAll('script'),b='';for(var i=0;i<s.length;i++){var t=s[i].textContent||'';if(t.length>b.length)b=t;}var p=b.indexOf('uaqdv');return [b.length,p,b.slice(0,120),b.slice(-120)].join('~');})(),
                        msgLog:(function(){try{return (globalThis.__oxFrameMsgLog||[]).join(' ;; ').slice(0,1500);}catch(e){return 'E';}})(),
                        scriptErrors:(globalThis.__scriptErrors||[]).slice(0,5),
                        rejections:(globalThis.__oxRejLog||[]).slice(0,5)
                    })"#,
                )
                .unwrap_or_else(|| "<missing frame>".to_string());
            eprintln!("TURNSTILE_TIMEOUT_FRAME[{index}]={state}");
        }
    }
    let last_frame = page
        .frame_tree_evaluate(
            0,
            r#"JSON.stringify({
                href:String(location.href),
                ready:document.readyState,
                bodyLength:(document.body&&document.body.innerHTML||'').length,
                big:(function(){var s=document.querySelectorAll('script'),b='';for(var i=0;i<s.length;i++){var t=s[i].textContent||'';if(t.length>b.length)b=t;}var p=b.indexOf('uaqdv');return [b.length,p,b.slice(0,120),b.slice(-120)].join('~');})(),
                hasStage:!!document.getElementById('stage'),
                msgLog:(function(){try{return (globalThis.__oxFrameMsgLog||[]).join(' ;; ').slice(0,1500);}catch(e){return 'E';}})(),
                bigFull:(function(){var s=document.querySelectorAll('script'),b='';for(var i=0;i<s.length;i++){var t=s[i].textContent||'';if(t.length>b.length)b=t;}return b;})(),
               CE:(function(){try{return (window.__oxCEc||0)+'/'+(document.body?document.body.children.length:0);}catch(e){return 'E';}})()
            })"#,
        )
        .unwrap_or_else(|| "<missing frame>".to_string());
    eprintln!("TURNSTILE_LAST_FRAME[0]={last_frame}");
    assert!(completed, "always-pass Turnstile callback did not fire");

    let verification = page
        .evaluate(
            r#"JSON.stringify({
                callback:!!__oxTsResult.callback,
                token:__oxTsResult.token,
                error:__oxTsResult.error,
                response:turnstile.getResponse(__oxTsWidgetId)||'',
                hidden:Array.from(document.querySelectorAll('input[name="cf-turnstile-response"]'))
                    .some(function(input){var v=input.value||'';return !!v&&v===__oxTsResult.token;})
            })"#,
        )
        .expect("read Turnstile result");
    if always_pass {
        assert_eq!(
            verification,
            format!(
                r#"{{"callback":true,"token":"{DUMMY_TOKEN}","error":"","response":"{DUMMY_TOKEN}","hidden":true}}"#
            )
        );
    } else {
        assert!(
            verification.contains(r#""callback":true"#)
                && verification.contains(r#""error":""#)
                && verification.contains(r#""hidden":true"#)
                && !verification.contains(r#""token":""#),
            "real-sitekey run incomplete: {verification}"
        );
        assert!(
            !verification.contains(DUMMY_TOKEN),
            "dummy token in real-sitekey run: {verification}"
        );
    }

    let token = page
        .evaluate("String(__oxTsResult&&__oxTsResult.token||'')")
        .unwrap_or_default();
    println!(
        "TURNSTILE_E2E_PASS url={url} key={sitekey} frames={} token={token}",
        page.frame_tree_count()
    );
}
