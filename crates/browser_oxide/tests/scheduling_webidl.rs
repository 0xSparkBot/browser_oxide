use browser_oxide::Page;

#[tokio::test]
async fn abort_and_scheduler_webidl_match_chrome_shape_and_behavior() {
    let mut page = Page::from_html("<!doctype html><html><body></body></html>", None)
        .await
        .expect("page");

    let result = page
        .evaluate(
            r#"JSON.stringify((() => {
                const keys = value => Reflect.ownKeys(value).map(k =>
                    typeof k === 'symbol' ? `Symbol(${k.description || ''})` : String(k));
                const ac1 = new AbortController();
                const ac2 = new AbortController();
                const any = AbortSignal.any([ac1.signal, ac2.signal]);
                const before = [any.aborted, any.reason];
                ac2.abort('propagated');

                const tc = new TaskController({ priority: 'background' });
                const priorityEvents = [];
                tc.signal.addEventListener('prioritychange', event => {
                    priorityEvents.push([
                        event.constructor.name,
                        event.previousPriority,
                        event.isTrusted,
                    ]);
                });
                tc.setPriority('user-blocking');

                let schedulerNew;
                try { new Scheduler(); schedulerNew = 'ok'; }
                catch (error) { schedulerNew = [error.name, error.message]; }
                let signalNew;
                try { new TaskSignal(); signalNew = 'ok'; }
                catch (error) { signalNew = [error.name, error.message]; }

                return {
                    abort: {
                        staticKeys: keys(AbortSignal),
                        protoKeys: keys(AbortSignal.prototype),
                        controllerProtoKeys: keys(AbortController.prototype),
                        anyOwn: keys(any),
                        before,
                        after: [any.aborted, any.reason],
                    },
                    task: {
                        controllerOwn: keys(tc),
                        signalOwn: keys(tc.signal),
                        signalBrand: Object.prototype.toString.call(tc.signal),
                        priority: tc.signal.priority,
                        events: priorityEvents,
                        controllerParent: Object.getPrototypeOf(TaskController) === AbortController,
                        controllerProtoParent: Object.getPrototypeOf(TaskController.prototype) === AbortController.prototype,
                        signalParent: Object.getPrototypeOf(TaskSignal) === AbortSignal,
                        signalProtoParent: Object.getPrototypeOf(TaskSignal.prototype) === AbortSignal.prototype,
                        controllerProtoKeys: keys(TaskController.prototype),
                        signalProtoKeys: keys(TaskSignal.prototype),
                        eventProtoKeys: keys(TaskPriorityChangeEvent.prototype),
                        schedulerNew,
                        signalNew,
                    },
                    scheduler: {
                        own: keys(scheduler),
                        brand: Object.prototype.toString.call(scheduler),
                        protoKeys: keys(Scheduler.prototype),
                    },
                };
            })())"#,
        )
        .expect("scheduler probe");

    let value: serde_json::Value = serde_json::from_str(&result).expect("probe JSON");
    assert_eq!(
        value["abort"]["staticKeys"],
        serde_json::json!(["length", "name", "prototype", "abort", "any", "timeout"])
    );
    assert_eq!(
        value["abort"]["protoKeys"],
        serde_json::json!([
            "aborted",
            "reason",
            "onabort",
            "throwIfAborted",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["abort"]["controllerProtoKeys"],
        serde_json::json!([
            "signal",
            "abort",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["abort"]["anyOwn"], serde_json::json!([]));
    assert_eq!(value["abort"]["before"], serde_json::json!([false, null]));
    assert_eq!(
        value["abort"]["after"],
        serde_json::json!([true, "propagated"])
    );

    assert_eq!(value["task"]["controllerOwn"], serde_json::json!([]));
    assert_eq!(value["task"]["signalOwn"], serde_json::json!([]));
    assert_eq!(value["task"]["signalBrand"], "[object TaskSignal]");
    assert_eq!(value["task"]["priority"], "user-blocking");
    assert_eq!(
        value["task"]["events"],
        serde_json::json!([["TaskPriorityChangeEvent", "background", false]])
    );
    for key in [
        "controllerParent",
        "controllerProtoParent",
        "signalParent",
        "signalProtoParent",
    ] {
        assert_eq!(value["task"][key], true, "{key}");
    }
    assert_eq!(
        value["task"]["controllerProtoKeys"],
        serde_json::json!(["setPriority", "constructor", "Symbol(Symbol.toStringTag)"])
    );
    assert_eq!(
        value["task"]["signalProtoKeys"],
        serde_json::json!([
            "priority",
            "onprioritychange",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["task"]["eventProtoKeys"],
        serde_json::json!([
            "previousPriority",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["task"]["schedulerNew"],
        serde_json::json!([
            "TypeError",
            "Failed to construct 'Scheduler': Illegal constructor"
        ])
    );
    assert_eq!(
        value["task"]["signalNew"],
        serde_json::json!([
            "TypeError",
            "Failed to construct 'TaskSignal': Illegal constructor"
        ])
    );
    assert_eq!(value["scheduler"]["own"], serde_json::json!([]));
    assert_eq!(value["scheduler"]["brand"], "[object Scheduler]");
    assert_eq!(
        value["scheduler"]["protoKeys"],
        serde_json::json!([
            "postTask",
            "yield",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
}

#[tokio::test]
async fn scheduler_post_task_runs_and_honors_abort_signal() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__schedulerProbe = null;
        (async () => {
            const first = await scheduler.postTask(() => 42, { priority: 'background' });
            const controller = new AbortController();
            controller.abort('cancelled');
            let cancelled;
            try {
                await scheduler.postTask(() => 1, { signal: controller.signal });
                cancelled = 'resolved';
            } catch (error) {
                cancelled = error;
            }
            globalThis.__schedulerProbe = [first, cancelled];
        })();
    </script></body></html>"#;
    let mut page = Page::from_html(html, None).await.expect("page");
    let result = page
        .evaluate("JSON.stringify(globalThis.__schedulerProbe)")
        .expect("postTask result");
    assert_eq!(result, r#"[42,"cancelled"]"#);
}

#[tokio::test]
async fn scheduling_webidl_is_available_in_worker_realm() {
    use browser_oxide::event_loop::BrowserEventLoop;
    use browser_oxide::js_runtime::BrowserJsRuntime;

    let dom =
        browser_oxide::html_parser::parse_html("<html><body><div id=\"out\"></div></body></html>");
    let mut event_loop = BrowserEventLoop::new(BrowserJsRuntime::new(dom));
    event_loop
        .execute_script(
            r#"
            (() => {
                const source = `
                    try {
                        const controller = new TaskController({ priority: 'background' });
                        const combined = AbortSignal.any([controller.signal]);
                        self.postMessage(JSON.stringify({
                            abortAny: typeof AbortSignal.any,
                            combinedOwn: Reflect.ownKeys(combined).length,
                            controllerOwn: Reflect.ownKeys(controller).length,
                            signalOwn: Reflect.ownKeys(controller.signal).length,
                            signalBrand: Object.prototype.toString.call(controller.signal),
                            priority: controller.signal.priority,
                            schedulerBrand: Object.prototype.toString.call(scheduler),
                            schedulerOwn: Reflect.ownKeys(scheduler).length,
                            schedulerPostTask: typeof scheduler.postTask,
                        }));
                    } catch (error) {
                        self.postMessage(JSON.stringify({ error: error.name + ':' + error.message }));
                    }
                `;
                const worker = new Worker(URL.createObjectURL(new Blob([source], { type: 'text/javascript' })));
                worker.onmessage = event => {
                    document.querySelector('#out').textContent = event.data;
                    worker.terminate();
                };
            })();
            "#,
        )
        .expect("spawn worker");

    event_loop
        .run_until_idle(std::time::Duration::from_secs(5))
        .await
        .expect("worker event loop");
    let out = event_loop
        .execute_script("document.querySelector('#out').textContent")
        .expect("worker result");
    let value: serde_json::Value = serde_json::from_str(&out).expect("worker result JSON");
    assert!(value.get("error").is_none(), "{out}");
    assert_eq!(value["abortAny"], "function");
    assert_eq!(value["combinedOwn"], 0);
    assert_eq!(value["controllerOwn"], 0);
    assert_eq!(value["signalOwn"], 0);
    assert_eq!(value["signalBrand"], "[object TaskSignal]");
    assert_eq!(value["priority"], "background");
    assert_eq!(value["schedulerBrand"], "[object Scheduler]");
    assert_eq!(value["schedulerOwn"], 0);
    assert_eq!(value["schedulerPostTask"], "function");
}
