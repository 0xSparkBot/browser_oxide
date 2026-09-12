use browser_oxide::js_runtime::{block_on_v8_thread, BrowserJsRuntime};
use browser_oxide::Page;

fn dom() -> browser_oxide::dom::Dom {
    browser_oxide::html_parser::parse_html("<html><body></body></html>")
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        return (*text).to_string();
    }
    "<non-string panic>".to_string()
}

/// deno_unsync's local async ops require a current-thread Tokio executor on the
/// same OS thread as the V8 isolate. Direct construction inside Tokio's default
/// multi-thread runtime must fail *before* V8/FFI can reach a non-unwinding
/// abort, while the dedicated `block_on_v8_thread` adapter remains safe for
/// multi-thread applications.
#[test]
fn multithread_tokio_requires_v8_thread_adapter() {
    let outer = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multi-thread tokio runtime");

    outer.block_on(async {
        assert_eq!(
            tokio::runtime::Handle::current().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread,
            "test precondition: call from a multi-thread runtime"
        );

        let direct = std::panic::catch_unwind(|| BrowserJsRuntime::new(dom()));
        let panic = match direct {
            Ok(_) => panic!("direct BrowserJsRuntime construction unexpectedly succeeded"),
            Err(payload) => panic_text(payload),
        };
        assert!(
            panic.contains("require a current-thread Tokio runtime")
                && panic.contains("block_on_v8_thread"),
            "unexpected fail-fast message: {panic}"
        );

        let result = block_on_v8_thread("tokio-multithread-adapter-test", || async {
            let html = r#"<!doctype html><html><body><script>
                globalThis.__pageTimer = 'pending';
                setTimeout(() => { globalThis.__pageTimer = 'done'; }, 10);
            </script></body></html>"#;
            let mut page =
                Page::from_html_with_url(html, "https://example.test/tokio-multithread", None)
                    .await
                    .expect("build page on dedicated V8 thread");
            page.evaluate("globalThis.__pageTimer")
                .expect("read page timer result")
        });
        assert_eq!(result, "done");

        assert_eq!(
            tokio::runtime::Handle::current().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread,
            "adapter must leave the caller's outer runtime intact"
        );
    });
}
