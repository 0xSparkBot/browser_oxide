//! Realm-purity probes — bot-detection sensors and open-source FP suites
//! (e.g. CreepJS) probe iframe.contentWindow with cross-realm
//! identity checks. Real Chrome 147: each iframe is a distinct realm so
//! `iframe.contentWindow.Navigator !== window.Navigator` while own-property
//! names of the prototypes still match. Without this, every Tier-1 vendor's
//! iframe-realm probe flags the engine.
//!
//! The current implementation is a parent-side mirror realm: a separate
//! constructor copy with identical prototype-property-names but distinct
//! function identity. This is sufficient for the realm-purity probes which
//! are shape-and-identity checks, not cross-isolate execution.

use browser_oxide::Page;

fn html(body: &str) -> String {
    format!("<!DOCTYPE html><html><head></head><body>{body}</body></html>")
}

async fn evaluate(js: &str) -> String {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"))
}

// Helper: create an iframe and return its contentWindow.
const IFRAME_SETUP: &str = "
    const f = document.createElement('iframe');
    document.body.appendChild(f);
    const cw = f.contentWindow;
";

// ================================================================
// Probe 1: Navigator identity
// ================================================================
#[tokio::test]
async fn iframe_navigator_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Navigator !== Navigator")).await;
    assert_eq!(
        r, "true",
        "iframe.contentWindow.Navigator must be distinct from window.Navigator"
    );
}

#[tokio::test]
async fn iframe_navigator_prototype_distinct_identity() {
    let r = evaluate(&format!(
        "{IFRAME_SETUP} cw.Navigator.prototype !== Navigator.prototype"
    ))
    .await;
    assert_eq!(
        r, "true",
        "iframe.contentWindow.Navigator.prototype must be distinct"
    );
}

// ================================================================
// Probe 2: same shape (own-property-names equal)
// ================================================================
#[tokio::test]
async fn iframe_navigator_prototype_same_shape() {
    let js = format!(
        "{IFRAME_SETUP}
        const a = Object.getOwnPropertyNames(cw.Navigator.prototype).sort().join(',');
        const b = Object.getOwnPropertyNames(Navigator.prototype).sort().join(',');
        a === b
    "
    );
    let r = evaluate(&js).await;
    assert_eq!(
        r, "true",
        "iframe Navigator.prototype must have same own-property-names as parent's"
    );
}

// ================================================================
// Probe 3: cross-realm Function.prototype.toString
// ================================================================
#[tokio::test]
#[allow(non_snake_case, reason = "mirrors JS API name under test")]
async fn iframe_function_toString_native_shape() {
    let js = format!(
        "{IFRAME_SETUP}
        const s = cw.Function.prototype.toString.call(window.fetch);
        s.includes('[native code]')
    "
    );
    let r = evaluate(&js).await;
    assert_eq!(
        r, "true",
        "cross-realm Function.prototype.toString must produce [native code]"
    );
}

// ================================================================
// Probe 4: Array / Object distinct constructors (cross-realm instanceof)
// ================================================================
#[tokio::test]
async fn iframe_array_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Array !== Array")).await;
    assert_eq!(
        r, "true",
        "iframe.contentWindow.Array must be distinct from window.Array"
    );
}

#[tokio::test]
async fn iframe_object_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Object !== Object")).await;
    assert_eq!(
        r, "true",
        "iframe.contentWindow.Object must be distinct from window.Object"
    );
}

// Real Chrome: parent-realm [] is NOT instanceof iframe-realm Array.
#[tokio::test]
async fn iframe_array_cross_realm_instanceof_false() {
    let js = format!("{IFRAME_SETUP} ([] instanceof cw.Array)");
    let r = evaluate(&js).await;
    assert_eq!(r, "false", "parent [] must NOT be instanceof iframe.Array");
}

// ================================================================
// Probe 5: HTMLElement / Element / Node identity
// ================================================================
#[tokio::test]
async fn iframe_html_element_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.HTMLElement !== HTMLElement")).await;
    assert_eq!(r, "true");
}

#[tokio::test]
async fn iframe_element_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Element !== Element")).await;
    assert_eq!(r, "true");
}

#[tokio::test]
async fn iframe_node_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Node !== Node")).await;
    assert_eq!(r, "true");
}

#[tokio::test]
async fn iframe_event_target_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.EventTarget !== EventTarget")).await;
    assert_eq!(r, "true");
}

// ================================================================
// Probe 6: Event constructor identity
// ================================================================
#[tokio::test]
async fn iframe_event_distinct_identity() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Event !== Event")).await;
    assert_eq!(r, "true");
}

// ================================================================
// Probe 7: native function name is preserved cross-realm
// ================================================================
#[tokio::test]
async fn iframe_navigator_constructor_name() {
    let r = evaluate(&format!("{IFRAME_SETUP} cw.Navigator.name")).await;
    assert_eq!(r, "Navigator");
}

#[tokio::test]
#[allow(non_snake_case, reason = "mirrors JS API name under test")]
async fn iframe_navigator_toString_native_shape() {
    let r = evaluate(&format!(
        "{IFRAME_SETUP} cw.Function.prototype.toString.call(cw.Navigator)"
    ))
    .await;
    assert_eq!(r, "function Navigator() { [native code] }");
}

// ================================================================
// Probe 8: DOM instances belong to the child realm
// ================================================================
#[tokio::test]
async fn iframe_dom_instances_belong_to_child_realm() {
    let r = evaluate(
        r#"
        const f = document.createElement('iframe');
        f.srcdoc = "<div id='child-node'>child</div>";
        document.body.appendChild(f);
        const cw = f.contentWindow;
        const node = cw.document.getElementById('child-node');
        JSON.stringify({
            windowTag: Object.prototype.toString.call(cw),
            nodeExists: !!node,
            divInstance: node instanceof cw.HTMLDivElement,
            elementInstance: node instanceof cw.Element,
            nodeInstance: node instanceof cw.Node,
            exactPrototype: Object.getPrototypeOf(node) === cw.HTMLDivElement.prototype,
            ownerDocument: node.ownerDocument === cw.document,
            bodyPrototype: Object.getPrototypeOf(cw.document.body) === cw.HTMLBodyElement.prototype,
        })
        "#,
    )
    .await;

    assert_eq!(
        r,
        r#"{"windowTag":"[object Window]","nodeExists":true,"divInstance":true,"elementInstance":true,"nodeInstance":true,"exactPrototype":true,"ownerDocument":true,"bodyPrototype":true}"#
    );
}

#[tokio::test]
async fn iframe_child_nodes_follow_child_document_lifecycle() {
    let r = evaluate(
        r#"
        const f = document.createElement('iframe');
        f.srcdoc = "<main id='root'></main>";
        document.body.appendChild(f);
        const cw = f.contentWindow;
        const node = cw.document.createElement('p');
        const before = {
            connected: node.isConnected,
            owner: node.ownerDocument === cw.document,
            rootIsSelf: node.getRootNode() === node,
            exactPrototype: Object.getPrototypeOf(node) === cw.HTMLParagraphElement.prototype,
        };
        cw.document.body.appendChild(node);
        const during = {
            connected: node.isConnected,
            rootIsDocument: node.getRootNode() === cw.document,
        };
        node.remove();
        const after = {
            connected: node.isConnected,
            rootIsSelf: node.getRootNode() === node,
        };
        JSON.stringify({before,during,after,defaultView:cw.document.defaultView===cw})
        "#,
    )
    .await;

    assert_eq!(
        r,
        r#"{"before":{"connected":false,"owner":true,"rootIsSelf":true,"exactPrototype":true},"during":{"connected":true,"rootIsDocument":true},"after":{"connected":false,"rootIsSelf":true},"defaultView":true}"#
    );
}

#[tokio::test]
async fn iframe_window_proxy_preserves_proxy_invariants() {
    let r = evaluate(
        r#"
        const f = document.createElement('iframe');
        f.srcdoc = '<div>child</div>';
        document.body.appendChild(f);
        const cw = f.contentWindow;
        let preventError = '';
        try { Object.preventExtensions(cw); } catch (e) { preventError = e.name; }
        const softDefined = Reflect.defineProperty(cw, 'softProperty', {
            value: 7, writable: true, enumerable: true, configurable: true,
        });
        const hardDefined = Reflect.defineProperty(cw, 'hardProperty', {
            value: 9, writable: true, enumerable: true, configurable: false,
        });
        cw.assignedProperty = 11;
        const assignedDesc = Object.getOwnPropertyDescriptor(cw, 'assignedProperty');
        const assignedOwnKey = Reflect.ownKeys(cw).includes('assignedProperty');
        const assignedDelete = delete cw.assignedProperty;
        const softDesc = Object.getOwnPropertyDescriptor(cw, 'softProperty');
        const hardDesc = Object.getOwnPropertyDescriptor(cw, 'hardProperty');
        JSON.stringify({
            extensible: Object.isExtensible(cw),
            preventError,
            softDefined,
            softValue: cw.softProperty,
            softConfigurable: softDesc ? softDesc.configurable : null,
            hardDefined,
            hardType: typeof cw.hardProperty,
            hardConfigurable: hardDesc ? hardDesc.configurable : null,
            ownKeysReadable: Reflect.ownKeys(cw).includes('softProperty'),
            assignedValue: assignedDesc ? assignedDesc.value : null,
            assignedConfigurable: assignedDesc ? assignedDesc.configurable : null,
            assignedOwnKey,
            assignedDelete,
            assignedAfterDelete: typeof cw.assignedProperty,
        })
        "#,
    )
    .await;

    println!("WINDOW_PROXY_SURFACE={r}");

    assert_eq!(
        r,
        r#"{"extensible":true,"preventError":"TypeError","softDefined":true,"softValue":7,"softConfigurable":true,"hardDefined":false,"hardType":"undefined","hardConfigurable":null,"ownKeysReadable":true,"assignedValue":11,"assignedConfigurable":true,"assignedOwnKey":true,"assignedDelete":true,"assignedAfterDelete":"undefined"}"#
    );
}
