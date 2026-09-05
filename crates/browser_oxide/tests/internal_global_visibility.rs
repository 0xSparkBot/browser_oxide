use browser_oxide::js_runtime::BrowserJsRuntime;

#[test]
fn engine_bridge_names_are_not_globally_enumerable() {
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    let mut runtime = BrowserJsRuntime::new(dom);

    let result = runtime
        .execute_script(
            r#"
            globalThis.__pageOwnedSentinel = 1;
            globalThis.__oxFrameSetup(7, 3, 1);
            JSON.stringify({
                names: Object.getOwnPropertyNames(globalThis).filter(k =>
                    k === '_browser_oxide' || k.startsWith('__bo_') ||
                    k.startsWith('__frame') || k.startsWith('__parentFrame') ||
                    k.startsWith('__topFrame') ||
                    k.startsWith('__ox') || k.startsWith('__oxide') ||
                    k === '__pendingNavigation' || k === '__pumpFrameMessages'
                ),
                keys: Object.keys(globalThis).filter(k =>
                    k === '_browser_oxide' || k.startsWith('__bo_') ||
                    k.startsWith('__frame') || k.startsWith('__parentFrame') ||
                    k.startsWith('__topFrame') ||
                    k.startsWith('__ox') || k.startsWith('__oxide')
                ),
                reflected: Reflect.ownKeys(globalThis).filter(k =>
                    typeof k === 'string' && (
                        k === '_browser_oxide' || k.startsWith('__bo_') ||
                        k.startsWith('__frame') || k.startsWith('__parentFrame') ||
                        k.startsWith('__topFrame') ||
                        k.startsWith('__ox') || k.startsWith('__oxide')
                    )
                ),
                descriptors: Object.keys(Object.getOwnPropertyDescriptors(globalThis)).filter(k =>
                    k === '_browser_oxide' || k.startsWith('__bo_') ||
                    k.startsWith('__frame') || k.startsWith('__parentFrame') ||
                    k.startsWith('__topFrame') ||
                    k.startsWith('__ox') || k.startsWith('__oxide')
                ),
                forIn: (() => {
                    const found = [];
                    for (const k in globalThis) {
                        if (k === '_browser_oxide' || k.startsWith('__bo_') ||
                            k.startsWith('__frame') || k.startsWith('__parentFrame') ||
                            k.startsWith('__topFrame') ||
                            k.startsWith('__ox') || k.startsWith('__oxide')) found.push(k);
                    }
                    return found;
                })(),
                numericNames: Object.getOwnPropertyNames(globalThis)
                    .filter(k => /^(0|[1-9]\d*)$/.test(k)),
                wrongWindowGlobals: [
                    'WorkerGlobalScope', 'DedicatedWorkerGlobalScope',
                    'Magnetometer', 'SpeechRecognitionAlternative',
                    'USBIsochronousOutPacket', 'webkitAudioContext',
                    'ApplePaySession', 'defaultStatus'
                ].filter(k => typeof globalThis[k] !== 'undefined'),
                pageNameVisible: Object.getOwnPropertyNames(globalThis)
                    .includes('__pageOwnedSentinel'),
                driverCanStillAccess: typeof globalThis.__pumpFrameMessages === 'function' &&
                    globalThis.__frameId === 7,
                nativeNames: String(Object.getOwnPropertyNames),
                nativeOwnKeys: String(Reflect.ownKeys),
                missingChromeGlobals: [
                    'HTMLAreaElement', 'HTMLBRElement', 'HTMLBaseElement',
                    'HTMLDListElement', 'HTMLDataElement', 'HTMLDataListElement',
                    'HTMLDetailsElement', 'HTMLDialogElement',
                    'HTMLDirectoryElement', 'MediaStream', 'MediaStreamTrack',
                    'USB', 'VideoDecoder', 'VideoEncoder'
                ].filter(k => typeof globalThis[k] !== 'function'),
                lateWindowTypes: [
                    typeof CSS,
                    typeof GPUBufferUsage,
                    typeof GPUColorWrite,
                    typeof GPUMapMode,
                    typeof GPUShaderStage,
                    typeof GPUTextureUsage,
                    typeof documentPictureInPicture,
                    typeof webkitCancelAnimationFrame,
                    typeof webkitRequestAnimationFrame,
                    typeof webkitURL,
                    typeof focus,
                    typeof blur,
                    typeof navigation,
                    typeof sharedStorage
                ],
                elementBrands: [
                    ['area', HTMLAreaElement, 'HTMLAreaElement'],
                    ['br', HTMLBRElement, 'HTMLBRElement'],
                    ['base', HTMLBaseElement, 'HTMLBaseElement'],
                    ['dl', HTMLDListElement, 'HTMLDListElement'],
                    ['data', HTMLDataElement, 'HTMLDataElement'],
                    ['datalist', HTMLDataListElement, 'HTMLDataListElement'],
                    ['details', HTMLDetailsElement, 'HTMLDetailsElement'],
                    ['dialog', HTMLDialogElement, 'HTMLDialogElement'],
                    ['dir', HTMLDirectoryElement, 'HTMLDirectoryElement']
                ].map(([tag, Ctor, brand]) => {
                    const element = document.createElement(tag);
                    return element instanceof Ctor &&
                        Object.prototype.toString.call(element) === `[object ${brand}]`;
                }),
            })
            "#,
            None,
        )
        .expect("global enumeration probe should execute");

    let value: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    for key in [
        "names",
        "keys",
        "reflected",
        "descriptors",
        "forIn",
        "numericNames",
        "wrongWindowGlobals",
    ] {
        assert_eq!(value[key], serde_json::json!([]), "{key}: {result}");
    }
    assert_eq!(value["pageNameVisible"], true, "{result}");
    assert_eq!(value["driverCanStillAccess"], true, "{result}");
    assert_eq!(
        value["nativeNames"], "function getOwnPropertyNames() { [native code] }",
        "{result}"
    );
    assert_eq!(
        value["nativeOwnKeys"], "function ownKeys() { [native code] }",
        "{result}"
    );
    assert_eq!(
        value["missingChromeGlobals"],
        serde_json::json!([]),
        "{result}"
    );
    assert_eq!(
        value["lateWindowTypes"],
        serde_json::json!([
            "object", "object", "object", "object", "object", "object", "object", "function",
            "function", "function", "function", "function", "object", "object"
        ]),
        "{result}"
    );
    assert_eq!(
        value["elementBrands"],
        serde_json::json!([true, true, true, true, true, true, true, true, true]),
        "{result}"
    );
}
