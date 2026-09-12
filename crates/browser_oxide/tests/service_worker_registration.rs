use browser_oxide::stealth::presets::chrome_148_macos;
use browser_oxide::Page;
use serde_json::Value;
use std::time::{Duration, Instant};

async fn pump_until(page: &mut Page, expression: &str, timeout: Duration) -> String {
    let started = Instant::now();
    loop {
        let value = page.evaluate(expression).unwrap_or_default();
        if !value.is_empty() && value != "undefined" && value != "null" {
            return value;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out waiting for {expression}"
        );
        let _ = page
            .event_loop()
            .run_until_idle(Duration::from_millis(50))
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn service_worker_registration_lifecycle_matches_chrome_basics() {
    let html = "<!doctype html><html><body></body></html>";
    let mut page = Page::from_html_with_url(
        html,
        "https://sw-lifecycle.example/app/page.html",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();

    page.evaluate(
        r#"
        globalThis.__swLifecycle = '';
        (async () => {
            try {
                const container = navigator.serviceWorker;
                const ready0 = container.ready;
                const reg = await container.register('/sw.js', { scope: '/app/' });
                const immediate = {
                    tag: Object.prototype.toString.call(reg),
                    own: Reflect.ownKeys(reg).map(String),
                    scope: reg.scope,
                    updateViaCache: reg.updateViaCache,
                    installing: !!reg.installing,
                    waiting: !!reg.waiting,
                    active: !!reg.active,
                    readyStable: ready0 === container.ready,
                };
                const ready = await ready0;
                const inScope = await container.getRegistration('/app/child');
                const outside = await container.getRegistration('/outside');
                const all = await container.getRegistrations();
                const worker = reg.active;
                const beforeUnregister = {
                    immediate,
                    readySame: ready === reg,
                    querySame: inScope === reg,
                    outsideMissing: outside === undefined,
                    listLength: all.length,
                    listSame: all[0] === reg,
                    activeStable: worker === reg.active,
                    workerTag: Object.prototype.toString.call(worker),
                    workerState: worker && worker.state,
                    workerURL: worker && worker.scriptURL,
                    updateSame: (await reg.update()) === reg,
                };
                const unregister = await reg.unregister();
                globalThis.__swLifecycle = JSON.stringify({
                    ...beforeUnregister,
                    unregister,
                    missingAfter: (await container.getRegistration('/app/child')) === undefined,
                    listAfter: (await container.getRegistrations()).length,
                });
            } catch (error) {
                globalThis.__swLifecycle = JSON.stringify({ error: String(error && error.stack || error) });
            }
        })();
        "#,
    )
    .unwrap();

    let raw = pump_until(
        &mut page,
        "globalThis.__swLifecycle",
        Duration::from_secs(3),
    )
    .await;
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid ServiceWorker JSON: {error}; raw={raw}"));
    assert_eq!(value["error"], Value::Null, "{raw}");
    assert_eq!(
        value["immediate"]["tag"],
        "[object ServiceWorkerRegistration]"
    );
    assert_eq!(value["immediate"]["own"], serde_json::json!([]));
    assert_eq!(
        value["immediate"]["scope"],
        "https://sw-lifecycle.example/app/"
    );
    assert_eq!(value["immediate"]["updateViaCache"], "imports");
    assert_eq!(value["immediate"]["installing"], true, "{raw}");
    assert_eq!(value["immediate"]["waiting"], false, "{raw}");
    assert_eq!(value["immediate"]["active"], false, "{raw}");
    assert_eq!(value["immediate"]["readyStable"], true, "{raw}");
    assert_eq!(value["readySame"], true, "{raw}");
    assert_eq!(value["querySame"], true, "{raw}");
    assert_eq!(value["outsideMissing"], true, "{raw}");
    assert_eq!(value["listLength"], 1, "{raw}");
    assert_eq!(value["listSame"], true, "{raw}");
    assert_eq!(value["activeStable"], true, "{raw}");
    assert_eq!(value["workerTag"], "[object ServiceWorker]", "{raw}");
    assert_eq!(value["workerState"], "activated", "{raw}");
    assert_eq!(
        value["workerURL"], "https://sw-lifecycle.example/sw.js",
        "{raw}"
    );
    assert_eq!(value["updateSame"], true, "{raw}");
    assert_eq!(value["unregister"], true, "{raw}");
    assert_eq!(value["missingAfter"], true, "{raw}");
    assert_eq!(value["listAfter"], 0, "{raw}");
}

#[tokio::test]
async fn service_worker_registrations_are_shared_by_origin_and_isolated_cross_origin() {
    let html = "<!doctype html><html><body></body></html>";
    let profile = chrome_148_macos();
    let mut first = Page::from_html_with_url(
        html,
        "https://sw-shared.example/app/one",
        Some(profile.clone()),
    )
    .await
    .unwrap();
    first
        .evaluate(
            r#"
            globalThis.__registered = '';
            navigator.serviceWorker.register('/sw.js', { scope: '/app/' }).then(reg => {
                globalThis.__registered = reg.scope;
            }, error => {
                globalThis.__registered = 'ERR:' + error;
            });
            "#,
        )
        .unwrap();
    assert_eq!(
        pump_until(
            &mut first,
            "globalThis.__registered",
            Duration::from_secs(2)
        )
        .await,
        "https://sw-shared.example/app/"
    );

    let mut second = Page::from_html_with_url(
        html,
        "https://sw-shared.example/app/two",
        Some(profile.clone()),
    )
    .await
    .unwrap();
    second
        .evaluate(
            r#"
            globalThis.__sharedQuery = '';
            Promise.all([
                navigator.serviceWorker.getRegistration('/app/child'),
                navigator.serviceWorker.getRegistrations(),
                navigator.serviceWorker.ready,
            ]).then(([reg, regs, ready]) => {
                globalThis.__sharedQuery = JSON.stringify({
                    found: !!reg,
                    scope: reg && reg.scope,
                    listLength: regs.length,
                    readyScope: ready.scope,
                    readySame: ready === reg,
                });
            }, error => {
                globalThis.__sharedQuery = JSON.stringify({ error: String(error) });
            });
            "#,
        )
        .unwrap();
    let shared_raw = pump_until(
        &mut second,
        "globalThis.__sharedQuery",
        Duration::from_secs(2),
    )
    .await;
    let shared: Value = serde_json::from_str(&shared_raw).unwrap();
    assert_eq!(shared["error"], Value::Null, "{shared_raw}");
    assert_eq!(shared["found"], true, "{shared_raw}");
    assert_eq!(shared["scope"], "https://sw-shared.example/app/");
    assert_eq!(shared["listLength"], 1, "{shared_raw}");
    assert_eq!(shared["readyScope"], "https://sw-shared.example/app/");
    assert_eq!(shared["readySame"], true, "{shared_raw}");

    let mut other =
        Page::from_html_with_url(html, "https://sw-other.example/app/page", Some(profile))
            .await
            .unwrap();
    other
        .evaluate(
            r#"
            globalThis.__otherQuery = '';
            Promise.all([
                navigator.serviceWorker.getRegistration('/app/child'),
                navigator.serviceWorker.getRegistrations(),
            ]).then(([reg, regs]) => {
                globalThis.__otherQuery = JSON.stringify({ missing: reg === undefined, len: regs.length });
            });
            "#,
        )
        .unwrap();
    let other_raw = pump_until(
        &mut other,
        "globalThis.__otherQuery",
        Duration::from_secs(2),
    )
    .await;
    let other_value: Value = serde_json::from_str(&other_raw).unwrap();
    assert_eq!(other_value["missing"], true, "{other_raw}");
    assert_eq!(other_value["len"], 0, "{other_raw}");

    // Cleanup the process-wide registry so this test does not leak into later
    // tests that reuse the same origin string.
    first
        .evaluate(
            r#"
            navigator.serviceWorker.getRegistration('/app/child').then(reg => reg && reg.unregister());
            "#,
        )
        .unwrap();
}

#[tokio::test]
async fn service_worker_webidl_surface_matches_chrome_148() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://sw-surface.example/app/page",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();
    let actual = page
        .evaluate(
            r#"
            (() => {
                const key = k => typeof k === 'symbol' ? '@@' + String(k.description) : String(k);
                const shape = proto => Reflect.ownKeys(proto).map(k => {
                    const d = Object.getOwnPropertyDescriptor(proto, k);
                    return [key(k), d.enumerable, d.configurable,
                        'writable' in d ? d.writable : null,
                        !!d.get, !!d.set,
                        typeof d.value === 'function' ? d.value.length : null];
                });
                return JSON.stringify({
                    container: shape(ServiceWorkerContainer.prototype),
                    registration: shape(ServiceWorkerRegistration.prototype),
                    worker: shape(ServiceWorker.prototype),
                    containerOwn: Reflect.ownKeys(navigator.serviceWorker).map(key),
                    containerTag: Object.prototype.toString.call(navigator.serviceWorker),
                });
            })()
            "#,
        )
        .unwrap();
    let value: Value = serde_json::from_str(&actual).unwrap();
    let names = |name: &str| -> Vec<String> {
        value[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry[0].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        names("container"),
        vec![
            "controller",
            "ready",
            "oncontrollerchange",
            "onmessage",
            "onmessageerror",
            "getRegistration",
            "getRegistrations",
            "register",
            "startMessages",
            "constructor",
            "@@Symbol.toStringTag",
        ]
    );
    assert_eq!(
        names("registration"),
        vec![
            "installing",
            "waiting",
            "active",
            "navigationPreload",
            "scope",
            "updateViaCache",
            "onupdatefound",
            "unregister",
            "update",
            "paymentManager",
            "constructor",
            "backgroundFetch",
            "periodicSync",
            "sync",
            "cookies",
            "pushManager",
            "getNotifications",
            "showNotification",
            "@@Symbol.toStringTag",
        ]
    );
    assert_eq!(
        names("worker"),
        vec![
            "scriptURL",
            "state",
            "onstatechange",
            "postMessage",
            "constructor",
            "onerror",
            "@@Symbol.toStringTag",
        ]
    );
    assert_eq!(value["containerOwn"], serde_json::json!([]));
    assert_eq!(value["containerTag"], "[object ServiceWorkerContainer]");

    let find = |group: &str, member: &str| -> &Value {
        value[group]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry[0] == member)
            .unwrap()
    };
    assert_eq!(find("container", "getRegistration")[6], 0);
    assert_eq!(find("container", "register")[6], 1);
    assert_eq!(find("registration", "showNotification")[6], 1);
    assert_eq!(find("worker", "postMessage")[6], 1);
}
