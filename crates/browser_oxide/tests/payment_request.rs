use browser_oxide::Page;

#[tokio::test]
async fn payment_request_matches_chrome_webidl_and_idle_abort_state() {
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
            const pr = new PaymentRequest(
                [{ supportedMethods: 'basic-card' }],
                { total: { label: 'x', amount: { currency: 'USD', value: '1.00' } } }
            );
            const desc = (obj, key) => {
                const d = Object.getOwnPropertyDescriptor(obj, key);
                return d && { enumerable:d.enumerable, configurable:d.configurable,
                    writable:'writable' in d ? d.writable : null,
                    getter:typeof d.get, setter:typeof d.set };
            };
            globalThis.__paymentRequestProbe = { pr };
            return {
                ctorLength: PaymentRequest.length,
                ctorKeys: Reflect.ownKeys(PaymentRequest).map(String),
                protoKeys: Reflect.ownKeys(PaymentRequest.prototype).map(String),
                ownKeys: Reflect.ownKeys(pr).map(String),
                tag: Object.prototype.toString.call(pr),
                idType: typeof pr.id,
                shipping: [pr.shippingAddress, pr.shippingOption, pr.shippingType],
                idDesc: desc(PaymentRequest.prototype, 'id'),
                showLength: PaymentRequest.prototype.show.length,
                abortLength: PaymentRequest.prototype.abort.length,
                canMakePaymentLength: PaymentRequest.prototype.canMakePayment.length,
                handlerBefore: pr.onpaymentmethodchange,
            };
        })())
    "#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["ctorLength"], 1);
    assert_eq!(
        value["ctorKeys"],
        serde_json::json!([
            "length",
            "name",
            "prototype",
            "securePaymentConfirmationAvailability"
        ])
    );
    assert_eq!(value["ownKeys"], serde_json::json!([]));
    assert_eq!(value["tag"], "[object PaymentRequest]");
    assert_eq!(value["idType"], "string");
    assert_eq!(value["shipping"], serde_json::json!([null, null, null]));
    assert_eq!(value["idDesc"]["enumerable"], true);
    assert_eq!(value["idDesc"]["configurable"], true);
    assert_eq!(value["idDesc"]["getter"], "function");
    assert_eq!(value["showLength"], 0);
    assert_eq!(value["abortLength"], 0);
    assert_eq!(value["canMakePaymentLength"], 0);
    assert!(value["handlerBefore"].is_null());
    assert_eq!(
        value["protoKeys"],
        serde_json::json!([
            "id",
            "shippingAddress",
            "shippingOption",
            "shippingType",
            "onshippingaddresschange",
            "onshippingoptionchange",
            "abort",
            "canMakePayment",
            "hasEnrolledInstrument",
            "show",
            "onpaymentmethodchange",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );

    page.evaluate(r#"
        (async () => {
            const pr = globalThis.__paymentRequestProbe.pr;
            const capture = p => p.then(v => ({ ok:true, value:String(v) }), e => ({ ok:false, name:e.name, message:e.message }));
            globalThis.__paymentRequestAsync = {
                basic: await capture(pr.canMakePayment()),
                abort: await capture(pr.abort()),
            };
        })()
    "#).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(100))
        .await
        .ok();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__paymentRequestAsync)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["basic"]["ok"], true);
    assert_eq!(value["basic"]["value"], "false");
    assert_eq!(value["abort"]["ok"], false);
    assert_eq!(value["abort"]["name"], "InvalidStateError");
}
