//! Live probe for the x.ai sign-up page Turnstile widget.
//!
//! Run: `cargo run -p browser_oxide --example xai_signup_probe -- --nocapture`
//! The page loads Turnstile itself; this probe waits for the widget iframe,
//! drives the frame tree, and reports the resulting token.

use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let url = std::env::var("BROWSER_OXIDE_TARGET")
        .unwrap_or_else(|_| "https://accounts.x.ai/sign-up".into());
    let profile = browser_oxide::stealth::presets::chrome_148_macos();

    // Wrap every deno op so op-level arg-conversion failures ("expected i32")
    // are captured with the JS call stack that triggered them.
    let init_ops_trap = r#"
    (function(){
        // The engine bootstrap (ops_trap_bootstrap.js) wraps every op and
        // owns `__oxOpErrors`/`__oxOpsWrapped`; keep both intact and only
        // append an extra record layer via the still-armed Deno setter below.
        globalThis.__oxOpErrors = globalThis.__oxOpErrors || [];
        function wrapOps(opsObj) {
            if (!opsObj || opsObj.__oxWrapped || globalThis.__oxOpsWrapped) return true;
            var names;
            try { names = Object.keys(opsObj); } catch (_) { return false; }
            for (var i = 0; i < names.length; i++) {
                (function(name){
                    var orig;
                    try { orig = opsObj[name]; } catch (_) { return; }
                    if (typeof orig !== 'function') return;
                    var wrapped = function(){
                        try {
                            return orig.apply(this, arguments);
                        } catch (e) {
                            var msg = String((e && e.message) || e);
                            if (globalThis.__oxOpErrors.length < 24) {
                                var args = [];
                                for (var j = 0; j < Math.min(arguments.length, 5); j++) {
                                    var a = arguments[j];
                                    try { args.push(typeof a === 'number' ? a : String(a).slice(0, 50)); }
                                    catch (_) { args.push('?'); }
                                }
                                globalThis.__oxOpErrors.push({
                                    op: name, msg: msg, args: args,
                                    stack: String(new Error().stack || '').slice(0, 3000),
                                });
                            }
                            throw e;
                        }
                    };
                    try { opsObj[name] = wrapped; } catch (_) {}
                })(names[i]);
            }
            try { opsObj.__oxWrapped = true; } catch (_) {}
            globalThis.__oxOpsWrapped = true;
            globalThis.__oxWrapCount = (globalThis.__oxWrapCount || 0) + 1;
            return true;
        }
        globalThis.__oxWrapOps = wrapOps;
        // Init scripts run before the engine bootstrap installs `Deno`.
        // Trap the global assignment and wrap ops the moment it lands.
        try {
            var d = Object.getOwnPropertyDescriptor(globalThis, 'Deno');
            globalThis.__oxTrapMode = d ? (d.value !== undefined ? 'data-wrap' : (d.configurable ? 'accessor' : 'existing-acc')) : 'no-deno-yet';
            if (d && d.value !== undefined) {
                // Engine bootstrap ran before this init script: Deno already
                // exists as a data property — wrap in place. Do NOT install an
                // accessor with an unset backing var (the getter would hide
                // the global for every later `Deno.` reference).
                wrapOps(d.value && d.value.core && d.value.core.ops);
            } else if (!d || d.configurable) {
                var _deno = d ? d.value : undefined;
                Object.defineProperty(globalThis, 'Deno', {
                    configurable: true,
                    get: function(){ return _deno; },
                    set: function(v){
                        _deno = v;
                        try { wrapOps(v && v.core && v.core.ops); }
                        catch (e) { globalThis.__oxOpErrors.push({op:'trap', msg:String(e)}); }
                    },
                });
            } else {
                wrapOps(globalThis.Deno && globalThis.Deno.core && globalThis.Deno.core.ops);
            }
        } catch (e) {
            globalThis.__oxOpErrors.push({op: 'trap-init', msg: String((e && e.message) || e)});
        }
        globalThis.__oxScriptErrors = [];
        window.addEventListener('DOMContentLoaded', function(){ window.__oxDcl = window.__oxDcl || performance.now(); });
        window.addEventListener('load', function(){ window.__oxLoad = window.__oxLoad || performance.now(); });
        document.addEventListener('readystatechange', function(){ (window.__oxRS = window.__oxRS || []).push(document.readyState); });
        window.addEventListener('error', function(e){
            var err = e && e.error;
            var st = err && err.stack ? String(err.stack).slice(0, 1200) : String(e.message || e);
            __oxScriptErrors.push({ msg: String(e.message || e), file: String(e.filename || ''), ln: e.lineno, stack: st });
        });
    })();"#;
    let mut page = match browser_oxide::Page::navigate_with_init(
        &url,
        profile.clone(),
        2,
        vec![init_ops_trap.to_string()],
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("navigate failed: {e}");
            return;
        }
    };
    let client = browser_oxide::net::HttpClient::shared(&profile).expect("http client");

    // Let the SPA settle and load the Turnstile script.
    for _ in 0..20 {
        page.drive_frame_tree(&client, &profile).await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(500))
            .await;
    }
    println!(
        "API={:?} TITLE={:?}",
        page.evaluate("typeof globalThis.turnstile"),
        page.evaluate("String(document.title)")
    );

    page.evaluate("globalThis.__browser_oxide_debug = true;")
        .ok();
    // Fallback: if the init trap missed the Deno install, wrap ops now so any
    // still-recurring "expected i32" (e.g. from timers/polling) is caught.
    let _ = page
        .evaluate(
            r#"globalThis.__diag = {
        hasWrap: typeof globalThis.__oxWrapOps,
        wrapped: !!globalThis.__oxOpsWrapped,
        cc: (typeof globalThis.__oxOpCallCount === 'number') ? globalThis.__oxOpCallCount : 'T:' + typeof globalThis.__oxOpCallCount,
        ringLen: (globalThis.__oxOpErrors || []).length,
        wrapCount: globalThis.__oxWrapCount || 0,
        trapMode: globalThis.__oxTrapMode || null,
        denoType: typeof globalThis.Deno,
        denoDesc: (function(){ var d = Object.getOwnPropertyDescriptor(globalThis, 'Deno'); return d ? (d.get ? 'accessor' : 'data') : 'none'; })(),
        opsKeys: (function(){ try { return Object.keys(globalThis.Deno.core.ops).length; } catch(e){ return 'E:'+e; } })(),
    };
    if (globalThis.__oxWrapOps && !globalThis.__oxOpsWrapped) {
        try { globalThis.__oxWrapOps(globalThis.Deno && globalThis.Deno.core && globalThis.Deno.core.ops); } catch(e) { globalThis.__oxDiag.wrapErr = String(e); }
    }
    JSON.stringify(globalThis.__diag);"#,
        )
        .ok()
        .map(|s| println!("WRAPDIAG={s}"));
    // DON'T reset __oxScriptErrors here — the init-script listener has been
    // capturing since document start; resetting would erase hydration-era
    // errors. Only add the rejection listener (init can't have one yet).
    page.evaluate(
        r#"globalThis.__oxScriptErrors = globalThis.__oxScriptErrors || [];
        if (!globalThis.__oxRejHooked) {
            globalThis.__oxRejHooked = true;
            window.addEventListener('unhandledrejection', function(e){ __oxScriptErrors.push('rej:' + String(e.reason && e.reason.message || e.reason)); });
        }"#,
    )
    .ok();

    // Report script inventory + resource loading state.
    let scripts = page
        .evaluate(
            r#"JSON.stringify({
        scripts: Array.from(document.querySelectorAll('script')).map(function(s){
            return {src: String(s.src || '').slice(0, 120), inline: !s.src, len: (s.textContent || '').length};
        }).slice(0, 30),
        nextData: !!window.__NEXT_DATA__,
        resEntries: (performance.getEntriesByType('resource') || []).map(function(r){ return r.name.slice(0, 110); }).slice(0, 40),
        navEntries: (performance.getEntriesByType('navigation') || []).map(function(r){ return {name: r.name.slice(0, 80), status: r.responseStatus}; }),
    })"#,
        )
        .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
    println!("SCRIPTS={scripts}");

    // Op-arg battery: reproduce candidate "expected i32" call shapes against
    // live ops. Also probe __bootstrap (Deno global is scrubbed post-bootstrap).
    let battery = page
        .evaluate(
            r#"(function(){
        var out = {bs: typeof globalThis.__bootstrap, results: {}};
        function t(name, fn){ try { var r = fn(); out.results[name] = 'ok:' + String(r).slice(0,20); } catch(e){ out.results[name] = 'THROW:' + String(e && e.message || e); } }
        t('setTimeout_undef', function(){ var h = setTimeout(function(){}, undefined); clearTimeout(h); return h; });
        t('setTimeout_noarg2', function(){ var h = setTimeout(function(){}); clearTimeout(h); return h; });
        t('clearTimeout_undef', function(){ clearTimeout(undefined); return 1; });
        t('raf_undef', function(){ var h = requestAnimationFrame(function(){}); cancelAnimationFrame(h); return h; });
        t('perf_now', function(){ return performance.now(); });
        t('ops_direct', function(){ var o = globalThis.__bootstrap && globalThis.__bootstrap.core && globalThis.__bootstrap.core.ops; return o ? Object.keys(o).length : 'noops'; });
        return JSON.stringify(out);
    })()"#,
        )
        .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
    println!("BATTERY={battery}");
    let containers = page
        .evaluate(
            r#"JSON.stringify({
        cfContainers: Array.from(document.querySelectorAll('.cf-turnstile, [data-sitekey]')).map(function(el){
            return {tag: el.tagName, sitekey: el.getAttribute('data-sitekey') || '', cls: el.className};
        }),
        hasTurnstile: typeof globalThis.turnstile === 'object',
        bodySnippet: String(document.body ? document.body.innerHTML : '').slice(0, 600),
        env: (function(){
            var q = function(s){ return document.querySelectorAll(s).length; };
            return {
                nextF: typeof self !== 'undefined' && Array.isArray(self.__next_f) ? self.__next_f.length : (typeof self !== 'undefined' && self.__next_f !== undefined ? 'notArr' : 'none'),
                mc: typeof MessageChannel,
                qmt: typeof queueMicrotask,
                ric: typeof requestIdleCallback,
                rs: document.readyState,
                bodyKids: document.body ? document.body.childElementCount : -1,
                next: q('#__next') + '/' + q('[data-reactroot]'),
                scripts: document.scripts ? document.scripts.length : -1,
                dcl: typeof window.__oxDcl === 'number' ? window.__oxDcl : 'NA',
                load: typeof window.__oxLoad === 'number' ? window.__oxLoad : 'NA',
                rsEvents: (window.__oxRS || []).join(','),
                topKids: Array.from(document.body ? document.body.children : []).slice(0, 10).map(function(e){ return e.tagName + '.' + String(e.className || '').slice(0, 30) + '#' + (e.id || ''); }),
                inputs: (function(){ var n = document.querySelectorAll('input,textarea,button').length; return n; })(),
            };
        })(),
    })"#,
        )
        .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
    println!("CONTAINERS={containers}");

    // Manually inject the Turnstile (api.js) in case the page's own lazy
    // loader never fires in this engine; the poll loop below observes both
    // the manual and the page-driven path via `ts`/`tsScript`.
    let _ = page
        .evaluate(
            r#"(function(){
        if (globalThis.__oxTsInjected) return 'already';
        globalThis.__oxTsInjected = 1;
        try {
            var s = document.createElement('script');
            s.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit&onload=__oxTsReady';
            s.async = true;
            (document.head || document.documentElement).appendChild(s);
            window.__oxTsReady = function(){
                window.__oxTsReadyFired = 1;
                try {
                    var all = JSON.stringify(self.__next_f || []);
                    var m = all.match(/0x[0-9a-fA-F]{22,}/);
                    var sk = m && m[0];
                    if (!sk || typeof turnstile === 'undefined') { window.__oxTsNoSk = 1; return; }
                    var d = document.createElement('div');
                    d.id = 'ox-ts-box';
                    document.body.appendChild(d);
                    window.__oxTsRendered = 1;
                    turnstile.render(d, { sitekey: sk, callback: function(t){ window.__oxToken = t; } });
                } catch (e) { window.__oxTsErr = String(e); }
            };
            return 'injected';
        } catch (e) { return 'E:' + e; }
    })()"#,
        )
        .ok()
        .map(|s| println!("TSINJECT={s}"));

    // Wait for the widget: implicit render fills .cf-turnstile with an iframe.
    let mut fired = false;
    for poll in 0..60 {
        page.drive_frame_tree(&client, &profile).await;
        let res = page
            .event_loop()
            .run_until_settled(Duration::from_millis(500))
            .await;
        if let Err(e) = &res {
            println!("LOOPERR@{poll}={e}");
        }
        let state = page
            .evaluate(
                r#"(function(){
        if (typeof turnstile === 'object' && !window.__oxTsRendered && !window.__oxTsNoSk) {
            try {
                var hay = '';
                try { hay = JSON.stringify(self.__next_f || []); } catch (e0) {}
                var m = hay.match(/0x[0-9a-fA-F]{20,}/);
                if (!m) { try { hay = document.documentElement.innerHTML; m = hay.match(/0x[0-9a-fA-F]{20,}/); } catch (e1) {} }
                var sk = m && m[0];
                if (!sk) { window.__oxTsNoSk = 1; }
                else {
                    var d = document.createElement('div');
                    d.id = 'ox-ts-box';
                    document.body.appendChild(d);
                    window.__oxTsRendered = 1;
                    turnstile.render(d, { sitekey: sk, callback: function(t){ window.__oxToken = t; } });
                }
            } catch (e) { window.__oxTsErr = String(e); }
        }
        return JSON.stringify({
            tokens: Array.from(document.querySelectorAll('input[name=cf-turnstile-response], textarea[name=cf-turnstile-response]')).map(function(i){return String(i.value||'').slice(0,80);}),
            iframes: Array.from(document.querySelectorAll('iframe')).map(function(f){return String(f.src||'').slice(0,110);}),
            frames: (typeof __oxideFrameDebug !== 'undefined') ? __oxideFrameDebug.length : -1,
            wrapped: !!globalThis.__oxOpsWrapped,
            opc: globalThis.__oxOpCallCount | 0,
            hyd: !!document.querySelector('#__next [class], main, #root > *'),
            ts: typeof turnstile,
            tsScript: !!document.querySelector('script[src*="challenges.cloudflare"], script[src*="turnstile"]'),
            tsR: window.__oxTsRendered | 0,
            ns: window.__oxTsNoSk | 0,
            sk: (function(){ try { var m=document.documentElement.innerHTML.match(/turnstile\/v0\/api\.js\?onload=([\w$]+)/); var n=m&&m[1]; if(!n) return 'NONAME'; var t=typeof window[n]; if(t==='function'){ window[n](); return 'FIRED:'+n; } return 'T:'+t+' n='+n; } catch(e){ return 'E:'+e; } })(),
            tsE: window.__oxTsErr || '',
            tok: String(window.__oxToken || '').slice(0, 40),
            bk: document.body ? document.body.childElementCount : -1,
        });
    })()"#,
            )
            .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
        if poll % 6 == 0 {
            println!("POLL{poll}={state}");
        }
        if state.contains("XXXX") || (state.contains("token") && state.contains("response")) {
            // crude check; final dump below is authoritative
        }
        let done = matches!(
            page.evaluate(
                "String(Array.from(document.querySelectorAll('input[name=cf-turnstile-response]')).some(function(i){return (i.value||'').length > 20;}))"
            )
            .as_deref(),
            Ok("true")
        );
        if done {
            fired = true;
            println!("TOKEN_FOUND poll={poll}");
            break;
        }
    }

    let final_state = page
        .evaluate(
            r#"JSON.stringify({
        tokens: Array.from(document.querySelectorAll('input[name=cf-turnstile-response], textarea[name=cf-turnstile-response]')).map(function(i){return String(i.value||'').slice(0,120);}),
        iframes: Array.from(document.querySelectorAll('iframe')).map(function(f){return String(f.src||'').slice(0,140);}),
        errors: (globalThis.__oxScriptErrors || []),
        opErrors: (globalThis.__oxOpErrors || []).map(function(e){ return {op: e.op, argc: e.argc, args: e.args, msg: e.msg, stack: e.stack}; }),
        wrapped: !!globalThis.__oxOpsWrapped,
        opc: globalThis.__oxOpCallCount | 0,
    })"#,
        )
        .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
    println!("FINAL={final_state}");
    println!(
        "SCRIPT_ERRORS={:?}",
        page.evaluate("JSON.stringify(globalThis.__oxScriptErrors || [])")
    );
    println!(
        "ENGINE_ERRORS={}",
        page.evaluate("JSON.stringify((globalThis.__scriptErrors || []).slice(0, 6))")
            .unwrap_or_default()
    );
    println!(
        "OP_ERRORS={:?}",
        page.evaluate("JSON.stringify(globalThis.__oxOpErrors || [], null, 1)")
    );
    println!("FRAMES={}", page.frame_tree_count());
    println!("TOKEN_FOUND={fired}");
}
