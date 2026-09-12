use browser_oxide::Page;
use serde_json::Value;
use std::time::Duration;

async fn secure_page() -> Page {
    Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://locks.example/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("secure page")
}

async fn drive(page: &mut Page, millis: u64) {
    page.evaluate_async(
        &format!("new Promise(resolve => setTimeout(resolve, {millis}))"),
        Duration::from_secs(3),
    )
    .await
    .expect("drive event loop");
}

#[tokio::test]
async fn web_locks_queue_query_abort_and_shared_semantics_match_chrome_148() {
    let mut page = secure_page().await;
    page.evaluate(
        r#"
        globalThis.__webLocksResult = null;
        (async () => {
            const result = {
                managerTag: Object.prototype.toString.call(navigator.locks),
                managerOwn: Reflect.ownKeys(navigator.locks).map(String),
                navigatorLocksDescriptor: (() => {
                    const descriptor = Object.getOwnPropertyDescriptor(
                        Object.getPrototypeOf(navigator), 'locks'
                    );
                    return descriptor && {
                        enumerable: descriptor.enumerable,
                        configurable: descriptor.configurable,
                        getter: typeof descriptor.get,
                        setter: typeof descriptor.set,
                    };
                })(),
                managerKeys: Reflect.ownKeys(LockManager.prototype).map(String),
                lockKeys: Reflect.ownKeys(Lock.prototype).map(String),
                requestLength: LockManager.prototype.request.length,
                queryLength: LockManager.prototype.query.length,
            };

            result.initial = await navigator.locks.query();
            let releaseFirst;
            const firstGate = new Promise(resolve => { releaseFirst = resolve; });
            const seen = [];
            const first = navigator.locks.request('alpha', async lock => {
                seen.push({
                    who: 'first',
                    name: lock.name,
                    mode: lock.mode,
                    tag: Object.prototype.toString.call(lock),
                    own: Reflect.ownKeys(lock).map(String),
                });
                await firstGate;
                return 'first-result';
            });
            await new Promise(resolve => setTimeout(resolve, 0));
            const second = navigator.locks.request('alpha', lock => {
                seen.push({ who: 'second', name: lock.name, mode: lock.mode });
                return 'second-result';
            });
            await new Promise(resolve => setTimeout(resolve, 0));
            result.mid = await navigator.locks.query();
            result.ifAvailable = await navigator.locks.request(
                'alpha',
                { ifAvailable: true },
                lock => lock ? 'got' : 'none',
            );

            const controller = new AbortController();
            const aborted = navigator.locks.request(
                'alpha',
                { signal: controller.signal },
                () => 'unexpected',
            ).then(value => `resolved:${value}`, error => error.name);
            controller.abort();
            result.abort = await aborted;

            releaseFirst();
            result.exclusiveResults = await Promise.all([first, second]);
            result.seen = seen;

            let releaseShared1;
            let releaseShared2;
            const sharedGate1 = new Promise(resolve => { releaseShared1 = resolve; });
            const sharedGate2 = new Promise(resolve => { releaseShared2 = resolve; });
            const sharedSeen = [];
            const shared1 = navigator.locks.request('beta', { mode: 'shared' }, async lock => {
                sharedSeen.push(`shared1:${lock.mode}`);
                await sharedGate1;
                return 1;
            });
            const shared2 = navigator.locks.request('beta', { mode: 'shared' }, async lock => {
                sharedSeen.push(`shared2:${lock.mode}`);
                await sharedGate2;
                return 2;
            });
            await new Promise(resolve => setTimeout(resolve, 0));
            const exclusive = navigator.locks.request('beta', lock => {
                sharedSeen.push(`exclusive:${lock.mode}`);
                return 3;
            });
            await new Promise(resolve => setTimeout(resolve, 0));
            result.sharedMid = await navigator.locks.query();
            releaseShared1();
            releaseShared2();
            result.sharedResults = await Promise.all([shared1, shared2, exclusive]);
            result.sharedSeen = sharedSeen;

            globalThis.__webLocksResult = JSON.stringify(result);
        })().catch(error => {
            globalThis.__webLocksResult = 'ERR:' + (error && error.stack || error);
        });
        "#,
    )
    .expect("start locks probe");
    drive(&mut page, 150).await;

    let raw = page.evaluate("globalThis.__webLocksResult").unwrap();
    assert!(!raw.starts_with("ERR:"), "web locks probe failed: {raw}");
    let value: Value = serde_json::from_str(&raw).expect("locks result json");

    assert_eq!(value["managerTag"], "[object LockManager]");
    assert_eq!(value["managerOwn"], serde_json::json!([]));
    assert_eq!(
        value["navigatorLocksDescriptor"],
        serde_json::json!({
            "enumerable": true,
            "configurable": true,
            "getter": "function",
            "setter": "undefined",
        })
    );
    assert_eq!(
        value["managerKeys"],
        serde_json::json!([
            "query",
            "request",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["lockKeys"],
        serde_json::json!(["name", "mode", "constructor", "Symbol(Symbol.toStringTag)"])
    );
    assert_eq!(value["requestLength"], 2);
    assert_eq!(value["queryLength"], 0);
    assert_eq!(
        value["initial"],
        serde_json::json!({"held": [], "pending": []})
    );

    assert_eq!(value["mid"]["held"].as_array().unwrap().len(), 1);
    assert_eq!(value["mid"]["pending"].as_array().unwrap().len(), 1);
    assert_eq!(value["mid"]["held"][0]["name"], "alpha");
    assert_eq!(value["mid"]["held"][0]["mode"], "exclusive");
    assert_eq!(value["mid"]["pending"][0]["name"], "alpha");
    assert_eq!(value["mid"]["pending"][0]["mode"], "exclusive");
    let client_id = value["mid"]["held"][0]["clientId"]
        .as_str()
        .expect("clientId");
    assert_eq!(client_id.len(), 32);
    assert!(client_id.chars().all(|c| c.is_ascii_hexdigit()));

    assert_eq!(value["ifAvailable"], "none");
    assert_eq!(value["abort"], "AbortError");
    assert_eq!(
        value["exclusiveResults"],
        serde_json::json!(["first-result", "second-result"])
    );
    assert_eq!(value["seen"][0]["tag"], "[object Lock]");
    assert_eq!(value["seen"][0]["own"], serde_json::json!([]));
    assert_eq!(value["seen"][0]["name"], "alpha");
    assert_eq!(value["seen"][0]["mode"], "exclusive");

    assert_eq!(value["sharedMid"]["held"].as_array().unwrap().len(), 2);
    assert_eq!(value["sharedMid"]["pending"].as_array().unwrap().len(), 1);
    assert_eq!(value["sharedMid"]["held"][0]["mode"], "shared");
    assert_eq!(value["sharedMid"]["held"][1]["mode"], "shared");
    assert_eq!(value["sharedMid"]["pending"][0]["mode"], "exclusive");
    assert_eq!(value["sharedResults"], serde_json::json!([1, 2, 3]));
    assert_eq!(
        value["sharedSeen"],
        serde_json::json!(["shared1:shared", "shared2:shared", "exclusive:exclusive"])
    );
}

#[tokio::test]
async fn web_locks_are_hidden_in_insecure_contexts() {
    let mut page = Page::from_html(
        "<!doctype html><html><body></body></html>",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("insecure page");

    let actual = page
        .evaluate(
            r#"JSON.stringify({
                secure: isSecureContext,
                locks: typeof navigator.locks,
                hasLocks: Object.prototype.hasOwnProperty.call(
                    Object.getPrototypeOf(navigator), 'locks'
                ),
                lockManager: typeof LockManager,
                lock: typeof Lock,
            })"#,
        )
        .expect("insecure exposure");
    let actual: Value = serde_json::from_str(&actual).expect("insecure json");
    assert_eq!(
        actual,
        serde_json::json!({
            "secure": false,
            "locks": "undefined",
            "hasLocks": false,
            "lockManager": "undefined",
            "lock": "undefined",
        })
    );
}

#[tokio::test]
async fn web_locks_coordinate_same_origin_window_and_worker() {
    let mut page = secure_page().await;
    page.evaluate(
        r#"
        globalThis.__workerLocksResult = null;
        const workerSource = `
            self.onmessage = async event => {
                if (event.data === 'check') {
                    const value = await navigator.locks.request(
                        'cross-runtime',
                        { ifAvailable: true },
                        lock => lock ? 'got' : 'none'
                    );
                    self.postMessage({ phase: 'check', value, origin: location.origin });
                } else if (event.data === 'after') {
                    const value = await navigator.locks.request(
                        'cross-runtime',
                        lock => lock.name + ':' + lock.mode
                    );
                    self.postMessage({ phase: 'after', value, origin: location.origin });
                }
            };
        `;
        const workerUrl = URL.createObjectURL(new Blob([workerSource], { type: 'text/javascript' }));
        const worker = new Worker(workerUrl);
        const seen = [];
        let releaseMain;
        worker.onmessage = event => {
            seen.push(event.data);
            if (event.data.phase === 'check') {
                releaseMain();
            } else if (event.data.phase === 'after') {
                globalThis.__workerLocksResult = JSON.stringify(seen);
                worker.terminate();
                URL.revokeObjectURL(workerUrl);
            }
        };

        navigator.locks.request('cross-runtime', async () => {
            await new Promise(resolve => {
                releaseMain = resolve;
                worker.postMessage('check');
            });
        }).then(() => worker.postMessage('after'));
        "#,
    )
    .expect("start worker lock probe");
    drive(&mut page, 500).await;

    let raw = page.evaluate("globalThis.__workerLocksResult").unwrap();
    assert!(!raw.starts_with("ERR:"), "worker lock probe failed: {raw}");
    let value: Value = serde_json::from_str(&raw).expect("worker result json");
    assert_eq!(value[0]["phase"], "check");
    assert_eq!(value[0]["value"], "none");
    assert_eq!(value[0]["origin"], "https://locks.example");
    assert_eq!(value[1]["phase"], "after");
    assert_eq!(value[1]["value"], "cross-runtime:exclusive");
    assert_eq!(value[1]["origin"], "https://locks.example");
}

#[tokio::test]
async fn web_locks_illegal_constructors_and_steal_match_chrome_148() {
    let mut page = secure_page().await;
    page.evaluate(
        r#"
        globalThis.__webLocksEdge = null;
        (async () => {
            const out = {};
            for (const name of ['LockManager', 'Lock']) {
                const C = globalThis[name];
                const entry = {
                    keys: Reflect.ownKeys(C).map(String),
                    length: C.length,
                    source: Function.prototype.toString.call(C),
                    protoParent: Object.getPrototypeOf(C.prototype).constructor.name,
                };
                try { new C(); entry.construct = 'ok'; }
                catch (error) { entry.construct = `${error.name}:${error.message}`; }
                try { C(); entry.call = 'ok'; }
                catch (error) { entry.call = `${error.name}:${error.message}`; }
                out[name] = entry;
            }

            for (const [key, options] of [
                ['sharedSteal', { mode: 'shared', steal: true }],
                ['availableSteal', { ifAvailable: true, steal: true }],
            ]) {
                try {
                    await navigator.locks.request(`invalid-${key}`, options, () => 1);
                    out[key] = 'ok';
                } catch (error) {
                    out[key] = `${error.name}:${error.message}`;
                }
            }

            out.events = [];
            let releaseFirst;
            const gate = new Promise(resolve => { releaseFirst = resolve; });
            const first = navigator.locks.request('steal-me', async lock => {
                out.events.push('first-start');
                await gate;
                out.events.push('first-end');
                return 'first-value';
            });
            await new Promise(resolve => setTimeout(resolve, 0));
            out.before = await navigator.locks.query();

            const stolen = navigator.locks.request('steal-me', { steal: true }, async lock => {
                out.events.push(`steal-start:${lock.mode}`);
                out.during = await navigator.locks.query();
                return 'steal-value';
            });
            await new Promise(resolve => setTimeout(resolve, 20));
            out.afterStealTick = await navigator.locks.query();
            releaseFirst();

            const settle = promise => promise.then(
                value => ({ status: 'fulfilled', value }),
                error => ({ status: 'rejected', name: error.name, message: error.message }),
            );
            out.stealResult = await settle(stolen);
            out.firstResult = await settle(first);
            out.final = await navigator.locks.query();
            globalThis.__webLocksEdge = JSON.stringify(out);
        })().catch(error => {
            globalThis.__webLocksEdge = 'ERR:' + (error && error.stack || error);
        });
        "#,
    )
    .expect("start edge probe");
    drive(&mut page, 180).await;

    let raw = page.evaluate("globalThis.__webLocksEdge").unwrap();
    assert!(
        !raw.starts_with("ERR:"),
        "web locks edge probe failed: {raw}"
    );
    let value: Value = serde_json::from_str(&raw).expect("edge result json");

    for name in ["LockManager", "Lock"] {
        assert_eq!(
            value[name]["keys"],
            serde_json::json!(["length", "name", "prototype"])
        );
        assert_eq!(value[name]["length"], 0);
        assert_eq!(
            value[name]["source"],
            format!("function {name}() {{ [native code] }}")
        );
        assert_eq!(value[name]["protoParent"], "Object");
        assert_eq!(
            value[name]["construct"],
            format!("TypeError:Failed to construct '{name}': Illegal constructor")
        );
        assert_eq!(value[name]["call"], "TypeError:Illegal constructor");
    }
    assert_eq!(
        value["sharedSteal"],
        "NotSupportedError:Failed to execute 'request' on 'LockManager': The 'steal' option may only be used with 'exclusive' locks."
    );
    assert_eq!(
        value["availableSteal"],
        "NotSupportedError:Failed to execute 'request' on 'LockManager': The 'steal' and 'ifAvailable' options cannot be used together."
    );

    assert_eq!(
        value["events"],
        serde_json::json!(["first-start", "steal-start:exclusive", "first-end"])
    );
    assert_eq!(value["before"]["held"].as_array().unwrap().len(), 1);
    assert_eq!(value["before"]["held"][0]["name"], "steal-me");
    assert_eq!(value["during"]["held"].as_array().unwrap().len(), 1);
    assert_eq!(value["during"]["held"][0]["name"], "steal-me");
    assert_eq!(value["during"]["held"][0]["mode"], "exclusive");
    assert_eq!(
        value["afterStealTick"],
        serde_json::json!({"held": [], "pending": []})
    );
    assert_eq!(
        value["stealResult"],
        serde_json::json!({"status": "fulfilled", "value": "steal-value"})
    );
    assert_eq!(value["firstResult"]["status"], "rejected");
    assert_eq!(value["firstResult"]["name"], "AbortError");
    assert_eq!(
        value["firstResult"]["message"],
        "Lock broken by another request with the 'steal' option."
    );
    assert_eq!(
        value["final"],
        serde_json::json!({"held": [], "pending": []})
    );
}

#[tokio::test]
async fn web_locks_share_same_origin_pages_and_isolate_different_origins() {
    async fn page(url: &str) -> Page {
        Page::from_html_with_url(
            "<!doctype html><html><body></body></html>",
            url,
            Some(browser_oxide::stealth::presets::chrome_148_macos()),
        )
        .await
        .expect("secure page")
    }

    let mut owner = page("https://same.example/owner").await;
    let mut peer = page("https://same.example/peer").await;
    let mut other = page("https://other.example/").await;

    owner
        .evaluate(
            r#"
            globalThis.__releaseOriginLock = null;
            globalThis.__ownerHeld = false;
            navigator.locks.request('origin-scope', async () => {
                globalThis.__ownerHeld = true;
                await new Promise(resolve => { globalThis.__releaseOriginLock = resolve; });
            });
            "#,
        )
        .expect("start owner lock");
    drive(&mut owner, 30).await;
    assert_eq!(owner.evaluate("String(__ownerHeld)").unwrap(), "true");

    peer.evaluate(
        r#"
            globalThis.__sameOriginAvailable = null;
            globalThis.__sameOriginQueued = false;
            navigator.locks.request('origin-scope', { ifAvailable: true }, lock => {
                globalThis.__sameOriginAvailable = lock ? 'got' : 'none';
            });
            navigator.locks.request('origin-scope', () => {
                globalThis.__sameOriginQueued = true;
            });
            "#,
    )
    .expect("start same-origin requests");
    drive(&mut peer, 30).await;
    assert_eq!(peer.evaluate("__sameOriginAvailable").unwrap(), "none");
    assert_eq!(
        peer.evaluate("String(__sameOriginQueued)").unwrap(),
        "false"
    );

    other
        .evaluate(
            r#"
            globalThis.__otherOriginAvailable = null;
            navigator.locks.request('origin-scope', { ifAvailable: true }, lock => {
                globalThis.__otherOriginAvailable = lock ? 'got' : 'none';
            });
            "#,
        )
        .expect("start cross-origin request");
    drive(&mut other, 30).await;
    assert_eq!(other.evaluate("__otherOriginAvailable").unwrap(), "got");

    owner
        .evaluate(
            r#"
            navigator.locks.query().then(value => {
                globalThis.__ownerQuery = JSON.stringify(value);
            });
            "#,
        )
        .expect("query owner origin");
    drive(&mut owner, 20).await;
    let query: Value =
        serde_json::from_str(&owner.evaluate("__ownerQuery").unwrap()).expect("owner query json");
    assert_eq!(query["held"].as_array().unwrap().len(), 1);
    assert_eq!(query["pending"].as_array().unwrap().len(), 1);
    assert_eq!(query["held"][0]["name"], "origin-scope");
    assert_eq!(query["pending"][0]["name"], "origin-scope");

    owner
        .evaluate("__releaseOriginLock()")
        .expect("release lock");
    drive(&mut owner, 20).await;
    drive(&mut peer, 40).await;
    assert_eq!(peer.evaluate("String(__sameOriginQueued)").unwrap(), "true");
}
