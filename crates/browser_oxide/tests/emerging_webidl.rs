use browser_oxide::{stealth::presets::chrome_148_macos, Page};

fn descriptor_probe() -> &'static str {
    r#"
    JSON.stringify((()=>{
        const desc = (object, key) => {
            const d = Object.getOwnPropertyDescriptor(object, key);
            if (!d) return null;
            return {
                e: d.enumerable,
                c: d.configurable,
                w: 'writable' in d ? d.writable : null,
                get: typeof d.get,
                set: typeof d.set,
                value: typeof d.value,
                length: typeof d.value === 'function' ? d.value.length : null,
            };
        };
        const invoke = fn => {
            try { return { ok: true, value: String(fn()) }; }
            catch (error) { return { ok: false, name: error.name, message: error.message }; }
        };
        const ctor = name => {
            const C = globalThis[name];
            if (typeof C !== 'function') return { type: typeof C };
            return {
                type: 'function',
                length: C.length,
                own: Reflect.ownKeys(C).map(String),
                source: Function.prototype.toString.call(C),
                protoOwn: Reflect.ownKeys(C.prototype).map(String),
                direct: invoke(() => C()),
                construct: invoke(() => new C()),
            };
        };
        const po = new PressureObserver(()=>{});
        const a = PressureObserver.knownSources;
        const b = PressureObserver.knownSources;
        const knownDesc = Object.getOwnPropertyDescriptor(PressureObserver, 'knownSources');
        const dpipDesc = Object.getOwnPropertyDescriptor(globalThis, 'documentPictureInPicture');
        return {
            secure: isSecureContext,
            pressureObserver: ctor('PressureObserver'),
            pressureRecord: ctor('PressureRecord'),
            mediaSourceHandle: ctor('MediaSourceHandle'),
            dpip: ctor('DocumentPictureInPicture'),
            pressureProto: Object.fromEntries(Reflect.ownKeys(PressureObserver.prototype).map(k => [String(k), desc(PressureObserver.prototype, k)])),
            known: {
                value: a,
                same: a === b,
                frozen: Object.isFrozen(a),
                own: Reflect.ownKeys(a).map(String),
                desc: {e:knownDesc.enumerable,c:knownDesc.configurable,get:typeof knownDesc.get},
                getterSource: Function.prototype.toString.call(knownDesc.get),
            },
            pressureInstance: {tag:Object.prototype.toString.call(po),own:Reflect.ownKeys(po).map(String)},
            observeCpuPromise: po.observe('cpu') instanceof Promise,
            observeBadPromise: po.observe('bogus') instanceof Promise,
            dpipGlobal: {e:dpipDesc.enumerable,c:dpipDesc.configurable,get:typeof dpipDesc.get},
            dpipInstance: {
                tag:Object.prototype.toString.call(documentPictureInPicture),
                own:Reflect.ownKeys(documentPictureInPicture).map(String),
                eventTarget:documentPictureInPicture instanceof EventTarget,
                instance:documentPictureInPicture instanceof DocumentPictureInPicture,
                window:documentPictureInPicture.window,
            },
            dpipParent:Object.getPrototypeOf(DocumentPictureInPicture).name,
            dpipProtoParent:Object.getPrototypeOf(DocumentPictureInPicture.prototype).constructor.name,
            dpipError:globalThis.__dpipError,
        };
    })())
    "#
}

#[tokio::test]
async fn emerging_secure_window_surface_matches_chrome_148() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__dpipError = 'pending';
        globalThis.__pressureBad = 'pending';
        documentPictureInPicture.requestWindow({width:320,height:180})
          .catch(error => { globalThis.__dpipError = error.name + ':' + error.message; });
        new PressureObserver(()=>{}).observe('bogus')
          .catch(error => { globalThis.__pressureBad = error.name + ':' + error.message; });
    </script></body></html>"#;
    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/emerging",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page.evaluate(descriptor_probe()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["secure"], true);
    assert_eq!(value["pressureObserver"]["length"], 1);
    assert_eq!(
        value["pressureObserver"]["own"],
        serde_json::json!(["length", "name", "prototype", "knownSources"])
    );
    assert_eq!(
        value["pressureObserver"]["protoOwn"],
        serde_json::json!([
            "disconnect",
            "observe",
            "takeRecords",
            "unobserve",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["pressureObserver"]["source"],
        "function PressureObserver() { [native code] }"
    );
    assert_eq!(value["pressureObserver"]["direct"]["name"], "TypeError");
    assert_eq!(
        value["pressureObserver"]["direct"]["message"],
        "Failed to construct 'PressureObserver': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
    );
    assert_eq!(value["pressureObserver"]["construct"]["name"], "TypeError");
    assert_eq!(
        value["pressureObserver"]["construct"]["message"],
        "Failed to construct 'PressureObserver': 1 argument required, but only 0 present."
    );
    assert_eq!(value["pressureRecord"]["length"], 0);
    assert_eq!(
        value["pressureRecord"]["source"],
        "function PressureRecord() { [native code] }"
    );
    assert_eq!(
        value["pressureRecord"]["own"],
        serde_json::json!(["length", "name", "prototype"])
    );
    assert_eq!(
        value["pressureRecord"]["protoOwn"],
        serde_json::json!([
            "source",
            "state",
            "time",
            "toJSON",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["pressureRecord"]["direct"]["message"],
        "Illegal constructor"
    );
    assert_eq!(
        value["pressureRecord"]["construct"]["message"],
        "Failed to construct 'PressureRecord': Illegal constructor"
    );
    assert_eq!(
        value["pressureInstance"]["tag"],
        "[object PressureObserver]"
    );
    assert_eq!(value["pressureInstance"]["own"], serde_json::json!([]));
    assert_eq!(value["known"]["value"], serde_json::json!(["cpu"]));
    assert_eq!(value["known"]["same"], true);
    assert_eq!(value["known"]["frozen"], true);
    assert_eq!(value["known"]["own"], serde_json::json!(["0", "length"]));
    assert_eq!(
        value["known"]["desc"],
        serde_json::json!({"e":true,"c":true,"get":"function"})
    );
    assert_eq!(
        value["known"]["getterSource"],
        "function get knownSources() { [native code] }"
    );
    for name in ["disconnect", "observe", "takeRecords", "unobserve"] {
        assert_eq!(value["pressureProto"][name]["e"], true);
        assert_eq!(value["pressureProto"][name]["c"], true);
        assert_eq!(value["pressureProto"][name]["w"], true);
    }
    assert_eq!(value["pressureProto"]["observe"]["length"], 1);
    assert_eq!(value["pressureProto"]["unobserve"]["length"], 1);
    assert_eq!(value["observeCpuPromise"], true);
    assert_eq!(value["observeBadPromise"], true);
    assert_eq!(
        page.evaluate("globalThis.__pressureBad").unwrap(),
        "TypeError:Failed to execute 'observe' on 'PressureObserver': The provided value 'bogus' is not a valid enum value of type PressureSource."
    );

    assert_eq!(value["mediaSourceHandle"]["length"], 0);
    assert_eq!(
        value["mediaSourceHandle"]["own"],
        serde_json::json!(["length", "name", "prototype"])
    );
    assert_eq!(
        value["mediaSourceHandle"]["source"],
        "function MediaSourceHandle() { [native code] }"
    );
    assert_eq!(
        value["mediaSourceHandle"]["protoOwn"],
        serde_json::json!(["constructor", "Symbol(Symbol.toStringTag)"])
    );
    assert_eq!(
        value["mediaSourceHandle"]["direct"]["message"],
        "Illegal constructor"
    );
    assert_eq!(
        value["mediaSourceHandle"]["construct"]["message"],
        "Failed to construct 'MediaSourceHandle': Illegal constructor"
    );

    assert_eq!(value["dpip"]["length"], 0);
    assert_eq!(
        value["dpip"]["own"],
        serde_json::json!(["length", "name", "prototype"])
    );
    assert_eq!(
        value["dpip"]["source"],
        "function DocumentPictureInPicture() { [native code] }"
    );
    assert_eq!(
        value["dpip"]["protoOwn"],
        serde_json::json!([
            "window",
            "onenter",
            "requestWindow",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["dpip"]["direct"]["message"], "Illegal constructor");
    assert_eq!(
        value["dpip"]["construct"]["message"],
        "Failed to construct 'DocumentPictureInPicture': Illegal constructor"
    );
    assert_eq!(value["dpipParent"], "EventTarget");
    assert_eq!(value["dpipProtoParent"], "EventTarget");
    assert_eq!(
        value["dpipGlobal"],
        serde_json::json!({"e":true,"c":true,"get":"function"})
    );
    assert_eq!(
        value["dpipInstance"]["tag"],
        "[object DocumentPictureInPicture]"
    );
    assert_eq!(value["dpipInstance"]["own"], serde_json::json!([]));
    assert_eq!(value["dpipInstance"]["eventTarget"], true);
    assert_eq!(value["dpipInstance"]["instance"], true);
    assert!(value["dpipInstance"]["window"].is_null());
    assert_eq!(
        value["dpipError"],
        "NotAllowedError:Failed to execute 'requestWindow' on 'DocumentPictureInPicture': Document PiP requires user activation"
    );
}

#[tokio::test]
async fn emerging_insecure_window_exposure_matches_chrome_148() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "http://example.test/emerging",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate(
            r#"JSON.stringify({
                secure:isSecureContext,
                pressure:typeof PressureObserver,
                record:typeof PressureRecord,
                msh:typeof MediaSourceHandle,
                dpipCtor:typeof DocumentPictureInPicture,
                dpip:typeof documentPictureInPicture
            })"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "secure":false,
            "pressure":"undefined",
            "record":"undefined",
            "msh":"function",
            "dpipCtor":"undefined",
            "dpip":"undefined"
        })
    );
}

#[tokio::test]
async fn emerging_secure_dedicated_worker_exposure_matches_chrome_148() {
    let html = r#"<!doctype html><html><body><div id=out></div><script>
        const source = `postMessage(JSON.stringify({
            secure:isSecureContext,
            pressure:typeof PressureObserver,
            record:typeof PressureRecord,
            msh:typeof MediaSourceHandle,
            dpipCtor:typeof DocumentPictureInPicture,
            dpip:typeof documentPictureInPicture
        }));`;
        const url = URL.createObjectURL(new Blob([source], {type:'text/javascript'}));
        const worker = new Worker(url);
        worker.onmessage = event => {
            document.querySelector('#out').textContent = event.data;
            worker.terminate();
            URL.revokeObjectURL(url);
        };
    </script></body></html>"#;
    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/emerging-worker",
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("document.querySelector('#out').textContent")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "secure":true,
            "pressure":"function",
            "record":"function",
            "msh":"function",
            "dpipCtor":"undefined",
            "dpip":"undefined"
        })
    );
}
