//! Live probe for the x.ai sign-up page Turnstile widget.
//!
//! Run: `cargo run -p browser_oxide --example xai_signup_probe -- --nocapture`
//! The page loads Turnstile itself; this probe waits for the widget iframe,
//! drives the frame tree, and reports the resulting token.

use std::time::Duration;

/// Frame indices are only stable within one drive cycle: the Turnstile widget
/// replaces its challenge iframe on crash-retry, so resolve the live challenge
/// frame (challenges.cloudflare / .../fbE/... URL) before every interaction.
/// Falls back to the first frame that answers at all; `None` mid-swap.
fn pick_challenge_frame(page: &mut browser_oxide::Page) -> Option<(usize, String)> {
    let n = page.frame_tree_count();
    let mut fallback: Option<(usize, String)> = None;
    for i in 0..n {
        let href = page
            .frame_tree_evaluate(
                i,
                "String((globalThis.location&&location.href)||'').slice(-90)",
            )
            .unwrap_or_default();
        if href.is_empty() {
            continue;
        }
        let challenge = href.contains("challenges.cloudflare")
            || href.contains("/fbE/")
            || href.contains("turnstile");
        if challenge {
            return Some((i, href));
        }
        if fallback.is_none() {
            fallback = Some((i, href));
        }
    }
    fallback
}

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
            globalThis.__oxIfrLog = [];
            var _uid = 0, _uidMap = new WeakMap();
            globalThis.__oxIfrUid = function(n){ var v = _uidMap.get(n); if (v === undefined){ v = ++_uid; _uidMap.set(n, v); } return v; };
            globalThis.__oxIfrEv = function(ev, node, xtra){
                try {
                    if (globalThis.__oxIfrLog.length >= 48) return;
                    var e = { ev: ev, uid: globalThis.__oxIfrUid(node) };
                    try { e.src = String((node.getAttribute && node.getAttribute('src')) || node.src || '').slice(-46); } catch(_){ }
                    try { e.nm = String((node.getAttribute && node.getAttribute('name')) || node.name || '').slice(0, 24); } catch(_){ }
                    try { e.conn = !!node.isConnected; } catch(_){ }
                    try { e.par = node.parentNode ? (String(node.parentNode.tagName) + (node.parentNode.id ? '#' + node.parentNode.id : '')) : null; } catch(_){ }
                    try { e.t = Math.round(performance.now()); } catch(_){ e.t = -1; }
                    if (xtra) e.x = String(xtra).slice(0, 90);
                    globalThis.__oxIfrLog.push(e);
                } catch (_er) { try { globalThis.__oxIfrLog.push({ ev: 'logE', x: String(_er).slice(0, 60) }); } catch(__){} }
            };
            document.createElement = function(tag){
                var low = String(tag).toLowerCase();
                if (low === 'script') globalThis.__oxCel.create++;
                var r = _CE.apply(document, arguments);
                try { if (r && low === 'iframe') globalThis.__oxIfrEv('create', r); } catch(_){ }
                return r;
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
        // Iframe life recorder: append/insert/remove/replace/innerHTML-wipe.
        try {
            var _sig = function(){ try { return String(new Error().stack || '').split('\n').slice(2, 4).join('|').slice(0, 90); } catch(e){ return ''; }; };
            var _ac2 = Node.prototype.appendChild;
            Node.prototype.appendChild = function(c){ var r = _ac2.call(this, c); try { if (c && c.tagName === 'IFRAME') globalThis.__oxIfrEv('append', c); } catch(_){} return r; };
            var _ib2 = Node.prototype.insertBefore;
            Node.prototype.insertBefore = function(n, r){ var out = _ib2.call(this, n, r); try { if (n && n.tagName === 'IFRAME') globalThis.__oxIfrEv('insert', n); } catch(_){} return out; };
            var _rc3 = Node.prototype.removeChild;
            Node.prototype.removeChild = function(c){ try { if (c && c.tagName === 'IFRAME') globalThis.__oxIfrEv('remove', c, _sig()); } catch(_){} return _rc3.call(this, c); };
            var _rp3 = Node.prototype.replaceChild;
            if (_rp3) Node.prototype.replaceChild = function(n, o){ try { if (o && o.tagName === 'IFRAME') globalThis.__oxIfrEv('replace-out', o, _sig()); } catch(_){} var r = _rp3.call(this, n, o); try { if (n && n.tagName === 'IFRAME') globalThis.__oxIfrEv('replace-in', n); } catch(_){} return r; };
            var _rm2 = Element.prototype.remove;
            Element.prototype.remove = function(){ try { if (this && this.tagName === 'IFRAME') globalThis.__oxIfrEv('elremove', this, _sig()); } catch(_){} return _rm2.apply(this, arguments); };
            var _ihD = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
            if (_ihD && _ihD.set && _ihD.get) {
                Object.defineProperty(Element.prototype, 'innerHTML', {
                    get: _ihD.get,
                    set: function(v){
                        try { if (this && this.querySelector && this.querySelector('iframe')) globalThis.__oxIfrEv('ih-wipe', this, _sig()); } catch(_){ }
                        _ihD.set.call(this, v);
                    },
                    configurable: true,
                });
            }
        } catch (e) { try { globalThis.__oxIfrHookErr = String(e); } catch(_){} }
        window.addEventListener('unhandledrejection', function(e){
            __oxScriptErrors.push('rej:' + String((e && e.reason && e.reason.message) || (e && e.reason) || e).slice(0, 300));
        });
        window.addEventListener('DOMContentLoaded', function(){ window.__oxDcl = window.__oxDcl || performance.now(); });
        window.addEventListener('load', function(){ window.__oxLoad = window.__oxLoad || performance.now(); });
        try { window.addEventListener('message', function(ev){ try { var d=ev&&ev.data; var t=window.__oxTopIn||(window.__oxTopIn=[]); if(t.length<14){ var s; try { s=(typeof d==='string')?d:JSON.stringify(d); } catch(_){ s='NC'; } t.push(Math.round(performance.now())+'|'+((s||'und').slice(0,100))+'@'+String(ev.origin).slice(0,24)+'#src'+(ev.source?'y':'n')); } if(d&&d&&d.event&&d.source==='cloudflare-challenge'){ window.__oxLastSrc=ev.source; if(!window.__oxTopSrcEv){ window.__oxTopSrcEv=ev.source; window.__oxTopTrusted=ev.isTrusted; } } if(!window.__oxDT0){ window.__oxDT0=typeof d; try { window.__oxDT0k=(d&&typeof d==='object')?Object.keys(d).slice(0,6).join('|'):String(d).slice(0,50); } catch(_){} } } catch(_){} }, true); } catch(e) {}
        try { var _as=Element.prototype&&Element.prototype.attachShadow; if(_as){ Element.prototype.attachShadow=function(){ var r=_as.apply(this,arguments); try { (window.__oxSRs||(window.__oxSRs=[])).push({h:String((this&&this.id)||this&&this.tagName||''),r:r}); } catch(_){} return r; }; } } catch(e) { try{window.__oxAsErr=String(e);}catch(_){} }
        try { var _cwd=Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype,'contentWindow'); if(_cwd&&_cwd.get){ var _cwg=_cwd.get; Object.defineProperty(HTMLIFrameElement.prototype,'contentWindow',{get:function(){ var v; try{v=_cwg.call(this);}catch(e){v=undefined;} try { var q=window.__oxCWQ||(window.__oxCWQ=[]); if(q.length<24) q.push({t:Math.round(performance.now()),conn:!!this.isConnected,id:String(this.id||'-').slice(0,18),v:v}); if(v&&typeof v==='object'&&typeof v.postMessage==='function'&&!v.__oxPmW){ var _pm=v.postMessage; v.__oxPmW=1; v.postMessage=function(d,t){ try { var p=window.__oxPmL||(window.__oxPmL=[]); if(p.length<10){ var s; try{s=(typeof d==='string')?d:JSON.stringify(d);}catch(e){s='NC';} p.push(Math.round(performance.now())+'#'+((s||'').slice(0,72))); } } catch(_){} return _pm.apply(v,arguments); }; } } catch(_){} return v; },configurable:true,enumerable:true}); window.__oxCwWrap='ok'; } else { window.__oxCwWrap='no-desc'; } } catch(e) { try{window.__oxCwWrap='E:'+String(e).slice(0,60);}catch(_){} }
        try {
            var _oael=window.addEventListener;
            window.addEventListener=function(t,fn,o){
                if(t==='message'&&typeof fn==='function'&&!fn.__oxWrapped){
                    var g=function(ev){ try { return fn.call(this,ev); } catch(e){ try { var q=window.__oxHdlErr||(window.__oxHdlErr=[]); if(q.length<6) q.push((String(e&&e.message||e)+' ~ '+String(e&&e.stack||'').split('\n')[1]).slice(0,220)); } catch(_){} throw e; } };
                    try { g.prototype=fn.prototype; } catch(_){ try { Object.setPrototypeOf(g,fn); } catch(_2){} }
                    try { g.__oxWrapped=1; } catch(_){}
                    return _oael.call(window,t,g,o);
                }
                return _oael.call(window,t,fn,o);
            };
        } catch(e) { try { window.__oxWrapAelErr=String(e).slice(0,80); } catch(_){} }
        try { window.addEventListener('error', function(ev){ try { var q=window.__oxTopErr||(window.__oxTopErr=[]); if(q.length<6) q.push(String(ev&&ev.message||ev).slice(0,160)+' ~ '+String(ev&&ev.filename||'').slice(-40)+':'+(ev&&ev.lineno||0)); } catch(_){} }, true); } catch(e) {}
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
        // A hung fetch/module-eval inside drive can park forever; cap it so
        // the probe keeps polling and the widget path stays observable.
        let _ = tokio::time::timeout(
            Duration::from_secs(20),
            page.drive_frame_tree(&client, &profile),
        )
        .await;
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
    let skov = std::env::var("BROWSER_OXIDE_SK").unwrap_or_default();
    let tsinject: String = r#"(function(){
        if (globalThis.__oxTsInjected) return 'already';
        globalThis.__oxTsInjected = 1;
        try {
            var _ce = document.createElement.bind(document);
            window.__oxPnet = { f: 0, ft: [], fe: [], x: 0, xt: [], xe: [] };
            try {
            var _pf = window.fetch;
            if (typeof _pf === 'function') {
                window.fetch = function(){
                    var u; try { u = String((arguments[0] && arguments[0].url) || arguments[0]).slice(0,110); } catch(_e0){ u='?'; }
                    window.__oxPnet.f++;
                    if (window.__oxPnet.ft.length < 12) window.__oxPnet.ft.push(u);
                    var p; try { p = _pf.apply(this, arguments); } catch(e){ window.__oxPnet.fe.push('sy:'+String(e).slice(0,60)); throw e; }
                    if (p && p.then) p.then(function(r){ if(window.__oxPnet.ft.length<20) window.__oxPnet.ft[window.__oxPnet.ft.length-1]+='>'+r.status; },
                                            function(e){ if(window.__oxPnet.fe.length<6) window.__oxPnet.fe.push(u+'~'+String(e&&e.message||e).slice(0,60)); });
                    return p;
                };
            }
            } catch(_pfE) {}
            try {
            var _xo = window.XMLHttpRequest;
            if (typeof _xo === 'function') {
                window.XMLHttpRequest = function(){
                    var x = new _xo(); var _os = x.send;
                    x.send = function(){ window.__oxPnet.x++; if(window.__oxPnet.xt.length<12) window.__oxPnet.xt.push(String(x.responseURL||x._oxU||'').slice(0,90)); try{ x.addEventListener('load',function(){ if(window.__oxPnet.xt.length<20) window.__oxPnet.xt[window.__oxPnet.xt.length-1]+='>'+x.status; }); x.addEventListener('error',function(){ if(window.__oxPnet.xe.length<4) window.__oxPnet.xe.push(String(x.responseURL||'').slice(0,60)); }); }catch(_ae){} return _os.apply(x,arguments); };
                    return x;
                };
                try { window.XMLHttpRequest.prototype = _xo.prototype; } catch(_pe){}
                ['UNSENT','OPENED','HEADERS_RECEIVED','LOADING','DONE'].forEach(function(k,i){ try{ window.XMLHttpRequest[k]=_xo[k]; }catch(_cE){} });
            }
            } catch(_xoE) {}
            try {
            var _Wc = window.Worker;
            if (typeof _Wc === 'function' && !_Wc.__oxW) {
                var W3 = function(u,o){
                    var w = new _Wc(u,o);
                    window.__oxPnet.w = (window.__oxPnet.w||0)+1;
                    var _pw = w.postMessage;
                    w.postMessage = function(){ window.__oxPnet.wt=(window.__oxPnet.wt||0)+1; return _pw.apply(w,arguments); };
                    w.addEventListener('message',function(){ window.__oxPnet.wr=(window.__oxPnet.wr||0)+1; });
                    w.addEventListener('error',function(ev){ window.__oxPnet.we=(window.__oxPnet.we||[]).slice(-3).concat([String(ev&&ev.message||'').slice(0,70)]); });
                    return w;
                };
                try { W3.prototype = _Wc.prototype; } catch(_wpe){}
                try { W3.__oxW = 1; } catch(_we2){}
                window.Worker = W3;
            }
            } catch(_wE) {}
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
                        if (window.__oxTsRendered) return true;
                        var sk = '__SKOV__' || null;
                        if (!sk) { try { var _ct=document.querySelector('.cf-turnstile[data-sitekey]'); if(_ct){ var _dv=_ct.getAttribute('data-sitekey'); if(_dv){ sk=_dv; } } } catch(_e2){} }
                        if (!sk) {
                            // Flight array gets drained by react-dom; read the
                            // SSR-inline payload from the document instead.
                            var all = document.documentElement.innerHTML;
                            var m = all.match(/"sitekey":"(0x[0-9A-Za-z_-]+)"/) || all.match(/[0-9]x[0-9A-Za-z_-]{14,}/);
                            if (m) { sk = m[1] || m[0]; }
                        }
                        if (!sk || typeof turnstile === 'undefined') { window.__oxTsNoSk = (window.__oxTsNoSk || 0) + 1; return false; }
                        try { if (window.__oxTsWid) { turnstile.remove(window.__oxTsWid); } } catch (_rm) {}
                        var d = document.createElement('div');
                        d.id = 'ox-ts-box';
                        (window.__oxMountRoot || document.documentElement).appendChild(d);
                        window.__oxTsRendered = 1;
                        window.__oxTsWid = turnstile.render(d, { sitekey: sk, callback: function(t){ window.__oxToken = t; } });
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
    })()"#
    .replace("__SKOV__", &skov)
    // Parent-side mutation counter on the widget container subtree: proves
    // (or refutes) that the challenge ever paints progress in the parent DOM.
    .replace(
        "return 'injected';",
        "try { window.__oxPMO = 0; var _pc = document.querySelector('[class*=cf-turnstile],[data-sitekey]'); if (_pc && window.MutationObserver) { new MutationObserver(function(ms){ window.__oxPMO += ms.length; }).observe(document.documentElement, { subtree: true, childList: true, attributes: true }); } } catch(_pmo) {} return 'injected';",
    );
    let _ = page
        .evaluate(tsinject.as_str())
        .ok()
        .map(|s| println!("TSINJECT={s}"));

    // Dump full engine DOM for offline diffing.
    if let Ok(dom) = page.evaluate("document.documentElement.outerHTML") {
        let _ = std::fs::write("/tmp/ox_engine_dom.html", dom);
    }

    // Wait for the widget: implicit render fills .cf-turnstile with an iframe.
    let mut fired = false;
    let mut seeded = false;
    let dump_worker =
        std::env::var("BROWSER_OXIDE_DUMP_WORKER").ok().as_deref() == Some("1");
    let mut worker_dumped = false;
    for poll in 0..480 {
        let _ = tokio::time::timeout(
            Duration::from_secs(20),
            page.drive_frame_tree(&client, &profile),
        )
        .await;
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
        try { if (!window.__oxEvsrcL) { window.__oxEvsrcL = 1; window.__oxParentEvalSrc = window.__oxParentEvalSrc || []; window.addEventListener('message', function (ev) { try { var d = ev && ev.data; if (d && d.__oxEvalSrc) { var arr = window.__oxParentEvalSrc, code = d.code || '', dup = false; for (var i = 0; i < arr.length; i++) { if (arr[i] && arr[i].code && arr[i].code.length === code.length) { dup = true; break; } } if (!dup && arr.length < 8) arr.push(d); } } catch (_esl) {} }); } } catch (_esg) {}
        if (typeof turnstile === 'object' && (window.__oxTsRn | 0) < 6 && (!window.__oxTsRendered || !document.getElementById('ox-ts-box'))) {
            try { if (!window.__oxGrWrapped) { window.__oxGrWrapped = 1; window.__oxGrLog = []; var _gr = turnstile.getResponse; turnstile.getResponse = function () { try { window.__oxGrLog.push({ t: Math.round(performance.now()), a: String(arguments[0] || '').slice(0, 24), n: (window.__oxGrLog.length) }); } catch (_gl) {} var r; try { r = _gr.apply(turnstile, arguments); try { window.__oxGrOk = (window.__oxGrOk || 0) + 1; var _s = String(r || ''); if (window.__oxGrLog.length && window.__oxGrLog[window.__oxGrLog.length - 1]) { window.__oxGrLog[window.__oxGrLog.length - 1].res = _s.slice(0, 40) || 'empty'; } } catch (_go) {} } catch (e) { try { var _li = window.__oxGrLog[window.__oxGrLog.length - 1]; if (_li) { _li.err = String(e && e.message || e).slice(0, 140); _li.st = String(e && e.stack || '').replace(/\n/g, '|').slice(0, 420); } } catch (_ge) {} throw e; } return r; }; } } catch (_gw) { window.__oxGrWrapE = String(_gw).slice(0, 80); }
            try {
                var sk = '__SKOV__' || null;
                if (!sk) { try { var _ct = document.querySelector('.cf-turnstile[data-sitekey]'); if (_ct) { sk = _ct.getAttribute('data-sitekey'); } } catch (e2) {} }
                if (!sk) {
                    var hay = '';
                    try { hay = JSON.stringify(self.__next_f || []); } catch (e0) {}
                    var m = hay.match(/[0-9]x[0-9A-Za-z_-]{14,}/);
                    if (!m) { try { hay = document.documentElement.innerHTML; m = hay.match(/[0-9]x[0-9A-Za-z_-]{14,}/); } catch (e1) {} }
                    sk = m && m[0];
                }
                if (!sk) { window.__oxTsNoSk = 1; }
                else {
                    try { var _old = document.getElementById('ox-ts-box'); if (_old) { _old.remove(); } } catch (_e3) {}
                    try { if (window.__oxTsWid) { turnstile.remove(window.__oxTsWid); } } catch (_rm2) {}
                    var d = document.createElement('div');
                    d.id = 'ox-ts-box';
                    // Next.js hydration wipes foreign body children (cW removeChild);
                    // mount on the documentElement, which React never touches.
                    (window.__oxMountRoot || document.documentElement).appendChild(d);
                    window.__oxTsRendered = 1;
                    window.__oxTsRn = (window.__oxTsRn | 0) + 1;
                    window.__oxTsWid = turnstile.render(d, { sitekey: sk, callback: function(t){ window.__oxToken = t; } });
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
    })()"#
                .replace("__SKOV__", &skov)
                .as_str(),
            )
            .unwrap_or_else(|e| format!("EVAL_ERR:{e}"));
        if poll % 2 == 0 {
            println!("POLL{poll}={state}");
        }
        if state.contains("XXXX") || (state.contains("token") && state.contains("response")) {
            // crude check; final dump below is authoritative
        }

        // Worker-side visibility: the challenge runs its PoW in a blob
        // Worker whose source only exists in the engine's worker-spawn
        // record; surface it (and the worker-realm bootstrap snapshot)
        // for the post-run analysis.
        if poll % 10 == 0 || poll < 3 {
            if let Ok(w) = page.evaluate(
                "JSON.stringify((globalThis.__oxOps&&globalThis.__oxOps.op_worker_last_spawn?globalThis.__oxOps.op_worker_last_spawn():[]).map(function(r){return [String(r[0]).slice(0,100),r[1].length];}))",
            ) {
                if w != "[]" {
                    println!("WORKER_SPAWNS@{poll}={w}");
                }
            }
            if let Ok(d) = page.evaluate(
                "JSON.stringify((globalThis.__oxOps&&globalThis.__oxOps.op_worker_diag_read?globalThis.__oxOps.op_worker_diag_read():[]).slice(-4))",
            ) {
                if d != "[]" {
                    println!("WORKER_DIAG@{poll}={d}");
                }
            }
            if dump_worker && !worker_dumped {
                if let Ok(src) = page.evaluate(
                    "String((globalThis.__oxOps&&globalThis.__oxOps.op_worker_last_spawn?globalThis.__oxOps.op_worker_last_spawn():[]).slice(-1).map(function(r){return r[1];})[0]||'')",
                ) {
                    if src.len() > 4 {
                        let _ = std::fs::write("/tmp/engine_worker.js", &src);
                        worker_dumped = true;
                        println!("WORKER_DUMPED@{poll} len={}", src.len());
                    }
                }
            }
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
        let mut fi_c = 0usize;
        let picked = pick_challenge_frame(&mut page);
        if let Some((fi, fhref)) = picked {
            fi_c = fi;
            if poll % 6 == 0 {
                println!("FRAME_PICK{poll}={fi} {fhref}");
            }
            let seed = r#"(function(){
        try {
            if (globalThis.__oxSeeded) return 1;
            globalThis.__oxSeeded = 1;
            globalThis.__oxErrs = [];
            try { Error.stackTraceLimit = 60; } catch(_){}
            globalThis.__oxFired = []; globalThis.__oxFiredN = 0; globalThis.__oxMsgs = [];
            var he = function(m){ try { __oxErrs.push(String(m).slice(0,150)); } catch(_){} };
            window.addEventListener('error', function(ev){ he('E:'+(ev&&ev.message)); });
            window.addEventListener('unhandledrejection', function(ev){ he('P:'+(ev&&ev.reason)); });
            window.addEventListener('message', function(ev){ try { var d=ev&&ev.data; if(d&&typeof d==='object'&&(d.event==='extraParams'||(d.ch&&d.apiJsMismatchReloadAttempts!==undefined))){ window.__oxEP={t:Math.round(performance.now()),ev:String(d.event||'-'),ch:d.ch,keys:Object.keys(d).length,src:String(location.href).slice(-30)}; } if(__oxMsgs.length<12){ var s; try { s=(typeof d==='string')?d:JSON.stringify(d); } catch(e){ s='NC'; } __oxMsgs.push((s||'und').slice(0,240)+'@'+String(ev&&ev.origin).slice(0,24)); try { if(s&&s.length>240){ window.__oxMgL=window.__oxMgL||[]; if(window.__oxMgL.length<4) window.__oxMgL.push(s.slice(0,2400)); } } catch(_){} } } catch(_){} });
            try { window.__oxHdlC=[]; window.__oxHdlS=[]; var _ael=window.addEventListener; window.addEventListener=function(t,fn,o){ if(t==='message'&&typeof fn==='function'&&!fn.__oxCw){ var g=function(ev){ try { return fn.call(this,ev); } catch(e){ try { if(__oxHdlC.length<6){ __oxHdlC.push((String(e&&e.message||e)+' ## '+String(e&&e.stack||'').replace(/\n/g,'|').slice(0,3000))); var _sc='',_sks=(document.querySelectorAll('script')||[]),_big=''; for(var _si2=0;_si2<_sks.length;_si2++){ var _tx=_sks[_si2].textContent||''; if(_tx.length>_big.length)_big=_tx; } var _cols=[],_mm; var _re=/location:1:(\d+)/g; while((_mm=_re.exec(String(e&&e.stack||'')))&&_cols.length<3)_cols.push(+_mm[1]); if(!_cols.length){var _m2=/\((?:[^()]*)?:1:(\d+)\)/; if((_m2.exec(String(e&&e.stack||'')))) _cols.push(+_m2[1]); } for(var _ci=0;_ci<_cols.length;_ci++){ var _c=_cols[_ci]; if(_c>0&&_c<_big.length) __oxHdlS.push({c:_c,x:_big.slice(Math.max(0,_c-70),_c+50)}); } try { window.__oxKMsg=(ev&&typeof ev=='object')?JSON.stringify(ev.data).slice(0,700):String(ev&&ev.data).slice(0,120); } catch(_){} try { window.__oxKOrigin=String(ev&&ev.origin).slice(0,48); } catch(_){} try { var _big2='',_sk2=(document.querySelectorAll('script')||[]); for(var _q=0;_q<_sk2.length;_q++){ var _tx2=_sk2[_q].textContent||''; if(_tx2.length>_big2.length)_big2=_tx2; } window.__oxBigLen=_big2.length; window.__oxBigHead=String(_big2).slice(0,110); var _st2=String(e&&e.stack||''),_cols2=[],_mm2; var _re2=/:1:(\d+)/g; while((_mm2=_re2.exec(_st2))&&_cols2.length<4){ var _cv=+_mm2[1]; if(_cols2.indexOf(_cv)<0)_cols2.push(_cv); } window.__oxHdlS=window.__oxHdlS||[]; for(var _ci2=0;_ci2<_cols2.length;_ci2++){ var _c2=_cols2[_ci2]; if(_c2>0&&_c2<_big2.length) __oxHdlS.push({c:_c2,x:_big2.slice(Math.max(0,_c2-120),_c2+80)}); } } catch(_){} try { if(!window.__oxReplay){ window.__oxReplay=1; setTimeout(function(){ try { window.__oxReplayRes='ok'; fn.call(window,ev); } catch(e2){ try { window.__oxReplayRes='E:'+String(e2&&e2.message||e2).slice(0,140); window.__oxReplayStack=String(e2&&e2.stack||'').replace(/\n/g,'|').slice(0,600); } catch(_){} } },1200); } } catch(_){} } } catch(_){} throw e; } }; try{g.prototype=fn.prototype;}catch(_){} try{g.__oxCw=1;}catch(_){} return _ael.call(window,t,g,o); } return _ael.call(window,t,fn,o); }; } catch(e){}
            try { window.__oxOErr=[]; window.onerror=function(){ try { if(window.__oxOErr.length<6) window.__oxOErr.push(Array.prototype.slice.call(arguments,0,3).join('|').slice(0,150)); } catch(_){} }; } catch(e){}
            try { var _dm=globalThis.__deliverMessage; if(typeof _dm==='function'){ globalThis.__deliverMessage=function(){ try { if(!window.__oxDM) window.__oxDM=[]; if(window.__oxDM.length<8){ var s; try { s=(typeof arguments[0]==='object')?JSON.stringify(arguments[0]).slice(0,100):String(arguments[0]).slice(0,100); } catch(e){ s='NC'; } window.__oxDM.push((s||'und')+'~org:'+String(arguments[1]).slice(0,28)); } } catch(_){} var r; try { r=_dm.apply(this,arguments); window.__oxDMOK=(window.__oxDMOK||0)+1; } catch(e){ window.__oxDME=window.__oxDME||[]; if(window.__oxDME.length<4) window.__oxDME.push(String(e&&e.message||e).slice(0,120)); throw e; } return r; }; window.__oxDMWrap='ok'; } else { window.__oxDMWrap='nofn:'+typeof _dm; } } catch(e){ try{window.__oxDMWrap='E:'+String(e).slice(0,40);}catch(_){} }
            try { var _pp = window.parent; var _opm = _pp.postMessage; window.__oxOut = []; window.__oxLong = []; if (_opm) { _pp.postMessage = function(d, t){ try { var s; try { s=(typeof d==='string')?d:JSON.stringify(d); } catch(e){ s='NC'; } if(window.__oxOut.length<8){ window.__oxOut.push((s||'und').slice(0,90)); } if(s&&s.length>90&&window.__oxLong.length<3){ window.__oxLong.push(s.slice(0,900)); } } catch(_){} return _opm.apply(this === undefined || this === null ? _pp : this, arguments); }; } } catch(e){ try { window.__oxOut = ['E:'+String(e).slice(0,60)]; } catch(_){} }
            var _st = window.setTimeout, _si = window.setInterval;
            window.__oxTm = 0; window.__oxTms = []; window.__oxTmSrc = '';
            window.setTimeout = function(f, t){
                __oxTm++; if(__oxTms.length<24) __oxTms.push('t'+(t|0));
                if(!__oxTmSrc) try { __oxTmSrc = 'T'+(t|0)+':'+String(f).slice(0,300); } catch(_){ }
                var g = function(){ try { __oxFiredN++; if(__oxFired.length<6) __oxFired.push('t'+(t|0)); } catch(_){} try { return f.apply(this, arguments); } catch(e){ try { (window.__oxTmrErr=window.__oxTmrErr||[]).length<5&&window.__oxTmrErr.push('t'+(t|0)+'~'+String(e&&e.message||e).slice(0,80)+'~'+String(e&&e.stack||'').split('\n')[1].replace(/^\s*at /,'').slice(0,70)); } catch(_){} throw e; } };
                return _st.call(window, g, t);
            };
            window.setInterval = function(f, t){
                __oxTm++; if(__oxTms.length<24) __oxTms.push('i'+(t|0));
                if(!__oxTmSrc) try { __oxTmSrc = 'I'+(t|0)+':'+String(f).slice(0,300); } catch(_){ }
                var g = function(){ try { __oxFiredN++; if(__oxFired.length<6) __oxFired.push('i'+(t|0)); } catch(_){} try { return f.apply(this, arguments); } catch(e){ try { (window.__oxTmrErr=window.__oxTmrErr||[]).length<5&&window.__oxTmrErr.push('i'+(t|0)+'~'+String(e&&e.message||e).slice(0,80)+'~'+String(e&&e.stack||'').split('\n')[1].replace(/^\s*at /,'').slice(0,70)); } catch(_){} throw e; } };
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
            // Event registration/fire ledger + then/createElement counters: the
            window.__oxEvReg = []; window.__oxEvLife = []; window.__oxThen = 0; window.__oxCEc = 0; window.__oxPreErr = []; window.__oxRej = [];
            try {
                var _pe = function(ev){ try { var e=(ev&&(ev.error||ev.reason||ev))||{}; if(window.__oxPreErr.length<4) window.__oxPreErr.push(String(ev.type)+'~'+String(e.message||ev.message||'?').slice(0,100)+' | '+String(e.stack||'').replace(/\n/g,'~').slice(0,240)); } catch(_){} };
                window.addEventListener('error', _pe, true);
                window.addEventListener('unhandledrejection', function(ev){ try { if(window.__oxRej.length<4) window.__oxRej.push(String(ev.reason&&ev.reason.message||ev.reason).slice(0,100)+' | '+String(ev.reason&&ev.reason.stack||'').replace(/\n/g,'~').slice(0,200)); } catch(_){} }, true);
            } catch(e){ try { window.__oxPreErr=['E:'+String(e).slice(0,60)]; } catch(_){} }
            try {
                var _cpl = function(tag){ return function(){ try { if(window.__oxEvLife.length<24) window.__oxEvLife.push(tag+'@'+Math.round(performance.now())); } catch(_){} }; };
                var _wa = window.addEventListener, _da = document.addEventListener;
                window.addEventListener = function(t, f, o){ try { if(window.__oxEvReg.length<24) window.__oxEvReg.push('w:'+t); } catch(_){} var g = (typeof f==='function')? function(){ _cpl('w!'+t)(); return f.apply(this, arguments); } : f; return _wa.call(window, t, g, o); };
                document.addEventListener = function(t, f, o){ try { if(window.__oxEvReg.length<24) window.__oxEvReg.push('d:'+t); } catch(_){} var g = (typeof f==='function')? function(){ _cpl('d!'+t)(); return f.apply(this, arguments); } : f; return _da.call(document, t, g, o); };
                var _pt = Promise && Promise.prototype && Promise.prototype.then;
                if (_pt) { var _ptw = function oxThenWrap(){ window.__oxThen++; return _pt.apply(this, arguments); }; try { Object.defineProperty(_ptw, 'name', {value:'oxThenWrap', configurable:true}); } catch(_){} Promise.prototype.then = _ptw; }
                window.__oxCETags = []; window.__oxCEDep = 0;
                var _ce2 = document.createElement;
                var _ceW = function oxCreateElement(t){ try { if(window.__oxCEDep>30) return _ce2.apply(document, arguments); window.__oxCEDep++; try { if(window.__oxCETags.length<20) window.__oxCETags.push(window.__oxCEDep+':'+String(t)); } catch(_){} window.__oxCEc++; var r = _ce2.apply(this === document || this === undefined || this === null ? document : this, arguments); window.__oxCEDep--; return r; } catch(e){ window.__oxCEDep--; try { if((window.__oxCEErr=window.__oxCEErr||[]).length<3) window.__oxCEErr.push(String(e&&e.message||e).slice(0,80)+' | '+String(e&&e.stack||'').replace(/\n/g,'~').slice(0,900)); } catch(_){} throw e; } };
                try { Object.defineProperty(_ceW, 'name', {value:'oxCreateElement', configurable:true}); } catch(_){}
                document.createElement = _ceW;
                // append destination ledger: where do created nodes go?
                window.__oxAp = [];
                var _ab = Element.prototype.appendChild, _ib2 = Node.prototype.insertBefore;
                var _sig = function(n){ try { if(!n||!n.tagName) return 'frag#'+(n&&n.nodeName); var p=n; var d=0; while(p&&(p=p.parentElement||p.parentNode)&&d<3){d++;} return n.tagName+(n.id?'#'+n.id:''); } catch(_){ return '?'; } };
                var _psig = function(p){ try { if(!p) return 'null'; if(p===document) return 'doc'; if(p===document.body) return 'body'; if(p===document.head) return 'head'; if(p.documentElement===p) return 'html'; return (p.nodeName||'?')+(p.tagName?'':':host'); } catch(_){ return 'E'; } };
                Element.prototype.appendChild = function(n){ try { if(window.__oxAp.length<16) window.__oxAp.push(_sig(n)+'->'+_psig(this)); } catch(_){} return _ab.apply(this, arguments); };
                Node.prototype.insertBefore = function(n,r){ try { if(window.__oxAp.length<16) window.__oxAp.push(_sig(n)+'->ins'+_psig(this)); } catch(_){} return _ib2.apply(this, arguments); };
                window.__oxAttr = [];
                var _sa = Element.prototype.setAttribute;
                Element.prototype.setAttribute = function(k,v){ try { if(window.__oxAttr.length<14) window.__oxAttr.push(this.tagName+'.'+k+'=' + String(v).slice(0,30)); } catch(_){} return _sa.apply(this, arguments); };
            } catch(e){ try { window.__oxEvReg = ['E:'+String(e).slice(0,60)]; } catch(_){} }
            // IIFE progress markers: the challenge prelude assigns
            // window.gZAJ3/gsnqe5/ECAU8/... in order; typeof-census brackets
            // where the boot sequence stops in a stalled run.
            window.__oxMK = ['gZAJ3', 'gsnqe5', 'ECAU8', 'WDCgH', 'DmKXC', 'JeITm', 'uSSvW', 'NlRIJ', 'lQEw9', 'ssSGH3', 'TGPOQ', 'MDOtK', 'EUkRT', 'PrMVM', 'BJzQd', 'fuXgt5', 'sWIdw'];
            // readyState/DCL/load timeline: did the child document ever
            // transition, and did DOMContentLoaded actually dispatch?
            window.__oxRS = [['s0', String(document.readyState), 0]];
            try {
                var _rsn = function(){ return Math.round(performance.now()); };
                document.addEventListener('readystatechange', function(){ try { window.__oxRS.push(['rc', String(document.readyState), _rsn()]); } catch(_){} }, true);
                document.addEventListener('DOMContentLoaded', function(){ try { window.__oxRS.push(['DCL', String(document.readyState), _rsn()]); } catch(_){} }, true);
                window.addEventListener('load', function(){ try { window.__oxRS.push(['LD', String(document.readyState), _rsn()]); } catch(_){} }, true);
            } catch(e){ try { window.__oxRS.push(['E', String(e).slice(0,40), -1]); } catch(_){} }
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
            let first = !seeded;
            let seeded_ok = page.frame_tree_evaluate(fi, seed);
            if seeded_ok.is_none() {
                if first {
                    println!("SEEDFAIL");
                }
            } else {
                seeded = true;
            }
            if first && seeded {
            let boot = page.frame_tree_evaluate(
                fi,
                r#"JSON.stringify({opt:typeof _cf_chl_opt, keys:(function(){try{return Object.keys(_cf_chl_opt||{}).length}catch(e){return 'E'}})(), u:window.UlwL3?'y':'n', rs:document.readyState, scr:(document.scripts||[]).length})"#,
            );
            println!("SEED_BOOT={boot:?}");
            let meta = page.frame_tree_evaluate(
                fi,
                r#"JSON.stringify({par:window.parent===window,top:(window.top===window?'self':(window.top?'obj':'none')),fe:!!window.frameElement,vis:(document.visibilityState||''),hf:(document.hasFocus?!!document.hasFocus():'nf'),dcr:(function(){try{return JSON.stringify(document.documentElement.getBoundingClientRect())}catch(e){return 'E'}})(),o:(function(){try{return JSON.stringify(_cf_chl_opt||{}).slice(0,420)}catch(e){return 'E'}})()})"#,
            );
            println!("FRAME0_META={meta:?}");
            }
            // Second-stage seed: wrap the async primitives the managed
            // challenge relies on (fetch / rAF / Worker) and record
            // visibility — the stall in run48 happened with zero network
            // activity and no way to see which wait never resolved.
            // Idempotent per realm (`__oxSeed2` guard): re-run on every poll
            // so a reloaded / crash-retry realm gets the evidence wraps too.
            let seed2 = page.frame_tree_evaluate(
                fi,
                r#"(function(){ if(globalThis.__oxSeed2) return 1; globalThis.__oxSeed2=1;
window.__oxFet=[];window.__oxFeE=[];try{var _f=window.fetch;if(typeof _f==='function'){window.fetch=function(){var u;try{u=String((arguments[0]&&arguments[0].url)||arguments[0]).slice(0,90);}catch(e){u='?';}var p;try{p=_f.apply(this,arguments);}catch(e){window.__oxFeE.push('sy:'+String(e).slice(0,70));throw e;}if(p&&p.then){p.then(function(r){window.__oxFe=(window.__oxFe||0)+1;if(window.__oxFet.length<10)window.__oxFet.push(u+'>'+r.status);},function(e){window.__oxFeE.push(u+'~'+String(e&&e.message||e).slice(0,70));});}return p;};}}catch(e){}
window.__oxRAF=0;window.__oxRAFf=0;try{var _r=window.requestAnimationFrame;if(typeof _r==='function'){window.requestAnimationFrame=function(f){window.__oxRAF++;var g=function(t){window.__oxRAFf++;return f.call(this,t);};return _r.call(window,g);};}}catch(e){}
window.__oxWk=[];window.__oxWT=0;window.__oxWR=0;window.__oxWE=0;window.__oxWLast='';window.__oxWPL='';try{var _W=window.Worker;if(typeof _W==='function'){var W2=function(u,o){window.__oxWk.push(String(u).slice(0,80));var w;try{w=new _W(u,o);}catch(e){window.__oxWk.push('E:'+String(e).slice(0,60));throw e;}
try{var _p=w&&w.postMessage;if(typeof _p==='function'){w.postMessage=function(m,t){window.__oxWT++;try{window.__oxWPL=('p'+window.__oxWT+':'+((typeof m==='object'&&m!==null)?JSON.stringify(m).slice(0,110):String(m).slice(0,110))).slice(0,150);}catch(_){}return _p.apply(w,arguments);};}}catch(_pw){}
try{w.addEventListener('message',function(ev){window.__oxWR++;try{window.__oxWLast=('r'+window.__oxWR+':'+((typeof ev.data==='object'&&ev.data!==null)?JSON.stringify(ev.data).slice(0,110):String(ev.data).slice(0,110))).slice(0,150);}catch(_){}});w.addEventListener('error',function(ev){window.__oxWE++;try{window.__oxWLast='err:'+String(ev&&(ev.message||ev.error&&ev.error.message)||ev).slice(0,110);}catch(_){}});w.addEventListener('messageerror',function(){window.__oxWE++;window.__oxWLast='merr';});}catch(_wl){}
return w;};W2.prototype=_W.prototype;window.Worker=W2;}}catch(e){}
try{window.__oxAPI={};['FontFace','WebAssembly','IntersectionObserver','ResizeObserver','MutationObserver','crypto','TextEncoder','Blob','URL','MessageChannel','CustomEvent','Proxy','Reflect','matchMedia','getComputedStyle','structuredClone','queueMicrotask','requestIdleCallback','PerformanceObserver','Notification','caches'].forEach(function(k){try{window.__oxAPI[k]=typeof window[k];}catch(e){}});try{window.__oxAPI['canvas2d']=!!(document.createElement('canvas').getContext('2d'));}catch(e){window.__oxAPI['canvas2d']='E'+String(e).slice(0,20);}window.__oxAPI['fonts']=!!document.fonts;window.__oxAPI['dpr']=window.devicePixelRatio;try{window.__oxAPI['plugins']=(navigator.plugins||[]).length;window.__oxAPI['mimeTypes']=(navigator.mimeTypes||[]).length;}catch(e){}}catch(e){window.__oxAPIE=String(e).slice(0,80);}
window.__oxWErr=[];window.__oxWRej=[];try{window.addEventListener('error',function(ev){try{if(window.__oxWErr.length<8)window.__oxWErr.push({m:String(ev&&ev.message||'').slice(0,120),f:String(ev&&ev.filename||'').slice(-40),ln:ev&&ev.lineno||0,col:ev&&ev.colno||0,s:String(ev&&ev.error&&ev.error.stack||'').split('|').join('<<').replace(/\n/g,'|').slice(0,900)});}catch(_){}},true);window.addEventListener('unhandledrejection',function(ev){try{if(window.__oxWRej.length<4)window.__oxWRej.push(String(ev&&ev.reason&&ev.reason.message||ev&&ev.reason||'').slice(0,120));}catch(_){}});}catch(e){}
try{window.__oxTapC=(function(){try{var f0=new globalThis.Function('return 1');f0();return {rd:!!globalThis.__oxEvalTapReady,fn:typeof globalThis.__oxInstallEvalTap,mk:1,tl:(globalThis.__oxEvalSrcLog||[]).length};}catch(e){return {E:String(e).slice(0,80)};}})();}catch(e){}
window.__oxGrT=[];window.__oxGrN=0;
(function(){function gr(){window.__oxGrN++;if(window.__oxGrT.length<6){var out={n:window.__oxGrN};try{var r=turnstile.getResponse();out.ok=1;out.res=String(r||'').slice(0,24);}catch(e){out.err=String(e&&e.message||e).slice(0,160);out.st=String(e&&e.stack||'').replace(/\\n/g,'|').slice(0,300);}window.__oxGrT.push(JSON.stringify(out).slice(0,420));}}try{gr();}catch(e){}
window.__oxGrTimer=setInterval(function(){try{gr();}catch(e){}},4000);})()
window.__oxClickLog=[];window.__oxClickN=0;
function oxClick(round){try{
 var c=window.__oxClickLog;
 try{window.focus();}catch(_f){}
 var t=document.querySelector('input[type=checkbox]');
 if(!t){var best=null,bA=0;try{var all=document.querySelectorAll('div,span,label,button');for(var i=0;i<all.length;i++){var e2=all[i];var r0;try{r0=e2.getBoundingClientRect();}catch(_){continue;}var a=(r0.width||0)*(r0.height||0);if((r0.width||0)>=40&&(r0.height||0)>=18&&a>bA){var st=document.defaultView.getComputedStyle(e2);if(!st||st.visibility==='hidden'||st.display==='none')continue;var bp=e2.parentElement?document.defaultView.getComputedStyle(e2.parentElement):null;if(bp&&bp.visibility==='hidden')continue;best=e2;bA=a;}}}catch(_s){}t=best;}
 if(!t){c.push('r'+round+':notarget');return;}
 var r;try{r=t.getBoundingClientRect();}catch(_r){r={left:0,top:0,width:10,height:10};}
 var x=Math.round((r.left||0)+(r.width||10)/2),y=Math.round((r.top||0)+(r.height||10)/2);
 c.push('r'+round+'@'+Math.round(performance.now())+' '+(t.tagName)+(t.id?'#'+t.id:'')+' cls='+String(t.className&&t.className&&t.className.baseVal===undefined?t.className:(t.className&&t.className.baseVal)||'').slice(0,36)+' xy='+x+','+y);
 try{t.scrollIntoView&&t.scrollIntoView();}catch(_sv){}
 try{t.focus&&t.focus();}catch(_fo){}
 function mk(C,ty){try{var O={bubbles:true,cancelable:true,composed:true,clientX:x,clientY:y,screenX:x,screenY:y,button:0,buttons:1,pointerId:1,pointerType:'mouse',isPrimary:true,view:window};return new (typeof C==='function'?C:MouseEvent)(ty,O);}catch(e){try{return new Event(ty,{bubbles:true,cancelable:true});}catch(_){return null;}}}
 var PE=(typeof PointerEvent==='function')?PointerEvent:MouseEvent;
 var seq=[['pointerover',PE],['pointerenter',PE],['pointermove',PE],['pointerdown',PE],['mousedown',MouseEvent],['pointerup',PE],['mouseup',MouseEvent],['click',MouseEvent]];
 for(var i=0;i<seq.length;i++){var e3=mk(seq[i][1],seq[i][0]);if(e3){try{t.dispatchEvent(e3);c.push('d:'+seq[i][0]);}catch(_e3){c.push('E:'+seq[i][0]+'~'+String(_e3).slice(0,40));}}}
 window.__oxClickN++;
}catch(e){try{window.__oxClickLog.push('F'+round+':'+String(e).slice(0,90));}catch(_){}}}
setTimeout(function(){oxClick(1);},20000);
setTimeout(function(){oxClick(2);},45000);
return 2;})()"#,
            );
            if seed2.is_none() {
                println!("SEED2FAIL");
            }
            // Seed3 — crash-cause evidence: record which property gets on
            // document / navigator / canvas-2d-ctx evaluate to `undefined`
            // (the challenge VM dies on `receiver[method]` undefined in its
            // call opcode), plus every inbound `message` payload. On first
            // window error the counters freeze into __oxUGF for the sample.
            let seed3 = page.frame_tree_evaluate(
                fi,
                r#"(function(){ if(globalThis.__oxSeed3) return 1; globalThis.__oxSeed3=1;
window.__oxMI=[];
try{window.addEventListener('message',function(ev){try{if(window.__oxMI.length>=24)return;var d=ev.data,s;try{s=(d&&typeof d==='object')?JSON.stringify(d):String(d);}catch(_){s='unser'+(typeof d);}window.__oxMI.push(''+(s||'').slice(0,140)+'@'+String(ev.origin||'').slice(0,24)+'@'+Math.round(performance.now()));}catch(_){}},true);}catch(e){}
window.__oxUG={};window.__oxUGL=[];window.__oxUGF='';
function cnt(k){try{window.__oxUG[k]=(window.__oxUG[k]||0)+1;window.__oxUGL.push(k);if(window.__oxUGL.length>12)window.__oxUGL.shift();}catch(_){}}
function pr(o,tag){try{return new Proxy(o,{get:function(t,k){var v;try{v=Reflect.get(t,k);}catch(e){return undefined;}try{if(v===undefined&&typeof k==='string'&&k.length>2&&k!=='constructor'&&k.indexOf('webkit')!==0&&k.indexOf('moz')!==0&&k.indexOf('ox')!==0)cnt(tag+'!'+k);}catch(_){}return v;}});}catch(e){return o;}}
try{var _d=window.document;try{Object.defineProperty(window,'document',{value:pr(_d,'doc'),configurable:true});}catch(e){}}catch(e){}
try{var _n=window.navigator;try{Object.defineProperty(window,'navigator',{value:pr(_n,'nav'),configurable:true});}catch(e){}}catch(e){}
try{var _gc=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(){var c;try{c=_gc.apply(this,arguments);}catch(e){return undefined;}try{if(!c)return c;if(!c.__oxPX){var pc=pr(c,'cx');try{Object.defineProperty(c,'__oxPX',{value:pc,configurable:true,writable:true});}catch(_){return c;}}return c.__oxPX;}catch(e){return c;};};}catch(e){}
window.__oxFreeze=function(){try{if(window.__oxUGF)return;var ent=[];for(var k in window.__oxUG)ent.push([k,window.__oxUG[k]]);ent.sort(function(a,b){return b[1]-a[1];});window.__oxUGF=JSON.stringify({top:ent.slice(0,14),last:window.__oxUGL});}catch(e){window.__oxUGF='E';}};
try{window.addEventListener('error',function(){window.__oxFreeze();},true);}catch(e){}
return 3;})()"#,
            );
            if seed3.is_none() {
                println!("SEED3FAIL");
            }
        }
        if poll == 1 && fi_c > 0 {
            // Seed4 — trace hooks inside the challenge frame: the same-isolate
            // `__deliverMessage` wrap, message-listener registration log, and
            // timestamped handler-error ring. The frame-tree pump's own
            // `__oxFrameMsgLog` ring + `__oxGP` gate trace (bootstrap) cover
            // the delivery path; this adds the handler-side view.
            let seed4 = page.frame_tree_evaluate(
                fi_c,
                r#"(function(){ if(globalThis.__oxSeed4) return 1; globalThis.__oxSeed4=1;
window.__oxDL=[];   // deliveries: post:seq@t{+/-}
window.__oxGP=[];   // gate pump: t,ready,forced
window.__oxHC2=[];  // handler errors with timestamps
window.__oxT0=performance.now();
try{var _dm=window.__deliverMessage;window.__oxDMWrap='v4';
if(typeof _dm==='function'){window.__deliverMessage=function(){var t=Math.round(performance.now()-window.__oxT0);var ok=0,e='';
try{ok=_dm.apply(this,arguments);}catch(err){e=(err&&err.message||'').slice(0,80);try{window.__oxHC2.push(t+'|'+e);}catch(_){}}
try{var a=arguments[0]||{},d=a.data===undefined?'':(typeof a.data==='object'?JSON.stringify(a.data):String(a.data));
window.__oxDL.push(t+'|'+(d||'').slice(0,48)+(ok?'':'+F'+e));if(window.__oxDL.length>40)window.__oxDL.shift();}catch(_){}
return ok;};}}catch(e){try{window.__oxHC2.push('wrap:'+e.message);}catch(_){}}
try{var _h=window.addEventListener;window.addEventListener=function(t2,f,o){try{if(t2==='message'){var t=Math.round(performance.now()-window.__oxT0);window.__oxHC2.push('reg@'+t);}}catch(_){}
try{return _h.call(this,t2,f,o);}catch(e2){try{window.__oxHC2.push('regE:'+e2.message);}catch(_){}}};}catch(e){}
return 4;})()"#,
            );
            if seed4.is_none() {
                println!("SEED4FAIL");
            }
        }
        if poll % 3 == 0 {
            let sample = page.frame_tree_evaluate(
                fi_c,
                r#"JSON.stringify({fh:String(location.href).slice(-52),s2d:!!globalThis.__oxSeed2,clk:Math.round(performance.now()),mut:window.__oxMut,tm:window.__oxTm,fn:window.__oxFiredN,tms:window.__oxTms,br:window.__oxBare,bi:(window.__oxBI||[]).length,fe:window.__oxFe,xhr:window.__oxXhr,mg:window.__oxMsgs,doc:(document.documentElement.outerHTML||'').length,head:document.head?document.head.children.length:-1,stg:!!document.getElementById('stage'),wk:(typeof Worker),bl:document.body?document.body.innerHTML.length:-1,errs:(window.__oxErrs||[]).slice(0,2),seedErr:window.__oxSeedErr||'',out:(window.__oxOut||[]).slice(-3),ifr:(function(){try{return document.querySelectorAll('iframe').length}catch(e){return 'E'}})(),evr:(window.__oxEvReg||[]).slice(0,12),evl:(window.__oxEvLife||[]).slice(-6),evlN:(window.__oxEvLife||[]).length,th:window.__oxThen,ce:window.__oxCEc,cn:(function(){try{return document.body?document.body.children.length:-1}catch(e){return 'E'}})(),cet:(window.__oxCETags||[]).slice(0,12),ap:(window.__oxAp||[]).slice(0,10),at:(window.__oxAttr||[]).slice(0,8),lg:(window.__oxLong||[]).slice(-1),pe:(window.__oxPreErr||[]).slice(0,2),rj:(window.__oxRej||[]).slice(0,2),mk:(window.__oxMK||[]).map(function(n){return n+':'+typeof window[n]}).filter(function(s){return !s.endsWith(':undefined')}),rs:(window.__oxRS||[]).slice(0,10),te:(window.__oxTmrErr||[]).slice(0,3),oe:(function(){try{return typeof window.onerror}catch(e){return 'E'}})(),dmW:window.__oxDMWrap||'',dm:(window.__oxDM||[]).slice(0,6),dmOK:window.__oxDMOK||0,dmE:(window.__oxDME||[]).slice(0,2),hc:(window.__oxHdlC||[]).slice(0,3),ceD:window.__oxCEDep||0,ceE:(window.__oxCEErr||[]).slice(0,2),hS:(window.__oxHdlS||[]).slice(0,3),mgL:(window.__oxMgL||[]).slice(0,2),oeR:(window.__oxOErr||[]).slice(0,3),apI:(function(){var a=window.__oxAPI||{};var o={};for(var k in a)o[k]=a[k];return o;})(),apE:window.__oxAPIE||'',we:(window.__oxWErr||[]).slice(0,3),rj2:(window.__oxWRej||[]).slice(0,2),ep:window.__oxEP||null,ceT:(window.__oxCETags||[]).join(',').slice(0,60),css:(function(){try{return typeof CSSStencil==='undefined'?[typeof CSSStyleSheet,!!(document.adoptedStyleSheets),typeof(CSSStyleSheet&&CSSStyleSheet.prototype&&CSSStyleSheet.prototype.replaceSync)].join('/'):'stencil'}catch(e){return 'E'}})(),co:(function(){try{return typeof window._cf_chl_opt+':'+(window._cf_chl_opt?Object.keys(window._cf_chl_opt).length:'-')}catch(e){return 'E'}})(),f2:(window.__oxFet||[]).slice(0,4),fe2:window.__oxFe||0,feE:(window.__oxFeE||[]).slice(0,2),rf:(window.__oxRAF||0)+'/'+(window.__oxRAFf||0),wk2:(window.__oxWk||[]).slice(0,3),wT:window.__oxWT||0,wR:window.__oxWR||0,wE:window.__oxWE||0,wL:window.__oxWLast||'',wP:window.__oxWPL||'',ckN:window.__oxClickN||0,ck:(window.__oxClickLog||[]).slice(-8),v2:(window.__oxV2||[]).join('/'),pn:(function(){var p=window.__oxPnet||{};return 'f'+(p.f||0)+'/'+((p.fe||[]).length)+' x'+(p.x||0)+' w'+(p.w||0)+'/'+(p.wt||0)+'/'+(p.wr||0);})(),pf:(function(){var p=window.__oxPnet||{};return (p.ft||[]).slice(0,5);})(),kMsg:window.__oxKMsg||'',kOrg:window.__oxKOrigin||'',bLen:window.__oxBigLen||0,bHead:window.__oxBigHead||'',rp:window.__oxReplayRes||'',rpS:window.__oxReplayStack||'',gr:(window.__oxGrLog||[]).slice(0,6),grE:window.__oxGrWrapE||'',grT:(window.__oxGrT||[]).slice(0,3),grN:window.__oxGrN||0,ugf:(window.__oxUGF||'').slice(0,400),ugl:(window.__oxUGL||[]).slice(-6),mi:(window.__oxMI||[]).slice(0,6),s3d:!!globalThis.__oxSeed3,tapC:(function(){var t=window.__oxTapC;if(!t)return 'no';return (t.E?'E:'+t.E:'rd:'+(t.rd?1:0)+' fn:'+t.fn+' tl:'+t.tl);})(),tn:globalThis.__oxTapN||0,tlL:(globalThis.__oxEvalSrcLog||[]).length,pS:(globalThis.__oxParentEvalSrc||[]).length,rdy:(function(){try{return window.__oxFrameReady===1?1:(window.hasOwnProperty('__oxFrameReady')?String(window.__oxFrameReady).slice(0,20):'nokey')}catch(e){return 'E'}})(),dl:(window.__oxDL||[]).slice(0,12),gp:(window.__oxGP||[]).slice(0,8),hc2:(window.__oxHC2||[]).slice(0,8),fmL:(window.__oxFrameMsgLog||[]).slice(-5),gn:window.__oxGateN||0,wtx:window.__oxWTX||0,wtxL:window.__oxWTXLast||'',wrx:window.__oxWRX||0})"#,
            );
            println!("FRAME0_S{poll}={sample:?}");
            let pst = page.evaluate(
                r#"JSON.stringify((function(){try{var c=document.querySelector('#ox-ts-box,[class*=cf-turnstile],[data-sitekey]');if(!c)return{noc:1};var r=c.getBoundingClientRect();var fr=c.querySelector('iframe');var gr='';try{gr=(window.turnstile&&window.turnstile.getResponse)?String(window.turnstile.getResponse()||'').slice(0,36):'';}catch(e){gr='E';}return {pm:window.__oxPMO|0,w:Math.round(r.width),h:Math.round(r.height),ifr:!!fr,vis:String(document.visibilityState),gr:gr,bs:(c.innerHTML||'').length};}catch(e){return {E:String(e).slice(0,50)};}})())"#,
            );
            println!("PST{poll}={pst:?}");
        }
        if poll % 6 == 5 {
            let ifl = page
                .evaluate("JSON.stringify(window.__oxIfrLog || [])")
                .unwrap_or_else(|e| format!("E:{e}"));
            println!("IFLIFE{poll}={ifl}");
            for ci in 0..page.frame_tree_count() {
                let h = page.frame_tree_evaluate(
                    ci,
                    "String((globalThis.location&&location.href)||'').slice(-64)",
                );
                println!("FCEN{poll}[{ci}]={:?}", h.unwrap_or_default());
            }
        }

        // Live eval-source sweep: realm logs die with their realm (challenge
        // frames get replaced by crashed_retry navigations, worker isolates
        // are unreachable), so flush every unseen captured source to disk the
        // moment it appears instead of relying on the end-of-run dump.
        let mut sweep_idx = 0usize;
        let sink_raw = page
            .evaluate("JSON.stringify(globalThis.__oxParentEvalSrc||[])")
            .unwrap_or_else(|_| "[]".into());
        if let Ok(entries) = serde_json::from_str::<serde_json::Value>(&sink_raw) {
            if let Some(list) = entries.as_array() {
                for ent in list {
                    let code = ent.get("code").and_then(|c| c.as_str()).unwrap_or("");
                    let key = format!("ox_eval_live_Q{sweep_idx}.js");
                    let path = std::path::Path::new("/tmp").join(&key);
                    if code.len() >= 512
                        && (!path.exists()
                            || std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0)
                                != code.len())
                    {
                        let _ = std::fs::write(&path, code);
                        println!(
                            "EVAL_LIVE{poll}: Q{sweep_idx} {}B -> {} ({})",
                            code.len(),
                            path.display(),
                            ent.get("href").and_then(|h| h.as_str()).unwrap_or(""),
                        );
                    }
                    sweep_idx += 1;
                }
            }
        }
        for ci in 0..page.frame_tree_count() {
            let raw = page.frame_tree_evaluate(
                ci,
                "JSON.stringify(globalThis.__oxEvalSrcLog||[])",
            );
            let Some(raw) = raw else { continue };
            if !raw.starts_with('[') || raw == "[]" {
                continue;
            }
            let Ok(entries) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let Some(list) = entries.as_array() else { continue };
            for (ri, ent) in list.iter().enumerate() {
                let Some(code) = ent.as_str() else { continue };
                let path = std::path::Path::new("/tmp")
                    .join(format!("ox_eval_live_F{ci}_{ri}.js"));
                if code.len() >= 512
                    && (!path.exists()
                        || std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0)
                            != code.len())
                {
                    let _ = std::fs::write(&path, code);
                    println!("EVAL_LIVE{poll}: F{ci}/{ri} {}B -> {}", code.len(), path.display());
                }
            }
        }
        let _ = sink_raw;

        // Engine executed-script ring sweep: dump every script source the
        // engine actually executed (top realm + each materialized frame
        // realm). These are the real runtime texts obfuscated stack frames
        // (v6@1:50607) must be resolved against.
        for (si, (name, code)) in page.executed_scripts().into_iter().enumerate() {
            let path = std::path::Path::new("/tmp").join(format!("ox_exec_T{si}.js"));
            if code.len() >= 512
                && (!path.exists()
                    || std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0)
                        != code.len())
            {
                let _ = std::fs::write(&path, &code);
                println!(
                    "EXEC ring{poll}: T{si} {}B {} -> {}",
                    code.len(),
                    name,
                    path.display()
                );
            }
        }
        for ci in 0..page.frame_tree_count() {
            for (si, (name, code)) in page.frame_executed_scripts(ci).into_iter().enumerate() {
                let path =
                    std::path::Path::new("/tmp").join(format!("ox_exec_F{ci}_{si}.js"));
                if code.len() >= 512
                    && (!path.exists()
                        || std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0)
                            != code.len())
                {
                    let _ = std::fs::write(&path, &code);
                    println!(
                        "EXEC ring{poll}: F{ci}/{si} {}B {} -> {}",
                        code.len(),
                        name,
                        path.display()
                    );
                }
            }
        }
    }

    // A crash-retry swap can leave the tree momentarily empty; give the
    // replacement frame a few drive cycles to materialize? Let's go
    // straightforwardly: wait for the tree to repopulate before the deep dump.
    for _ in 0..20 {
        if page.frame_tree_count() > 0 {
            break;
        }
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            page.drive_frame_tree(&client, &profile),
        )
        .await;
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(300))
            .await;
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
        let cssp = page.frame_tree_evaluate(
            i,
            r#"(function(){try{var s=new CSSStyleSheet();s.replaceSync('a{color:red}');document.adoptedStyleSheets=[s];return 'OK:'+document.adoptedStyleSheets.length+':'+(document.adoptedStyleSheets[0]===s)}catch(e){return 'E:'+e}})()"#,
        );
        println!("FRAME{i}_CSS={cssp:?}");
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

    // Iframe life ledger + fate of every created iframe element.
    let iflog = page
        .evaluate("JSON.stringify((window.__oxIfrLog || []).slice(-24))")
        .unwrap_or_else(|e| format!("E:{e}"));
    println!("IFLOG={iflog}");
    let ifrs = page
        .evaluate(
            r#"JSON.stringify((window.__oxIfrs || []).map(function(f){ return { uid: (globalThis.__oxIfrUid ? globalThis.__oxIfrUid(f) : -1), src: String(f.getAttribute('src')||'').slice(-60), nm: String(f.getAttribute('name')||''), conn: !!f.isConnected, par: (function(){ try { return f.parentNode ? (f.parentNode.tagName + (f.parentNode.id ? '#'+f.parentNode.id : '')) : null; } catch(e){ return 'E'; } })(), cw: !!f.contentWindow }; }))"#,
        )
        .unwrap_or_else(|e| format!("E:{e}"));
    println!("IFRS={ifrs}");
    let msgs = page
        .frame_tree_evaluate(
            0,
            r#"JSON.stringify({in:(window.__oxMsgs||[]).slice(-4),out:(window.__oxOut||[]).slice(-4),ifr:document.querySelectorAll('iframe').length,head:(document.head||[]).children?document.head.children.length:-1})"#,
        )
        .unwrap_or_default();
    println!("CHILD_MSGS={msgs:?}");

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
        tco: (function(){ try { return typeof window._cf_chl_opt + ':' + (window._cf_chl_opt ? Object.keys(window._cf_chl_opt).length : '-'); } catch(e){ return 'E'; } })(),
        tin: (window.__oxTopIn || []),
        hdl: (window.__oxHdlErr || []),
        terr: (window.__oxTopErr || []),
        idn: (function(){ try {
            var last=window.__oxLastSrc||null;
            var out={cwWrap:window.__oxCwWrap||'-',asErr:window.__oxAsErr||'-',nSR:(window.__oxSRs||[]).length,
                hosts:(window.__oxSRs||[]).map(function(s){return s.h;}).join(',').slice(0,50),ifr:[],last:!!last,lastH:!!(last&&last.__isFrameHandle),cwqN:(window.__oxCWQ||[]).length};
            (window.__oxSRs||[]).forEach(function(s){ try { [].slice.call(s.r.querySelectorAll('iframe')).forEach(function(f){ var c=null; try{c=f.contentWindow;}catch(e){c='E';} out.ifr.push({via:'SR',id:f.id||'-',src:String(f.src||'').slice(-20),conn:!!f.isConnected,cw:!!c,eqNow:c===last}); }); } catch(e){} });
            try { [].slice.call(document.querySelectorAll('iframe')).forEach(function(f){ var c=null; try{c=f.contentWindow;}catch(e){c='E';} out.ifr.push({via:'doc',id:f.id||'-',src:String(f.src||'').slice(-20),conn:!!f.isConnected,cw:!!c,eqNow:c===last}); }); } catch(e){}
            out.cwq=(window.__oxCWQ||[]).slice(-6).map(function(p){ return p.t+' '+p.id+' conn'+(p.conn?1:0)+' '+(p.v===undefined?'und':(p.v===null?'null':(p.v===last?'EQsrc':'obj'))); });
            return JSON.stringify(out).slice(0,700);
        } catch(e){ return 'E:'+e; } })(),
        tt: String(window.__oxTopTrusted),
        pm: (window.__oxPmL || []).slice(-8),
        dt0: String(window.__oxDT0)+' / '+(window.__oxDT0k||'-'),
        wkn: (function(){ try { return globalThis.__oxOps&&globalThis.__oxOps.op_worker_last_spawn?globalThis.__oxOps.op_worker_last_spawn().map(function(r){return String(r[0]).slice(-48)+'/'+r[1].length;}).join('|'):'noop'; } catch(e){ return 'E:'+e; } })(),
        wkd: (function(){ try { return globalThis.__oxOps&&globalThis.__oxOps.op_worker_diag_read?globalThis.__oxOps.op_worker_diag_read().slice(-3):'noop'; } catch(e){ return 'E:'+e; } })(),
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

    // Dump every Function-constructor source the taps captured (challenge
    // builds its whole VM through `new Function`, invisible to static HTML).
    // One file per source, page-realm then every live frame realm.
    let dump_snippet = r#"(function(){
        var out = [];
        var seen = {};
        function collect(log, tag) {
            try {
                if (!log || !log.length) return;
                for (var i = 0; i < log.length; i++) {
                    var e = log[i]; if (!e) continue;
                    var code = (typeof e === 'string') ? e : (e.code || '');
                    if (code.length < 512) continue;
                    var key = code.length + ':' + String((e && e.href) || '').slice(-40);
                    if (seen[key]) continue; seen[key] = 1;
                    out.push({ tag: tag + i, href: String((e && e.href) || '').slice(-80), code: code });
                }
            } catch (_) {}
        }
        collect(globalThis.__oxEvalSrcLog, 'P');
        collect(globalThis.__oxParentEvalSrc, 'Q');
        return JSON.stringify(out);
    })()"#;
    let mut dump_idx = 0usize;
    if let Ok(raw) = page.evaluate(dump_snippet) {
        if raw.starts_with('[') {
            if let Ok(entries) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(list) = entries.as_array() {
                    for ent in list {
                        let code = ent.get("code").and_then(|c| c.as_str()).unwrap_or("");
                        let tag = ent.get("tag").and_then(|t| t.as_str()).unwrap_or("X");
                        let path = format!("/tmp/ox_eval_{dump_idx}_{tag}.js");
                        let _ = std::fs::write(&path, code);
                        println!(
                            "EVAL_DUMP{dump_idx}: {tag} {}B -> {path} ({})",
                            code.len(),
                            ent.get("href").and_then(|h| h.as_str()).unwrap_or("")
                        );
                        dump_idx += 1;
                    }
                }
            }
        }
    }
    // Frame realms keep their own tap logs; the parent-side mirror only
    // carries what was posted before the frame went away.
    for fi in 0..page.frame_tree_count() {
        let Some(raw) = page.frame_tree_evaluate(
            fi,
            "JSON.stringify(globalThis.__oxEvalSrcLog||[])",
        ) else {
            continue;
        };
        if !raw.starts_with('[') || raw == "[]" {
            continue;
        }
        let Ok(entries) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(list) = entries.as_array() else { continue };
        for ent in list {
            let code = ent
                .get("code")
                .and_then(|c| c.as_str())
                .or_else(|| ent.as_str())
                .unwrap_or("");
            if code.len() < 512 {
                continue;
            }
            let path = format!("/tmp/ox_eval_{dump_idx}_F{fi}.js");
            let _ = std::fs::write(&path, code);
            let href = ent.get("href").and_then(|h| h.as_str()).unwrap_or("");
            println!("EVAL_DUMP{dump_idx}: F{fi} {}B -> {path} {href}", code.len());
            dump_idx += 1;
        }
    }
    if dump_idx == 0 {
        println!("EVAL_DUMP=none");
    }

    // API-surface census: window own-keys + key prototype key lists, same
    // shape as the real-Chrome baseline (/tmp/real_chal_keys.json) so the
    // two can be diffed to find engine API gaps that crash the challenge VM.
    const KEYS_SNIPPET: &str = r#"JSON.stringify((function(){
        function names(o){ try { return Object.getOwnPropertyNames(o||{}).sort(); } catch(e){ return []; } }
        var w = Object.getOwnPropertyNames(globalThis).sort();
        var n = names(Object.getPrototypeOf(globalThis.navigator));
        var d = names(globalThis.Document && Document.prototype);
        var e = names(globalThis.Element && Element.prototype);
        var f = names(Function.prototype);
        var nav = globalThis.navigator || {};
        return { w: w, n: n, d: d, e: e, f: f,
            misc: { mc: typeof MessageChannel, raf: typeof requestAnimationFrame,
                sx: typeof (window.screen && screen.orientation),
                perm: typeof (navigator.permissions && navigator.permissions.query),
                cka: typeof (navigator.cookieEnabled) } };
    })())"#;
    match page.evaluate(KEYS_SNIPPET) {
        Ok(raw) if raw.starts_with('{') => {
            let _ = std::fs::write("/tmp/ox_winkeys_parent.json", &raw);
            println!("WINKEYS: parent {}B", raw.len());
        }
        _ => println!("WINKEYS: parent FAIL"),
    }
    for fi in 0..page.frame_tree_count() {
        if let Some(raw) = page.frame_tree_evaluate(fi, KEYS_SNIPPET) {
            if raw.starts_with('{') {
                let path = format!("/tmp/ox_winkeys_F{fi}.json");
                let _ = std::fs::write(&path, &raw);
                println!("WINKEYS: F{fi} {}B -> {path}", raw.len());
            }
        }
    }
}
