use browser_oxide::Page;

#[tokio::test]
async fn dom_exception_matches_chrome_legacy_webidl_shape() {
    let mut page = Page::from_html("<!doctype html><html><body></body></html>", None)
        .await
        .expect("page");

    let result = page
        .evaluate(
            r#"JSON.stringify((() => {
                const names = [
                    ['IndexSizeError', 1], ['DOMStringSizeError', 2],
                    ['HierarchyRequestError', 3], ['WrongDocumentError', 4],
                    ['InvalidCharacterError', 5], ['NoDataAllowedError', 6],
                    ['NoModificationAllowedError', 7], ['NotFoundError', 8],
                    ['NotSupportedError', 9], ['InUseAttributeError', 10],
                    ['InvalidStateError', 11], ['SyntaxError', 12],
                    ['InvalidModificationError', 13], ['NamespaceError', 14],
                    ['InvalidAccessError', 15], ['ValidationError', 16],
                    ['TypeMismatchError', 17], ['SecurityError', 18],
                    ['NetworkError', 19], ['AbortError', 20],
                    ['URLMismatchError', 21], ['QuotaExceededError', 22],
                    ['TimeoutError', 23], ['InvalidNodeTypeError', 24],
                    ['DataCloneError', 25], ['OperationError', 0]
                ];
                const constants = [
                    'INDEX_SIZE_ERR','DOMSTRING_SIZE_ERR','HIERARCHY_REQUEST_ERR',
                    'WRONG_DOCUMENT_ERR','INVALID_CHARACTER_ERR','NO_DATA_ALLOWED_ERR',
                    'NO_MODIFICATION_ALLOWED_ERR','NOT_FOUND_ERR','NOT_SUPPORTED_ERR',
                    'INUSE_ATTRIBUTE_ERR','INVALID_STATE_ERR','SYNTAX_ERR',
                    'INVALID_MODIFICATION_ERR','NAMESPACE_ERR','INVALID_ACCESS_ERR',
                    'VALIDATION_ERR','TYPE_MISMATCH_ERR','SECURITY_ERR','NETWORK_ERR',
                    'ABORT_ERR','URL_MISMATCH_ERR','QUOTA_EXCEEDED_ERR','TIMEOUT_ERR',
                    'INVALID_NODE_TYPE_ERR','DATA_CLONE_ERR'
                ];
                const e = new DOMException('x', 'NotSupportedError');
                let callError = null;
                try { DOMException(); } catch (err) { callError = [err.name, err.message]; }
                let illegalGetter = null;
                try {
                    Object.getOwnPropertyDescriptor(DOMException.prototype, 'code').get.call({});
                } catch (err) { illegalGetter = [err.name, err.message]; }
                let createEventError = null;
                try { document.createEvent('InputEvent'); }
                catch (err) { createEventError = [err.name, err.code]; }
                return {
                    codes: names.map(([name, expected]) => [name, new DOMException('x', name).code, expected]),
                    own: Reflect.ownKeys(e).map(String),
                    brand: Object.prototype.toString.call(e),
                    errorInstance: e instanceof Error,
                    stackType: typeof e.stack,
                    length: DOMException.length,
                    ctorParent: Object.getPrototypeOf(DOMException) === Function.prototype,
                    protoParent: Object.getPrototypeOf(DOMException.prototype) === Error.prototype,
                    protoKeys: Reflect.ownKeys(DOMException.prototype).map(String),
                    constants: constants.map((name) => {
                        const cd = Object.getOwnPropertyDescriptor(DOMException, name);
                        const pd = Object.getOwnPropertyDescriptor(DOMException.prototype, name);
                        return [name, DOMException[name], DOMException.prototype[name], cd.enumerable,
                            cd.writable, cd.configurable, pd.enumerable, pd.writable, pd.configurable];
                    }),
                    accessors: ['code','name','message'].map((name) => {
                        const d = Object.getOwnPropertyDescriptor(DOMException.prototype, name);
                        return [name, d.enumerable, d.configurable, typeof d.get, d.set];
                    }),
                    values: [e.name, e.message, e.code],
                    callError,
                    illegalGetter,
                    createEventError,
                };
            })())"#,
        )
        .expect("DOMException probe");

    let value: serde_json::Value = serde_json::from_str(&result).expect("probe JSON");
    assert_eq!(value["own"], serde_json::json!([]));
    assert_eq!(value["brand"], "[object DOMException]");
    assert_eq!(value["errorInstance"], true);
    assert_eq!(value["stackType"], "undefined");
    assert_eq!(value["length"], 0);
    assert_eq!(value["ctorParent"], true);
    assert_eq!(value["protoParent"], true);
    assert_eq!(
        value["values"],
        serde_json::json!(["NotSupportedError", "x", 9])
    );
    assert_eq!(
        value["callError"],
        serde_json::json!([
            "TypeError",
            "Failed to construct 'DOMException': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
        ])
    );
    assert_eq!(
        value["illegalGetter"],
        serde_json::json!(["TypeError", "Illegal invocation"])
    );
    assert_eq!(
        value["createEventError"],
        serde_json::json!(["NotSupportedError", 9])
    );

    for row in value["codes"].as_array().expect("codes") {
        assert_eq!(row[1], row[2], "wrong legacy code for {}", row[0]);
    }
    for row in value["constants"].as_array().expect("constants") {
        assert_eq!(row[1], row[2]);
        assert_eq!(row[3], true);
        assert_eq!(row[4], false);
        assert_eq!(row[5], false);
        assert_eq!(row[6], true);
        assert_eq!(row[7], false);
        assert_eq!(row[8], false);
    }
    for row in value["accessors"].as_array().expect("accessors") {
        assert_eq!(row[1], true);
        assert_eq!(row[2], true);
        assert_eq!(row[3], "function");
        assert!(row[4].is_null());
    }
}
