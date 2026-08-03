//! Regression coverage for `Node.isConnected` across a shadow-root boundary.

use browser_oxide::js_runtime::BrowserJsRuntime;

#[test]
fn shadow_tree_nodes_follow_host_connectivity() {
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    let mut runtime = BrowserJsRuntime::new(dom);

    let result = runtime
        .execute_script(
            r#"(function () {
                const host = document.createElement('div');
                const shadow = host.attachShadow({ mode: 'closed' });
                const iframe = document.createElement('iframe');
                shadow.appendChild(iframe);
                document.body.appendChild(host);
                const whileAttached = [
                    host.isConnected,
                    shadow.isConnected,
                    iframe.isConnected,
                    shadow instanceof ShadowRoot,
                    shadow instanceof DocumentFragment,
                ];
                host.remove();
                const afterRemoval = [host.isConnected, shadow.isConnected, iframe.isConnected];
                return JSON.stringify({ whileAttached, afterRemoval });
            })()"#,
            None,
        )
        .expect("shadow connectivity script should execute");

    assert_eq!(
        result,
        r#"{"whileAttached":[true,true,true,true,true],"afterRemoval":[false,false,false]}"#
    );
}
