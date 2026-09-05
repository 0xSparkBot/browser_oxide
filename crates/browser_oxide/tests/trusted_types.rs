use browser_oxide::js_runtime::BrowserJsRuntime;

fn runtime() -> BrowserJsRuntime {
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    BrowserJsRuntime::new(dom)
}

#[tokio::test]
async fn trusted_script_executes_via_direct_eval_in_lexical_scope() {
    let mut runtime = runtime();
    let result = runtime
        .execute_script(
            r#"
            (() => {
                const policy = trustedTypes.createPolicy('direct-eval-test', {
                    createScript(source) {
                        return source.replace('fortyOne', 'localValue');
                    }
                });
                const localValue = 41;
                const script = policy.createScript('fortyOne + 1');
                return JSON.stringify({
                    result: eval(script),
                    recognized: trustedTypes.isScript(script)
                });
            })()
            "#,
            None,
        )
        .unwrap();

    assert_eq!(result, r#"{"result":42,"recognized":true}"#);
}
