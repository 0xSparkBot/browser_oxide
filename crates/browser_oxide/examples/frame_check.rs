//! Debug: navigate a page and report the frame tree and render state.
//! Run with BROWSER_OXIDE_FRAME_TREE=1 to exercise the frame tree.
fn main() {
    std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
fn run() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let url = std::env::args().nth(1).expect("url");
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let local = tokio::task::LocalSet::new();
    rt.block_on(
        local
        .run_until(async move {
            let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
            let pool = browser_oxide::PagePool::new(2);
            let mut page = pool.acquire(Some(profile.clone())).await.expect("acquire");
            page.navigate_warm(&url).await.expect("warm nav");
            let install_so = r#"
                globalThis.__so=globalThis.__so||[];
                function _rec(tag,e){try{var m=(e&&e.message)||String(e);if(/call stack|Maximum call/.test(m))globalThis.__so.push(tag+':'+(((e&&e.stack)||m).replace(/\n/g,' | ').slice(0,1100)));}catch(_){}}
                try{addEventListener('error',function(e){_rec('ERR',(e&&e.error)||e);});}catch(_){}
                try{addEventListener('unhandledrejection',function(e){_rec('REJ',e&&e.reason);});}catch(_){}
                globalThis.onerror=function(msg,u,l,c,err){_rec('ONERR',err||msg);};
                globalThis.onunhandledrejection=function(e){_rec('ONREJ',e&&e.reason);};
                (function(){var _st=globalThis.setTimeout;globalThis.setTimeout=function(cb,d){if(typeof cb==='function'){var w=function(){try{return cb.apply(this,arguments);}catch(e){_rec('TIMER',e);throw e;}};var a=[w,d];for(var i=2;i<arguments.length;i++)a.push(arguments[i]);return _st.apply(this,a);}return _st.apply(this,arguments);};})();
                (function(){var _si=globalThis.setInterval;globalThis.setInterval=function(cb,d){if(typeof cb==='function'){var w=function(){try{return cb.apply(this,arguments);}catch(e){_rec('INTERVAL',e);throw e;}};var a=[w,d];for(var i=2;i<arguments.length;i++)a.push(arguments[i]);return _si.apply(this,a);}return _si.apply(this,arguments);};})();
                (function(){var _qm=globalThis.queueMicrotask;if(_qm)globalThis.queueMicrotask=function(cb){if(typeof cb==='function'){return _qm(function(){try{return cb.apply(this,arguments);}catch(e){_rec('MICRO',e);throw e;}});}return _qm.apply(this,arguments);};})();
            "#;
            let _ = page.evaluate(install_so);
            let store_hook = r#"(function(){
                if(globalThis.__storeHooked) return 'already';
                var r=document.getElementById('root'); if(!r) return 'no-root';
                var ks=Object.keys(r),k='';for(var i=0;i<ks.length;i++){if(ks[i].indexOf('__reactContainer$')===0)k=ks[i];}
                if(!k) return 'no-key';
                var root=r[k].current||r[k];
                var q=[root],store=null,seen=0;
                while(q.length&&seen<6000){var f=q.shift();seen++;try{var mp=f.memoizedProps;if(mp&&mp.store&&typeof mp.store.getState==='function'){store=mp.store;break;}}catch(_){}if(f.child)q.push(f.child);if(f.sibling)q.push(f.sibling);}
                if(!store) return 'no-store seen='+seen;
                var od=store.dispatch;
                store.dispatch=function(a){
                    var t=a&&a.type;
                    var res=od.apply(this,arguments);
                    try{
                        var st=store.getState(),summ={};
                        for(var kk in st){var v=st[kk];summ[kk]=(v===undefined)?'undef':(v===null?'null':(Array.isArray(v)?('arr'+v.length):(typeof v==='object'?('obj{'+Object.keys(v).slice(0,8).join(',')+'}'):typeof v)));}
                        (globalThis.__actLog=globalThis.__actLog||[]).push(t);
                        globalThis.__lastStateSumm=summ;
                    }catch(_){}
                    return res;
                };
                globalThis.__storeHooked=true;
                return 'hooked';
            })()"#;
            // Hook the Redux store the instant it appears so we capture the
            // state the crashing render reads.
            let mut hooked = false;
            let mut frames_setup = false;
            for step in 0..80 {
                page.drive_frame_tree(&client, &profile)
                    .await;
                if !frames_setup && page.frame_tree_count() > 0 {
                    for i in 0..page.frame_tree_count() {
                        let _ = page.frame_tree_evaluate(i, install_so);
                    }
                    frames_setup = true;
                }
                if !hooked {
                    let h = page.evaluate(store_hook).unwrap_or_default();
                    if h == "hooked" {
                        hooked = true;
                        println!("STOREHOOK[step{step}] hooked");
                    }
                }
            }
            // Test promise machinery: native Promise.all and a script-injection
            // resolved promise (mimics webpack chunk load), plus MessageChannel/MO.
            let _ = page.evaluate(r#"
                globalThis.__pt='pending';
                Promise.all([Promise.resolve(1),Promise.resolve(2)]).then(function(){globalThis.__pt='resolved';});
                globalThis.__ct='pending'; globalThis.__lt='pending';
                (function(){
                  var res; var prom=new Promise(function(r){res=r;});
                  var s=document.createElement('script');
                  s.onload=function(){ globalThis.__lt='onload'; res(); };
                  s.onerror=function(){ globalThis.__lt='onerror'; res(); };
                  s.src='https://js.stripe.com/v3/fingerprinted/js/m-outer-15a2b40a058ddff1cffdb63779fe3de1.js';
                  document.head.appendChild(s);
                  Promise.all([prom]).then(function(){globalThis.__ct='resolved';});
                })();
                globalThis.__mc='pending';
                (function(){
                  try {
                    var ch=new MessageChannel();
                    ch.port1.onmessage=function(){ globalThis.__mc='fired'; };
                    ch.port2.postMessage(null);
                  } catch(e){ globalThis.__mc='err:'+e; }
                })();
                globalThis.__mo='pending';
                (function(){
                  try {
                    var tn=document.createTextNode('x'); document.body.appendChild(tn);
                    var obs=new MutationObserver(function(){ globalThis.__mo='fired'; });
                    obs.observe(tn, {characterData:true});
                    tn.data='y';
                  } catch(e){ globalThis.__mo='err:'+e; }
                })();
                globalThis.__et='pending'; globalThis.__ee='notrun';
                (function(){
                  var res; var prom=new Promise(function(r){res=r;});
                  globalThis.__oxPush=function(){ globalThis.__ee='ran'; res(); };
                  var s=document.createElement('script');
                  s.textContent='globalThis.__oxPush&&globalThis.__oxPush();';
                  document.head.appendChild(s);
                  Promise.all([prom]).then(function(){globalThis.__et='resolved';});
                })();
            "#);
            for _ in 0..4 {
                let _ = page
                    .event_loop()
                    .run_until_idle(std::time::Duration::from_millis(250))
                    .await;
            }
            println!("DEFERCOUNT {}", page.evaluate("'defers='+(globalThis.__deferCount||0)").unwrap_or_default());
            println!("PFINALLY {}", page.evaluate("'finally='+typeof Promise.prototype.finally+' allSettled='+typeof Promise.allSettled+' PromiseName='+(Promise.name||'?')").unwrap_or_default());
            println!("PA {}", page.evaluate("JSON.stringify((globalThis.__pa||[]).map(function(r){return r.len+':'+r.state;}))").unwrap_or_default());
            println!("CHUNK_IDS {}", page.evaluate("JSON.stringify((globalThis.webpackChunkStripeJShosted||[]).map(function(e){return e&&e[0];}))").unwrap_or_default());
            println!("ROOTGETS_CLEAN {}", page.evaluate("'count='+globalThis.__rootGets").unwrap_or_default());
            for st in page.evaluate("JSON.stringify(globalThis.__rootStacks||[])").unwrap_or_default().split("\",\"").take(4) {
                println!("ROOTSTACK {}", st.chars().take(200).collect::<String>());
            }
            println!("PROMISE_TEST {}", page.evaluate("'native='+globalThis.__pt+' scriptChunk='+globalThis.__ct+' onload='+globalThis.__lt+' inlineEval='+globalThis.__et+' pushRan='+globalThis.__ee+' charDataMO='+globalThis.__mo+' msgChannel='+globalThis.__mc").unwrap_or_default());
            println!("STORE {}", page.evaluate("(function(){try{var r=document.getElementById('root');if(!r)return 'no-root';var ks=Object.keys(r);var k='';for(var i=0;i<ks.length;i++){if(ks[i].indexOf('__reactContainer$')===0)k=ks[i];}if(!k)return 'nokey='+ks.join('|');var root=r[k].current||r[k];var q=[root],store=null,seen=0;while(q.length&&seen<4000){var f=q.shift();seen++;try{var mp=f.memoizedProps;if(mp&&mp.store&&typeof mp.store.getState==='function'){store=mp.store;break;}}catch(_){}if(f.child)q.push(f.child);if(f.sibling)q.push(f.sibling);}if(!store)return 'no-store seen='+seen;var s=store.getState();var pp=s.paymentPage||{};var sp=s.stripePassUi||{};function pv(x){try{return !!x&&(typeof x==='object'?Object.keys(x).length:x);}catch(_){return '?';}}return 'keys='+Object.keys(s).join(',').slice(0,200)+' ||| pp={loading:'+pp.loading+',value:'+pv(pp.value)+',error:'+pv(pp.error)+'} stripePassUi={loading:'+sp.loading+',keys:'+Object.keys(sp).join('|').slice(0,120)+'}';}catch(e){return 'ERR:'+(e&&e.message||e);}})()").unwrap_or_else(|e| format!("EVALERR:{e}")));
            page.consume_and_print_logs();
            println!("SCRIPTSRCS {}", page.evaluate("JSON.stringify(Array.from(document.querySelectorAll('script')).map(function(s){return s.src;}).filter(Boolean))").unwrap_or_default());
            println!("FULLMSG {}", page.evaluate("(function(){var r=document.getElementById('root');if(!r)return '';var els=r.querySelectorAll('*');var t='';for(var i=0;i<els.length;i++){var c=els[i].childNodes;for(var j=0;j<c.length;j++){if(c[j].nodeType===3&&c[j].textContent.trim())t+=c[j].textContent.trim()+' | ';}}return t.slice(0,500);})()").unwrap_or_default());
            println!("FIELDS {}", page.evaluate("JSON.stringify(Array.from(document.querySelectorAll('input,select,button')).map(function(e){return e.tagName+':'+(e.getAttribute('name')||e.getAttribute('placeholder')||e.getAttribute('aria-label')||e.type||(e.textContent||'').trim().slice(0,20));}))").unwrap_or_default());
            println!("ACTLOG {}", page.evaluate("JSON.stringify((globalThis.__actLog||[]).slice(-22))").unwrap_or_default());
            println!("LASTSTATE {}", page.evaluate("JSON.stringify(globalThis.__lastStateSumm||{})").unwrap_or_default());
            println!("ERR_TOP n={} {}", page.evaluate("String((globalThis.__oxErrLog||[]).length)").unwrap_or_default(), page.evaluate("JSON.stringify((globalThis.__oxErrLog||[]).slice(0,3))").unwrap_or_default());
            println!("REJ_TOP n={} {}", page.evaluate("String((globalThis.__oxRejLog||[]).length)").unwrap_or_default(), page.evaluate("JSON.stringify((globalThis.__oxRejLog||[]).slice(0,2))").unwrap_or_default());
            for i in 0..page.frame_tree_count() {
                let n = page.frame_tree_evaluate(i, "String((globalThis.__oxRejLog||[]).length)").unwrap_or_default();
                if n != "0" {
                    let rj = page.frame_tree_evaluate(i, "JSON.stringify((globalThis.__oxRejLog||[]).slice(0,2))").unwrap_or_default();
                    println!("REJ_FRAME[{i}] n={n} {rj}");
                }
            }
            println!("ROOTHTML {}", page.evaluate("(function(){var r=document.getElementById('root'); if(!r) return 'no-root'; var h=r.innerHTML||''; return 'len='+h.length+' head='+h.slice(0,180).replace(/\\s+/g,' ');})()").unwrap_or_default());
            println!("BODYHTML {}", page.evaluate("'bodyChildren='+document.body.childElementCount+' bodyLen='+(document.body.innerHTML||'').length").unwrap_or_default());
            println!("ROOT_BEFORE_INJECT {}", page.evaluate("String((document.getElementById('root')||{}).childElementCount||0)").unwrap_or_default());
            // Inject a synthetic measurement-ids response (what m-outer relays)
            // to resolve the metrics _idsPromise; if #root fills, muid gates it.
            for org in ["https://js.stripe.com", "https://m.stripe.network"] {
                let js = format!("window.dispatchEvent(new MessageEvent('message',{{data:JSON.stringify({{originatingScript:'m2',payload:{{guid:'g_test_guid_0001',muid:'m_test_muid_00000000-0000-4000-8000-000000000001',sid:'s_test_sid_0001'}}}}),origin:'{org}'}}))");
                let _ = page.evaluate(&js);
            }
            for _ in 0..10 {
                let _ = page
                    .event_loop()
                    .run_until_idle(std::time::Duration::from_millis(250))
                    .await;
                page.drive_frame_tree(&client, &profile).await;
            }
            println!("ROOT_AFTER_INJECT {}", page.evaluate("String((document.getElementById('root')||{}).childElementCount||0)+' inputs='+document.querySelectorAll('input').length+' buttons='+document.querySelectorAll('button').length").unwrap_or_default());

            // Manually invoke StripeM.p in inner-preview frames then pump, to see
            // if the fingerprint post routes (onload-never-fired vs compute-broken).
            for i in 0..page.frame_tree_count() {
                if page.frame_tree_evaluate(i, "typeof globalThis.StripeM").as_deref() == Some("object") {
                    let r = page.frame_tree_evaluate(i, "(function(){globalThis.__OX_FT_MSGDBG=true;var b=(globalThis.__OXPOSTS||[]).length;try{StripeM.p({t:true,v:'4'});}catch(e){return 'pErr='+e;}return 'called b='+b;})()");
                    let op = page.frame_tree_evaluate(i, "'opener='+typeof window.opener+'/'+(!!window.opener)+' parent='+typeof window.parent+'/'+(!!(window.parent&&window.parent.__isFrameHandle))+' win='+typeof window+' self==='+(window===window.self)");
                    println!("OPENER[{}] {:?}", i, op);
                    println!("MANUAL_P[{}] {:?}", i, r);
                }
            }
            for _ in 0..6 {
                let _ = page
                    .event_loop()
                    .run_until_idle(std::time::Duration::from_millis(250))
                    .await;
                page.drive_frame_tree(&client, &profile).await;
            }
            for i in 0..page.frame_tree_count() {
                if page.frame_tree_evaluate(i, "typeof globalThis.StripeM").as_deref() == Some("object") {
                    let r = page.frame_tree_evaluate(i, "'posts='+JSON.stringify(globalThis.__OXPOSTS||[])+' pErr='+(globalThis.__pErr||'-')");
                    println!("AFTER_P[{}] {:?}", i, r);
                }
            }
            let p = |pg: &mut browser_oxide::Page, js: &str| {
                pg.evaluate(js).unwrap_or_else(|e| format!("E:{e}"))
            };
            println!("FRAME_TREE_COUNT {}", page.frame_tree_count());
            for i in 0..page.frame_tree_count() {
                let info = page.frame_tree_evaluate(i, "(function(){var h=(globalThis.location?location.hash:'');return (globalThis.location?location.href:'?').split('/').slice(-1)[0].slice(0,20)+' fid='+globalThis.__frameId+' parent='+globalThis.__parentFrameId+' StripeM='+(typeof globalThis.StripeM)+' hashLen='+h.length+' pingFalse='+/ping=false/.test(h)+' preview='+/preview=true/.test(h);})()").unwrap_or_default();
                println!("FRAME[{}] {}", i, info);
            }
            println!("TOP_LOC origin={} href={} ref={}",
                p(&mut page, "String(location.origin)"),
                p(&mut page, "String(location.href).slice(0,55)"),
                p(&mut page, "String(document.referrer).slice(0,55)"));
            for i in 0..page.frame_tree_count() {
                let o = page.frame_tree_evaluate(i, "'origin='+String(location.origin)+' href='+String(location.href).split('/').slice(0,4).join('/').slice(0,45)+' ref='+String(document.referrer).slice(0,40)").unwrap_or_default();
                println!("FRAMELOC[{i}] {o}");
            }
            println!(
                "ROOT {}",
                p(&mut page, "String((document.getElementById('root')||{}).childElementCount||0)")
            );
            println!(
                "INPUTS {}",
                p(&mut page, "String(document.querySelectorAll('input').length)+' buttons='+document.querySelectorAll('button').length")
            );
            println!("TEL {}", p(&mut page, "JSON.stringify(globalThis.__tel||[])"));
            println!("TOPMSGS {}", p(&mut page, "JSON.stringify(globalThis.__topmsgs||[])"));
            println!("POSTS {}", p(&mut page, "JSON.stringify(globalThis.__OXPOSTS||[])"));
            println!("TOPFID {}", p(&mut page, "String(globalThis.__frameId)+' map='+JSON.stringify(globalThis.__frameIdForNode||{})"));
            println!("IFRAMES {}", p(&mut page, "JSON.stringify(Array.from(document.querySelectorAll('iframe')).map(function(f){return (f.name||'?').slice(0,20)+'|handle='+!!(f.contentWindow&&f.contentWindow.__isFrameHandle);}))"));
            println!("STRIPE {}", p(&mut page, "'calls='+(globalThis.__stripeCalls||0)+' arg='+(globalThis.__stripeArg||'-')+' typeof='+typeof globalThis.Stripe"));
            println!("ERRS {}", p(&mut page, "JSON.stringify(globalThis.__errs||[])"));
            println!("REJS {}", p(&mut page, "JSON.stringify(globalThis.__rejs||[])"));
            println!("NET {}", p(&mut page, "JSON.stringify(globalThis.__net||[])"));
            println!("IFCREATED {}", p(&mut page, "JSON.stringify(globalThis.__ifcreated||[])"));
            println!("PPURL {}", p(&mut page, "String(globalThis.__ppurl||'')"));
            println!("PPOPT {}", p(&mut page, "String(globalThis.__ppopt||'')"));
            println!("PPSTATUS {}", p(&mut page, "String(globalThis.__ppstatus)"));
            println!("PPBODY {}", p(&mut page, "String(globalThis.__ppbody||'')"));
            println!("PPLEN {}", p(&mut page, "String(globalThis.__pplen)"));
            println!("SCRIPTS {}", p(&mut page, "JSON.stringify((globalThis.__scripts||[]).filter(function(s){return s.src;}).map(function(s){return s.src.split('/').pop().slice(0,32)+' L'+s.loadFired+' E'+s.errFired;}))"));
            println!("ROOTGET {}", p(&mut page, "'rootGets='+(globalThis.__rootGets||0)+' rootHTML='+((document.getElementById('root')||{}).innerHTML||'').length+' typeofReactDOM='+typeof globalThis.ReactDOM"));
            println!("CERR {}", p(&mut page, "JSON.stringify(globalThis.__cerr||[])"));
            println!("REACT_MARKERS {}", p(&mut page, "(function(){var r=document.getElementById('root'); if(!r)return 'no-root'; var ks=Object.keys(r); return 'reactKeys='+JSON.stringify(ks.filter(function(k){return /react|React|__ox/.test(k);}))+' totalKeys='+ks.length+' _reactRootContainer='+(typeof r._reactRootContainer);})()"));
            println!("WEBPACK {}", p(&mut page, "(function(){var h=globalThis.webpackChunkStripeJShosted;function inf(w){if(!w)return 'none';return 'len='+w.length+' overridden='+(w.push!==Array.prototype.push);}return 'hosted['+inf(h)+'] sentry='+(!!globalThis.__SENTRY__)+' initCalled='+(!!globalThis.__stripeCalls);})()"));
            println!("LOADTEST {}", p(&mut page, "(function(){var f=document.createElement('iframe');var a=0;f.addEventListener('load',function(){a++;});var b=0;f.onload=function(){b++;};document.body.appendChild(f);try{f.dispatchEvent(new Event('load'));}catch(e){return 'ERR:'+e;}return 'addEventListener-fired='+a+' onload-fired='+b;})()"));

            let fill_js = r#"(function(){
                function setVal(sel,val){
                    var el=document.querySelector(sel);
                    if(!el) return sel+':MISSING';
                    var proto=el.tagName==='SELECT'?window.HTMLSelectElement.prototype:window.HTMLInputElement.prototype;
                    var d=Object.getOwnPropertyDescriptor(proto,'value');
                    try{ if(d&&d.set) d.set.call(el,val); else el.value=val; }catch(e){return sel+':SETERR:'+e;}
                    try{ el.dispatchEvent(new Event('input',{bubbles:true})); }catch(e){}
                    try{ el.dispatchEvent(new Event('change',{bubbles:true})); }catch(e){}
                    return (el.getAttribute('name')||el.tagName)+'='+el.value;
                }
                var o=[];
                o.push(setVal('input[name=email]','test@example.com'));
                o.push(setVal('input[name=cardNumber]','4242424242424242'));
                o.push(setVal('input[name=cardExpiry]','12 / 34'));
                o.push(setVal('input[name=cardCvc]','123'));
                o.push(setVal('input[name=billingName]','Test Person'));
                o.push(setVal('select[name=billingCountry]','US'));
                return JSON.stringify(o);
            })()"#;
            println!("EVTBUBBLE {}", p(&mut page, "(function(){var got={cap:0,bub:0};document.addEventListener('input',function(){got.cap++;},true);document.addEventListener('input',function(){got.bub++;},false);var el=document.querySelector('input[name=email]');if(!el)return 'no-email';el.dispatchEvent(new Event('input',{bubbles:true}));return 'cap='+got.cap+' bub='+got.bub;})()"));
            println!("TRACKER {}", p(&mut page, "(function(){var el=document.querySelector('input[name=email]');if(!el)return 'no-email';var keys=Object.keys(el).filter(function(k){return /react|value|Track|__ox/i.test(k);});return 'ownkeys='+JSON.stringify(keys)+' _valueTracker='+(typeof el._valueTracker);})()"));
            println!("ONINPUT_IN {}", p(&mut page, "'doc='+('oninput' in document)+' el='+('oninput' in document.createElement('input'))+' body='+('onclick' in document.body)"));
            println!("FILL {}", p(&mut page, fill_js));
            page.drive_frame_tree(&client, &profile).await;
            page.consume_and_print_logs();
            println!("READBACK {}", p(&mut page, "(function(){function v(s){var e=document.querySelector(s);return e?e.value:'?';}return 'email='+v('input[name=email]')+' card='+v('input[name=cardNumber]')+' exp='+v('input[name=cardExpiry]')+' cvc='+v('input[name=cardCvc]')+' name='+v('input[name=billingName]')+' country='+v('select[name=billingCountry]');})()"));
            println!("SUBMITBTN {}", p(&mut page, "(function(){var b=document.querySelector('button[type=submit]')||document.querySelector('button');if(!b)return 'no-btn';return 'text='+(b.textContent||'').trim().slice(0,30)+' disabled='+b.disabled;})()"));
            println!("CLICK {}", p(&mut page, "(function(){var b=document.querySelector('button[type=submit]')||document.querySelector('button');if(!b)return 'no-btn';try{b.click();}catch(e){return 'clickerr:'+e;}return 'clicked:'+(b.textContent||'').trim().slice(0,20);})()"));
            page.drive_frame_tree(&client, &profile).await;
            page.consume_and_print_logs();
            println!("AFTER_SUBMIT root={}", p(&mut page, "'len='+((document.getElementById('root')||{}).innerHTML||'').length+' btn='+((document.querySelector('button[type=submit]')||{}).textContent||'?').trim().slice(0,30)"));
            println!("SUBMIT_MSGS {}", p(&mut page, "(function(){var r=document.getElementById('root');if(!r)return '';var t=[];var els=r.querySelectorAll('[role=alert],.FieldError,[data-testid*=rror],[class*=rror]');for(var i=0;i<els.length;i++){var x=(els[i].textContent||'').trim();if(x&&t.indexOf(x)<0)t.push(x.slice(0,70));}return JSON.stringify(t.slice(0,8));})()"));
        }),
    );
}
