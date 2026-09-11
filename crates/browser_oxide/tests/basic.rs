use browser_oxide::js_runtime::BrowserJsRuntime;
use std::time::{Duration, Instant};

fn create_test_runtime() -> BrowserJsRuntime {
    let dom = browser_oxide::html_parser::parse_html(
        "<html><head><title>Test</title></head><body><div id=\"main\" class=\"container\"><p>Hello world</p></div></body></html>"
    );
    BrowserJsRuntime::new(dom)
}

#[tokio::test]
async fn basic_js_execution() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("1 + 2", None).unwrap();
    assert_eq!(result, "3");
}

#[tokio::test]
async fn scope_selector_matches_document_and_element_semantics() {
    let dom = browser_oxide::html_parser::parse_html(
        r#"<html><body><div id="root"><span id="a"></span><div><span id="b"></span></div></div></body></html>"#,
    );
    let mut rt = BrowserJsRuntime::new(dom);
    let result = rt
        .execute_script(
            r#"
            (() => {
                const root = document.getElementById('root');
                const id = el => el ? el.id : null;
                return JSON.stringify({
                    qScope: id(root.querySelector(':scope')),
                    qaScope: Array.from(root.querySelectorAll(':scope')).map(id),
                    direct: Array.from(root.querySelectorAll(':scope > span')).map(id),
                    nested: Array.from(root.querySelectorAll(':scope span')).map(id),
                    matches: root.matches(':scope'),
                    closest: id(root.closest(':scope')),
                    childMatches: document.getElementById('a').matches(':scope'),
                    docScope: document.querySelector(':scope').tagName,
                    docHtml: document.querySelector('html').tagName,
                    docAllScope: Array.from(document.querySelectorAll(':scope')).map(el => el.tagName),
                });
            })()
            "#,
            None,
        )
        .unwrap();
    assert_eq!(
        result,
        r#"{"qScope":null,"qaScope":[],"direct":["a"],"nested":["a","b"],"matches":true,"closest":"root","childMatches":true,"docScope":"HTML","docHtml":"HTML","docAllScope":["HTML"]}"#
    );
}

#[tokio::test]
async fn nth_child_of_selector_list_filters_the_subject_like_chrome() {
    let dom = browser_oxide::html_parser::parse_html(
        r#"<html><body><div id="root"><span id="s1" class="a"></span><span id="s2" class="b"></span><span id="s3" class="a"></span></div></body></html>"#,
    );
    let mut rt = BrowserJsRuntime::new(dom);
    let result = rt
        .execute_script(
            r#"(() => {
                const root = document.getElementById('root');
                return JSON.stringify({
                    secondA: Array.from(root.querySelectorAll(':scope > span:nth-child(2 of .a)'), e => e.id),
                    lastA: Array.from(root.querySelectorAll(':scope > span:nth-last-child(1 of .a)'), e => e.id),
                    bMatchesSecondA: document.getElementById('s2').matches(':nth-child(2 of .a)')
                });
            })()"#,
            None,
        )
        .unwrap();
    assert_eq!(
        result,
        r#"{"secondA":["s3"],"lastA":["s3"],"bMatchesSecondA":false}"#
    );
}

#[tokio::test]
async fn attribute_selector_s_modifier_is_rejected_like_chrome() {
    let dom = browser_oxide::html_parser::parse_html(
        r#"<html><body><div data-x="Foo"></div></body></html>"#,
    );
    let mut rt = BrowserJsRuntime::new(dom);
    assert_eq!(
        rt.execute_script(
            r#"(() => {
                try {
                    document.querySelectorAll('[data-x="Foo" s]');
                    return 'NO_THROW';
                } catch (e) {
                    return e.name;
                }
            })()"#,
            None,
        )
        .unwrap(),
        "SyntaxError"
    );
}

#[tokio::test]
async fn html_default_attribute_value_case_sensitivity_matches_chrome() {
    let dom = browser_oxide::html_parser::parse_html(
        r#"<!doctype html><html><body>
            <input id="i" type="text" autocomplete="ON" inputmode="NUMERIC" name="Foo">
            <form id="f" method="POST" enctype="MULTIPART/FORM-DATA"></form>
            <button id="btn" type="SUBMIT"></button>
            <ol id="ol" type="A"></ol>
            <a id="a" target="_BLANK" rel="NOFOLLOW" hreflang="EN"></a>
            <link id="link" rel="STYLESHEET" media="SCREEN">
            <meta id="meta" http-equiv="REFRESH">
            <iframe id="frame" scrolling="NO"></iframe>
            <table id="table" frame="BOX"><tbody><tr>
                <th id="th" scope="ROW"></th><td id="td" valign="MIDDLE"></td>
            </tr></tbody></table>
            <p id="p" align="CENTER"></p>
            <div id="d" dir="RTL" lang="EN" class="Foo" data-x="Bar"
                 contenteditable="TRUE" draggable="TRUE" translate="YES"></div>
            <img id="img" crossorigin="ANONYMOUS">
        </body></html>"#,
    );
    let mut rt = BrowserJsRuntime::new(dom);
    let result = rt
        .execute_script(
            r#"(() => {
                const ids = s => Array.from(document.querySelectorAll(s), e => e.id);
                return JSON.stringify({
                    inputType: ids('input[type="TEXT"]'),
                    formMethod: ids('form[method="post"]'),
                    formEnctype: ids('form[enctype="multipart/form-data"]'),
                    buttonType: ids('button[type="submit"]'),
                    olType: ids('ol[type="a"]'),
                    target: ids('a[target="_blank"]'),
                    rel: ids('a[rel="nofollow"]'),
                    hreflang: ids('a[hreflang="en"]'),
                    linkRel: ids('link[rel="stylesheet"]'),
                    media: ids('link[media="screen"]'),
                    scope: ids('th[scope="row"]'),
                    httpEquiv: ids('meta[http-equiv="refresh"]'),
                    scrolling: ids('iframe[scrolling="no"]'),
                    frame: ids('table[frame="box"]'),
                    valign: ids('td[valign="middle"]'),
                    align: ids('p[align="center"]'),
                    dir: ids('[dir="rtl"]'),
                    lang: ids('[lang="en"]'),
                    autocompleteStrict: ids('input[autocomplete="on"]'),
                    inputmodeStrict: ids('input[inputmode="numeric"]'),
                    nameStrict: ids('input[name="foo"]'),
                    classStrict: ids('[class="foo"]'),
                    dataStrict: ids('[data-x="bar"]'),
                    contenteditableStrict: ids('[contenteditable="true"]'),
                    draggableStrict: ids('[draggable="true"]'),
                    translateStrict: ids('[translate="yes"]'),
                    crossoriginStrict: ids('img[crossorigin="anonymous"]'),
                    explicitI: ids('[data-x="bar" i]')
                });
            })()"#,
            None,
        )
        .unwrap();

    assert_eq!(
        result,
        r#"{"inputType":["i"],"formMethod":["f"],"formEnctype":["f"],"buttonType":["btn"],"olType":["ol"],"target":["a"],"rel":["a"],"hreflang":["a"],"linkRel":["link"],"media":["link"],"scope":["th"],"httpEquiv":["meta"],"scrolling":["frame"],"frame":["table"],"valign":["td"],"align":["p"],"dir":["d"],"lang":["d"],"autocompleteStrict":[],"inputmodeStrict":[],"nameStrict":[],"classStrict":[],"dataStrict":[],"contenteditableStrict":[],"draggableStrict":[],"translateStrict":[],"crossoriginStrict":[],"explicitI":["d"]}"#
    );
}

#[tokio::test]
async fn has_relative_selectors_match_chrome_semantics() {
    let dom = browser_oxide::html_parser::parse_html(
        r#"<!doctype html><html><body>
            <div id="a"><span class="child"></span></div>
            <div id="b"></div><div id="c" class="sib"></div>
            <div id="d"></div><div id="e" class="sib"></div>
        </body></html>"#,
    );
    let mut rt = BrowserJsRuntime::new(dom);
    let result = rt
        .execute_script(
            r#"JSON.stringify({
                direct: Array.from(document.querySelectorAll('div:has(> .child)'), e => e.id),
                adjacent: Array.from(document.querySelectorAll('#b:has(+ .sib)'), e => e.id),
                general: Array.from(document.querySelectorAll('#b:has(~ .sib)'), e => e.id),
                descendant: Array.from(document.querySelectorAll('div:has(.child)'), e => e.id),
                complex: Array.from(document.querySelectorAll('#b:has(+ .sib + #d ~ .sib)'), e => e.id),
                list: Array.from(document.querySelectorAll('div:has(> .child, + .sib)'), e => e.id),
                matchesDirect: document.getElementById('a').matches(':has(> .child)'),
                matchesAdjacent: document.getElementById('b').matches(':has(+ .sib)')
            })"#,
            None,
        )
        .unwrap();
    assert_eq!(
        result,
        r#"{"direct":["a"],"adjacent":["b"],"general":["b"],"descendant":["a"],"complex":["b"],"list":["a","b","d"],"matchesDirect":true,"matchesAdjacent":true}"#
    );

    let invalid = rt
        .execute_script(
            r#"(()=>{try{document.querySelectorAll('div:has(> )');return 'NO_THROW'}catch(e){return JSON.stringify({name:e.name,code:e.code,ctor:e.constructor.name,tag:Object.prototype.toString.call(e),isDOM:e instanceof DOMException,message:e.message})}})()"#,
            None,
        )
        .unwrap();
    assert_eq!(
        invalid,
        r#"{"name":"SyntaxError","code":12,"ctor":"DOMException","tag":"[object DOMException]","isDOM":true,"message":"Failed to execute 'querySelectorAll' on 'Document': 'div:has(> )' is not a valid selector."}"#
    );
}

#[tokio::test]
async fn pseudo_elements_parse_but_never_match_dom_elements() {
    let dom = browser_oxide::html_parser::parse_html(
        r#"<html><body><div id="target"></div></body></html>"#,
    );
    let mut rt = BrowserJsRuntime::new(dom);
    assert_eq!(
        rt.execute_script(
            r#"JSON.stringify({
                q: document.querySelector('html::before') === null,
                qa: document.querySelectorAll('html::before').length,
                m: document.documentElement.matches('html::before'),
                c: document.documentElement.closest('html::before') === null
            })"#,
            None,
        )
        .unwrap(),
        r#"{"q":true,"qa":0,"m":false,"c":true}"#
    );
}

#[tokio::test]
async fn module_evaluation_does_not_wait_for_unrelated_refed_timer() {
    let mut rt = create_test_runtime();
    let started = Instant::now();

    tokio::time::timeout(
        Duration::from_secs(1),
        rt.load_eval_module_code(
            "https://example.test/entry.js",
            r#"
                setInterval(() => {}, 25);
                await new Promise(resolve => setTimeout(resolve, 10));
                globalThis.__moduleDone = true;
            "#
            .to_string(),
        ),
    )
    .await
    .expect("module evaluation must not wait for unrelated page background timers")
    .expect("module evaluation");

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "module evaluation was pinned by unrelated runtime work"
    );
    assert_eq!(
        rt.execute_script("String(globalThis.__moduleDone)", None)
            .unwrap(),
        "true"
    );
}

fn runtime_at(url: &str) -> BrowserJsRuntime {
    let dom = browser_oxide::html_parser::parse_html(
        "<!doctype html><html><head></head><body></body></html>",
    );
    let mut rt = BrowserJsRuntime::new(dom);
    let encoded = serde_json::to_string(url).unwrap();
    rt.execute_script(&format!("location.href = {encoded}"), None)
        .unwrap();
    rt.reset_nav_pending();
    rt
}

#[tokio::test(flavor = "current_thread")]
async fn broadcast_channel_isolates_origins_across_page_runtimes() {
    let mut page_a = runtime_at("https://a.example/page-a");
    let mut page_b = runtime_at("https://a.example/page-b");
    let mut cross_origin = runtime_at("https://b.example/page-c");

    page_a
        .execute_script(
            "globalThis.got=[]; globalThis.bc=new BroadcastChannel('room'); bc.onmessage=e=>got.push(e.data)",
            None,
        )
        .unwrap();
    page_b
        .execute_script("globalThis.bc=new BroadcastChannel('room')", None)
        .unwrap();
    cross_origin
        .execute_script(
            "globalThis.got=[]; globalThis.bc=new BroadcastChannel('room'); bc.onmessage=e=>got.push(e.data)",
            None,
        )
        .unwrap();

    page_b
        .execute_script("bc.postMessage('same-origin')", None)
        .unwrap();
    cross_origin
        .execute_script("bc.postMessage('cross-origin')", None)
        .unwrap();

    page_a.run_event_loop().await.unwrap();
    cross_origin.run_event_loop().await.unwrap();

    assert_eq!(
        page_a.execute_script("JSON.stringify(got)", None).unwrap(),
        r#"["same-origin"]"#
    );
    assert_eq!(
        cross_origin
            .execute_script("JSON.stringify(got)", None)
            .unwrap(),
        "[]"
    );

    page_a.execute_script("bc.close()", None).unwrap();
    page_b
        .execute_script("bc.postMessage('after-close')", None)
        .unwrap();
    page_a.run_event_loop().await.unwrap();
    assert_eq!(
        page_a.execute_script("JSON.stringify(got)", None).unwrap(),
        r#"["same-origin"]"#
    );
}

#[tokio::test]
async fn console_log_capture() {
    let mut rt = create_test_runtime();
    rt.execute_script("console.log('hello from JS')", None)
        .unwrap();
    let output = rt.console_output();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].args[0], "[string] hello from JS");
}

#[tokio::test]
async fn document_exists() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("typeof document", None).unwrap();
    assert_eq!(result, "object");
}

#[tokio::test]
async fn document_query_selector() {
    let mut rt = create_test_runtime();
    let result = rt
        .execute_script("document.querySelector('#main').tagName", None)
        .unwrap();
    assert_eq!(result, "DIV");
}

#[tokio::test]
async fn document_get_element_by_id() {
    let mut rt = create_test_runtime();
    let result = rt
        .execute_script("document.getElementById('main').className", None)
        .unwrap();
    assert_eq!(result, "container");
}

#[tokio::test]
async fn element_text_content() {
    let mut rt = create_test_runtime();
    let result = rt
        .execute_script("document.querySelector('p').textContent", None)
        .unwrap();
    assert_eq!(result, "Hello world");
}

#[tokio::test]
async fn element_inner_html() {
    let mut rt = create_test_runtime();
    let result = rt
        .execute_script("document.querySelector('#main').innerHTML", None)
        .unwrap();
    assert!(
        result.contains("<p>"),
        "innerHTML should contain <p>, got: {}",
        result
    );
    assert!(
        result.contains("Hello world"),
        "innerHTML should contain text"
    );
}

#[tokio::test]
async fn create_element_and_append() {
    let mut rt = create_test_runtime();
    rt.execute_script(
        r#"
        const div = document.createElement('span');
        div.setAttribute('id', 'new-span');
        document.querySelector('#main').appendChild(div);
    "#,
        None,
    )
    .unwrap();

    let result = rt
        .execute_script("document.querySelector('#new-span').tagName", None)
        .unwrap();
    assert_eq!(result, "SPAN");
}

#[tokio::test]
async fn set_text_content() {
    let mut rt = create_test_runtime();
    rt.execute_script(
        r#"
        document.querySelector('p').textContent = 'Modified!';
    "#,
        None,
    )
    .unwrap();

    let result = rt
        .execute_script("document.querySelector('p').textContent", None)
        .unwrap();
    assert_eq!(result, "Modified!");
}

#[tokio::test]
async fn set_inner_html() {
    let mut rt = create_test_runtime();
    rt.execute_script(
        r#"
        document.querySelector('#main').innerHTML = '<span>New content</span>';
    "#,
        None,
    )
    .unwrap();

    let result = rt
        .execute_script("document.querySelector('#main span').textContent", None)
        .unwrap();
    assert_eq!(result, "New content");
}

#[tokio::test]
async fn set_attribute() {
    let mut rt = create_test_runtime();
    rt.execute_script(
        r#"
        document.querySelector('#main').setAttribute('data-test', 'hello');
    "#,
        None,
    )
    .unwrap();

    let result = rt
        .execute_script(
            "document.querySelector('#main').getAttribute('data-test')",
            None,
        )
        .unwrap();
    assert_eq!(result, "hello");
}

#[tokio::test]
async fn class_list() {
    let mut rt = create_test_runtime();
    rt.execute_script(
        r#"
        const el = document.querySelector('#main');
        el.classList.add('new-class');
    "#,
        None,
    )
    .unwrap();

    let result = rt
        .execute_script("document.querySelector('#main').className", None)
        .unwrap();
    assert!(
        result.contains("new-class"),
        "className should contain new-class, got: {}",
        result
    );
    assert!(
        result.contains("container"),
        "className should still contain container"
    );
}

#[tokio::test]
async fn document_title() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("document.title", None).unwrap();
    assert_eq!(result, "Test");
}

#[tokio::test]
async fn document_has_focus() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("document.hasFocus()", None).unwrap();
    assert_eq!(result, "true");
}

#[tokio::test]
async fn document_visibility_state() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("document.visibilityState", None).unwrap();
    assert_eq!(result, "visible");
}

#[tokio::test]
async fn window_self_reference() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("window === globalThis", None).unwrap();
    assert_eq!(result, "true");
}

#[tokio::test]
async fn node_types_exist() {
    let mut rt = create_test_runtime();
    let result = rt.execute_script("typeof Element", None).unwrap();
    assert_eq!(result, "function");
    let result = rt.execute_script("typeof Document", None).unwrap();
    assert_eq!(result, "function");
    let result = rt.execute_script("typeof Node", None).unwrap();
    assert_eq!(result, "function");
}

#[tokio::test]
async fn take_dom_back() {
    let rt = create_test_runtime();
    let dom = rt.take_dom();
    // Verify the DOM is intact
    let html = dom.child_elements(browser_oxide::dom::NodeId::DOCUMENT);
    assert!(!html.is_empty());
}

#[tokio::test]
async fn query_selector_all() {
    let dom = browser_oxide::html_parser::parse_html(
        "<html><body><ul><li>a</li><li>b</li><li>c</li></ul></body></html>",
    );
    let mut rt = BrowserJsRuntime::new(dom);
    let result = rt
        .execute_script("document.querySelectorAll('li').length", None)
        .unwrap();
    assert_eq!(result, "3");
}

#[tokio::test]
async fn get_elements_by_tag_name() {
    let mut rt = create_test_runtime();
    // The test HTML has: html, head, title, body, div, p
    let result = rt
        .execute_script("document.getElementsByTagName('div').length", None)
        .unwrap();
    assert_eq!(result, "1");
}

// === Stealth profile tests ===

#[tokio::test]
async fn stealth_profile_overrides_navigator() {
    let profile = browser_oxide::stealth::chrome_148_windows();
    let dom = browser_oxide::html_parser::parse_html("<html><head></head><body></body></html>");
    let mut rt = BrowserJsRuntime::with_profile(dom, profile);

    let ua = rt.execute_script("navigator.userAgent", None).unwrap();
    assert!(
        ua.contains("Windows NT 10.0"),
        "UA should be Windows: {}",
        ua
    );

    let platform = rt.execute_script("navigator.platform", None).unwrap();
    assert_eq!(platform, "Win32");

    let cores = rt
        .execute_script("navigator.hardwareConcurrency", None)
        .unwrap();
    assert_eq!(cores, "8");
}

#[tokio::test]
async fn stealth_profile_overrides_screen() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let dom = browser_oxide::html_parser::parse_html("<html><head></head><body></body></html>");
    let mut rt = BrowserJsRuntime::with_profile(dom, profile);

    let w = rt.execute_script("screen.width", None).unwrap();
    assert_eq!(w, "1512"); // macOS M3 MacBook Pro profile = 1512

    let dpr = rt.execute_script("devicePixelRatio", None).unwrap();
    assert_eq!(dpr, "2"); // macOS Retina = 2x
}

#[tokio::test]
async fn stealth_profile_overrides_window_dims() {
    let profile = browser_oxide::stealth::chrome_148_linux();
    let dom = browser_oxide::html_parser::parse_html("<html><head></head><body></body></html>");
    let mut rt = BrowserJsRuntime::with_profile(dom, profile);

    let iw = rt.execute_script("window.innerWidth", None).unwrap();
    assert_eq!(iw, "1920");

    let ih = rt.execute_script("window.innerHeight", None).unwrap();
    assert_eq!(ih, "969");
}

#[tokio::test]
async fn get_computed_style_basic() {
    let mut rt = create_test_runtime();
    let result = rt
        .execute_script("getComputedStyle(document.body).display", None)
        .unwrap();
    assert_eq!(result, "block");
}

#[tokio::test]
async fn get_computed_style_instanceof() {
    let mut rt = create_test_runtime();
    let result = rt
        .execute_script(
            "getComputedStyle(document.body) instanceof CSSStyleDeclaration",
            None,
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[tokio::test]
async fn no_profile_uses_defaults() {
    let mut rt = create_test_runtime(); // no profile
    let ua = rt.execute_script("navigator.userAgent", None).unwrap();
    assert!(
        ua.contains("Chrome"),
        "Default UA should contain Chrome: {}",
        ua
    );
}

// ===== vNext/10 — URL polyfill opaque-scheme handling =====

/// Real Chrome on `new URL("blob:null/uuid").protocol` returns `"blob:"`.
/// Pre-fix, BO's URL polyfill emitted `""`. Caught during worker-URL testing
/// and fixed in commit (this commit).
#[tokio::test]
async fn url_blob_scheme_protocol_and_origin() {
    let mut rt = create_test_runtime();
    let proto = rt
        .execute_script(r#"new URL("blob:null/7aeb61c9-deadbeef").protocol"#, None)
        .unwrap();
    assert_eq!(proto, "blob:");
    let origin = rt
        .execute_script(r#"new URL("blob:null/7aeb61c9-deadbeef").origin"#, None)
        .unwrap();
    assert_eq!(origin, "null");
    let href = rt
        .execute_script(r#"new URL("blob:null/7aeb61c9-deadbeef").href"#, None)
        .unwrap();
    assert_eq!(href, "blob:null/7aeb61c9-deadbeef");

    let inherited = rt
        .execute_script(
            r#"JSON.stringify((()=>{const u=new URL("blob:https://example.com/id?q=1#h");return {origin:u.origin,pathname:u.pathname,search:u.search,hash:u.hash};})())"#,
            None,
        )
        .unwrap();
    assert_eq!(
        inherited,
        r##"{"origin":"https://example.com","pathname":"https://example.com/id","search":"?q=1","hash":"#h"}"##
    );
}

/// `data:` URLs are opaque per WHATWG URL spec: protocol="data:",
/// origin="null". Real Chrome behavior.
#[tokio::test]
async fn url_data_scheme_protocol_and_origin() {
    let mut rt = create_test_runtime();
    let proto = rt
        .execute_script(r#"new URL("data:text/html,<p>hi</p>").protocol"#, None)
        .unwrap();
    assert_eq!(proto, "data:");
    let origin = rt
        .execute_script(r#"new URL("data:text/html,<p>hi</p>").origin"#, None)
        .unwrap();
    assert_eq!(origin, "null");
}

/// `javascript:` URLs are also opaque.
#[tokio::test]
async fn url_javascript_scheme_protocol() {
    let mut rt = create_test_runtime();
    let proto = rt
        .execute_script(r#"new URL("javascript:void(0)").protocol"#, None)
        .unwrap();
    assert_eq!(proto, "javascript:");
}

/// `about:` URLs (about:blank, about:srcdoc) are opaque.
#[tokio::test]
async fn url_about_scheme_protocol() {
    let mut rt = create_test_runtime();
    let proto = rt
        .execute_script(r#"new URL("about:blank").protocol"#, None)
        .unwrap();
    assert_eq!(proto, "about:");
}

/// Regression: http(s) URLs must still parse correctly — the opaque-scheme
/// branch is added BEFORE the http regex, so it must not divert non-opaque
/// schemes.
#[tokio::test]
async fn url_https_still_parses_after_opaque_branch() {
    let mut rt = create_test_runtime();
    let proto = rt
        .execute_script(r#"new URL("https://example.com/foo?a=1#b").protocol"#, None)
        .unwrap();
    assert_eq!(proto, "https:");
    let host = rt
        .execute_script(r#"new URL("https://example.com/foo?a=1#b").host"#, None)
        .unwrap();
    assert_eq!(host, "example.com");
    let origin = rt
        .execute_script(r#"new URL("https://example.com/foo?a=1#b").origin"#, None)
        .unwrap();
    assert_eq!(origin, "https://example.com");
}

#[tokio::test]
async fn selector_dir_matches_inherited_and_auto_directionality() {
    let mut rt = BrowserJsRuntime::new(browser_oxide::html_parser::parse_html(
        r#"<!doctype html><html dir="ltr"><body>
          <div id="outer" dir="rtl"><span id="inherit"></span><span id="override" dir="ltr"></span></div>
          <div id="auto-ltr" dir="auto">abc אבג</div>
          <div id="auto-rtl" dir="auto">123 אבג abc</div>
          <div id="auto-neutral" dir="auto">123 !!!</div>
          <bdi id="bdi-rtl">אבג abc</bdi>
          <input id="input-rtl" dir="auto" value="אבג abc">
          <div id="comma-ltr" dir="auto">،abc</div>
          <div id="ancient-rtl" dir="auto">𐠀 abc</div>
          <div id="plain"></div>
        </body></html>"#,
    ));

    let result = rt
        .execute_script(
            r#"JSON.stringify({
          ltr:Array.from(document.querySelectorAll(':dir(ltr)'), e=>e.id||e.tagName),
          rtl:Array.from(document.querySelectorAll(':dir(rtl)'), e=>e.id||e.tagName),
          inherit:document.getElementById('inherit').matches(':dir(rtl)'),
          override:document.getElementById('override').matches(':dir(ltr)'),
          upper:Array.from(document.querySelectorAll(':dir(RTL)'), e=>e.id||e.tagName),
          unknown:document.querySelectorAll(':dir(sideways)').length,
          autoLtr:document.getElementById('auto-ltr').matches(':dir(ltr)'),
          autoRtl:document.getElementById('auto-rtl').matches(':dir(rtl)'),
          autoNeutral:document.getElementById('auto-neutral').matches(':dir(ltr)'),
          bdiRtl:document.getElementById('bdi-rtl').matches(':dir(rtl)'),
          inputRtl:document.getElementById('input-rtl').matches(':dir(rtl)'),
          commaLtr:document.getElementById('comma-ltr').matches(':dir(ltr)'),
          ancientRtl:document.getElementById('ancient-rtl').matches(':dir(rtl)')
        })"#,
            None,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["inherit"], true);
    assert_eq!(value["override"], true);
    assert_eq!(value["unknown"], 0);
    assert_eq!(value["autoLtr"], true);
    assert_eq!(value["autoRtl"], true);
    assert_eq!(value["autoNeutral"], true);
    assert_eq!(value["bdiRtl"], true);
    assert_eq!(value["inputRtl"], true);
    assert_eq!(value["commaLtr"], true);
    assert_eq!(value["ancientRtl"], true);
    assert_eq!(
        value["rtl"],
        serde_json::json!([
            "outer",
            "inherit",
            "auto-rtl",
            "bdi-rtl",
            "input-rtl",
            "ancient-rtl"
        ])
    );

    for selector in [":dir()", ":dir(ltr rtl)"] {
        let js = format!("(()=>{{try{{document.querySelectorAll({selector:?});return 'NO_THROW'}}catch(e){{return e.name}}}})()");
        assert_eq!(
            rt.execute_script(&js, None).unwrap(),
            "SyntaxError",
            "{selector}"
        );
    }
}

#[tokio::test]
async fn selector_lang_matches_inherited_html_language() {
    let mut rt = BrowserJsRuntime::new(browser_oxide::html_parser::parse_html(
        r#"<!doctype html><html lang="en-US"><body>
          <div id="fr" lang="fr-CA"><span id="fr-child"></span></div>
          <div id="empty" lang=""><span id="empty-child"></span></div>
          <div id="private" lang="x-private"><span id="private-child"></span></div>
          <div id="plain"><span id="plain-child"></span></div>
        </body></html>"#,
    ));

    let result = rt
        .execute_script(
            r#"JSON.stringify({
              bodyEn:document.body.matches(':lang(en)'),
              bodyEnUs:document.body.matches(':lang(en-US)'),
              bodyUpper:document.body.matches(':lang(EN)'),
              fr:document.getElementById('fr').matches(':lang(fr)'),
              frChild:document.getElementById('fr-child').matches(':lang(fr-ca)'),
              emptyEn:document.getElementById('empty').matches(':lang(en)'),
              emptyChildEn:document.getElementById('empty-child').matches(':lang(en)'),
              private:document.getElementById('private-child').matches(':lang(x-private)'),
              plain:document.getElementById('plain-child').matches(':lang(en)'),
              unknown:document.querySelectorAll(':lang(zz)').length
            })"#,
            None,
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result).unwrap(),
        serde_json::json!({
            "bodyEn": true,
            "bodyEnUs": true,
            "bodyUpper": true,
            "fr": true,
            "frChild": true,
            "emptyEn": false,
            "emptyChildEn": false,
            "private": true,
            "plain": true,
            "unknown": 0
        })
    );

    for selector in [
        ":lang()",
        ":lang(de, fr)",
        ":lang(\"fr\")",
        ":lang(en fr)",
        ":lang(*)",
    ] {
        let js = format!("(()=>{{try{{document.querySelectorAll({selector:?});return 'NO_THROW'}}catch(e){{return e.name}}}})()");
        assert_eq!(
            rt.execute_script(&js, None).unwrap(),
            "SyntaxError",
            "{selector}"
        );
    }
}
