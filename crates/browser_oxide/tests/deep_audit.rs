//! Focused browser-semantics audit for high-risk DOM/frame paths.

use std::time::Duration;

use browser_oxide::Page;

async fn empty_page() -> Page {
    Page::from_html_with_url(
        "<!doctype html><html><head></head><body></body></html>",
        "https://example.com/audit",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn inline_classic_script_has_current_script() {
    let mut page = Page::from_html_with_url(
        r#"<!doctype html><html><body>
        <script id="audit-script" data-probe="yes">
          globalThis.__currentScriptAudit = document.currentScript
            ? document.currentScript.id + ':' + document.currentScript.getAttribute('data-probe')
            : 'null';
        </script>
        </body></html>"#,
        "https://example.com/current-script",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate("globalThis.__currentScriptAudit").unwrap(),
        "audit-script:yes"
    );
}

#[tokio::test]
async fn document_lifecycle_observes_browser_order_and_trust() {
    let mut page = Page::from_html_with_url(
        r#"<!doctype html><html><body>
        <script>
          globalThis.__life = {states: [], windowDcl: 0};
          window.addEventListener('DOMContentLoaded', function() { __life.windowDcl++; });
          document.addEventListener('readystatechange', function(e) {
            __life.states.push(document.readyState + ':' + e.isTrusted);
          });
          document.addEventListener('DOMContentLoaded', function(e) {
            __life.dcl = document.readyState + ':' + e.isTrusted;
          });
          window.addEventListener('load', function(e) {
            __life.load = document.readyState + ':' + e.isTrusted;
          });
        </script>
        </body></html>"#,
        "https://example.com/lifecycle",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__life)").unwrap(),
        r#"{"states":["interactive:true","complete:true"],"windowDcl":0,"dcl":"interactive:true","load":"complete:true"}"#
    );
}

#[tokio::test]
async fn document_fragment_constructor_creates_real_fragment() {
    let mut page = empty_page().await;
    let result = page
        .evaluate(
            r#"(function(){
              try {
                const f = new DocumentFragment();
                const x = document.createElement('span');
                f.appendChild(x);
                return JSON.stringify({nodeType:f.nodeType,length:f.childNodes.length,parent:x.parentNode===f});
              } catch (e) {
                return 'ERROR:' + e.name + ':' + e.message;
              }
            })()"#,
        )
        .unwrap();
    assert_eq!(result, r#"{"nodeType":11,"length":1,"parent":true}"#);
}

#[tokio::test]
async fn fragment_children_is_html_collection() {
    let mut page = empty_page().await;
    assert_eq!(
        page.evaluate(
            r#"(function(){
              const f = document.createDocumentFragment();
              f.appendChild(document.createElement('div'));
              return JSON.stringify({
                ctor:f.children.constructor.name,
                htmlCollection:typeof HTMLCollection==='function' && f.children instanceof HTMLCollection,
                nodeList:f.children instanceof NodeList
              });
            })()"#,
        )
        .unwrap(),
        r#"{"ctor":"HTMLCollection","htmlCollection":true,"nodeList":false}"#
    );
}

#[tokio::test]
async fn shadow_root_constructor_and_attach_shadow_validate() {
    let mut page = empty_page().await;
    assert_eq!(
        page.evaluate(
            r#"(function(){
              let ctorThrows=false, invalidThrows=false, secondThrows=false;
              try { new ShadowRoot(); } catch (e) { ctorThrows = e instanceof TypeError; }
              const host=document.createElement('div');
              document.body.appendChild(host);
              try { host.attachShadow({mode:'invalid'}); } catch (e) { invalidThrows = e instanceof TypeError; }
              const root=host.attachShadow({mode:'open'});
              try { host.attachShadow({mode:'open'}); } catch (e) { secondThrows = true; }
              return JSON.stringify({ctorThrows,invalidThrows,secondThrows,root:root instanceof ShadowRoot});
            })()"#,
        )
        .unwrap(),
        r#"{"ctorThrows":true,"invalidThrows":true,"secondThrows":true,"root":true}"#
    );
}

#[tokio::test]
async fn get_root_node_honors_composed_option() {
    let mut page = empty_page().await;
    assert_eq!(
        page.evaluate(
            r#"(function(){
              const host=document.createElement('div');
              document.body.appendChild(host);
              const root=host.attachShadow({mode:'open'});
              const child=document.createElement('span');
              root.appendChild(child);
              return JSON.stringify({plain:child.getRootNode()===root,composed:child.getRootNode({composed:true})===document});
            })()"#,
        )
        .unwrap(),
        r#"{"plain":true,"composed":true}"#
    );
}

#[tokio::test]
async fn window_post_message_enforces_target_origin_and_is_trusted() {
    let mut page = empty_page().await;
    page.evaluate(
        r#"globalThis.__messages=[];
           window.addEventListener('message', e => __messages.push({data:e.data,origin:e.origin,trusted:e.isTrusted}));
           window.postMessage('blocked','https://wrong.example');
           window.postMessage('ok','*');"#,
    )
    .unwrap();
    page.event_loop()
        .run_until_idle(Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__messages)")
            .unwrap(),
        r#"[{"data":"ok","origin":"https://example.com","trusted":true}]"#
    );
}

#[tokio::test]
async fn srcdoc_post_message_enforces_target_origin_and_is_trusted() {
    let mut page = Page::from_html_with_url(
        r#"<!doctype html><html><body><iframe srcdoc="<!doctype html><html><body></body></html>"></iframe></body></html>"#,
        "https://example.com/parent",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();

    page.evaluate(
        r#"const w=document.querySelector('iframe').contentWindow;
           w.__messages=[];
           w.addEventListener('message', e => w.__messages.push({data:e.data,origin:e.origin,trusted:e.isTrusted}));
           w.postMessage('blocked','https://wrong.example');
           w.postMessage('ok','*');"#,
    )
    .unwrap();
    page.event_loop()
        .run_until_idle(Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(
        page.evaluate("JSON.stringify(document.querySelector('iframe').contentWindow.__messages)",)
            .unwrap(),
        r#"[{"data":"ok","origin":"https://example.com","trusted":true}]"#
    );
}
