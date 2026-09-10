use browser_oxide::Page;

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_executes_blob_url_module_source() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="module">
            const source = new Blob(['export default 42;'], { type: 'text/javascript' });
            const url = URL.createObjectURL(source);
            try {
                const namespace = await import(url);
                globalThis.__blobModuleResult = String(namespace.default);
            } catch (error) {
                globalThis.__blobModuleResult = 'ERR:' + error.name + ':' + error.message;
            } finally {
                URL.revokeObjectURL(url);
            }
        </script>
    </body></html>"#;

    let mut page =
        Page::from_html_with_url(html, "https://example.test/blob-module/", Some(profile))
            .await
            .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__blobModuleResult")
            .expect("blob module result"),
        "42"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_rejects_revoked_blob_url() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="module">
            const source = new Blob(['export default 7;'], { type: 'text/javascript' });
            const url = URL.createObjectURL(source);
            URL.revokeObjectURL(url);
            try {
                await import(url);
                globalThis.__revokedBlobImport = 'resolved';
            } catch (_) {
                globalThis.__revokedBlobImport = 'rejected';
            }
        </script>
    </body></html>"#;

    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/revoked-blob-module/",
        Some(profile),
    )
    .await
    .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__revokedBlobImport")
            .expect("revoked blob module result"),
        "rejected"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_uses_document_import_map() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="importmap">
            {"imports":{"mapped-dynamic":"data:text/javascript,export default 99"}}
        </script>
        <script type="module">
            const namespace = await import('mapped-dynamic');
            globalThis.__mappedDynamicImport = String(namespace.default);
        </script>
    </body></html>"#;

    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/dynamic-import-map/",
        Some(profile),
    )
    .await
    .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__mappedDynamicImport")
            .expect("mapped dynamic import result"),
        "99"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_import_rejects_unsupported_module_scheme() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let html = r#"<!doctype html><html><body>
        <script type="module">
            try {
                await import('about:blank');
                globalThis.__unsupportedModuleImport = 'resolved';
            } catch (_) {
                globalThis.__unsupportedModuleImport = 'rejected';
            }
        </script>
    </body></html>"#;

    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/unsupported-module/",
        Some(profile),
    )
    .await
    .expect("page");

    assert_eq!(
        page.evaluate("globalThis.__unsupportedModuleImport")
            .expect("unsupported module result"),
        "rejected"
    );
}
