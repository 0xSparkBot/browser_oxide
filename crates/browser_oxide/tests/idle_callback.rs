use browser_oxide::Page;
use std::time::Duration;

fn profile() -> browser_oxide::stealth::StealthProfile {
    browser_oxide::stealth::presets::chrome_148_macos()
}

#[tokio::test(flavor = "current_thread")]
async fn idle_deadline_matches_chrome_webidl_shape_and_budget() {
    let mut page = Page::from_html("<!doctype html><body></body>", Some(profile()))
        .await
        .unwrap();

    let surface = page
        .evaluate(
            r#"JSON.stringify({
                keys: Reflect.ownKeys(IdleDeadline.prototype).map(String),
                prototypeWritable: Object.getOwnPropertyDescriptor(IdleDeadline, 'prototype').writable,
                length: IdleDeadline.length,
                source: Function.prototype.toString.call(IdleDeadline),
                construct: (() => { try { new IdleDeadline(); return 'ok'; } catch (e) { return e.name + ': ' + e.message; } })(),
                noArg: (() => { try { requestIdleCallback(); return 'ok'; } catch (e) { return e.name + ': ' + e.message; } })(),
                nonFunction: (() => { try { requestIdleCallback(1); return 'ok'; } catch (e) { return e.name + ': ' + e.message; } })(),
                timeIllegal: (() => { try { IdleDeadline.prototype.timeRemaining.call({}); return 'ok'; } catch (e) { return e.name + ': ' + e.message; } })(),
                getterIllegal: (() => { try { Object.getOwnPropertyDescriptor(IdleDeadline.prototype, 'didTimeout').get.call({}); return 'ok'; } catch (e) { return e.name + ': ' + e.message; } })(),
            })"#,
        )
        .unwrap();
    assert_eq!(
        surface,
        r#"{"keys":["didTimeout","timeRemaining","constructor","Symbol(Symbol.toStringTag)"],"prototypeWritable":false,"length":0,"source":"function IdleDeadline() { [native code] }","construct":"TypeError: Failed to construct 'IdleDeadline': Illegal constructor","noArg":"TypeError: Failed to execute 'requestIdleCallback' on 'Window': 1 argument required, but only 0 present.","nonFunction":"TypeError: Failed to execute 'requestIdleCallback' on 'Window': parameter 1 is not of type 'Function'.","timeIllegal":"TypeError: Illegal invocation","getterIllegal":"TypeError: Illegal invocation"}"#
    );

    page.evaluate_async(
        r#"requestIdleCallback(deadline => {
            const first = deadline.timeRemaining();
            const start = performance.now();
            while (performance.now() - start < 5) {}
            const second = deadline.timeRemaining();
            globalThis.__idleResult = JSON.stringify({
                brand: Object.prototype.toString.call(deadline),
                ctor: deadline.constructor && deadline.constructor.name,
                own: Reflect.ownKeys(deadline).map(String),
                instance: deadline instanceof IdleDeadline,
                didTimeout: deadline.didTimeout,
                first,
                second,
            });
        })"#,
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&page.evaluate("globalThis.__idleResult || 'null'").unwrap()).unwrap();
    assert_eq!(value["brand"], "[object IdleDeadline]");
    assert_eq!(value["ctor"], "IdleDeadline");
    assert_eq!(value["own"], serde_json::json!([]));
    assert_eq!(value["instance"], true);
    assert_eq!(value["didTimeout"], false);
    let first = value["first"].as_f64().unwrap();
    let second = value["second"].as_f64().unwrap();
    assert!((0.0..=50.0).contains(&first));
    assert!((0.0..=first).contains(&second));
}

#[tokio::test(flavor = "current_thread")]
async fn idle_callback_cancel_and_cross_realm_deadline_match_chrome() {
    let mut page = Page::from_html(
        r#"<!doctype html><body><iframe srcdoc="<p>child</p>"></iframe></body>"#,
        Some(profile()),
    )
    .await
    .unwrap();
    page.evaluate("void document.querySelector('iframe').contentWindow")
        .unwrap();

    page.evaluate_async(
        r#"(() => {
            const cw = document.querySelector('iframe').contentWindow;
            const cancelled = requestIdleCallback(() => { globalThis.__idleCancelled = true; });
            cancelIdleCallback(cancelled);
            globalThis.__idleCancelled = false;

            cw.requestIdleCallback(deadline => {
                globalThis.__parentCallbackDeadline = JSON.stringify({
                    idleType: typeof cw.IdleDeadline,
                    child: deadline instanceof cw.IdleDeadline,
                    parent: deadline instanceof IdleDeadline,
                    proto: Object.getPrototypeOf(deadline) === IdleDeadline.prototype,
                    sameCtor: cw.IdleDeadline === IdleDeadline,
                    construct: (() => {
                        try { new cw.IdleDeadline(); return 'ok'; }
                        catch (e) { return e.name + ': ' + e.message; }
                    })(),
                });
            });

            const childCallback = cw.Function('deadline', `
                parent.__childCallbackDeadline = JSON.stringify({
                    child: deadline instanceof IdleDeadline,
                    parent: deadline instanceof parent.IdleDeadline,
                    proto: Object.getPrototypeOf(deadline) === IdleDeadline.prototype,
                    keys: Reflect.ownKeys(IdleDeadline.prototype).map(String),
                    prototypeWritable: Object.getOwnPropertyDescriptor(IdleDeadline, 'prototype').writable
                });
            `);
            cw.requestIdleCallback(childCallback);
        })()"#,
        Duration::from_secs(1),
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate("String(globalThis.__idleCancelled)").unwrap(),
        "false"
    );
    assert_eq!(
        page.evaluate("globalThis.__parentCallbackDeadline || 'wait'")
            .unwrap(),
        r#"{"idleType":"function","child":false,"parent":true,"proto":true,"sameCtor":false,"construct":"TypeError: Failed to construct 'IdleDeadline': Illegal constructor"}"#
    );
    assert_eq!(
        page.evaluate("globalThis.__childCallbackDeadline || 'wait'")
            .unwrap(),
        r#"{"child":true,"parent":false,"proto":true,"keys":["didTimeout","timeRemaining","constructor","Symbol(Symbol.toStringTag)"],"prototypeWritable":false}"#
    );
}
