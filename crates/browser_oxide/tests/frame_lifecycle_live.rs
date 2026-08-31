//! Live network checks for frame-tree resource lifecycle.

use browser_oxide::Page;

#[ignore = "network: frame-tree lifecycle cleanup"]
#[tokio::test]
async fn frame_tree_prunes_removed_cross_origin_iframe() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://parent.example/frame-prune",
        Some(profile.clone()),
    )
    .await
    .unwrap();

    page.evaluate(
        "const f=document.createElement('iframe');f.id='audit-frame';f.src='https://example.com/';document.body.appendChild(f);",
    )
    .unwrap();
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);
    assert_eq!(
        page.evaluate("String(document.getElementById('audit-frame').contentDocument === null)")
            .unwrap(),
        "true",
        "cross-origin frame must not expose contentDocument"
    );

    page.evaluate(
        "globalThis.__oldFrameHandle=document.getElementById('audit-frame').contentWindow;document.getElementById('audit-frame').remove();",
    )
    .unwrap();
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(
        page.frame_tree_count(),
        0,
        "removed frame isolate must be pruned"
    );
    assert_eq!(
        page.evaluate("String(globalThis.__oldFrameHandle.closed)")
            .unwrap(),
        "true",
        "previous WindowProxy handle must become closed"
    );
}

#[ignore = "network: frame-tree iframe navigation"]
#[tokio::test]
async fn frame_tree_replaces_context_when_src_changes() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://parent.example/frame-nav",
        Some(profile.clone()),
    )
    .await
    .unwrap();

    page.evaluate(
        "const f=document.createElement('iframe');f.id='nav-frame';f.src='https://example.com/?first=1';document.body.appendChild(f);",
    )
    .unwrap();
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);

    let switch_state = page
        .evaluate(
            "JSON.stringify((()=>{const frame=document.getElementById('nav-frame');globalThis.__firstHandle=frame.contentWindow;globalThis.__oldFrameId=__firstHandle.__frameId;frame.src='https://example.com/?second=1';globalThis.__secondHandle=frame.contentWindow;return {oldId:__oldFrameId,newId:__secondHandle.__frameId,same:__firstHandle===__secondHandle,closed:__firstHandle.closed,attr:frame.getAttribute('src'),src:frame.src,map:globalThis.__frameIdForNode};})())",
        )
        .unwrap();
    assert_eq!(
        page.evaluate("String(__firstHandle === __secondHandle && __firstHandle.__frameId !== __oldFrameId && !__firstHandle.closed)")
            .unwrap(),
        "true",
        "src navigation must preserve WindowProxy identity while retargeting its backend: {switch_state}"
    );

    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(
        page.frame_tree_count(),
        1,
        "navigation must replace, not append, the frame"
    );
    let href = page
        .frame_tree_evaluate(0, "String(location.href)")
        .unwrap_or_default();
    assert!(
        href.contains("second=1"),
        "replacement frame URL mismatch: {href}"
    );
    assert_eq!(
        page.evaluate("String(__firstHandle === __secondHandle && !__secondHandle.closed)")
            .unwrap(),
        "true"
    );
}

#[ignore = "network: frame-tree switches between remote and srcdoc backends"]
#[tokio::test]
async fn frame_tree_switches_backends_without_replacing_window_proxy() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://parent.example/frame-backend-switch",
        Some(profile.clone()),
    )
    .await
    .unwrap();

    page.evaluate(
        "const f=document.createElement('iframe');f.id='switch-frame';f.src='https://example.com/?remote=1';document.body.appendChild(f);",
    )
    .unwrap();
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);

    assert_eq!(
        page.evaluate(
            r#"String((()=>{
                const f=document.getElementById('switch-frame');
                globalThis.__stableFrameWindow=f.contentWindow;
                f.setAttribute('srcdoc', '<div id="local-doc">local</div><script>globalThis.__localRealm=7<\/script>');
                return __stableFrameWindow===f.contentWindow
                    && f.contentWindow.__localRealm===7
                    && f.contentWindow.document.getElementById('local-doc').textContent==='local';
            })())"#,
        )
        .unwrap(),
        "true"
    );
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(
        page.frame_tree_count(),
        0,
        "remote isolate must be released after switching to srcdoc"
    );

    assert_eq!(
        page.evaluate(
            r#"String((()=>{
                const f=document.getElementById('switch-frame');
                f.removeAttribute('srcdoc');
                return __stableFrameWindow===f.contentWindow && !__stableFrameWindow.closed;
            })())"#,
        )
        .unwrap(),
        "true"
    );
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);
    assert_eq!(
        page.evaluate(
            "String(__stableFrameWindow===document.getElementById('switch-frame').contentWindow && document.getElementById('switch-frame').contentDocument===null)",
        )
        .unwrap(),
        "true"
    );
}

#[ignore = "network: duplicate remote navigation signal"]
#[tokio::test]
async fn duplicate_remote_navigation_restores_existing_mapping_without_reload() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://parent.example/duplicate-frame",
        Some(profile.clone()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"globalThis.__frameLoads=0;
        const f=document.createElement('iframe');
        f.id='duplicate-frame';
        f.addEventListener('load',()=>__frameLoads++);
        f.src='https://example.com/?duplicate=1';
        document.body.appendChild(f);"#,
    )
    .unwrap();
    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);

    let initial_id = page
        .evaluate("String(document.getElementById('duplicate-frame').contentWindow.__frameId)")
        .unwrap();
    assert_eq!(page.evaluate("String(__frameLoads)").unwrap(), "1");

    let pending_state = page
        .evaluate(
            r#"JSON.stringify((()=>{
                const f=document.getElementById('duplicate-frame');
                globalThis.__duplicateStableWindow=f.contentWindow;
                f.setAttribute('src','https://example.com/?duplicate=1');
                return {same:__duplicateStableWindow===f.contentWindow,pendingId:String(f.contentWindow.__frameId)};
            })())"#,
        )
        .unwrap();
    assert!(
        !pending_state.contains(&format!(r#""pendingId":"{initial_id}""#)),
        "same-URL assignment should allocate a pending id before deduplication: {pending_state}"
    );

    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 1);
    assert_eq!(
        page.evaluate(
            "String(__duplicateStableWindow===document.getElementById('duplicate-frame').contentWindow)"
        )
        .unwrap(),
        "true"
    );
    assert_eq!(
        page.evaluate("String(document.getElementById('duplicate-frame').contentWindow.__frameId)")
            .unwrap(),
        initial_id
    );
    assert_eq!(
        page.evaluate("String(__frameLoads)").unwrap(),
        "1",
        "deduplicating a pending navigation must not dispatch a second load"
    );
}

#[ignore = "network: failed remote frame materialization"]
#[tokio::test]
async fn failed_remote_materialization_clears_pending_mapping() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).unwrap();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://parent.example/failed-frame",
        Some(profile.clone()),
    )
    .await
    .unwrap();
    page.init_top_frame();
    page.evaluate(
        "const f=document.createElement('iframe');f.id='failed-frame';f.src='https://127.0.0.1:9/unreachable';document.body.appendChild(f);",
    )
    .unwrap();
    assert_eq!(
        page.evaluate(
            "String((()=>{const f=document.getElementById('failed-frame');globalThis.__failedFrameWindow=f.contentWindow;globalThis.__pendingFailedFrameId=f.contentWindow.__frameId;return __pendingFailedFrameId!==undefined;})())"
        )
        .unwrap(),
        "true"
    );

    page.drive_frame_tree(&client, &profile).await;
    assert_eq!(page.frame_tree_count(), 0);
    assert_eq!(
        page.evaluate(
            "String(__failedFrameWindow===document.getElementById('failed-frame').contentWindow && document.getElementById('failed-frame').contentWindow.__frameId===undefined)"
        )
        .unwrap(),
        "true",
        "a failed fetch must not leave a route to a disposed frame id"
    );
}
