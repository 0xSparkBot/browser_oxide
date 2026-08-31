//! Observable surface and scheduling semantics audit.

use std::time::Duration;

use browser_oxide::Page;

async fn page(html: &str) -> Page {
    Page::from_html_with_url(
        html,
        "https://example.com/surface",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn collection_surface_does_not_leak_internal_slots() {
    let mut page = page("<!doctype html><html><body><span></span></body></html>").await;
    assert_eq!(
        page.evaluate(
            r#"(function(){
              const html=document.body.children;
              const nodes=document.body.childNodes;
              return JSON.stringify({
                htmlTag:Object.prototype.toString.call(html),
                nodeTag:Object.prototype.toString.call(nodes),
                htmlKeys:Object.keys(html),
                nodeKeys:Object.keys(nodes),
                htmlInternal:['_ids','_source','_mode'].some(k=>Object.prototype.hasOwnProperty.call(html,k)),
                nodeInternal:['_ids','_source','_mode'].some(k=>Object.prototype.hasOwnProperty.call(nodes,k))
              });
            })()"#,
        )
        .unwrap(),
        r#"{"htmlTag":"[object HTMLCollection]","nodeTag":"[object NodeList]","htmlKeys":["0"],"nodeKeys":["0"],"htmlInternal":false,"nodeInternal":false}"#
    );
}

#[tokio::test]
async fn document_fragment_ignores_extra_constructor_arguments() {
    let mut page = page("<!doctype html><html><body></body></html>").await;
    assert_eq!(
        page.evaluate(
            "JSON.stringify({nodeType:new DocumentFragment(123).nodeType,length:new DocumentFragment(123).childNodes.length})",
        )
        .unwrap(),
        r#"{"nodeType":11,"length":0}"#
    );
}

#[tokio::test]
async fn invalid_post_message_target_origin_throws_syntax_error() {
    let mut page = page("<!doctype html><html><body></body></html>").await;
    assert_eq!(
        page.evaluate(
            "(function(){try{postMessage('x','http://[invalid');return 'none';}catch(e){return e.name;}})()",
        )
        .unwrap(),
        "SyntaxError"
    );
}

#[tokio::test]
async fn srcdoc_post_message_clones_at_call_time_and_queues_a_task() {
    let mut page = page(
        r#"<!doctype html><html><body><iframe srcdoc="<!doctype html><html><body></body></html>"></iframe></body></html>"#,
    )
    .await;
    page.evaluate(
        r#"const w=document.querySelector('iframe').contentWindow;
           w.__order=[];w.__messageData=null;
           w.addEventListener('message',e=>{w.__order.push('message');w.__messageData=e.data;});
           const payload={value:1};
           w.postMessage(payload,'*');
           payload.value=2;
           Promise.resolve().then(()=>w.__order.push('promise'));"#,
    )
    .unwrap();
    page.event_loop()
        .run_until_idle(Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(
        page.evaluate(
            "JSON.stringify({order:document.querySelector('iframe').contentWindow.__order,data:document.querySelector('iframe').contentWindow.__messageData})",
        )
        .unwrap(),
        r#"{"order":["promise","message"],"data":{"value":1}}"#
    );
}
