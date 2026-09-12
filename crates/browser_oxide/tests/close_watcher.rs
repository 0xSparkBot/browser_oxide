use browser_oxide::{js_runtime::BrowserJsRuntime, stealth::presets::chrome_148_macos, Page};
use std::time::Duration;

async fn page() -> Page {
    Page::from_html(
        "<!doctype html><html><body><div id=out></div></body></html>",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn close_watcher_webidl_surface_matches_chrome_148() {
    let mut page = page().await;
    let value = page
        .evaluate(
            r#"JSON.stringify((()=>{
                const descriptor = (object, key) => {
                    const d = Object.getOwnPropertyDescriptor(object, key);
                    return d && {
                        enumerable: d.enumerable,
                        configurable: d.configurable,
                        writable: 'writable' in d ? d.writable : null,
                        get: typeof d.get,
                        set: typeof d.set,
                        value: typeof d.value,
                    };
                };
                const construction = (fn) => {
                    try {
                        const value = fn();
                        return {
                            ok: true,
                            tag: Object.prototype.toString.call(value),
                            keys: Reflect.ownKeys(value).map(String),
                            instance: value instanceof CloseWatcher,
                        };
                    } catch (error) {
                        return { ok: false, name: error.name, message: error.message };
                    }
                };
                const proto = CloseWatcher.prototype;
                const descriptors = {};
                for (const key of Reflect.ownKeys(proto)) {
                    descriptors[String(key)] = descriptor(proto, key);
                }
                return {
                    type: typeof CloseWatcher,
                    constructor: {
                        length: CloseWatcher.length,
                        name: CloseWatcher.name,
                        own: Reflect.ownKeys(CloseWatcher).map(String),
                        source: Function.prototype.toString.call(CloseWatcher),
                        parent: Object.getPrototypeOf(CloseWatcher).name,
                    },
                    prototype: {
                        own: Reflect.ownKeys(proto).map(String),
                        tag: Object.prototype.toString.call(proto),
                        parent: Object.getPrototypeOf(proto).constructor.name,
                        descriptors,
                    },
                    call: construction(() => CloseWatcher()),
                    new0: construction(() => new CloseWatcher()),
                    newEmpty: construction(() => new CloseWatcher({})),
                    newSignal: construction(() => new CloseWatcher({ signal: new AbortController().signal })),
                    lengths: [proto.close.length, proto.destroy.length, proto.requestClose.length],
                    sources: [
                        Function.prototype.toString.call(proto.close),
                        Function.prototype.toString.call(proto.destroy),
                        Function.prototype.toString.call(proto.requestClose),
                    ],
                };
            })())"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&value).unwrap();

    assert_eq!(value["type"], "function");
    assert_eq!(value["constructor"]["length"], 0);
    assert_eq!(value["constructor"]["name"], "CloseWatcher");
    assert_eq!(
        value["constructor"]["own"],
        serde_json::json!(["length", "name", "prototype"])
    );
    assert_eq!(
        value["constructor"]["source"],
        "function CloseWatcher() { [native code] }"
    );
    assert_eq!(value["constructor"]["parent"], "EventTarget");
    assert_eq!(
        value["prototype"]["own"],
        serde_json::json!([
            "oncancel",
            "onclose",
            "close",
            "destroy",
            "requestClose",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["prototype"]["tag"], "[object CloseWatcher]");
    assert_eq!(value["prototype"]["parent"], "EventTarget");
    for name in ["oncancel", "onclose"] {
        assert_eq!(value["prototype"]["descriptors"][name]["enumerable"], true);
        assert_eq!(
            value["prototype"]["descriptors"][name]["configurable"],
            true
        );
        assert_eq!(value["prototype"]["descriptors"][name]["get"], "function");
        assert_eq!(value["prototype"]["descriptors"][name]["set"], "function");
    }
    for name in ["close", "destroy", "requestClose"] {
        assert_eq!(value["prototype"]["descriptors"][name]["enumerable"], true);
        assert_eq!(
            value["prototype"]["descriptors"][name]["configurable"],
            true
        );
        assert_eq!(value["prototype"]["descriptors"][name]["writable"], true);
        assert_eq!(value["prototype"]["descriptors"][name]["value"], "function");
    }
    assert_eq!(
        value["call"]["message"],
        "Failed to construct 'CloseWatcher': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
    );
    for name in ["new0", "newEmpty", "newSignal"] {
        assert_eq!(value[name]["ok"], true);
        assert_eq!(value[name]["tag"], "[object CloseWatcher]");
        assert_eq!(value[name]["keys"], serde_json::json!([]));
        assert_eq!(value[name]["instance"], true);
    }
    assert_eq!(value["lengths"], serde_json::json!([0, 0, 0]));
    assert_eq!(
        value["sources"],
        serde_json::json!([
            "function close() { [native code] }",
            "function destroy() { [native code] }",
            "function requestClose() { [native code] }"
        ])
    );
}

#[tokio::test]
async fn close_watcher_lifecycle_matches_chrome_148() {
    let mut page = page().await;
    let value = page
        .evaluate(
            r#"JSON.stringify((()=>{
                const result = {};
                const invocation = (fn) => {
                    try { return { ok: true, value: String(fn()) }; }
                    catch (error) { return { ok: false, name: error.name, message: error.message }; }
                };
                result.illegal = {
                    close: invocation(() => CloseWatcher.prototype.close.call({})),
                    destroy: invocation(() => CloseWatcher.prototype.destroy.call({})),
                    requestClose: invocation(() => CloseWatcher.prototype.requestClose.call({})),
                };
                result.badSignal = invocation(() => new CloseWatcher({ signal: {} }));

                {
                    const watcher = new CloseWatcher();
                    const events = [];
                    watcher.addEventListener('cancel', event => {
                        events.push(['cancel', event.cancelable, event.defaultPrevented, event.isTrusted]);
                        event.preventDefault();
                    });
                    watcher.addEventListener('close', event => {
                        events.push(['close', event.cancelable, event.defaultPrevented, event.isTrusted]);
                    });
                    watcher.requestClose();
                    result.prevent1 = events.slice();
                    watcher.requestClose();
                    result.prevent2 = events.slice();
                    watcher.close();
                    result.preventClose = events.slice();
                    watcher.requestClose();
                    result.preventAfterClose = events.slice();
                }

                {
                    const watcher = new CloseWatcher();
                    const events = [];
                    watcher.oncancel = () => events.push('oncancel');
                    watcher.onclose = () => events.push('onclose');
                    watcher.requestClose();
                    watcher.requestClose();
                    watcher.close();
                    watcher.destroy();
                    result.repeat = events;
                }

                {
                    const watcher = new CloseWatcher();
                    const events = [];
                    watcher.oncancel = () => events.push('cancel');
                    watcher.onclose = () => events.push('close');
                    watcher.destroy();
                    watcher.requestClose();
                    watcher.close();
                    result.destroyFirst = events;
                }

                {
                    const controller = new AbortController();
                    controller.abort();
                    const watcher = new CloseWatcher({ signal: controller.signal });
                    const events = [];
                    watcher.oncancel = () => events.push('cancel');
                    watcher.onclose = () => events.push('close');
                    watcher.requestClose();
                    watcher.close();
                    result.alreadyAbort = events;
                }

                {
                    const controller = new AbortController();
                    const watcher = new CloseWatcher({ signal: controller.signal });
                    const events = [];
                    watcher.oncancel = () => events.push('cancel');
                    watcher.onclose = () => events.push('close');
                    controller.abort();
                    watcher.requestClose();
                    watcher.close();
                    result.abortLater = events;
                }

                {
                    const watcher = new CloseWatcher();
                    let receiver = null;
                    let count = 0;
                    function handler() { receiver = this; count++; }
                    watcher.onclose = handler;
                    result.handlerGet = watcher.onclose === handler;
                    watcher.onclose = null;
                    result.handlerNull = watcher.onclose === null;
                    watcher.onclose = handler;
                    watcher.close();
                    result.handlerThis = receiver === watcher;
                    result.handlerCount = count;
                }
                return result;
            })())"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&value).unwrap();

    for name in ["close", "destroy", "requestClose"] {
        assert_eq!(value["illegal"][name]["name"], "TypeError");
        assert_eq!(value["illegal"][name]["message"], "Illegal invocation");
    }
    assert_eq!(value["badSignal"]["name"], "TypeError");
    assert_eq!(
        value["badSignal"]["message"],
        "Failed to construct 'CloseWatcher': Failed to read the 'signal' property from 'CloseWatcherOptions': Failed to convert value to 'AbortSignal'."
    );
    assert_eq!(
        value["prevent1"],
        serde_json::json!([["cancel", true, false, true]])
    );
    assert_eq!(
        value["prevent2"],
        serde_json::json!([["cancel", true, false, true], ["cancel", true, false, true]])
    );
    assert_eq!(
        value["preventClose"],
        serde_json::json!([
            ["cancel", true, false, true],
            ["cancel", true, false, true],
            ["close", false, false, true]
        ])
    );
    assert_eq!(value["preventAfterClose"], value["preventClose"]);
    assert_eq!(value["repeat"], serde_json::json!(["oncancel", "onclose"]));
    assert_eq!(value["destroyFirst"], serde_json::json!([]));
    assert_eq!(value["alreadyAbort"], serde_json::json!([]));
    assert_eq!(value["abortLater"], serde_json::json!([]));
    assert_eq!(value["handlerGet"], true);
    assert_eq!(value["handlerNull"], true);
    assert_eq!(value["handlerThis"], true);
    assert_eq!(value["handlerCount"], 1);
}

#[test]
fn close_watcher_is_not_exposed_in_dedicated_worker() {
    let dom =
        browser_oxide::html_parser::parse_html("<html><body><div id=out></div></body></html>");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    let out = local.block_on(&runtime, async move {
        let mut runtime = BrowserJsRuntime::new(dom);
        runtime
            .execute_script(
                r#"
                    const source = `postMessage(JSON.stringify({
                        closeWatcher: typeof CloseWatcher,
                        eventTarget: typeof EventTarget
                    }));`;
                    const worker = new Worker(URL.createObjectURL(
                        new Blob([source], { type: 'text/javascript' })
                    ));
                    worker.onmessage = event => {
                        document.querySelector('#out').textContent = event.data;
                        worker.terminate();
                    };
                "#,
                None,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if std::time::Instant::now() >= deadline {
                break String::new();
            }
            let tick = Duration::from_millis(50);
            let _ = tokio::time::timeout(tick, runtime.run_event_loop()).await;
            let value = runtime
                .execute_script("document.querySelector('#out').textContent || ''", None)
                .unwrap_or_default();
            if !value.is_empty() {
                break value;
            }
        }
    });
    assert_eq!(
        out,
        r#"{"closeWatcher":"undefined","eventTarget":"function"}"#
    );
}
