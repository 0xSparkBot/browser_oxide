//! Live probe for the x.ai sign-up page Turnstile widget.
//!
//! Run: `cargo run -p browser_oxide --example xai_signup_probe -- --nocapture`
//! The page loads Turnstile itself; this probe waits for the widget iframe,
//! drives the frame tree, and reports the resulting token.

use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("browser_oxide=warn")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .try_init();
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
        // Capture console.error/warn — Next's loadScriptsInSequence swallows
        // bootstrap failures via .catch(console.error), which the engine
        // never prints, making hydration stalls invisible.
        globalThis.__oxCE = [];
        ['error', 'warn'].forEach(function(level){
            var orig = console[level];
            console[level] = function(){
                try {
                    if (globalThis.__oxCE.length < 40) {
                        var parts = [];
                        for (var i = 0; i < arguments.length; i++) {
                            var a = arguments[i];
                            var s;
                            try { s = (a && a.stack) ? String(a.stack).slice(0, 500) : String(a && a.message || a).slice(0, 300); }
                            catch (_) { s = '?'; }
                            parts.push(s);
                        }
                        globalThis.__oxCE.push(level + ': ' + parts.join(' | '));
                    }
                } catch (_) {}
                try { return orig.apply(console, arguments); } catch (_) {}
            };
        });
        // document.currentScript read log: which modules read it and what
        // they saw (null reads during module eval = the engine gap).
        try {
            var dDesc = Object.getOwnPropertyDescriptor(Document.prototype, 'currentScript');
            if (dDesc && dDesc.get) {
                var _reads = [];
                Object.defineProperty(Document.prototype, 'currentScript', {
                    get: function(){
                        var v;
                        try { v = dDesc.get.call(this); } catch (e) { return dDesc.get.call(this); }
                        if (_reads.length < 24) _reads.push(v ? 'el' : 'null');
                        return v;
                    },
                    configurable: true,
                });
                globalThis.__oxCsReads = function(){ return { n: _reads.length, tail: _reads.slice(-8).join(','), last: _reads[_reads.length - 1] || 'none' }; };
            } else { globalThis.__oxCsReads = function(){ return 'nodesc'; }; }
        } catch (e) { globalThis.__oxCsReads = function(){ return 'E:' + e; }; }
        // Dynamic <script> creation counter (Next loadScriptsInSequence path).
        globalThis.__oxCel = { create: 0, headAppends: 0 };
        try {
            var _CE = document.createElement.bind(document);
            document.createElement = function(tag){
                if (String(tag).toLowerCase() === 'script') globalThis.__oxCel.create++;
                return _CE.apply(document, arguments);
            };
            var _ha = HTMLElement.prototype.appendChild;
            var head = document.head || document.getElementsByTagName('head')[0];
            if (head) {
                head.appendChild = function(){
                    globalThis.__oxCel.headAppends++;
                    return _ha.apply(this, arguments);
                };
            }
        } catch (_) {}
        window.addEventListener('unhandledrejection', function(e){
            __oxScriptErrors.push('rej:' + String((e && e.reason && e.reason.message) || (e && e.reason) || e).slice(0, 300));
        });
        window.addEventListener('DOMContentLoaded', function(){ window.__oxDcl = window.__oxDcl || performance.now(); });
        window.addEventListener('load', function(){ window.__oxLoad = window.__oxLoad || performance.now(); });
        document.addEventListener('readystatechange', function(){ (window.__oxRS = window.__oxRS || []).push(document.readyState); });
        // React scheduler probe: count MessageChannel constructions and
        // postMessage calls. Hydration stalls show up as c>0,p==0 (scheduler
        // built but never triggered) or c==0 (app boot never ran).
        try {
            var _MC = window.MessageChannel;
            if (typeof _MC === 'function') {
                var _c = 0, _p = 0;
                function _MCW(){
                    _c++;
                    var ch = new _MC();
                    [ch.port1, ch.port2].forEach(function(port){
                        var op = port.postMessage;
                        port.postMessage = function(d){ _p++; return op.call(port, d); };
                    });
                    return ch;
                }
                _MCW.prototype = _MC.prototype;
                window.MessageChannel = _MCW;
                window.__oxMc = function(){ return { c: _c, p: _p }; };
            } else {
                window.__oxMc = function(){ return { c: -1, p: -1 }; };
            }
        } catch (e) { window.__oxMc = function(){ return { c: -2, p: String(e).slice(0,80) }; }; }
        // Document-level DCL marker (Next app-index hooks this to close the
        // flight stream; window-level listeners do NOT see document events).
        document.addEventListener('DOMContentLoaded', function(){ window.__oxDocDcl = window.__oxDocDcl || performance.now(); });
        // ReadableStream construction counter: react-server-dom-webpack wraps
        // the flight payload in one; c==0 means the stream was never created.
        try {
            var _RS = window.ReadableStream;
            if (typeof _RS === 'function') {
                var _rc = 0;
                function _RSW(){ _rc++; return new _RS(...arguments); }
                _RSW.prototype = _RS.prototype;
                window.ReadableStream = _RSW;
                window.__oxRs = function(){ return _rc; };
            } else { window.__oxRs = function(){ return -1; }; }
        } catch (e) { window.__oxRs = function(){ return -2; }; }
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
            var _ce = document.createElement.bind(document);
            window.__oxCreateLog = [];
            document.createElement = function(t){
                window.__oxCreateLog.push(String(t).toUpperCase());
                var r;
                try { r = _ce(t); }
                catch (e) { (window.__oxCE = window.__oxCE || []).push('ceE:' + t + ':' + e); throw e; }
                try {
                    if (r && String(r.tagName).toUpperCase() === 'IFRAME') {
                        (window.__oxIfrs = window.__oxIfrs || []).push(r);
                    }
                } catch (_) {}
                return r;
            };
            var _ap = Element.prototype.appendChild;
            Element.prototype.appendChild = function(c){
                try { return _ap.call(this, c); }
                catch (e) { (window.__oxCE = window.__oxCE || []).push('apE:' + (c && c.tagName) + ':' + e); throw e; }
            };
            var s = document.createElement('script');
            s.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit&onload=__oxTsReady';
            s.async = true;
            (document.head || document.documentElement).appendChild(s);
            window.__oxTsReady = function(){
                window.__oxTsReadyFired = 1;
                var tryRender = function(){
                    try {
                        // Flight array gets drained by react-dom; read the
                        // SSR-inline payload from the document instead.
                        var all = document.documentElement.innerHTML;
                        var m = all.match(/"sitekey":"(0x[0-9A-Za-z_-]+)"/) || all.match(/0x[0-9A-Za-z_-]{20,}/);
                        var sk = m ? (m[1] || m[0]) : null;
                        if (!sk || typeof turnstile === 'undefined') { window.__oxTsNoSk = (window.__oxTsNoSk || 0) + 1; return false; }
                        var d = document.createElement('div');
                        d.id = 'ox-ts-box';
                        document.body.appendChild(d);
                        window.__oxTsRendered = 1;
                        turnstile.render(d, { sitekey: sk, callback: function(t){ window.__oxToken = t; } });
                        return true;
                    } catch (e) { window.__oxTsErr = String(e); return true; }
                };
                if (!tryRender()) {
                    var n = 0;
                    var iv = setInterval(function(){ n++; if (tryRender() || n > 20) clearInterval(iv); }, 500);
                }
            };
            return 'injected';
        } catch (e) { return 'E:' + e; }
    })()"#,
        )
        .ok()
        .map(|s| println!("TSINJECT={s}"));

    // Dump full engine DOM for offline diffing.
    if let Ok(dom) = page.evaluate("document.documentElement.outerHTML") {
        let _ = std::fs::write("/tmp/ox_engine_dom.html", dom);
    }

    // Wait for the widget: implicit render fills .cf-turnstile with an iframe.
    let mut fired = false;
    let mut seeded = false;
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
            dcl: window.__oxDcl ? 'y' : 'n',
            load: window.__oxLoad ? 'y' : 'n',
            rs: (window.__oxRS || []).join(','),
            rd: document.readyState,
            mc: window.__oxMc ? window.__oxMc() : null,
            box: (function(){ var b=document.getElementById('ox-ts-box'); return b?String(b.innerHTML).slice(0,150):'nobox'; })(),
            cf: document.querySelectorAll('.cf-turnstile, [class*=turnstile]').length,
        });
    })()"#,
            )
            .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
        if poll % 2 == 0 {
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

        // Child-frame probe: seed as soon as the frame exists, sample after.
        // The challenge script's state machine only advances while the child
        // isolate runs, so watch for _cf_chl_opt / DOM writes / timers / errors.
        if !seeded && page.frame_tree_count() > 0 {
            let seed = r#"(function(){
        try {
            globalThis.__oxErrs = [];
            globalThis.__oxFired = []; globalThis.__oxFiredN = 0; globalThis.__oxMsgs = [];
            var he = function(m){ try { __oxErrs.push(String(m).slice(0,150)); } catch(_){} };
            window.addEventListener('error', function(ev){ he('E:'+(ev&&ev.message)); });
            window.addEventListener('unhandledrejection', function(ev){ he('P:'+(ev&&ev.reason)); });
            window.addEventListener('message', function(ev){ try { if(__oxMsgs.length<6) __oxMsgs.push(String(ev&&ev.data).slice(0,60)); } catch(_){} });
            var _st = window.setTimeout, _si = window.setInterval;
            window.__oxTm = 0; window.__oxTms = []; window.__oxTmSrc = '';
            window.setTimeout = function(f, t){
                __oxTm++; if(__oxTms.length<24) __oxTms.push('t'+(t|0));
                if(!__oxTmSrc) try { __oxTmSrc = 'T'+(t|0)+':'+String(f).slice(0,300); } catch(_){ }
                var g = function(){ try { __oxFiredN++; if(__oxFired.length<6) __oxFired.push('t'+(t|0)); } catch(_){} return f.apply(this, arguments); };
                return _st.call(window, g, t);
            };
            window.setInterval = function(f, t){
                __oxTm++; if(__oxTms.length<24) __oxTms.push('i'+(t|0));
                if(!__oxTmSrc) try { __oxTmSrc = 'I'+(t|0)+':'+String(f).slice(0,300); } catch(_){ }
                var g = function(){ try { __oxFiredN++; if(__oxFired.length<6) __oxFired.push('i'+(t|0)); } catch(_){} return f.apply(this, arguments); };
                return _si.call(window, g, t);
            };
            var _fe = window.fetch;
            window.__oxFe = 0; window.__oxFeLog = [];
            if (_fe) window.fetch = function(){ __oxFe++; if(__oxFeLog.length<6) try { __oxFeLog.push(String(arguments[0]).slice(0,70)); } catch(_){} return _fe.apply(this, arguments); };
            var _rp = XMLHttpRequest.prototype, _open = _rp.open;
            window.__oxXhr = 0; window.__oxXhrLog = [];
            _rp.open = function(m, u){ __oxXhr++; if(__oxXhrLog.length<6) __oxXhrLog.push(String(u).slice(0,70)); try { return _open.apply(this, arguments); } catch(e){ he('xhr:'+e); throw e; } };
            if (window.MutationObserver) {
                window.__oxMut = 0;
                new MutationObserver(function(ms){ __oxMut += ms.length; }).observe(document, {childList:true, subtree:true, characterData:true});
            } else { window.__oxMut = 'none'; }
            // Bare timer chain: independent of the wrapped counters above.
            // If these don't advance either, the child isolate's timer drive
            // is broken at the engine level, not in the challenge VM.
            window.__oxBare = []; window.__oxBareT = [];
            var now = function(){ try { return Math.round(performance.now()); } catch(e){ return -1; } };
            setTimeout(function(){ __oxBare.push('t100'); __oxBareT.push(now()); }, 100);
            setTimeout(function(){ __oxBare.push('t400'); __oxBareT.push(now()); }, 400);
            var iv = setInterval(function(){ __oxBare.push('I'); __oxBareT.push(now()); }, 500);
            setTimeout(function(){ clearInterval(iv); __oxBare.push('stop@2s'); }, 2000);
            setTimeout(function(){ __oxBare.push('t2500'); __oxBareT.push(now()); }, 2500);
            window.__oxBI = []; window.__oxBIT = [];
            setInterval(function(){ __oxBI.push(1); __oxBIT.push(now()); }, 500);
        } catch (e) { try { globalThis.__oxSeedErr = String(e); } catch(_){} }
    })()"#;
            let seeded_ok = page.frame_tree_evaluate(0, seed);
            if seeded_ok.is_none() {
                println!("SEEDFAIL");
            } else {
                seeded = true;
            }
            let boot = page.frame_tree_evaluate(
                0,
                r#"JSON.stringify({opt:typeof _cf_chl_opt, keys:(function(){try{return Object.keys(_cf_chl_opt||{}).length}catch(e){return 'E'}})(), u:window.UlwL3?'y':'n', rs:document.readyState, scr:(document.scripts||[]).length})"#,
            );
            println!("SEED_BOOT={boot:?}");
            let meta = page.frame_tree_evaluate(
                0,
                r#"JSON.stringify({par:window.parent===window,top:(window.top===window?'self':(window.top?'obj':'none')),fe:!!window.frameElement,vis:(document.visibilityState||''),hf:(document.hasFocus?!!document.hasFocus():'nf'),dcr:(function(){try{return JSON.stringify(document.documentElement.getBoundingClientRect())}catch(e){return 'E'}})(),o:(function(){try{return JSON.stringify(_cf_chl_opt||{}).slice(0,420)}catch(e){return 'E'}})()})"#,
            );
            println!("FRAME0_META={meta:?}");
        }
        if poll % 3 == 0 {
            let sample = page.frame_tree_evaluate(
                0,
                r#"JSON.stringify({clk:Math.round(performance.now()),mut:window.__oxMut,tm:window.__oxTm,fn:window.__oxFiredN,tms:window.__oxTms,br:window.__oxBare,bi:(window.__oxBI||[]).length,fe:window.__oxFe,xhr:window.__oxXhr,mg:window.__oxMsgs,doc:(document.documentElement.outerHTML||'').length,head:document.head?document.head.children.length:-1,stg:!!document.getElementById('stage'),wk:(typeof Worker),bl:document.body?document.body.innerHTML.length:-1,errs:(window.__oxErrs||[]).slice(0,2),seedErr:window.__oxSeedErr||''})"#,
            );
            println!("FRAME0_S{poll}={sample:?}");
        }
    }

    // Deep structural dump: full element tree under <body> including shadow
    // roots, plus fate of every iframe the page created (parent chain,
    // connectedness, contentWindow) and the engine frame ledger tail.
    // Child-frame introspection: the Turnstile challenge runs in a separate
    // isolate; surface its href, readiness, script coverage and error ring.
    let frame_n = page.frame_tree_count();
    for i in 0..frame_n {
        let href = page.frame_tree_evaluate(
            i,
            "String((globalThis.location&&location.href)||'noloc').slice(0,110)",
        );
        println!("FRAME{i}_HREF={href:?}");
        let st = page.frame_tree_evaluate(
            i,
            "JSON.stringify({rs:document.readyState,ttl:String(document.title||'').slice(0,40),bl:document.body?document.body.innerHTML.length:-1,scr:document.scripts?document.scripts.length:-1,ifr:(function(){try{return document.querySelectorAll('iframe').length}catch(e){return 'E'}})(),sh:(function(){try{return document.querySelectorAll('*').length}catch(e){return 'E'}})()})",
        );
        println!("FRAME{i}_ST={st:?}");
        let errs = page.frame_tree_evaluate(
            i,
            "JSON.stringify((globalThis.__scriptErrors||[]).slice(0,4).map(function(e){return String(e&&e.msg||e).slice(0,160)}))",
        );
        println!("FRAME{i}_ERR={errs:?}");
    }

    let deep = page
        .evaluate(
            r#"(function(){
        var out = {tree: [], ifrs: [], frames: [], app: ''};
        function desc(n){
            try {
                var s = n.nodeType + ':' + String(n.nodeName || '');
                if (n.id) s += '#' + n.id;
                if (n.nodeType === 1) {
                    var c = String(n.className || '');
                    if (c) s += '.' + c.slice(0, 40);
                }
                return s;
            } catch (e) { return '?'; }
        }
        function walk(n, path, depth){
            if (!n || depth > 10 || out.tree.length > 220) return;
            var kids;
            try { kids = n.childNodes; } catch (e) { kids = null; }
            if (kids) for (var i = 0; i < kids.length; i++) {
                var c = kids[i];
                if (c.nodeType === 3) {
                    var t;
                    try { t = String(c.textContent || '').trim().slice(0, 40); } catch (e) { t = '?'; }
                    if (t) out.tree.push(path + '/"' + t + '"');
                    continue;
                }
                var p = path + '/' + desc(c);
                out.tree.push(p);
                walk(c, p, depth + 1);
            }
            var sr = null;
            try { sr = n.shadowRoot; } catch (e) {}
            if (sr) { out.tree.push(path + '##SHADOW'); walk(sr, path + '#SR', depth + 1); }
        }
        var bodyEl = document.body;
        if (bodyEl) walk(bodyEl, 'BODY', 0); else out.tree.push('NOBODY');
        (window.__oxIfrs || []).forEach(function(f, i){
            var chain = [], n = f, hops = 0;
            try {
                while (n && n !== document && hops < 8) { chain.push(desc(n)); n = n.parentNode; hops++; }
                if (n === document) chain.push('#document');
            } catch (e) { chain.push('E:' + e); }
            var cw = 'n';
            try { cw = f.contentWindow ? 'y' : 'n'; } catch (e) { cw = 'E'; }
            var sr2 = null; try { sr2 = f.shadowRoot; } catch (e) {}
            out.ifrs.push({
                i: i, src: String(f.getAttribute && f.getAttribute('src') || f.src || '').slice(0, 90),
                connected: !!f.isConnected, chain: chain.join(' < '),
                parentHTML: (f.parentNode && f.parentNode.outerHTML || '').slice(0, 200),
                cw: cw, shadow: !!sr2,
            });
        });
        out.frames = (globalThis.__oxideFrameDebug || []).slice(-30);
        try {
            var app = document.body && document.body.children[1] && document.body.children[1].firstElementChild;
            out.app = app ? String(app.outerHTML).slice(0, 900) : 'none';
        } catch (e) { out.app = 'E:' + e; }
        return JSON.stringify(out);
    })()"#,
        )
        .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
    println!("DEEP={deep}");
    if let Ok(dom) = page.evaluate("document.documentElement.outerHTML") {
        let _ = std::fs::write("/tmp/ox_engine_dom_after.html", dom);
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
        nnext: (function(){ var n=document.getElementById('__next'); return n ? {kids:n.childElementCount, len:n.innerHTML.length} : null; })(),
        nf: (self.__next_f || []).length,
        nx: typeof window.next,
        cf: (function(){ var n=document.querySelector('.cf-turnstile'); return n ? {kids:n.childElementCount, ih:n.innerHTML.length, sk:n.getAttribute('data-sitekey'), oh:n.outerHTML.slice(0,160)} : null; })(),
        dsk: (function(){ var n=document.querySelector('[data-sitekey]'); return n ? n.getAttribute('data-sitekey') : null; })(),
        bkids: (function(){ var a=[],b=document.body; for(var i=0;i<Math.min(b.childElementCount,60);i++){var c=b.children[i]; a.push(c.tagName+(c.id?'#'+c.id:''));} return a; })(),
        inps: Array.from(document.querySelectorAll('input,button,select')).slice(0,25).map(function(e){return e.tagName+':'+(e.name||e.id||e.type);}),
        cbdef: (function(){ var h=document.documentElement.innerHTML; var m=h.match(/onload=([\w$]+)/); if(!m) return 'NONAME'; var n=m[1]; var i=h.indexOf('function '+(window[n]?'':'')); var j=h.indexOf(n+'='); var k=h.indexOf(n+' ='); var p=Math.min.apply(null,[j,k].filter(function(x){return x>=0;})); return p>=0 ? h.slice(Math.max(0,p-60),p+150) : ('name='+n+' typeof='+typeof window[n]); })(),
        sscripts: Array.from(document.querySelectorAll('script[src]')).map(function(s){return String(s.src).slice(-60);}),
        nd: typeof window.__NEXT_DATA__,
        nf0: String(JSON.stringify((self.__next_f || [])[0] || null)).slice(0, 260),
        sec: (function(){ var s=document.querySelector('section'); return s ? s.innerHTML.length : -1; })(),
        forms: document.querySelectorAll('form').length,
        divs: (function(){ var a=[]; for (var i=0;i<document.body.children.length;i++){ var c=document.body.children[i]; if(c.tagName==='DIV') a.push({id:c.id,cls:String(c.className).slice(0,40),kids:c.childElementCount,ih:c.innerHTML.length}); } return a; })(),
        mc: window.__oxMc ? window.__oxMc() : null,
        dcl: window.__oxDcl || -1,
        load: window.__oxLoad || -1,
        docdcl: window.__oxDocDcl || -1,
        rs: (window.__oxRS || []).join(','),
        rsn: window.__oxRs ? window.__oxRs() : -3,
        npush: (self.__next_f && self.__next_f.push === Array.prototype.push) ? 'native' : 'hooked',
        ce: (globalThis.__oxCE || []).slice(0, 12),
        nxv: (function(){ try { return window.next && window.next.version ? String(window.next.version) : 'noversion'; } catch (e) { return 'E'; } })(),
        sN: (function(){ try { return (self.__next_s || []).length; } catch (e) { return 'E'; } })(),
        cel: globalThis.__oxCel || null,
        cs: (function(){ try { return typeof globalThis.__oxCsReads === 'function' ? globalThis.__oxCsReads() : 'nowrap'; } catch (e) { return 'E:' + e; } })(),
        rej: (globalThis.__oxScriptErrors || []).filter(function(s){ return String(s).indexOf('rej:') === 0; }).slice(0, 8),
        zfn: typeof window['zasz6ne759'],
        readyF: window.__oxTsReadyFired || 0,
        noSk: window.__oxTsNoSk || 0,
        tsErr: window.__oxTsErr || '',
        tsRn: window.__oxTsRendered || 0,
        tok: window.__oxToken ? String(window.__oxToken).slice(0, 24) : '',
        cfC: (function(){ try { return document.querySelectorAll('.cf-turnstile, [data-sitekey], [id^=cf-chl] iframe, iframe[src*=turnstile], iframe[src*=challenges]').length; } catch (e) { return 'E'; } })(),
        box: (function(){ var b=document.getElementById('ox-ts-box'); return b ? b.outerHTML.slice(0,300) : 'NOBOX'; })(),
        tK: (function(){ try { return typeof turnstile==='object' ? Object.keys(turnstile).join(',') : 'noT:'+typeof turnstile; } catch(e){ return 'E'; } })(),
        gr: (function(){ try { return typeof turnstile!=='undefined'&&turnstile.getResponse ? String(turnstile.getResponse()).slice(0,40) : 'N'; } catch(e){ return 'E:'+e; } })(),
        clog: (window.__oxCreateLog || []).join(','),
        apE: (window.__oxCE || []).filter(function(s){ return String(s).indexOf('apE:') === 0 || String(s).indexOf('ceE:') === 0; }).slice(0, 6),
        sh: (function(){
            var b = document.getElementById('ox-ts-box'); if (!b) return 'NOBOX';
            var d = b.firstElementChild && b.firstElementChild.firstElementChild; if (!d) return 'NOD';
            var sr = d.shadowRoot; if (!sr) return 'NOSR';
            var out = [];
            for (var i = 0; i < sr.childNodes.length; i++) {
                var c = sr.childNodes[i];
                var src = ''; try { src = (c.getAttribute && c.getAttribute('src')) || c.src || ''; } catch (e) { src = 'E'; }
                out.push(c.tagName + 'src=' + String(src).slice(-42) + 'cw=' + (c.contentWindow ? 'y' : 'n'));
            }
            return out;
        })(),
        ffn: JSON.stringify(globalThis.__frameIdForNode || {}),
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
