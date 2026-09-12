#[cfg(test)]
mod tests {
    use browser_oxide::Page;

    #[tokio::test]
    #[ignore = "network: live HTTP against abrahamjuliot.github.io/creepjs"]
    async fn test_creepjs_oxide() {
        let profile = browser_oxide::stealth::presets::chrome_148_macos();
        let init = r#"
            globalThis.__oxideAuditErrors = [];
            addEventListener('error', event => {
                __oxideAuditErrors.push({
                    type: 'error',
                    message: String(event.message || ''),
                    filename: String(event.filename || ''),
                    lineno: event.lineno || 0,
                    colno: event.colno || 0,
                    stack: event.error && event.error.stack ? String(event.error.stack) : ''
                });
            });
            addEventListener('unhandledrejection', event => {
                const reason = event.reason;
                __oxideAuditErrors.push({
                    type: 'unhandledrejection',
                    message: reason && reason.message ? String(reason.message) : String(reason),
                    stack: reason && reason.stack ? String(reason.stack) : ''
                });
            });
        "#;
        let mut page = Page::navigate_with_init(
            "https://abrahamjuliot.github.io/creepjs/",
            profile,
            5,
            vec![init.to_string()],
        )
        .await
        .unwrap();

        // Wait for CreepJS to compute while actually driving BrowserOxide's
        // V8 event loop. Sleeping only the Tokio task leaves page timers,
        // promises, workers, and other async browser work unpolled, which can
        // make a healthy page look permanently stuck at "Computing...".
        for _ in 0..80 {
            let _ = page
                .evaluate_async(
                    "new Promise(resolve => setTimeout(resolve, 250))",
                    std::time::Duration::from_secs(1),
                )
                .await;
            let status = page
                .evaluate(
                    "document.querySelector('.fingerprint-header-container')?.innerText || ''",
                )
                .unwrap_or_default();
            if !status.is_empty() && !status.contains("Computing...") {
                break;
            }
        }

        let js = r#"
            (() => {
                const lies = [...document.querySelectorAll('.lies')].map(el => el.innerText);
                const status = document.querySelector('.fingerprint-header-container')?.innerText;
                const prediction = document.querySelector('.fuzzy-signature')?.innerText;
                return JSON.stringify({ status, prediction, lies }, null, 2);
            })()
        "#;

        let r = page
            .evaluate(js)
            .unwrap_or_else(|e| format!("ERROR: {}", e));
        println!("CREEPJS OXIDE:\n{}", r);

        let inner_text = page
            .evaluate("document.body.innerText.substring(0, 1000)")
            .unwrap_or_default();
        println!("CREEPJS TEXT: {}", inner_text);

        let state = page
            .evaluate(
                r#"JSON.stringify({
                    readyState: document.readyState,
                    scripts: Array.from(document.scripts).map(s => ({
                        src: s.src,
                        type: s.type,
                        async: s.async,
                        defer: s.defer,
                        textLen: (s.textContent || '').length
                    })),
                    resources: performance.getEntriesByType('resource').map(e => e.name),
                    bodyLen: document.body ? document.body.innerHTML.length : -1
                }, null, 2)"#,
            )
            .unwrap_or_default();
        println!("CREEPJS STATE:\n{}", state);

        let errors = page
            .evaluate("JSON.stringify(globalThis.__oxideAuditErrors || [], null, 2)")
            .unwrap_or_default();
        println!("CREEPJS ERRORS:\n{}", errors);

        let lie_detail = page
            .evaluate(
                r#"JSON.stringify(
                    globalThis.Fingerprint && globalThis.Fingerprint.lies || null,
                    null,
                    2
                )"#,
            )
            .unwrap_or_default();
        println!("CREEPJS LIE DETAIL:\n{}", lie_detail);

        let executed = page.executed_scripts();
        println!("CREEPJS EXECUTED SCRIPT COUNT: {}", executed.len());
        for (index, (name, code)) in executed.iter().enumerate() {
            println!(
                "CREEPJS EXECUTED[{index}]: name={name:?} bytes={}",
                code.len()
            );
        }
    }
}
