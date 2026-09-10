#[cfg(test)]
mod tests {
    use browser_oxide::Page;

    #[tokio::test]
    #[ignore = "network: live HTTP against example.com"]
    async fn audit_js_surface() {
        let profile = browser_oxide::stealth::presets::chrome_148_macos();
        let mut page = Page::navigate("https://example.com/", profile, 1)
            .await
            .unwrap();

        let js = r#"
            (async () => {
                const res = {
                    screenWidth: typeof screen !== 'undefined' ? screen.width : 'no screen',
                    screenHeight: typeof screen !== 'undefined' ? screen.height : 'no screen',
                    availWidth: typeof screen !== 'undefined' ? screen.availWidth : 'no screen',
                    colorDepth: typeof screen !== 'undefined' ? screen.colorDepth : 'no screen',
                    plugins: navigator.plugins ? navigator.plugins.length : 'no plugins'
                };
                if (navigator.userAgentData && navigator.userAgentData.getHighEntropyValues) {
                    try {
                        const vals = await navigator.userAgentData.getHighEntropyValues(["formFactors"]);
                        res.formFactors = vals.formFactors;
                    } catch(e) {
                        res.formFactors = "err: " + e;
                    }
                } else {
                    res.formFactors = 'no uad';
                }
                globalThis.__surface_res = JSON.stringify(res, null, 2);
            })();
        "#;
        page.evaluate(js).unwrap();

        // The probe above awaits getHighEntropyValues(). Sleeping the Tokio
        // task does not drive BrowserOxide's V8 event loop, so the old
        // diagnostic could print `wait` and still pass. Pump the page runtime
        // the same way the non-network UAData regressions do.
        page.evaluate_async("void 0", std::time::Duration::from_millis(500))
            .await
            .unwrap();

        let r = page.evaluate("globalThis.__surface_res || 'wait'").unwrap();
        println!("JS SURFACE OXIDE:\n{}", r);
        assert_ne!(r, "wait", "async JS-surface probe never settled");

        let value: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert!(value["screenWidth"].as_i64().is_some_and(|v| v > 0));
        assert!(value["screenHeight"].as_i64().is_some_and(|v| v > 0));
        assert!(value["availWidth"].as_i64().is_some_and(|v| v > 0));
        assert!(value["colorDepth"].as_i64().is_some_and(|v| v > 0));
        assert!(value["plugins"].as_i64().is_some());
        assert!(value["formFactors"].is_array());
    }
}
