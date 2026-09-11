use browser_oxide::Page;

fn profile() -> browser_oxide::stealth::StealthProfile {
    browser_oxide::stealth::presets::chrome_148_macos()
}

#[tokio::test]
async fn next_task_handler_prevents_unhandled_rejection() {
    let html = r#"<!doctype html><div id="out"></div><script>
        const rows=[];
        addEventListener('unhandledrejection', () => rows.push('unhandled'));
        const p=Promise.reject('boom');
        Promise.resolve().then(()=>rows.push('microtask'));
        setTimeout(()=>{p.catch(()=>rows.push('catch'));},0);
        setTimeout(()=>{document.getElementById('out').textContent=JSON.stringify(rows)},50);
    </script>"#;
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile()))
        .await
        .expect("browser page must survive a temporarily-unhandled rejection");
    assert_eq!(
        page.evaluate("document.getElementById('out').textContent")
            .unwrap(),
        r#"["microtask","catch"]"#
    );
}

#[tokio::test]
async fn unhandled_rejection_dispatches_trusted_cancelable_event() {
    let html = r#"<!doctype html><div id="out"></div><script>
        const rows=[];
        let p;
        addEventListener('unhandledrejection', e=>{
            rows.push(['unhandled',e.isTrusted,e.cancelable,e.bubbles,e.promise===p,String(e.reason),e.defaultPrevented]);
            e.preventDefault();
            rows.push(['afterPrevent',e.defaultPrevented]);
        });
        p=Promise.reject('boom');
        Promise.resolve().then(()=>rows.push(['micro']));
        setTimeout(()=>{document.getElementById('out').textContent=JSON.stringify(rows)},50);
    </script>"#;
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile()))
        .await
        .expect("unhandled rejection is a browser event, not a fatal runtime error");
    assert_eq!(
        page.evaluate("document.getElementById('out').textContent")
            .unwrap(),
        r#"[["micro"],["unhandled",true,true,false,true,"boom",false],["afterPrevent",true]]"#
    );
}

#[tokio::test]
async fn late_handler_dispatches_rejectionhandled_after_catch() {
    let html = r#"<!doctype html><div id="out"></div><script>
        const rows=[];
        let p;
        addEventListener('unhandledrejection', e=>{
            rows.push(['unhandled',e.isTrusted,e.cancelable,e.promise===p,String(e.reason)]);
            e.preventDefault();
        });
        addEventListener('rejectionhandled', e=>{
            rows.push(['handled',e.isTrusted,e.cancelable,e.promise===p,String(e.reason)]);
        });
        p=Promise.reject('boom');
        Promise.resolve().then(()=>rows.push(['micro']));
        setTimeout(()=>{p.catch(()=>rows.push(['catch']));},50);
        setTimeout(()=>{document.getElementById('out').textContent=JSON.stringify(rows)},150);
    </script>"#;
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile()))
        .await
        .expect("page");
    assert_eq!(
        page.evaluate("document.getElementById('out').textContent")
            .unwrap(),
        r#"[["micro"],["unhandled",true,true,true,"boom"],["catch"],["handled",true,false,true,"boom"]]"#
    );
}

#[tokio::test]
async fn unhandled_rejection_task_order_matches_chrome() {
    let html = r#"<!doctype html><div id="out"></div><script>
        const rows=[];
        addEventListener('unhandledrejection', event=>{
            rows.push('unhandled');
            event.preventDefault();
        });
        Promise.reject('boom');
        Promise.resolve().then(()=>rows.push('microtask'));
        scheduler.postTask(()=>rows.push('postTask'));
        const channel=new MessageChannel();
        channel.port1.onmessage=()=>rows.push('messageChannel');
        channel.port2.postMessage('x');
        setTimeout(()=>rows.push('timeout0'),0);
        requestAnimationFrame(()=>rows.push('raf'));
        setTimeout(()=>{document.getElementById('out').textContent=JSON.stringify(rows)},100);
    </script>"#;
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile()))
        .await
        .expect("page");
    assert_eq!(
        page.evaluate("document.getElementById('out').textContent")
            .unwrap(),
        r#"["microtask","postTask","messageChannel","timeout0","unhandled","raf"]"#
    );
}
