//! Turnstile DOM-sequence probe: replicates what `turnstile.render()` does
//! internally, step by step, to find which DOM primitive the engine misses.
//!
//! Run: `cargo run -p browser_oxide --example turnstile_probe -- --nocapture`
//! Optional: BROWSER_OXIDE_TARGET=<url> to probe another page.

use std::time::Duration;

// deno_core 0.408 drives ops on a current-thread tokio flavor only.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let url = std::env::var("BROWSER_OXIDE_TARGET")
        .unwrap_or_else(|_| "https://turnstiletestingkeyformprotection.pages.dev/".into());
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).expect("http client");
    let mut page = browser_oxide::Page::navigate(&url, profile.clone(), 2)
        .await
        .expect("navigation");

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
    println!("API={:?}", page.evaluate("typeof globalThis.turnstile"));

    // Enable engine debug ledgers + record Turnstile's DOM call sequence.
    page.evaluate(
        r#"globalThis.__browser_oxide_debug = true;
        globalThis.__oxApiCalls = [];
        (function(){
            var log = globalThis.__oxApiCalls;
            var oc = document.createElement.bind(document);
            document.createElement = function(tag, opts){
                log.push('createElement:' + String(tag).toLowerCase());
                return oc(tag, opts);
            };
            var oa = Element.prototype.attachShadow;
            Element.prototype.attachShadow = function(init){
                var tag = String(this.tagName || this.nodeName || '?');
                log.push('attachShadow:' + tag + ':' + JSON.stringify(init && {mode: init.mode}));
                try { var r = oa.apply(this, arguments); log.push('attachShadow:ok'); return r; }
                catch (e) { log.push('attachShadow:ERR:' + String(e && e.message || e)); throw e; }
            };
            var osp = Element.prototype.setAttribute;
            Element.prototype.setAttribute = function(n, v){
                if (String(n).toLowerCase() === 'src' && this.tagName === 'IFRAME') log.push('iframe.setAttribute.src:' + String(v).slice(0, 140));
                return osp.apply(this, arguments);
            };
            var d = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, 'src');
            if (d && d.set) {
                Object.defineProperty(HTMLIFrameElement.prototype, 'src', {
                    get: d.get,
                    set: function(v){ log.push('iframe.src=' + String(v).slice(0, 140)); return d.set.call(this, v); },
                    configurable: true,
                });
            }
            var oap = Node.prototype.appendChild;
            Node.prototype.appendChild = function(child){
                try {
                    var pt = String(this.nodeName || '?');
                    var ct = child && child.nodeName ? String(child.nodeName) : '?';
                    log.push('append:' + pt + '<' + ct + ':conn=' + !!(child && child.isConnected) + ',hostConn=' + !!(this.isConnected));
                } catch (_) {}
                return oap.apply(this, arguments);
            };
            var oib = Node.prototype.insertBefore;
            Node.prototype.insertBefore = function(child, ref){
                try {
                    var pt = String(this.nodeName || '?');
                    var ct = child && child.nodeName ? String(child.nodeName) : '?';
                    log.push('insertBefore:' + pt + '<' + ct + ':conn=' + !!(child && child.isConnected) + ',hostConn=' + !!(this.isConnected));
                } catch (_) {}
                return oib.apply(this, arguments);
            };
            if (Element.prototype.append) {
                var oelAppend = Element.prototype.append;
                Element.prototype.append = function(){
                    try {
                        var pts = [];
                        for (var i = 0; i < arguments.length; i++) pts.push(String(arguments[i] && arguments[i].nodeName || arguments[i]));
                        log.push('Element.append:' + String(this.nodeName) + '<' + pts.join(',') + ':hostConn=' + !!(this.isConnected));
                    } catch (_) {}
                    return oelAppend.apply(this, arguments);
                };
            }
        })();"#,
    )
    .expect("hook eval");

    // A) Smoke-test the DOM primitives Turnstile's render path uses.
    let smoke = page
        .evaluate(
            r#"(function(){
        var out = {};
        try {
            var host = document.createElement('div');
            document.body.appendChild(host);
            out.hostConnected = host.isConnected;
            var shadow = host.attachShadow({mode:'closed'});
            out.attachShadow = !!shadow && shadow instanceof ShadowRoot;
            var iframe = document.createElement('iframe');
            iframe.setAttribute('src','https://challenges.cloudflare.com/probe');
            out.iframeSrcAttr = String(iframe.getAttribute('src'));
            iframe.src = 'https://challenges.cloudflare.com/probe2';
            out.iframeSrcProp = String(iframe.src);
            shadow.appendChild(iframe);
            out.shadowChildCount = shadow.childNodes.length;
            out.iframeConnected = iframe.isConnected;
            out.iframeContentWindow = !!iframe.contentWindow;
            var host2 = document.createElement('div');
            document.body.appendChild(host2);
            try {
                host2.innerHTML = '<iframe src="https://example.com/x"></iframe>';
                out.innerHTMLChildCount = host2.childNodes.length;
                out.innerHTMLQuery = !!host2.querySelector('iframe');
            } catch (e) { out.innerHTML = 'ERR:' + String(e && e.message || e); }
        } catch (e) { out.fatal = String(e && e.message || e); }
        return JSON.stringify(out);
    })()"#,
        )
        .expect("smoke eval");
    println!("SMOKE={smoke}");
    page.drive_frame_tree(&client, &profile).await;
    let _ = page
        .event_loop()
        .run_until_settled(Duration::from_millis(500))
        .await;
    println!("FRAMES_AFTER_SMOKE={}", page.frame_tree_count());

    // B) Real render on a fresh host, with error capture.
    let render = page
        .evaluate(
            r#"(function(){
        globalThis.__oxErrs = [];
        window.addEventListener('error', function(e){ __oxErrs.push(String(e.message||e)); });
        globalThis.__oxTsResult = {callback:false, token:'', error:''};
        var host = document.createElement('div');
        host.id = 'probe-host';
        document.body.appendChild(host);
        globalThis.__oxHost = host;
        globalThis.__oxTsWidgetId = turnstile.render(host, {
            sitekey:'1x00000000000000000000AA',
            callback:function(t){ __oxTsResult.callback = true; __oxTsResult.token = String(t||''); },
            'error-callback':function(c){ __oxTsResult.error = String(c||'unknown'); }
        });
        return JSON.stringify({id:String(globalThis.__oxTsWidgetId), hostHTML:String(host.outerHTML).slice(0,2000)});
    })()"#,
        )
        .expect("render eval");
    println!("RENDER={render}");

    for poll in 0..20 {
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
            println!("CALLBACK_FIRED poll={poll}");
            break;
        }
    }

    let state = page
        .evaluate(
            r#"JSON.stringify({
        result: globalThis.__oxTsResult || null,
        hostHTML: String(globalThis.__oxHost ? globalThis.__oxHost.outerHTML : '').slice(0,3000),
        topIframes: Array.from(document.querySelectorAll('iframe')).map(function(f){
            return {id:String(f.id||''), src:String(f.src||'').slice(0,140), connected:f.isConnected};
        }),
        errors: globalThis.__oxErrs || [],
        apiCalls: (globalThis.__oxApiCalls || []).slice(-80),
        shadowDebug: Array.from(globalThis.__oxideShadowDebug || []).length,
        frameDebug: (globalThis.__oxideFrameDebug || []).slice(-24)
    })"#,
        )
        .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
    println!("STATE={state}");
    println!("FRAMES={}", page.frame_tree_count());
}
