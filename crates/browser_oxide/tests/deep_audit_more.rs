//! Additional high-risk browser-semantics audit cases.

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
async fn dom_collections_have_browser_liveness() {
    let mut page = empty_page().await;
    assert_eq!(
        page.evaluate(
            r#"(function(){
              const host=document.createElement('div');
              document.body.appendChild(host);
              const children=host.children;
              const childNodes=host.childNodes;
              const tags=document.getElementsByTagName('span');
              const staticQuery=document.querySelectorAll('span');
              host.appendChild(document.createTextNode('x'));
              host.appendChild(document.createElement('span'));
              const afterAdd={children:children.length,nodes:childNodes.length,tags:tags.length,staticQuery:staticQuery.length};
              host.firstElementChild.remove();
              return JSON.stringify({afterAdd,afterRemove:{children:children.length,nodes:childNodes.length,tags:tags.length}});
            })()"#,
        )
        .unwrap(),
        r#"{"afterAdd":{"children":1,"nodes":2,"tags":1,"staticQuery":0},"afterRemove":{"children":0,"nodes":1,"tags":0}}"#
    );
}

#[tokio::test]
async fn post_message_is_a_task_and_clone_errors_propagate() {
    let mut page = empty_page().await;
    page.evaluate(
        r#"globalThis.__order=[];
           addEventListener('message',()=>__order.push('message'));
           postMessage('x','*');
           Promise.resolve().then(()=>__order.push('promise'));
           globalThis.__cloneError='none';
           try { postMessage(function(){}, '*'); } catch(e) { __cloneError=e.name; }"#,
    )
    .unwrap();
    page.event_loop()
        .run_until_idle(Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(
        page.evaluate("JSON.stringify(__order)").unwrap(),
        r#"["promise","message"]"#
    );
    assert_eq!(page.evaluate("__cloneError").unwrap(), "DataCloneError");
}

#[tokio::test]
async fn attach_shadow_rejects_invalid_host_element() {
    let mut page = empty_page().await;
    assert_eq!(
        page.evaluate(
            r#"(function(){try{document.createElement('img').attachShadow({mode:'open'});return 'no-error';}catch(e){return e.name;}})()"#,
        )
        .unwrap(),
        "NotSupportedError"
    );
}

#[tokio::test]
async fn srcdoc_public_child_api_matches_content_window_state() {
    let mut page = Page::from_html_with_url(
        r#"<!doctype html><html><body><iframe srcdoc="<script>globalThis.__realmMarker='from-srcdoc'</script>"></iframe></body></html>"#,
        "https://example.com/parent",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();

    page.evaluate("document.querySelector('iframe').contentWindow.__sharedMarker='from-window'")
        .unwrap();
    let browser_marker = page
        .evaluate("document.querySelector('iframe').contentWindow.__realmMarker")
        .unwrap();
    let api_marker = page
        .child_iframe(0)
        .unwrap()
        .evaluate("globalThis.__realmMarker")
        .unwrap();
    let api_shared = page
        .child_iframe(0)
        .unwrap()
        .evaluate("String(globalThis.__sharedMarker)")
        .unwrap();
    assert_eq!(browser_marker, "from-srcdoc");
    assert_eq!(api_marker, browser_marker);
    assert_eq!(api_shared, "from-window");
}

#[tokio::test]
async fn srcdoc_navigation_preserves_window_proxy_and_replaces_inner_global() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe srcdoc="<script>globalThis.firstDocument=1</script>"></iframe>"#,
        "https://example.com/",
        None,
    )
    .await
    .unwrap();

    let state = page
        .evaluate(
            r#"(() => {
                const frame = document.querySelector('iframe');
                const proxy = frame.contentWindow;
                const oldDocument = proxy.document;
                proxy.transientValue = 42;
                frame.srcdoc = '<script>globalThis.secondDocument=2<\/script>';
                return JSON.stringify({
                    sameProxy: proxy === frame.contentWindow,
                    newDocument: oldDocument !== proxy.document,
                    oldGlobalCleared: typeof proxy.transientValue,
                    firstCleared: typeof proxy.firstDocument,
                    secondLoaded: proxy.secondDocument,
                });
            })()"#,
        )
        .unwrap();

    assert_eq!(
        state,
        r#"{"sameProxy":true,"newDocument":true,"oldGlobalCleared":"undefined","firstCleared":"undefined","secondLoaded":2}"#
    );

    assert_eq!(
        page.child_iframe(0)
            .unwrap()
            .evaluate("String(globalThis.secondDocument)")
            .unwrap(),
        "2"
    );
}

#[tokio::test]
async fn nested_async_iframe_stays_attached_to_its_parent_realm() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="outer" srcdoc="<main id='outer-body'></main>"></iframe>"#,
        "https://example.com/nested",
        None,
    )
    .await
    .unwrap();
    page.evaluate("globalThis.__browser_oxide_debug=true; 'ok'")
        .unwrap();
    // Activate the Rust frame-tree path before the child creates its nested
    // iframe. A missing parent-realm marker would incorrectly register it as a
    // top-level network frame from this point onward.
    page.init_top_frame();

    {
        let mut outer = page.child_iframe(0).unwrap();
        assert_eq!(outer.evaluate("typeof document").unwrap(), "object");
    }

    {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .execute_and_run(
                r#"Promise.resolve().then(() => {
                    globalThis.__nestedEvents = [];
                    addEventListener('message', e => __nestedEvents.push(e));
                    const inner = document.createElement('iframe');
                    inner.id = 'inner';
                    inner.srcdoc = '<script>globalThis.__innerValue=11;parent.postMessage({nested:true},"*")<\/script>';
                    document.body.appendChild(inner);
                    void inner.contentWindow;
                })"#,
                Duration::from_millis(250),
            )
            .await
            .unwrap();
    }

    let state = page
        .evaluate(
            r#"(() => {
                const outer = document.getElementById('outer').contentWindow;
                const innerElement = outer.document.getElementById('inner');
                const inner = innerElement && innerElement.contentWindow;
                const event = outer.__nestedEvents && outer.__nestedEvents[0];
                return JSON.stringify({
                    innerValue: inner && inner.__innerValue,
                    parentDocument: !!inner && inner.parent.document === outer.document,
                    parentWindow: !!inner && inner.parent.window === outer,
                    topDocument: !!inner && inner.top.document === document,
                    distinctParentAndTop: !!inner && inner.parent !== inner.top,
                    messageDelivered: !!event && event.data.nested === true,
                    sourceIdentity: !!event && event.source === inner,
                    topLength: window.length,
                    outerLength: outer.length,
                    top0Outer: window[0] === outer,
                    top1Missing: window[1] === undefined,
                    outer0Inner: outer[0] === inner,
                });
            })()"#,
        )
        .unwrap();

    assert_eq!(
        state,
        r#"{"innerValue":11,"parentDocument":true,"parentWindow":true,"topDocument":true,"distinctParentAndTop":true,"messageDelivered":true,"sourceIdentity":true,"topLength":1,"outerLength":1,"top0Outer":true,"top1Missing":true,"outer0Inner":true}"#
    );
    assert_eq!(
        page.frame_tree_count(),
        0,
        "a nested same-isolate frame must not be misregistered under the top frame tree"
    );
}

#[tokio::test]
async fn nested_frame_registry_reindexes_after_removal() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="outer" srcdoc="<main id='root'></main>"></iframe>"#,
        "https://example.com/nested-registry",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    const a=document.createElement('iframe');
                    a.id='a';
                    a.srcdoc='<script>globalThis.name="a"<\/script>';
                    const b=document.createElement('iframe');
                    b.id='b';
                    b.srcdoc='<script>globalThis.name="b"<\/script>';
                    document.body.append(a,b);
                    const aw=a.contentWindow;
                    const bw=b.contentWindow;
                    const before={
                        length:window.length,
                        a0:window[0]===aw,
                        b1:window[1]===bw,
                    };
                    a.remove();
                    const after={
                        length:window.length,
                        b0:window[0]===bw,
                        slot1:typeof window[1],
                        removedClosed:aw.closed,
                    };
                    return JSON.stringify({before,after});
                })()"#,
            )
            .unwrap()
    };

    assert_eq!(
        state,
        r#"{"before":{"length":2,"a0":true,"b1":true},"after":{"length":1,"b0":true,"slot1":"undefined","removedClosed":true}}"#
    );
    assert_eq!(
        page.evaluate(
            "JSON.stringify({length:window.length,outer0:window[0]===document.getElementById('outer').contentWindow,slot1:typeof window[1]})"
        )
        .unwrap(),
        r#"{"length":1,"outer0":true,"slot1":"undefined"}"#
    );
    assert_eq!(page.frame_tree_count(), 0);
}

#[tokio::test]
async fn removed_iframe_window_proxy_stays_detached_across_reinsert() {
    let mut page = empty_page().await;
    let state = page
        .evaluate(
            r#"(() => {
                const frame=document.createElement('iframe');
                frame.srcdoc='<main id="old-doc"></main>';
                const initial={
                    windowNull:frame.contentWindow===null,
                    documentNull:frame.contentDocument===null,
                };
                document.body.appendChild(frame);
                const oldWindow=frame.contentWindow;
                const oldDocument=oldWindow.document;
                oldWindow.marker='old-generation';

                frame.remove();
                const detached={
                    closed:oldWindow.closed,
                    parentNull:oldWindow.parent===null,
                    topNull:oldWindow.top===null,
                    frameElementNull:oldWindow.frameElement===null,
                    keepsDocument:oldWindow.document===oldDocument,
                    defaultViewNull:oldDocument.defaultView===null,
                    marker:oldWindow.marker,
                    directWindowNull:frame.contentWindow===null,
                    directDocumentNull:frame.contentDocument===null,
                };

                document.body.appendChild(frame);
                const freshWindow=frame.contentWindow;
                const freshDocument=freshWindow.document;
                return JSON.stringify({
                    initial,
                    detached,
                    fresh:{
                        distinct:oldWindow!==freshWindow,
                        open:freshWindow.closed===false,
                        newDocument:freshDocument!==oldDocument,
                        marker:typeof freshWindow.marker,
                        contentWindowCurrent:frame.contentWindow===freshWindow,
                    },
                });
            })()"#,
        )
        .unwrap();

    assert_eq!(
        state,
        r#"{"initial":{"windowNull":true,"documentNull":true},"detached":{"closed":true,"parentNull":true,"topNull":true,"frameElementNull":true,"keepsDocument":true,"defaultViewNull":true,"marker":"old-generation","directWindowNull":true,"directDocumentNull":true},"fresh":{"distinct":true,"open":true,"newDocument":true,"marker":"undefined","contentWindowCurrent":true}}"#
    );
}

#[tokio::test]
async fn top_frame_registry_tracks_dom_order_and_removal() {
    let mut page = empty_page().await;
    let state = page
        .evaluate(
            r#"(() => {
                const a=document.createElement('iframe');
                a.srcdoc='<p>a</p>';
                const b=document.createElement('iframe');
                b.srcdoc='<p>b</p>';
                document.body.appendChild(a);
                document.body.appendChild(b);
                const aw=a.contentWindow;
                const bw=b.contentWindow;
                const initial={
                    length:window.length,
                    a0:window[0]===aw,
                    b1:window[1]===bw,
                };
                a.remove();
                const removed={
                    length:window.length,
                    b0:window[0]===bw,
                    slot1:typeof window[1],
                    oldClosed:aw.closed,
                };
                document.body.insertBefore(a,b);
                const aw2=a.contentWindow;
                const reinserted={
                    length:window.length,
                    a0:window[0]===aw2,
                    b1:window[1]===bw,
                    fresh:aw2!==aw,
                    oldClosed:aw.closed,
                };
                return JSON.stringify({initial,removed,reinserted});
            })()"#,
        )
        .unwrap();

    assert_eq!(
        state,
        r#"{"initial":{"length":2,"a0":true,"b1":true},"removed":{"length":1,"b0":true,"slot1":"undefined","oldClosed":true},"reinserted":{"length":2,"a0":true,"b1":true,"fresh":true,"oldClosed":true}}"#
    );
}

#[tokio::test]
async fn frame_registry_tracks_inner_html_and_insert_adjacent_html() {
    let mut page = empty_page().await;
    let top = page
        .evaluate(
            r#"(() => {
                document.body.innerHTML='<iframe id="a" srcdoc="<p>a</p>"></iframe>';
                const a=document.getElementById('a');
                const aw=a.contentWindow;
                const first={length:window.length,a0:window[0]===aw};
                a.insertAdjacentHTML('afterend','<iframe id="b" srcdoc="<p>b</p>"></iframe>');
                const b=document.getElementById('b');
                const bw=b.contentWindow;
                const second={length:window.length,a0:window[0]===aw,b1:window[1]===bw};
                document.body.innerHTML='<iframe id="c" srcdoc="<p>c</p>"></iframe>';
                const c=document.getElementById('c');
                const cw=c.contentWindow;
                const replaced={
                    length:window.length,
                    c0:window[0]===cw,
                    slot1:typeof window[1],
                    aClosed:aw.closed,
                    bClosed:bw.closed,
                };
                return JSON.stringify({first,second,replaced});
            })()"#,
        )
        .unwrap();
    assert_eq!(
        top,
        r#"{"first":{"length":1,"a0":true},"second":{"length":2,"a0":true,"b1":true},"replaced":{"length":1,"c0":true,"slot1":"undefined","aClosed":true,"bClosed":true}}"#
    );

    page.evaluate(
        r#"(() => {
            document.body.innerHTML='<iframe id="outer" srcdoc="<main id=\"root\"></main>"></iframe>';
            return true;
        })()"#,
    )
    .unwrap();
    let nested = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    document.body.innerHTML='<iframe id="x" srcdoc="<p>x</p>"></iframe>';
                    const x=document.getElementById('x');
                    const xw=x.contentWindow;
                    x.insertAdjacentHTML('afterend','<iframe id="y" srcdoc="<p>y</p>"></iframe>');
                    const y=document.getElementById('y');
                    const yw=y.contentWindow;
                    const before={length:window.length,x0:window[0]===xw,y1:window[1]===yw};
                    document.body.innerHTML='<iframe id="z" srcdoc="<p>z</p>"></iframe>';
                    const z=document.getElementById('z');
                    const zw=z.contentWindow;
                    return JSON.stringify({
                        before,
                        after:{
                            length:window.length,
                            z0:window[0]===zw,
                            slot1:typeof window[1],
                            xClosed:xw&&xw.closed,
                            yClosed:yw&&yw.closed,
                        }
                    });
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(
        nested,
        r#"{"before":{"length":2,"x0":true,"y1":true},"after":{"length":1,"z0":true,"slot1":"undefined","xClosed":true,"yClosed":true}}"#
    );
}

#[tokio::test]
async fn frame_registry_exposes_named_child_windows() {
    let mut page = empty_page().await;
    let top = page
        .evaluate(
            r#"(() => {
                const f=document.createElement('iframe');
                f.name='namedChild';
                f.srcdoc='<p>named</p>';
                document.body.appendChild(f);
                const cw=f.contentWindow;
                const before={
                    viaWindow:window.namedChild===cw,
                    viaFrames:frames.namedChild===cw,
                    viaBracket:frames['namedChild']===cw,
                };
                f.remove();
                const after={
                    windowType:typeof window.namedChild,
                    framesType:typeof frames.namedChild,
                    oldClosed:cw.closed,
                };
                return JSON.stringify({before,after});
            })()"#,
        )
        .unwrap();
    assert_eq!(
        top,
        r#"{"before":{"viaWindow":true,"viaFrames":true,"viaBracket":true},"after":{"windowType":"undefined","framesType":"undefined","oldClosed":true}}"#
    );

    page.evaluate(
        "document.body.innerHTML='<iframe id=\"outer\" srcdoc=\"<main></main>\"></iframe>'; true",
    )
    .unwrap();
    let nested = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    const f=document.createElement('iframe');
                    f.name='nestedNamed';
                    f.srcdoc='<p>nested</p>';
                    document.body.appendChild(f);
                    const cw=f.contentWindow;
                    const before={
                        viaWindow:window.nestedNamed===cw,
                        viaFrames:frames.nestedNamed===cw,
                    };
                    f.remove();
                    return JSON.stringify({
                        before,
                        after:{type:typeof window.nestedNamed,closed:cw.closed},
                    });
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(
        nested,
        r#"{"before":{"viaWindow":true,"viaFrames":true},"after":{"type":"undefined","closed":true}}"#
    );
}

#[tokio::test]
async fn named_frame_registry_respects_existing_globals_and_renames() {
    let mut page = empty_page().await;
    let top = page
        .evaluate(
            r#"(() => {
                window.keepMe=17;
                const f=document.createElement('iframe');
                f.name='keepMe';
                f.srcdoc='<p>x</p>';
                document.body.appendChild(f);
                const cw=f.contentWindow;
                const collision={value:window.keepMe,isFrame:window.keepMe===cw};
                f.name='freeFrameName';
                const renamed={
                    oldValue:window.keepMe,
                    newIsFrame:window.freeFrameName===cw,
                };
                f.remove();
                return JSON.stringify({
                    collision,
                    renamed,
                    removedType:typeof window.freeFrameName,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        top,
        r#"{"collision":{"value":17,"isFrame":false},"renamed":{"oldValue":17,"newIsFrame":true},"removedType":"undefined"}"#
    );

    page.evaluate(
        "document.body.innerHTML='<iframe id=\"outer\" srcdoc=\"<main></main>\"></iframe>'; true",
    )
    .unwrap();
    let nested = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    globalThis.keepNested=23;
                    const f=document.createElement('iframe');
                    f.name='keepNested';
                    f.srcdoc='<p>x</p>';
                    document.body.appendChild(f);
                    const cw=f.contentWindow;
                    const collision={value:keepNested,isFrame:keepNested===cw};
                    f.name='freeNestedName';
                    const renamed={oldValue:keepNested,newIsFrame:freeNestedName===cw};
                    f.remove();
                    return JSON.stringify({collision,renamed,removedType:typeof freeNestedName});
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(
        nested,
        r#"{"collision":{"value":23,"isFrame":false},"renamed":{"oldValue":23,"newIsFrame":true},"removedType":"undefined"}"#
    );
}

#[tokio::test]
async fn parser_created_iframe_is_in_indexed_registry_before_materialization() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="parsed" srcdoc="<p>parsed</p>"></iframe>"#,
        "https://example.com/parser-frame",
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        page.evaluate(
            r#"JSON.stringify({
                length:window.length,
                same:window[0]===document.getElementById('parsed').contentWindow,
                framesSame:frames[0]===window[0]
            })"#
        )
        .unwrap(),
        r#"{"length":1,"same":true,"framesSame":true}"#
    );
}

#[tokio::test]
async fn document_write_updates_the_owning_frame_registry_only() {
    let mut top_page = empty_page().await;
    let top = top_page
        .evaluate(
            r#"(() => {
                document.write('<script>globalThis.__topWriteScript=41<\/script><iframe id="written-top" srcdoc="<p>top</p>"></iframe>');
                const f=document.getElementById('written-top');
                const cw=f&&f.contentWindow;
                return JSON.stringify({exists:!!f,length:window.length,indexed:window[0]===cw,script:__topWriteScript});
            })()"#,
        )
        .unwrap();
    assert_eq!(
        top,
        r#"{"exists":true,"length":1,"indexed":true,"script":41}"#
    );

    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="outer" srcdoc="<main id='root'></main>"></iframe>"#,
        "https://example.com/write-frame",
        None,
    )
    .await
    .unwrap();
    let nested = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    document.write('<script>globalThis.__innerWriteScript=42<\/script><iframe id="written-inner" srcdoc="<p>inner</p>"></iframe>');
                    const f=document.getElementById('written-inner');
                    const cw=f&&f.contentWindow;
                    return JSON.stringify({exists:!!f,length:window.length,indexed:window[0]===cw,script:__innerWriteScript});
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(
        nested,
        r#"{"exists":true,"length":1,"indexed":true,"script":42}"#
    );
    assert_eq!(
        page.evaluate(
            "JSON.stringify({topHas:!!document.getElementById('written-inner'),length:window.length})"
        )
        .unwrap(),
        r#"{"topHas":false,"length":1}"#
    );
}

#[tokio::test]
async fn shadow_tree_iframe_has_window_but_is_not_indexed_child() {
    let mut page = empty_page().await;
    let state = page
        .evaluate(
            r#"(() => {
                const host=document.createElement('div');
                document.body.appendChild(host);
                const root=host.attachShadow({mode:'open'});
                const frame=document.createElement('iframe');
                frame.srcdoc='<p>shadow</p>';
                root.appendChild(frame);
                const cw=frame.contentWindow;
                const mounted={
                    connected:frame.isConnected,
                    hasWindow:!!cw,
                    open:cw.closed===false,
                    topLength:window.length,
                    slot0:typeof window[0],
                };
                frame.remove();
                return JSON.stringify({mounted,removedClosed:cw.closed});
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"mounted":{"connected":true,"hasWindow":true,"open":true,"topLength":0,"slot0":"undefined"},"removedClosed":true}"#
    );
}

#[tokio::test]
async fn connected_shadow_remote_iframe_registers_frame_without_index_slot() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body><div id="host"></div></body>"#,
        "https://example.com/shadow-remote",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = page
        .evaluate(
            r#"(() => {
                const host=document.getElementById('host');
                const root=host.attachShadow({mode:'closed'});
                const frame=document.createElement('iframe');
                frame.src='https://example.com/frame';
                root.appendChild(frame);
                return JSON.stringify({
                    connected:frame.isConnected,
                    hasWindow:!!frame.contentWindow,
                    topLength:window.length,
                    mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"connected":true,"hasWindow":true,"topLength":0,"mapped":1}"#
    );
}

#[tokio::test]
async fn shadow_inner_html_remote_iframe_registers_frame_without_index_slot() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body><div id="host"></div></body>"#,
        "https://example.com/shadow-inner-html",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = page
        .evaluate(
            r#"(() => {
                const root=document.getElementById('host').attachShadow({mode:'closed'});
                root.innerHTML='<iframe src="https://example.com/frame"></iframe>';
                const frame=root.querySelector('iframe');
                return JSON.stringify({
                    exists:!!frame,
                    connected:!!frame&&frame.isConnected,
                    hasWindow:!!frame&&!!frame.contentWindow,
                    topLength:window.length,
                    mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"exists":true,"connected":true,"hasWindow":true,"topLength":0,"mapped":1}"#
    );
}

#[tokio::test]
async fn connected_shadow_iframe_src_assignment_switches_to_remote_frame() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body><div id="host"></div></body>"#,
        "https://example.com/shadow-src-assignment",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = page
        .evaluate(
            r#"(() => {
                const root=document.getElementById('host').attachShadow({mode:'closed'});
                const frame=document.createElement('iframe');
                root.appendChild(frame);
                const before=frame.contentWindow;
                const mappedBefore=Object.keys(globalThis.__frameIdForNode||{}).length;
                frame.src='https://example.com/frame';
                const after=frame.contentWindow;
                return JSON.stringify({
                    connected:frame.isConnected,
                    mappedBefore,
                    mappedAfter:Object.keys(globalThis.__frameIdForNode||{}).length,
                    stable:before===after,
                    topLength:window.length,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"connected":true,"mappedBefore":0,"mappedAfter":1,"stable":true,"topLength":0}"#
    );
}

#[tokio::test]
async fn connecting_prebuilt_shadow_host_activates_remote_iframe_once() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/shadow-prebuilt",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = page
        .evaluate(
            r#"(() => {
                const host=document.createElement('div');
                const root=host.attachShadow({mode:'closed'});
                const frame=document.createElement('iframe');
                frame.src='https://example.com/frame';
                root.appendChild(frame);
                const before={
                    connected:frame.isConnected,
                    mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                    windowNull:frame.contentWindow===null,
                };
                document.body.appendChild(host);
                const cw=frame.contentWindow;
                const after={
                    connected:frame.isConnected,
                    mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                    hasWindow:!!cw,
                    topLength:window.length,
                };
                host.remove();
                return JSON.stringify({
                    before,
                    after,
                    removed:{
                        connected:frame.isConnected,
                        mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                        directWindowNull:frame.contentWindow===null,
                        oldClosed:cw&&cw.closed===true,
                    },
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"before":{"connected":false,"mapped":0,"windowNull":true},"after":{"connected":true,"mapped":1,"hasWindow":true,"topLength":0},"removed":{"connected":false,"mapped":0,"directWindowNull":true,"oldClosed":true}}"#
    );
}

#[tokio::test]
async fn active_same_origin_frames_expose_their_container_element() {
    let mut page = empty_page().await;
    let top = page
        .evaluate(
            r#"(() => {
                const f=document.createElement('iframe');
                f.srcdoc='<main></main>';
                document.body.appendChild(f);
                const cw=f.contentWindow;
                return JSON.stringify({
                    same:cw.frameElement===f,
                    owner:cw.frameElement&&cw.frameElement.ownerDocument===document,
                    parent:cw.parent===window,
                    top:cw.top===window,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        top,
        r#"{"same":true,"owner":true,"parent":true,"top":true}"#
    );

    page.evaluate(
        "document.body.innerHTML='<iframe id=\"outer\" srcdoc=\"<main></main>\"></iframe>'; true",
    )
    .unwrap();
    let nested = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    const f=document.createElement('iframe');
                    f.srcdoc='<p>inner</p>';
                    document.body.appendChild(f);
                    const cw=f.contentWindow;
                    return JSON.stringify({
                        same:cw.frameElement===f,
                        owner:cw.frameElement&&cw.frameElement.ownerDocument===document,
                        parent:cw.parent===window,
                        topDistinct:cw.top!==window,
                    });
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(
        nested,
        r#"{"same":true,"owner":true,"parent":true,"topDistinct":true}"#
    );
}

#[tokio::test]
async fn child_promise_microtask_checkpoint_runs() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="child" srcdoc="<main></main>"></iframe>"#,
        "https://example.com/microtask",
        None,
    )
    .await
    .unwrap();

    let mut child = page.child_iframe(0).unwrap();
    child
        .evaluate("globalThis.__microtaskValue=0; Promise.resolve().then(()=>{globalThis.__microtaskValue=1;});")
        .unwrap();
    assert_eq!(
        child
            .evaluate("String(globalThis.__microtaskValue)")
            .unwrap(),
        "1"
    );
}

#[tokio::test]
async fn child_execute_and_run_preserves_promise_state() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="child" srcdoc="<main></main>"></iframe>"#,
        "https://example.com/microtask-pump",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let mut child = page.child_iframe(0).unwrap();
    child
        .execute_and_run(
            "globalThis.__microtaskPump=0; Promise.resolve().then(()=>{globalThis.__microtaskPump=7;});",
            Duration::from_millis(250),
        )
        .await
        .unwrap();
    assert_eq!(
        child
            .evaluate("String(globalThis.__microtaskPump)")
            .unwrap(),
        "7"
    );
}

#[tokio::test]
async fn child_reacquired_handle_preserves_same_realm() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="child" srcdoc="<main></main>"></iframe>"#,
        "https://example.com/reacquire",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();
    assert_eq!(page.frame_tree_count(), 0);

    {
        let mut first = page.child_iframe(0).unwrap();
        assert_eq!(first.evaluate("typeof document").unwrap(), "object");
        first.evaluate("globalThis.__reacquireMarker=3").unwrap();
    }

    let mut second = page.child_iframe(0).unwrap();
    assert_eq!(
        second
            .evaluate("String(globalThis.__reacquireMarker)")
            .unwrap(),
        "3"
    );
    second
        .execute_and_run(
            "Promise.resolve().then(()=>{globalThis.__reacquireMarker=9;});",
            Duration::from_millis(250),
        )
        .await
        .unwrap();
    assert_eq!(
        second
            .evaluate("String(globalThis.__reacquireMarker)")
            .unwrap(),
        "9"
    );
}

#[tokio::test]
async fn repeated_nested_content_window_reads_do_not_rerun_srcdoc() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="outer" srcdoc="<main></main>"></iframe>"#,
        "https://example.com/reentrant-content-window",
        None,
    )
    .await
    .unwrap();

    let state = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    globalThis.__nestedRuns = 0;
                    const f = document.createElement('iframe');
                    f.srcdoc = '<script>parent.__nestedRuns++<\/script>';
                    document.body.appendChild(f);
                    const a = f.contentWindow;
                    const b = f.contentWindow;
                    const c = document.querySelector('iframe').contentWindow;
                    return JSON.stringify({
                        runs: globalThis.__nestedRuns,
                        same: a === b && b === c,
                        length: window.length,
                        indexed: window[0] === a,
                    });
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(state, r#"{"runs":1,"same":true,"length":1,"indexed":true}"#);
}

#[tokio::test]
async fn parser_nested_iframe_is_registered_in_parent_realm() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><iframe id="outer"></iframe>"#,
        "https://example.com/parser-nested",
        None,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"document.getElementById('outer').srcdoc='<main><iframe id="inner" srcdoc="<p>inner</p>"></iframe></main>'; true"#,
    )
    .unwrap();

    let state = {
        let mut outer = page.child_iframe(0).unwrap();
        outer
            .evaluate(
                r#"(() => {
                    const inner = document.getElementById('inner');
                    const cw = inner && inner.contentWindow;
                    return JSON.stringify({
                        exists: !!inner,
                        length: window.length,
                        indexed: !!cw && window[0] === cw,
                        parent: !!cw && cw.parent === window,
                    });
                })()"#,
            )
            .unwrap()
    };
    assert_eq!(
        state,
        r#"{"exists":true,"length":1,"indexed":true,"parent":true}"#
    );
    assert_eq!(page.frame_tree_count(), 0);
}

#[tokio::test]
async fn top_frame_registry_reorders_without_recreating_windows() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/reorder",
        None,
    )
    .await
    .unwrap();

    let state = page
        .evaluate(
            r#"(() => {
                const a=document.createElement('iframe');
                const b=document.createElement('iframe');
                a.srcdoc='<p>a</p>'; b.srcdoc='<p>b</p>';
                document.body.append(a,b);
                const aw=a.contentWindow, bw=b.contentWindow;
                document.body.insertBefore(b,a);
                return JSON.stringify({
                    length:window.length,
                    b0:window[0]===bw,
                    a1:window[1]===aw,
                    stableA:a.contentWindow===aw,
                    stableB:b.contentWindow===bw,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"length":2,"b0":true,"a1":true,"stableA":true,"stableB":true}"#
    );
}

#[tokio::test]
async fn template_iframe_does_not_enter_window_frame_registry() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/template-frame",
        None,
    )
    .await
    .unwrap();

    let state = page
        .evaluate(
            r#"(() => {
                const t=document.createElement('template');
                t.innerHTML='<iframe id="latent" srcdoc="<p>latent</p>"></iframe>';
                document.body.appendChild(t);
                return JSON.stringify({
                    length:window.length,
                    slot0:typeof window[0],
                    documentIframe:document.querySelector('iframe')!==null,
                    hasContent:'content' in t,
                    contentIframe:!!t.content.querySelector('iframe'),
                    contentWindowNull:t.content.querySelector('iframe').contentWindow===null,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"length":0,"slot0":"undefined","documentIframe":false,"hasContent":true,"contentIframe":true,"contentWindowNull":true}"#
    );
}

#[tokio::test]
async fn parser_template_iframe_stays_inert() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body><template id="t"><iframe id="latent" srcdoc="<p>latent</p>"></iframe></template></body>"#,
        "https://example.com/parser-template-frame",
        None,
    )
    .await
    .unwrap();

    let state = page
        .evaluate(
            r#"(() => {
                const t=document.getElementById('t');
                const latent=t.content.querySelector('#latent');
                return JSON.stringify({
                    length:window.length,
                    slot0:typeof window[0],
                    documentIframe:document.querySelector('iframe')!==null,
                    childNodes:t.childNodes.length,
                    contentIframe:!!latent,
                    contentWindowNull:latent&&latent.contentWindow===null,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"length":0,"slot0":"undefined","documentIframe":false,"childNodes":0,"contentIframe":true,"contentWindowNull":true}"#
    );
}

#[tokio::test]
async fn template_iframe_activates_only_after_entering_document() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/template-activate",
        None,
    )
    .await
    .unwrap();

    let state = page
        .evaluate(
            r#"(() => {
                const t=document.createElement('template');
                t.innerHTML='<iframe id="moving" srcdoc="<script>globalThis.__activated=17<\/script>"></iframe>';
                document.body.appendChild(t);
                const moving=t.content.querySelector('#moving');
                const before={length:window.length,windowNull:moving.contentWindow===null};
                document.body.appendChild(moving);
                const cw=moving.contentWindow;
                return JSON.stringify({
                    before,
                    after:{
                        length:window.length,
                        indexed:window[0]===cw,
                        activated:cw&&cw.__activated,
                        templateEmpty:t.content.childNodes.length===0,
                    },
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"before":{"length":0,"windowNull":true},"after":{"length":1,"indexed":true,"activated":17,"templateEmpty":true}}"#
    );
}

#[tokio::test]
async fn disconnected_fragment_iframe_does_not_register_frame() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/fragment-frame",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = page
        .evaluate(
            r#"(() => {
                const fragment=document.createDocumentFragment();
                const f=document.createElement('iframe');
                f.src='https://example.com/frame';
                fragment.appendChild(f);
                return JSON.stringify({
                    connected:f.isConnected,
                    windowNull:f.contentWindow===null,
                    length:window.length,
                    mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"connected":false,"windowNull":true,"length":0,"mapped":0}"#
    );
}

#[tokio::test]
async fn moving_live_iframe_into_fragment_unregisters_it() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/fragment-detach",
        None,
    )
    .await
    .unwrap();
    page.init_top_frame();

    let state = page
        .evaluate(
            r#"(() => {
                const f=document.createElement('iframe');
                f.srcdoc='<p>live</p>';
                document.body.appendChild(f);
                const cw=f.contentWindow;
                cw.__marker='live';
                const fragment=document.createDocumentFragment();
                fragment.appendChild(f);
                return JSON.stringify({
                    connected:f.isConnected,
                    directWindowNull:f.contentWindow===null,
                    oldClosed:cw.closed===true,
                    oldMarker:cw.__marker,
                    length:window.length,
                    mapped:Object.keys(globalThis.__frameIdForNode||{}).length,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"connected":false,"directWindowNull":true,"oldClosed":true,"oldMarker":"live","length":0,"mapped":0}"#
    );
}

#[tokio::test]
async fn deep_cloned_template_preserves_inert_contents() {
    let mut page = browser_oxide::Page::from_html_with_url(
        r#"<!doctype html><body></body>"#,
        "https://example.com/template-clone",
        None,
    )
    .await
    .unwrap();

    let state = page
        .evaluate(
            r#"(() => {
                const original=document.createElement('template');
                original.innerHTML='<section id="inside"><iframe srcdoc="<p>x</p>"></iframe></section>';
                const clone=original.cloneNode(true);
                document.body.appendChild(clone);
                const iframe=clone.content&&clone.content.querySelector('iframe');
                return JSON.stringify({
                    distinct:clone!==original && clone.content!==original.content,
                    html:clone.innerHTML,
                    section:!!clone.content&&!!clone.content.querySelector('#inside'),
                    iframe:!!iframe,
                    inert:!!iframe&&iframe.contentWindow===null,
                    length:window.length,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        state,
        r#"{"distinct":true,"html":"<section id=\"inside\"><iframe srcdoc=\"<p>x</p>\"></iframe></section>","section":true,"iframe":true,"inert":true,"length":0}"#
    );
}
