use browser_oxide::Page;

#[tokio::test]
async fn payment_related_interfaces_match_chrome_webidl() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/",
        Some(profile),
    )
    .await
    .unwrap();

    let raw = page
        .evaluate(
            r#"
        JSON.stringify((() => {
            const protoKeys = C => Reflect.ownKeys(C.prototype).map(String);
            const parent = C => Object.getPrototypeOf(C).name;
            const protoParent = C => Object.getPrototypeOf(C.prototype).constructor.name;
            const capture = fn => { try { return { ok:true, value:fn() }; } catch (e) { return { ok:false, name:e.name, message:e.message }; } };
            const method = new PaymentMethodChangeEvent('paymentmethodchange', { methodName:'x', methodDetails:{a:1} });
            const methodDefault = new PaymentMethodChangeEvent('paymentmethodchange');
            const update = new PaymentRequestUpdateEvent('shippingaddresschange');
            return {
                response: {
                    length: PaymentResponse.length,
                    keys: Reflect.ownKeys(PaymentResponse).map(String),
                    proto: protoKeys(PaymentResponse),
                    parent: parent(PaymentResponse),
                    protoParent: protoParent(PaymentResponse),
                    construct: capture(() => new PaymentResponse()),
                },
                method: {
                    length: PaymentMethodChangeEvent.length,
                    proto: protoKeys(PaymentMethodChangeEvent),
                    parent: parent(PaymentMethodChangeEvent),
                    protoParent: protoParent(PaymentMethodChangeEvent),
                    own: Reflect.ownKeys(method).map(String),
                    tag: Object.prototype.toString.call(method),
                    name: method.methodName,
                    details: method.methodDetails,
                    defaultName: methodDefault.methodName,
                    defaultDetails: methodDefault.methodDetails,
                    updateType: typeof method.updateWith,
                    staleUpdate: capture(() => method.updateWith(Promise.resolve({}))),
                },
                update: {
                    length: PaymentRequestUpdateEvent.length,
                    proto: protoKeys(PaymentRequestUpdateEvent),
                    parent: parent(PaymentRequestUpdateEvent),
                    protoParent: protoParent(PaymentRequestUpdateEvent),
                    own: Reflect.ownKeys(update).map(String),
                    tag: Object.prototype.toString.call(update),
                    staleUpdate: capture(() => update.updateWith(Promise.resolve({}))),
                },
            };
        })())
    "#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["response"]["length"], 0);
    assert_eq!(
        value["response"]["keys"],
        serde_json::json!(["length", "name", "prototype"])
    );
    assert_eq!(value["response"]["parent"], "EventTarget");
    assert_eq!(value["response"]["protoParent"], "EventTarget");
    assert_eq!(value["response"]["construct"]["ok"], false);
    assert_eq!(value["response"]["construct"]["name"], "TypeError");
    assert_eq!(
        value["response"]["proto"],
        serde_json::json!([
            "requestId",
            "methodName",
            "details",
            "shippingAddress",
            "shippingOption",
            "payerName",
            "payerEmail",
            "payerPhone",
            "onpayerdetailchange",
            "complete",
            "retry",
            "toJSON",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );

    assert_eq!(value["method"]["length"], 1);
    assert_eq!(value["method"]["parent"], "PaymentRequestUpdateEvent");
    assert_eq!(value["method"]["protoParent"], "PaymentRequestUpdateEvent");
    assert_eq!(
        value["method"]["proto"],
        serde_json::json!([
            "methodName",
            "methodDetails",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["method"]["own"], serde_json::json!(["isTrusted"]));
    assert_eq!(value["method"]["tag"], "[object PaymentMethodChangeEvent]");
    assert_eq!(value["method"]["name"], "x");
    assert_eq!(value["method"]["details"], serde_json::json!({"a":1}));
    assert_eq!(value["method"]["defaultName"], "");
    assert!(value["method"]["defaultDetails"].is_null());
    assert_eq!(value["method"]["updateType"], "function");
    assert_eq!(value["method"]["staleUpdate"]["ok"], false);
    assert_eq!(value["method"]["staleUpdate"]["name"], "InvalidStateError");

    assert_eq!(value["update"]["length"], 1);
    assert_eq!(value["update"]["parent"], "Event");
    assert_eq!(value["update"]["protoParent"], "Event");
    assert_eq!(
        value["update"]["proto"],
        serde_json::json!(["updateWith", "constructor", "Symbol(Symbol.toStringTag)"])
    );
    assert_eq!(value["update"]["own"], serde_json::json!(["isTrusted"]));
    assert_eq!(value["update"]["tag"], "[object PaymentRequestUpdateEvent]");
    assert_eq!(value["update"]["staleUpdate"]["ok"], false);
    assert_eq!(value["update"]["staleUpdate"]["name"], "InvalidStateError");
}
