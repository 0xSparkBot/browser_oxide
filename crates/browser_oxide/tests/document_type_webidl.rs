use browser_oxide::Page;

fn profile() -> browser_oxide::stealth::StealthProfile {
    browser_oxide::stealth::presets::chrome_148_macos()
}

#[tokio::test]
async fn document_type_and_dom_implementation_match_chrome_148() {
    let mut page = Page::from_html(
        "<!doctype html><html><head></head><body></body></html>",
        Some(profile()),
    )
    .await
    .unwrap();

    let result = page
        .evaluate(
            r#"JSON.stringify((()=>{
                const d = document.doctype;
                const clone = d.cloneNode();
                const impl = document.implementation;
                const created = impl.createDocumentType('root', 'PUB', 'SYS');
                const detached = impl.createHTMLDocument('abc');
                return {
                    protoKeys: Reflect.ownKeys(DocumentType.prototype).map(k => typeof k === 'symbol' ? String(k) : k),
                    protoParent: Object.getPrototypeOf(DocumentType.prototype) === Node.prototype,
                    ctorParent: Object.getPrototypeOf(DocumentType) === Node,
                    parsed: [Object.prototype.toString.call(d), d instanceof DocumentType, d.nodeType, d.nodeName, d.name, d.publicId, d.systemId, d.ownerDocument === document, d.parentNode === document, d.nextSibling === document.documentElement, Reflect.ownKeys(d).length, d === document.doctype],
                    clone: [Object.prototype.toString.call(clone), clone instanceof DocumentType, clone.nodeType, clone.name, clone.publicId, clone.systemId],
                    implShape: [Object.prototype.toString.call(impl), impl instanceof DOMImplementation, Reflect.ownKeys(impl).length, impl === document.implementation],
                    implKeys: Reflect.ownKeys(DOMImplementation.prototype).map(k => typeof k === 'symbol' ? String(k) : k),
                    created: [Object.prototype.toString.call(created), created instanceof DocumentType, created.nodeType, created.nodeName, created.name, created.publicId, created.systemId, created.ownerDocument === document, created.parentNode === null, Reflect.ownKeys(created).length],
                    detached: [Object.prototype.toString.call(detached), detached instanceof HTMLDocument, detached.title, detached.defaultView === null, detached.doctype && detached.doctype.name, detached.documentElement && detached.documentElement.tagName, detached.head && detached.head.tagName, detached.body && detached.body.tagName],
                };
            })())"#,
        )
        .unwrap();

    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(
        value["protoKeys"],
        serde_json::json!([
            "name",
            "publicId",
            "systemId",
            "after",
            "before",
            "remove",
            "replaceWith",
            "constructor",
            "Symbol(Symbol.toStringTag)",
            "Symbol(Symbol.unscopables)"
        ])
    );
    assert_eq!(value["protoParent"], true);
    assert_eq!(value["ctorParent"], true);
    assert_eq!(
        value["parsed"],
        serde_json::json!([
            "[object DocumentType]",
            true,
            10,
            "html",
            "html",
            "",
            "",
            true,
            true,
            true,
            0,
            true
        ])
    );
    assert_eq!(
        value["clone"],
        serde_json::json!(["[object DocumentType]", true, 10, "html", "", ""])
    );
    assert_eq!(
        value["implShape"],
        serde_json::json!(["[object DOMImplementation]", true, 0, true])
    );
    assert_eq!(
        value["implKeys"],
        serde_json::json!([
            "createDocument",
            "createDocumentType",
            "createHTMLDocument",
            "hasFeature",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["created"],
        serde_json::json!([
            "[object DocumentType]",
            true,
            10,
            "root",
            "root",
            "PUB",
            "SYS",
            true,
            true,
            0
        ])
    );
    assert_eq!(
        value["detached"],
        serde_json::json!([
            "[object HTMLDocument]",
            true,
            "abc",
            true,
            "html",
            "HTML",
            "HEAD",
            "BODY"
        ])
    );
}
