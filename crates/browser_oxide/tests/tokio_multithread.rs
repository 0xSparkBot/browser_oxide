use browser_oxide::js_runtime::BrowserJsRuntime;
use browser_oxide::Page;
use std::time::Duration;

fn dom() -> browser_oxide::dom::Dom {
    browser_oxide::html_parser::parse_html("<html><body></body></html>")
}

/// deno_unsync's async ops require a current-thread Tokio executor. BrowserOxide
/// is commonly embedded by applications using Tokio's default multi-thread
/// runtime, so public runtime calls must transparently route V8 async ops onto
/// the process-lifetime current-thread fallback instead of aborting the process.
#[test]
fn async_ops_work_inside_multithread_tokio_runtime() {
    let outer = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multi-thread tokio runtime");

    outer.block_on(async {
        assert_eq!(
            tokio::runtime::Handle::current().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread,
            "test precondition: BrowserOxide must be called from a multi-thread runtime"
        );

        {
            let mut browser = BrowserJsRuntime::new(dom());
            browser
                .execute_script(
                    "globalThis.__mtTimer = 'pending'; setTimeout(() => { globalThis.__mtTimer = 'done'; }, 10);",
                    None,
                )
                .expect("schedule timer");

            tokio::time::timeout(Duration::from_secs(1), browser.run_event_loop())
                .await
                .expect("BrowserOxide event loop must not be pinned or abort")
                .expect("BrowserOxide event loop");

            assert_eq!(
                browser
                    .execute_script("globalThis.__mtTimer", None)
                    .expect("read timer result"),
                "done"
            );
        }

        // Exercise the public async Page builder too. It performs the same V8
        // async-op scheduling that real navigation does, but stays fully local.
        {
            let html = r#"<!doctype html><html><body><script>
                globalThis.__pageTimer = 'pending';
                setTimeout(() => { globalThis.__pageTimer = 'done'; }, 10);
            </script></body></html>"#;
            let mut page = Page::from_html_with_url(
                html,
                "https://example.test/tokio-multithread",
                None,
            )
            .await
            .expect("build page from multi-thread runtime");
            assert_eq!(
                page.evaluate("globalThis.__pageTimer")
                    .expect("read page timer result"),
                "done"
            );
        }

        assert_eq!(
            tokio::runtime::Handle::current().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread,
            "fallback EnterGuard must restore the caller's outer runtime"
        );
    });
}
