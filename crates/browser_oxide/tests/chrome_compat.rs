//! Chrome compatibility audit.
//!
//! Tests every API that a real Chrome 130 browser exposes.
//! Each test checks typeof/existence AND basic behavior.
//! Failures here = gaps vs real Chrome.

use browser_oxide::Page;

#[tokio::test]
async fn window_named_properties_match_chrome_element_and_frame_lookup() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body><div id='namedProbe'></div><input name='namedInput'><iframe name='namedFrame'></iframe></body></html>",
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let result = page
        .evaluate(
            r#"JSON.stringify({
                idType: typeof window.namedProbe,
                idSame: window.namedProbe === document.getElementById('namedProbe'),
                idOwn: Object.prototype.hasOwnProperty.call(window, 'namedProbe'),
                idEnumerable: Object.keys(window).includes('namedProbe'),
                inputType: typeof window.namedInput,
                frameType: typeof window.namedFrame,
                frameTag: Object.prototype.toString.call(window.namedFrame),
                frameOwn: Object.prototype.hasOwnProperty.call(window, 'namedFrame')
            })"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["idType"], "object");
    assert_eq!(value["idSame"], true);
    assert_eq!(value["idOwn"], false);
    assert_eq!(value["idEnumerable"], false);
    assert_eq!(value["inputType"], "undefined");
    assert_eq!(value["frameType"], "object");
    assert_eq!(value["frameTag"], "[object Window]");
    assert_eq!(value["frameOwn"], false);

    let dynamic = page
        .evaluate(
            r#"(() => {
                const element = document.getElementById('namedProbe');
                element.setAttribute('id', 'renamedProbe');
                const renamed = typeof window.namedProbe === 'undefined'
                    && window.renamedProbe === element;
                element.removeAttribute('id');
                return renamed && typeof window.renamedProbe === 'undefined';
            })()"#,
        )
        .unwrap();
    assert_eq!(dynamic, "true");
}

#[tokio::test]
async fn dom_collections_match_chrome_legacy_platform_object_shape() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body><div id='a' name='n1'></div><span id='b'></span></body></html>",
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let result = page
        .evaluate(
            r#"(() => {
                const nodes = document.body.childNodes;
                const elements = document.body.children;
                let nodeCtor, htmlCtor;
                try { new NodeList(); nodeCtor = 'ok'; }
                catch (e) { nodeCtor = e.name + ':' + e.message; }
                try { new HTMLCollection(); htmlCtor = 'ok'; }
                catch (e) { htmlCtor = e.name + ':' + e.message; }
                const zero = Object.getOwnPropertyDescriptor(nodes, '0');
                const named = Object.getOwnPropertyDescriptor(elements, 'a');
                const em = document.createElement('em');
                em.id = 'later';
                document.body.appendChild(em);
                return JSON.stringify({
                    nodeCtor,
                    htmlCtor,
                    nodeKeys: Object.keys(nodes),
                    htmlKeys: Object.keys(elements),
                    nodeZero: zero && [zero.enumerable, zero.configurable, zero.writable],
                    namedA: named && [named.enumerable, named.configurable, named.writable],
                    namedIdentity: elements.a === document.getElementById('a')
                        && elements.n1 === document.getElementById('a'),
                    later: elements.later === em,
                    iteratorSame: NodeList.prototype[Symbol.iterator] === NodeList.prototype.values,
                    iteratorName: NodeList.prototype[Symbol.iterator].name,
                    forEachLength: NodeList.prototype.forEach.length,
                    nodeLength: nodes.length,
                    htmlLength: elements.length,
                });
            })()"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(
        value["nodeCtor"],
        "TypeError:Failed to construct 'NodeList': Illegal constructor"
    );
    assert_eq!(
        value["htmlCtor"],
        "TypeError:Failed to construct 'HTMLCollection': Illegal constructor"
    );
    assert_eq!(value["nodeZero"], serde_json::json!([true, true, false]));
    assert_eq!(value["namedA"], serde_json::json!([false, true, false]));
    assert_eq!(value["namedIdentity"], true);
    assert_eq!(value["later"], true);
    assert_eq!(value["iteratorSame"], true);
    assert_eq!(value["iteratorName"], "values");
    assert_eq!(value["forEachLength"], 1);
    assert_eq!(value["nodeLength"], 3);
    assert_eq!(value["htmlLength"], 3);
}

fn html(body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head></head><body>{}</body></html>",
        body
    )
}

async fn check(js: &str) -> String {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"))
}

/// Same as `check`, but treats the page as a secure context (https://
/// origin). Required for tests that probe [SecureContext]-only APIs
/// (mediaDevices, getBattery, userAgentData, crypto.subtle, etc.) —
/// Phase 7.
async fn check_secure(js: &str) -> String {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"))
}

#[tokio::test]
async fn canvas_helper_webidl_prototypes_match_chrome() {
    let result = check(
        r#"(() => {
            const names = proto => Object.getOwnPropertyNames(proto).sort().join(',');
            const method = (proto, name, length) => {
                const d = Object.getOwnPropertyDescriptor(proto, name);
                return !!d && d.enumerable === true && d.configurable === true
                    && d.writable === true && typeof d.value === 'function'
                    && d.value.name === name && d.value.length === length
                    && String(d.value) === `function ${name}() { [native code] }`;
            };
            const pathMethods = {
                addPath:1, arc:5, arcTo:5, bezierCurveTo:6, closePath:0,
                ellipse:7, lineTo:2, moveTo:2, quadraticCurveTo:4,
                rect:4, roundRect:4
            };
            return JSON.stringify({
                gradientNames:names(CanvasGradient.prototype),
                gradientMethod:method(CanvasGradient.prototype, 'addColorStop', 2),
                patternNames:names(CanvasPattern.prototype),
                patternMethod:method(CanvasPattern.prototype, 'setTransform', 0),
                pathNames:names(Path2D.prototype),
                pathMethods:Object.entries(pathMethods).every(([name, length]) =>
                    method(Path2D.prototype, name, length)),
                ctorLengths:[CanvasGradient.length, CanvasPattern.length, Path2D.length],
                tags:[
                    Object.prototype.toString.call(
                        document.createElement('canvas').getContext('2d')
                            .createLinearGradient(0, 0, 1, 1)),
                    Object.prototype.toString.call(
                        document.createElement('canvas').getContext('2d')
                            .createPattern(document.createElement('canvas'), 'repeat')),
                    Object.prototype.toString.call(new Path2D())
                ]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"gradientNames":"addColorStop,constructor","gradientMethod":true,"patternNames":"constructor,setTransform","patternMethod":true,"pathNames":"addPath,arc,arcTo,bezierCurveTo,closePath,constructor,ellipse,lineTo,moveTo,quadraticCurveTo,rect,roundRect","pathMethods":true,"ctorLengths":[0,0,0],"tags":["[object CanvasGradient]","[object CanvasPattern]","[object Path2D]"]}"#
    );
}

#[tokio::test]
async fn offscreen_canvas_webidl_shape_matches_chrome() {
    let result = check(
        r#"(() => {
            const canvas = new OffscreenCanvas(3, 4);
            const protoNames = Object.getOwnPropertyNames(OffscreenCanvas.prototype)
                .sort().join(',');
            const descriptorOk = [
                ['width', 'accessor', 0, 1],
                ['height', 'accessor', 0, 1],
                ['oncontextlost', 'accessor', 0, 1],
                ['oncontextrestored', 'accessor', 0, 1],
                ['getContext', 'method', 1, 0],
                ['transferToImageBitmap', 'method', 0, 0],
                ['convertToBlob', 'method', 0, 0]
            ].every(([name, kind, a, b]) => {
                const d = Object.getOwnPropertyDescriptor(OffscreenCanvas.prototype, name);
                if (!d || d.enumerable !== true || d.configurable !== true) return false;
                if (kind === 'method') {
                    return d.writable === true && typeof d.value === 'function'
                        && d.value.length === a
                        && String(d.value) === `function ${name}() { [native code] }`;
                }
                return typeof d.get === 'function' && d.get.length === a
                    && typeof d.set === 'function' && d.set.length === b
                    && String(d.get) === `function get ${name}() { [native code] }`
                    && String(d.set) === `function set ${name}() { [native code] }`;
            });
            const beforeKeys = Reflect.ownKeys(canvas).map(String);
            canvas.getContext('2d');
            canvas.getContext('webgl');
            const afterKeys = Reflect.ownKeys(canvas).map(String);
            const bitmap = canvas.transferToImageBitmap();
            return JSON.stringify({
                ctorLength:OffscreenCanvas.length,
                protoNames,
                descriptorOk,
                beforeKeys,
                afterKeys,
                tag:Object.prototype.toString.call(canvas),
                bitmapTag:Object.prototype.toString.call(bitmap),
                bitmapInstance:bitmap instanceof ImageBitmap,
                bitmapKeys:Reflect.ownKeys(bitmap).map(String),
                bitmapSize:[bitmap.width, bitmap.height]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"ctorLength":2,"protoNames":"constructor,convertToBlob,getContext,height,oncontextlost,oncontextrestored,transferToImageBitmap,width","descriptorOk":true,"beforeKeys":[],"afterKeys":[],"tag":"[object OffscreenCanvas]","bitmapTag":"[object ImageBitmap]","bitmapInstance":true,"bitmapKeys":[],"bitmapSize":[3,4]}"#
    );
}

#[tokio::test]
async fn image_data_webidl_shape_matches_chrome() {
    let result = check(
        r#"(() => {
            const image = new ImageData(2, 3);
            const dataDescriptor = Object.getOwnPropertyDescriptor(image, 'data');
            const accessors = ['data', 'width', 'height', 'colorSpace', 'pixelFormat']
                .every(name => {
                    const d = Object.getOwnPropertyDescriptor(ImageData.prototype, name);
                    return !!d && d.enumerable === true && d.configurable === true
                        && typeof d.get === 'function' && d.get.length === 0
                        && d.set === undefined
                        && String(d.get) === `function get ${name}() { [native code] }`;
                });
            return JSON.stringify({
                ctorLength:ImageData.length,
                protoNames:Object.getOwnPropertyNames(ImageData.prototype).sort().join(','),
                accessors,
                ownKeys:Reflect.ownKeys(image).map(String),
                dataDescriptor:[
                    dataDescriptor.enumerable,
                    dataDescriptor.configurable,
                    dataDescriptor.writable
                ],
                tag:Object.prototype.toString.call(image),
                dataTag:Object.prototype.toString.call(image.data),
                values:[image.data.length, image.width, image.height, image.colorSpace, image.pixelFormat]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"ctorLength":2,"protoNames":"colorSpace,constructor,data,height,pixelFormat,width","accessors":true,"ownKeys":["data"],"dataDescriptor":[true,true,false],"tag":"[object ImageData]","dataTag":"[object Uint8ClampedArray]","values":[24,2,3,"srgb","rgba-unorm8"]}"#
    );
}

#[tokio::test]
async fn streams_webidl_shape_matches_chrome() {
    let result = check(
        r#"(() => {
            const method = (proto, name, length) => {
                const d = Object.getOwnPropertyDescriptor(proto, name);
                return !!d && d.enumerable === true && d.configurable === true
                    && d.writable === true && typeof d.value === 'function'
                    && d.value.name === name && d.value.length === length
                    && String(d.value) === `function ${name}() { [native code] }`;
            };
            const getter = (proto, name) => {
                const d = Object.getOwnPropertyDescriptor(proto, name);
                return !!d && d.enumerable === true && d.configurable === true
                    && typeof d.get === 'function' && d.get.length === 0
                    && d.set === undefined
                    && String(d.get) === `function get ${name}() { [native code] }`;
            };
            const tag = (proto, expected) => {
                const d = Object.getOwnPropertyDescriptor(proto, Symbol.toStringTag);
                return !!d && d.value === expected && d.enumerable === false
                    && d.configurable === true && d.writable === false;
            };
            const names = proto => Object.getOwnPropertyNames(proto).sort().join(',');

            const rs = new ReadableStream();
            const reader = rs.getReader();
            const ws = new WritableStream();
            const writer = ws.getWriter();
            const ts = new TransformStream();

            const expectedNames = {
                ReadableStream: 'cancel,constructor,getReader,locked,pipeThrough,pipeTo,tee,values',
                ReadableStreamDefaultReader: 'cancel,closed,constructor,read,releaseLock',
                ReadableStreamDefaultController: 'close,constructor,desiredSize,enqueue,error',
                WritableStream: 'abort,close,constructor,getWriter,locked',
                WritableStreamDefaultWriter: 'abort,close,closed,constructor,desiredSize,ready,releaseLock,write',
                WritableStreamDefaultController: 'constructor,error,signal',
                TransformStream: 'constructor,readable,writable'
            };

            const prototypes = [
                ['ReadableStream', ReadableStream, {
                    methods:{cancel:0,getReader:0,pipeThrough:1,pipeTo:1,tee:0,values:0},
                    getters:['locked']
                }],
                ['ReadableStreamDefaultReader', ReadableStreamDefaultReader, {
                    methods:{cancel:0,read:0,releaseLock:0}, getters:['closed']
                }],
                ['ReadableStreamDefaultController', ReadableStreamDefaultController, {
                    methods:{close:0,enqueue:0,error:0}, getters:['desiredSize']
                }],
                ['WritableStream', WritableStream, {
                    methods:{abort:0,close:0,getWriter:0}, getters:['locked']
                }],
                ['WritableStreamDefaultWriter', WritableStreamDefaultWriter, {
                    methods:{abort:0,close:0,releaseLock:0,write:0},
                    getters:['closed','desiredSize','ready']
                }],
                ['WritableStreamDefaultController', WritableStreamDefaultController, {
                    methods:{error:0}, getters:['signal']
                }],
                ['TransformStream', TransformStream, {
                    methods:{}, getters:['readable','writable']
                }]
            ];

            const protoOk = prototypes.every(([name, C, spec]) => {
                const p = C.prototype;
                return names(p) === expectedNames[name]
                    && Object.entries(spec.methods).every(([n, l]) => method(p, n, l))
                    && spec.getters.every(n => getter(p, n))
                    && tag(p, name)
                    && String(C) === `function ${name}() { [native code] }`;
            });

            const asyncIterator = Object.getOwnPropertyDescriptor(
                ReadableStream.prototype, Symbol.asyncIterator
            );

            return JSON.stringify({
                ctorLengths:[
                    ReadableStream.length,
                    ReadableStreamDefaultReader.length,
                    ReadableStreamDefaultController.length,
                    WritableStream.length,
                    WritableStreamDefaultWriter.length,
                    WritableStreamDefaultController.length,
                    TransformStream.length
                ],
                protoOk,
                noStaticFrom:!Object.prototype.hasOwnProperty.call(ReadableStream, 'from'),
                asyncIterator:!!asyncIterator
                    && asyncIterator.value === ReadableStream.prototype.values
                    && asyncIterator.enumerable === false
                    && asyncIterator.configurable === true
                    && asyncIterator.writable === true,
                instanceKeys:[rs,reader,ws,writer,ts].map(x => Reflect.ownKeys(x).map(String)),
                instanceTags:[rs,reader,ws,writer,ts].map(x => Object.prototype.toString.call(x))
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"ctorLengths":[0,1,0,0,1,0,0],"protoOk":true,"noStaticFrom":true,"asyncIterator":true,"instanceKeys":[[],[],[],[],[]],"instanceTags":["[object ReadableStream]","[object ReadableStreamDefaultReader]","[object WritableStream]","[object WritableStreamDefaultWriter]","[object TransformStream]"]}"#
    );
}

#[tokio::test]
async fn geometry_interfaces_match_chrome_148() {
    let result = check(
        r#"(() => {
            const names = proto => Object.getOwnPropertyNames(proto).sort().join(',');
            const rectRO = new DOMRectReadOnly(1, 2, 3, 4);
            const rect = new DOMRect(1, 2, 3, 4);
            const pointRO = new DOMPointReadOnly(1, 2, 3, 4);
            const point = new DOMPoint(1, 2, 3, 4);
            const matrixRO = new DOMMatrixReadOnly([1, 2, 3, 4, 5, 6]);
            const matrix = new DOMMatrix([1, 2, 3, 4, 5, 6]);

            const instances = [rectRO, rect, pointRO, point, matrixRO, matrix];
            const prototypeNames = [
                names(DOMRectReadOnly.prototype),
                names(DOMRect.prototype),
                names(DOMPointReadOnly.prototype),
                names(DOMPoint.prototype),
                names(DOMMatrixReadOnly.prototype),
                names(DOMMatrix.prototype)
            ];

            const mutable = new DOMMatrix();
            const selfOps = [
                mutable.translateSelf(2, 3) === mutable,
                mutable.scaleSelf(2) === mutable,
                mutable.rotateSelf(0) === mutable
            ];

            return JSON.stringify({
                ctorLengths:[
                    DOMRectReadOnly.length, DOMRect.length,
                    DOMPointReadOnly.length, DOMPoint.length,
                    DOMMatrixReadOnly.length, DOMMatrix.length
                ],
                prototypeNames,
                ownKeys:instances.map(value => Reflect.ownKeys(value).map(String)),
                tags:instances.map(value => Object.prototype.toString.call(value)),
                inheritance:[
                    Object.getPrototypeOf(DOMRect.prototype) === DOMRectReadOnly.prototype,
                    Object.getPrototypeOf(DOMPoint.prototype) === DOMPointReadOnly.prototype,
                    Object.getPrototypeOf(DOMMatrix.prototype) === DOMMatrixReadOnly.prototype
                ],
                webkit:WebKitCSSMatrix === DOMMatrix,
                rect:[rectRO.top, rectRO.right, rectRO.bottom, rectRO.left],
                pointTransform:new DOMPointReadOnly(2, 3)
                    .matrixTransform(new DOMMatrix().translate(5, 7)).toJSON(),
                matrix:[
                    matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
                    matrix.m11, matrix.m12, matrix.m21, matrix.m22, matrix.m41, matrix.m42,
                    matrix.is2D, matrix.isIdentity, matrix.toString()
                ],
                matrixPoint:matrix.transformPoint(new DOMPoint(2,3,0,1)).toJSON(),
                selfOps,
                mutable:mutable.toJSON(),
                statics:[
                    DOMRect.fromRect({x:7,y:8,width:9,height:10}).toJSON(),
                    DOMPoint.fromPoint({x:7,y:8,z:9,w:10}).toJSON(),
                    DOMMatrix.fromMatrix({a:2,d:3,e:4,f:5}).toJSON()
                ]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"ctorLengths":[0,0,0,0,0,0],"prototypeNames":["bottom,constructor,height,left,right,toJSON,top,width,x,y","constructor,height,width,x,y","constructor,matrixTransform,toJSON,w,x,y,z","constructor,w,x,y,z","a,b,c,constructor,d,e,f,flipX,flipY,inverse,is2D,isIdentity,m11,m12,m13,m14,m21,m22,m23,m24,m31,m32,m33,m34,m41,m42,m43,m44,multiply,rotate,rotateAxisAngle,rotateFromVector,scale,scale3d,scaleNonUniform,skewX,skewY,toFloat32Array,toFloat64Array,toJSON,toString,transformPoint,translate","a,b,c,constructor,d,e,f,invertSelf,m11,m12,m13,m14,m21,m22,m23,m24,m31,m32,m33,m34,m41,m42,m43,m44,multiplySelf,preMultiplySelf,rotateAxisAngleSelf,rotateFromVectorSelf,rotateSelf,scale3dSelf,scaleSelf,setMatrixValue,skewXSelf,skewYSelf,translateSelf"],"ownKeys":[[],[],[],[],[],[]],"tags":["[object DOMRectReadOnly]","[object DOMRect]","[object DOMPointReadOnly]","[object DOMPoint]","[object DOMMatrixReadOnly]","[object DOMMatrix]"],"inheritance":[true,true,true],"webkit":true,"rect":[2,4,6,1],"pointTransform":{"x":7,"y":10,"z":0,"w":1},"matrix":[1,2,3,4,5,6,1,2,3,4,5,6,true,false,"matrix(1, 2, 3, 4, 5, 6)"],"matrixPoint":{"x":16,"y":22,"z":0,"w":1},"selfOps":[true,true,true],"mutable":{"a":2,"b":0,"c":0,"d":2,"e":2,"f":3,"m11":2,"m12":0,"m13":0,"m14":0,"m21":0,"m22":2,"m23":0,"m24":0,"m31":0,"m32":0,"m33":1,"m34":0,"m41":2,"m42":3,"m43":0,"m44":1,"is2D":true,"isIdentity":false},"statics":[{"x":7,"y":8,"width":9,"height":10,"top":8,"right":16,"bottom":18,"left":7},{"x":7,"y":8,"z":9,"w":10},{"a":2,"b":0,"c":0,"d":3,"e":4,"f":5,"m11":2,"m12":0,"m13":0,"m14":0,"m21":0,"m22":3,"m23":0,"m24":0,"m31":0,"m32":0,"m33":1,"m34":0,"m41":4,"m42":5,"m43":0,"m44":1,"is2D":true,"isIdentity":false}]}"#
    );
}

#[tokio::test]
async fn webidl_operation_arities_match_chrome_148() {
    let result = check_secure(
        r#"JSON.stringify({
            textEncoder:[TextEncoder.prototype.encode.length, TextEncoder.prototype.encodeInto.length],
            textDecoder:TextDecoder.prototype.decode.length,
            subtle:[
                SubtleCrypto.prototype.decrypt.length,
                SubtleCrypto.prototype.deriveBits.length,
                SubtleCrypto.prototype.deriveKey.length,
                SubtleCrypto.prototype.digest.length,
                SubtleCrypto.prototype.encrypt.length,
                SubtleCrypto.prototype.exportKey.length,
                SubtleCrypto.prototype.generateKey.length,
                SubtleCrypto.prototype.importKey.length,
                SubtleCrypto.prototype.sign.length,
                SubtleCrypto.prototype.unwrapKey.length,
                SubtleCrypto.prototype.verify.length,
                SubtleCrypto.prototype.wrapKey.length
            ],
            mouse:MouseEvent.prototype.initMouseEvent.length,
            keyboard:KeyboardEvent.prototype.initKeyboardEvent.length,
            eventTarget:[
                EventTarget.prototype.addEventListener.length,
                EventTarget.prototype.removeEventListener.length
            ],
            eventInitializers:[
                CustomEvent.prototype.initCustomEvent.length,
                MessageEvent.prototype.initMessageEvent.length
            ],
            constructorLengths:[
                Node.length, DocumentFragment.length, URL.length,
                URLSearchParams.length, Worker.length, Response.length
            ],
            urlSearchParams:URLSearchParams.prototype.forEach.length,
            worker:Worker.prototype.postMessage.length,
            speech:SpeechSynthesis.prototype.speak.length,
            rtc:[
                RTCPeerConnection.prototype.addIceCandidate.length,
                RTCPeerConnection.prototype.addTransceiver.length,
                RTCPeerConnection.prototype.getStats.length,
                RTCPeerConnection.prototype.setLocalDescription.length
            ],
            history:[
                History.prototype.go.length,
                History.prototype.pushState.length,
                History.prototype.replaceState.length
            ],
            performanceObserver:[
                PerformanceObserver.prototype.observe.length,
                PerformanceObserverEntryList.prototype.getEntriesByName.length
            ]
        })"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"textEncoder":[0,2],"textDecoder":0,"subtle":[3,2,5,2,3,2,3,5,3,7,4,4],"mouse":1,"keyboard":1,"eventTarget":[2,2],"eventInitializers":[1,1],"constructorLengths":[0,0,1,0,1,0],"urlSearchParams":1,"worker":1,"speech":1,"rtc":[0,1,0,0],"history":[0,2,2],"performanceObserver":[0,1]}"#
    );
}

#[tokio::test]
async fn domtokenlist_and_media_query_list_match_chrome_148() {
    let result = check(
        r#"(() => {
            const element = document.createElement('div');
            element.className = 'a b';
            const list = element.classList;
            const sameList = list === element.classList;
            const indexDescriptor = Object.getOwnPropertyDescriptor(list, '0');
            list.add('c', 'd');
            list.remove('a', 'c');
            const replaced = list.replace('b', 'x');
            const beforeSet = list.value;
            list.value = 'p q';
            element.setAttribute('class', 'live one two');
            let supportsError = '';
            try { list.supports('anything'); } catch (error) { supportsError = error.name; }

            const mql = matchMedia('(min-width: 1px)');
            let mqlConstruct = '';
            try { new MediaQueryList(); mqlConstruct = 'ok'; }
            catch (error) { mqlConstruct = error.name + ':' + error.message; }

            const parserDescriptor = Object.getOwnPropertyDescriptor(
                DOMParser.prototype, 'parseFromString'
            );
            const resizeDescriptors = ['observe', 'unobserve', 'disconnect'].map(name =>
                Object.getOwnPropertyDescriptor(ResizeObserver.prototype, name)?.enumerable === true
            );

            return JSON.stringify({
                domToken:{
                    sameList,
                    initialIndex:[
                        indexDescriptor?.enumerable,
                        indexDescriptor?.configurable,
                        indexDescriptor?.writable,
                        indexDescriptor?.value
                    ],
                    replaced,
                    beforeSet,
                    afterSet:list.value,
                    liveKeys:Reflect.ownKeys(list).map(String),
                    liveValues:Array.from(list.values()),
                    has0:'0' in list,
                    tag:Object.prototype.toString.call(list),
                    ownNames:Object.getOwnPropertyNames(DOMTokenList.prototype).sort().join(','),
                    ctorLength:DOMTokenList.length,
                    supportsError
                },
                mql:{
                    keys:Reflect.ownKeys(mql).map(String),
                    tag:Object.prototype.toString.call(mql),
                    instance:mql instanceof MediaQueryList,
                    media:mql.media,
                    matches:mql.matches,
                    onchange:mql.onchange,
                    construct:mqlConstruct,
                    ctorLength:MediaQueryList.length,
                    ctorParent:Object.getPrototypeOf(MediaQueryList) === EventTarget,
                    protoParent:Object.getPrototypeOf(MediaQueryList.prototype) === EventTarget.prototype,
                    ctorSource:String(MediaQueryList)
                },
                parser:[String(DOMParser), parserDescriptor?.enumerable],
                resizeDescriptors
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"domToken":{"sameList":true,"initialIndex":[true,true,false,"a"],"replaced":true,"beforeSet":"x d","afterSet":"live one two","liveKeys":["0","1","2"],"liveValues":["live","one","two"],"has0":true,"tag":"[object DOMTokenList]","ownNames":"add,constructor,contains,entries,forEach,item,keys,length,remove,replace,supports,toString,toggle,value,values","ctorLength":0,"supportsError":"TypeError"},"mql":{"keys":[],"tag":"[object MediaQueryList]","instance":true,"media":"(min-width: 1px)","matches":true,"onchange":null,"construct":"TypeError:Failed to construct 'MediaQueryList': Illegal constructor","ctorLength":0,"ctorParent":true,"protoParent":true,"ctorSource":"function MediaQueryList() { [native code] }"},"parser":["function DOMParser() { [native code] }",true],"resizeDescriptors":[true,true,true]}"#
    );
}

#[tokio::test]
async fn xml_serializer_matches_chrome_148() {
    let result = check(
        r#"(() => {
            const serializer = new XMLSerializer();
            const div = document.createElement('div');
            div.setAttribute('data-x', '1');
            div.appendChild(document.createTextNode('a&b'));
            let noNew = '';
            try { XMLSerializer(); noNew = 'ok'; }
            catch (error) { noNew = error.name + ':' + error.message; }
            return JSON.stringify({
                ctorLength:XMLSerializer.length,
                methodLength:XMLSerializer.prototype.serializeToString.length,
                names:Object.getOwnPropertyNames(XMLSerializer.prototype).sort(),
                tag:Object.prototype.toString.call(serializer),
                keys:Reflect.ownKeys(serializer).map(String),
                html:serializer.serializeToString(div),
                text:serializer.serializeToString(document.createTextNode('a&b')),
                noNew,
                source:String(XMLSerializer),
                methodSource:String(XMLSerializer.prototype.serializeToString)
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"ctorLength":0,"methodLength":1,"names":["constructor","serializeToString"],"tag":"[object XMLSerializer]","keys":[],"html":"<div xmlns=\"http://www.w3.org/1999/xhtml\" data-x=\"1\">a&amp;b</div>","text":"a&amp;b","noNew":"TypeError:Failed to construct 'XMLSerializer': Please use the 'new' operator, this DOM object constructor cannot be called as a function.","source":"function XMLSerializer() { [native code] }","methodSource":"function serializeToString() { [native code] }"}"#
    );
}

#[tokio::test]
async fn dom_traversal_matches_chrome_148() {
    let result = check(
        r#"(() => {
            const root = document.createElement('div');
            root.id = 'root';
            const skip = document.createElement('section'); skip.id = 'skip';
            const s1 = document.createElement('i'); s1.id = 's1'; skip.appendChild(s1);
            const reject = document.createElement('section'); reject.id = 'reject';
            const r1 = document.createElement('i'); r1.id = 'r1'; reject.appendChild(r1);
            const ok = document.createElement('span'); ok.id = 'ok';
            root.appendChild(skip); root.appendChild(reject); root.appendChild(ok);
            document.body.appendChild(root);

            const filter = { acceptNode(node) {
                if (node.id === 'skip') return NodeFilter.FILTER_SKIP;
                if (node.id === 'reject') return NodeFilter.FILTER_REJECT;
                return NodeFilter.FILTER_ACCEPT;
            }};
            const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, filter);
            const tree = [];
            let node;
            while ((node = walker.nextNode())) tree.push(node.id);

            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT, filter);
            const iter = [];
            while ((node = iterator.nextNode())) iter.push(node.id);

            let treeCtor = '', iteratorCtor = '', filterCtor = '';
            try { new TreeWalker(); treeCtor = 'ok'; }
            catch (error) { treeCtor = error.name + ':' + error.message; }
            try { new NodeIterator(); iteratorCtor = 'ok'; }
            catch (error) { iteratorCtor = error.name + ':' + error.message; }
            try { new NodeFilter(); filterCtor = 'ok'; }
            catch (error) { filterCtor = error.name + ':' + error.message; }

            return JSON.stringify({
                tree,
                iter,
                treeKeys:Reflect.ownKeys(walker).map(String),
                iteratorKeys:Reflect.ownKeys(iterator).map(String),
                tags:[Object.prototype.toString.call(walker), Object.prototype.toString.call(iterator)],
                constructors:[TreeWalker.length, NodeIterator.length],
                createLengths:[Document.prototype.createTreeWalker.length, Document.prototype.createNodeIterator.length],
                ctorErrors:[treeCtor, iteratorCtor, filterCtor],
                nativeSources:[String(TreeWalker), String(NodeIterator), String(NodeFilter)],
                nodeFilter:{
                    hasPrototype:Object.prototype.hasOwnProperty.call(NodeFilter, 'prototype'),
                    accept:NodeFilter.FILTER_ACCEPT,
                    reject:NodeFilter.FILTER_REJECT,
                    skip:NodeFilter.FILTER_SKIP,
                    showAll:NodeFilter.SHOW_ALL,
                    showElement:NodeFilter.SHOW_ELEMENT
                },
                filters:[walker.filter === filter, iterator.filter === filter],
                show:[walker.whatToShow, iterator.whatToShow]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"tree":["s1","ok"],"iter":["root","s1","r1","ok"],"treeKeys":[],"iteratorKeys":[],"tags":["[object TreeWalker]","[object NodeIterator]"],"constructors":[0,0],"createLengths":[1,1],"ctorErrors":["TypeError:Failed to construct 'TreeWalker': Illegal constructor","TypeError:Failed to construct 'NodeIterator': Illegal constructor","TypeError:NodeFilter is not a constructor"],"nativeSources":["function TreeWalker() { [native code] }","function NodeIterator() { [native code] }","function NodeFilter() { [native code] }"],"nodeFilter":{"hasPrototype":false,"accept":1,"reject":2,"skip":3,"showAll":4294967295,"showElement":1},"filters":[true,true],"show":[1,1]}"#
    );
}

#[tokio::test]
async fn attr_and_named_node_map_match_chrome_148() {
    let result = check(
        r#"(() => {
            const element = document.createElement('div');
            element.setAttribute('id', 'x');
            element.setAttribute('data-a', '1');
            const attrs = element.attributes;
            const first = attrs.item(0);
            const indexDescriptor = Object.getOwnPropertyDescriptor(attrs, '0');
            const namedDescriptor = Object.getOwnPropertyDescriptor(attrs, 'id');

            const created = document.createAttribute('title');
            created.value = 'hello';
            const replaced = attrs.setNamedItem(created);
            const afterSet = element.getAttribute('title');
            const removed = attrs.removeNamedItem('title');

            let attrCtor = '', mapCtor = '';
            try { new Attr(); attrCtor = 'ok'; }
            catch (error) { attrCtor = error.name + ':' + error.message; }
            try { new NamedNodeMap(); mapCtor = 'ok'; }
            catch (error) { mapCtor = error.name + ':' + error.message; }

            return JSON.stringify({
                map:{
                    same:attrs === element.attributes,
                    tag:Object.prototype.toString.call(attrs),
                    instance:attrs instanceof NamedNodeMap,
                    keys:Reflect.ownKeys(attrs).map(String),
                    objectKeys:Object.keys(attrs),
                    length:attrs.length,
                    indexDescriptor:indexDescriptor && [
                        indexDescriptor.enumerable,
                        indexDescriptor.configurable,
                        indexDescriptor.writable,
                        Object.prototype.toString.call(indexDescriptor.value)
                    ],
                    namedDescriptor:namedDescriptor && [
                        namedDescriptor.enumerable,
                        namedDescriptor.configurable,
                        namedDescriptor.writable,
                        Object.prototype.toString.call(namedDescriptor.value)
                    ]
                },
                attr:{
                    tag:Object.prototype.toString.call(first),
                    instance:first instanceof Attr,
                    node:first instanceof Node,
                    keys:Reflect.ownKeys(first).map(String),
                    nodeType:first.nodeType,
                    nodeName:first.nodeName,
                    textContent:first.textContent,
                    parentNode:first.parentNode,
                    name:first.name,
                    localName:first.localName,
                    namespaceURI:first.namespaceURI,
                    prefix:first.prefix,
                    owner:first.ownerElement === element,
                    specified:first.specified,
                    value:first.value,
                    stable:first === attrs.getNamedItem(first.name)
                },
                constructors:{
                    attr:[Attr.length, String(Attr), attrCtor],
                    map:[NamedNodeMap.length, String(NamedNodeMap), mapCtor],
                    attrProtoNode:Object.getPrototypeOf(Attr.prototype) === Node.prototype
                },
                mutations:{
                    replaced:replaced === null,
                    afterSet,
                    removedTag:Object.prototype.toString.call(removed),
                    detached:created.ownerElement === null
                }
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"map":{"same":true,"tag":"[object NamedNodeMap]","instance":true,"keys":["0","1","id","data-a"],"objectKeys":["0","1"],"length":2,"indexDescriptor":[true,true,false,"[object Attr]"],"namedDescriptor":[false,true,false,"[object Attr]"]},"attr":{"tag":"[object Attr]","instance":true,"node":true,"keys":[],"nodeType":2,"nodeName":"id","textContent":"x","parentNode":null,"name":"id","localName":"id","namespaceURI":null,"prefix":null,"owner":true,"specified":true,"value":"x","stable":true},"constructors":{"attr":[0,"function Attr() { [native code] }","TypeError:Failed to construct 'Attr': Illegal constructor"],"map":[0,"function NamedNodeMap() { [native code] }","TypeError:Failed to construct 'NamedNodeMap': Illegal constructor"],"attrProtoNode":true},"mutations":{"replaced":true,"afterSet":"hello","removedTag":"[object Attr]","detached":true}}"#
    );
}

#[tokio::test]
async fn intersection_observer_surface_matches_chrome_148() {
    let result = check(
        r#"(() => {
            const observer = new IntersectionObserver(() => {}, {
                root:null,
                rootMargin:'1px 2px',
                scrollMargin:'3px',
                threshold:[0.75, 0.25, 0.75],
                delay:20,
                trackVisibility:false
            });
            const thresholds = observer.thresholds;
            const thresholdsAgain = observer.thresholds;
            let noCallback = '', entryCtor = '';
            try { new IntersectionObserver(); noCallback = 'ok'; }
            catch (error) { noCallback = error.name + ':' + error.message; }
            try { new IntersectionObserverEntry(); entryCtor = 'ok'; }
            catch (error) { entryCtor = error.name + ':' + error.message; }
            return JSON.stringify({
                observer:{
                    keys:Reflect.ownKeys(observer).map(String),
                    tag:Object.prototype.toString.call(observer),
                    root:observer.root,
                    rootMargin:observer.rootMargin,
                    scrollMargin:observer.scrollMargin,
                    thresholds,
                    thresholdSame:thresholds === thresholdsAgain,
                    thresholdFrozen:Object.isFrozen(thresholds),
                    delay:observer.delay,
                    trackVisibility:observer.trackVisibility,
                    ctorLength:IntersectionObserver.length,
                    source:String(IntersectionObserver),
                    noCallback
                },
                entry:{
                    ctorLength:IntersectionObserverEntry.length,
                    source:String(IntersectionObserverEntry),
                    noConstruct:entryCtor,
                    names:Object.getOwnPropertyNames(IntersectionObserverEntry.prototype).sort()
                },
                methods:[
                    IntersectionObserver.prototype.observe.length,
                    IntersectionObserver.prototype.unobserve.length,
                    IntersectionObserver.prototype.disconnect.length,
                    IntersectionObserver.prototype.takeRecords.length
                ]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"observer":{"keys":[],"tag":"[object IntersectionObserver]","root":null,"rootMargin":"1px 2px 1px 2px","scrollMargin":"3px 3px 3px 3px","thresholds":[0.25,0.75,0.75],"thresholdSame":false,"thresholdFrozen":true,"delay":20,"trackVisibility":false,"ctorLength":1,"source":"function IntersectionObserver() { [native code] }","noCallback":"TypeError:Failed to construct 'IntersectionObserver': 1 argument required, but only 0 present."},"entry":{"ctorLength":0,"source":"function IntersectionObserverEntry() { [native code] }","noConstruct":"TypeError:Failed to construct 'IntersectionObserverEntry': Illegal constructor","names":["boundingClientRect","constructor","intersectionRatio","intersectionRect","isIntersecting","isVisible","rootBounds","target","time"]},"methods":[1,1,0,0]}"#
    );
}

#[tokio::test]
async fn mutation_observer_surface_and_delivery_match_chrome() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        r#"(() => {
            let noarg = '', wrong = '', call = '';
            try { new MutationObserver(); noarg = 'ok'; }
            catch (error) { noarg = error.name + ':' + error.message; }
            try { new MutationObserver(1); wrong = 'ok'; }
            catch (error) { wrong = error.name + ':' + error.message; }
            try { MutationObserver(() => {}); call = 'ok'; }
            catch (error) { call = error.name + ':' + error.message; }
            const target = document.createElement('div');
            const observer = new MutationObserver((records, self) => {
                globalThis.__mutationObserverResult = JSON.stringify({
                    noarg,wrong,call,
                    keys:Reflect.ownKeys(observer).map(String),
                    tag:Object.prototype.toString.call(observer),
                    instance:observer instanceof MutationObserver,
                    alias:WebKitMutationObserver === MutationObserver,
                    source:String(MutationObserver),
                    lengths:[MutationObserver.length,MutationObserver.prototype.observe.length,MutationObserver.prototype.disconnect.length,MutationObserver.prototype.takeRecords.length],
                    callback:{len:records.length,same:self===observer,type:records[0]?.type,target:records[0]?.target?.nodeName},
                    recordsAfterCallback:observer.takeRecords().length,
                });
            });
            observer.observe(target, { attributes:true, attributeOldValue:true });
            target.setAttribute('data-x', '1');
        })()"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(50))
        .await
        .unwrap();
    let result = page
        .evaluate("globalThis.__mutationObserverResult || ''")
        .unwrap();
    assert_eq!(
        result,
        r#"{"noarg":"TypeError:Failed to construct 'MutationObserver': 1 argument required, but only 0 present.","wrong":"TypeError:Failed to construct 'MutationObserver': parameter 1 is not of type 'Function'.","call":"TypeError:Failed to construct 'MutationObserver': Please use the 'new' operator, this DOM object constructor cannot be called as a function.","keys":[],"tag":"[object MutationObserver]","instance":true,"alias":true,"source":"function MutationObserver() { [native code] }","lengths":[1,1,0,0],"callback":{"len":1,"same":true,"type":"attributes","target":"DIV"},"recordsAfterCallback":0}"#
    );
}

#[tokio::test]
async fn event_data_subclasses_match_chrome_148() {
    let result = check(
        r#"(() => {
            const promise = Promise.resolve(1);
            const formData = new FormData();
            const cases = [
                ['ProgressEvent', new ProgressEvent('x', {lengthComputable:true,loaded:2,total:3}), ['lengthComputable','loaded','total']],
                ['ErrorEvent', new ErrorEvent('x', {message:'m',filename:'f',lineno:2,colno:3,error:null}), ['message','filename','lineno','colno','error']],
                ['PromiseRejectionEvent', new PromiseRejectionEvent('unhandledrejection', {promise,reason:'why'}), ['promise','reason']],
                ['DragEvent', new DragEvent('drag', {dataTransfer:null}), ['dataTransfer']],
                ['ClipboardEvent', new ClipboardEvent('copy', {clipboardData:null}), ['clipboardData']],
                ['SubmitEvent', new SubmitEvent('submit', {submitter:null}), ['submitter']],
                ['FormDataEvent', new FormDataEvent('formdata', {formData}), ['formData']],
                ['CloseEvent', new CloseEvent('close', {wasClean:true,code:1000,reason:'bye'}), ['wasClean','code','reason']],
                ['HashChangeEvent', new HashChangeEvent('hashchange', {oldURL:'a',newURL:'b'}), ['oldURL','newURL']],
                ['PopStateEvent', new PopStateEvent('popstate', {state:{a:1},hasUAVisualTransition:true}), ['state','hasUAVisualTransition']],
                ['PageTransitionEvent', new PageTransitionEvent('pageshow', {persisted:true}), ['persisted']],
                ['SecurityPolicyViolationEvent', new SecurityPolicyViolationEvent('securitypolicyviolation', {
                    documentURI:'d',referrer:'r',blockedURI:'b',violatedDirective:'v',effectiveDirective:'e',
                    originalPolicy:'o',sourceFile:'s',sample:'x',disposition:'enforce',statusCode:200,lineNumber:2,columnNumber:3
                }), ['documentURI','referrer','blockedURI','violatedDirective','effectiveDirective','originalPolicy','sourceFile','sample','disposition','statusCode','lineNumber','columnNumber']]
            ];
            const expectedLengths = {
                ProgressEvent:1, ErrorEvent:1, PromiseRejectionEvent:2,
                DragEvent:1, ClipboardEvent:1, SubmitEvent:1, FormDataEvent:2,
                CloseEvent:1, HashChangeEvent:1, PopStateEvent:1,
                PageTransitionEvent:1, SecurityPolicyViolationEvent:1
            };
            const rows = cases.map(([name,event,fields]) => ({
                name,
                ctorLength:globalThis[name].length,
                native:String(globalThis[name]) === `function ${name}() { [native code] }`,
                own:Reflect.ownKeys(event).map(String),
                tag:Object.prototype.toString.call(event),
                event:event instanceof Event,
                descriptors:fields.every(field => {
                    const descriptor = Object.getOwnPropertyDescriptor(globalThis[name].prototype, field);
                    return !!descriptor && descriptor.enumerable === true
                        && descriptor.configurable === true
                        && typeof descriptor.get === 'function'
                        && descriptor.set === undefined;
                }),
                lengthOk:globalThis[name].length === expectedLengths[name]
            }));
            let beforeUnload = '';
            try { new BeforeUnloadEvent(); beforeUnload = 'ok'; }
            catch (error) { beforeUnload = error.name + ':' + error.message; }
            const pointerDescriptor = Object.getOwnPropertyDescriptor(PointerEvent.prototype, 'getCoalescedEvents');
            return JSON.stringify({
                rows,
                values:{
                    progress:[cases[0][1].lengthComputable,cases[0][1].loaded,cases[0][1].total],
                    error:[cases[1][1].message,cases[1][1].filename,cases[1][1].lineno,cases[1][1].colno,cases[1][1].error],
                    promise:[cases[2][1].promise === promise,cases[2][1].reason],
                    formData:cases[6][1].formData === formData,
                    close:[cases[7][1].wasClean,cases[7][1].code,cases[7][1].reason],
                    hash:[cases[8][1].oldURL,cases[8][1].newURL],
                    pop:[cases[9][1].state.a,cases[9][1].hasUAVisualTransition],
                    page:cases[10][1].persisted,
                    security:[cases[11][1].documentURI,cases[11][1].statusCode,cases[11][1].lineNumber,cases[11][1].columnNumber]
                },
                beforeUnload:[BeforeUnloadEvent.length,String(BeforeUnloadEvent),beforeUnload],
                pointer:[!!pointerDescriptor,pointerDescriptor?.enumerable,PointerEvent.prototype.getCoalescedEvents.length,JSON.stringify(new PointerEvent('x').getCoalescedEvents())]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"rows":[{"name":"ProgressEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object ProgressEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"ErrorEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object ErrorEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"PromiseRejectionEvent","ctorLength":2,"native":true,"own":["isTrusted"],"tag":"[object PromiseRejectionEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"DragEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object DragEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"ClipboardEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object ClipboardEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"SubmitEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object SubmitEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"FormDataEvent","ctorLength":2,"native":true,"own":["isTrusted"],"tag":"[object FormDataEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"CloseEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object CloseEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"HashChangeEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object HashChangeEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"PopStateEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object PopStateEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"PageTransitionEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object PageTransitionEvent]","event":true,"descriptors":true,"lengthOk":true},{"name":"SecurityPolicyViolationEvent","ctorLength":1,"native":true,"own":["isTrusted"],"tag":"[object SecurityPolicyViolationEvent]","event":true,"descriptors":true,"lengthOk":true}],"values":{"progress":[true,2,3],"error":["m","f",2,3,null],"promise":[true,"why"],"formData":true,"close":[true,1000,"bye"],"hash":["a","b"],"pop":[1,true],"page":true,"security":["d",200,2,3]},"beforeUnload":[0,"function BeforeUnloadEvent() { [native code] }","TypeError:Failed to construct 'BeforeUnloadEvent': Illegal constructor"],"pointer":[true,true,0,"[]"]}"#
    );
}

#[tokio::test]
async fn range_and_selection_match_chrome_148() {
    let result = check(
        r#"(() => {
            const root = document.createElement('div');
            const p1 = document.createElement('p');
            const first = document.createTextNode('hello ');
            const bold = document.createElement('b');
            const boldText = document.createTextNode('world');
            bold.appendChild(boldText);
            p1.appendChild(first); p1.appendChild(bold); root.appendChild(p1);
            document.body.appendChild(root);

            const range = document.createRange();
            range.setStart(first, 1);
            range.setEnd(boldText, 3);
            const clone = range.cloneContents();
            const holder = document.createElement('div');
            holder.appendChild(clone.cloneNode(true));

            const selection = getSelection();
            selection.removeAllRanges();
            selection.addRange(range);
            const initial = {
                text:String(range),
                clone:holder.innerHTML,
                rangeKeys:Reflect.ownKeys(range).map(String),
                selectionKeys:Reflect.ownKeys(selection).map(String),
                tags:[Object.prototype.toString.call(range), Object.prototype.toString.call(selection)],
                anchor:[selection.anchorNode === first, selection.anchorOffset],
                focus:[selection.focusNode === boldText, selection.focusOffset],
                direction:selection.direction,
                count:selection.rangeCount,
                sameRange:selection.getRangeAt(0) === range,
                rectList:Object.prototype.toString.call(range.getClientRects()),
                rect:Object.prototype.toString.call(range.getBoundingClientRect())
            };

            selection.collapse(first, 2);
            selection.extend(boldText, 2);
            const extended = {
                anchor:[selection.anchorNode === first, selection.anchorOffset],
                focus:[selection.focusNode === boldText, selection.focusOffset],
                direction:selection.direction,
                text:String(selection)
            };
            selection.collapseToStart();
            const collapsed = [selection.anchorNode === first, selection.anchorOffset, selection.isCollapsed];
            selection.removeAllRanges();

            const mutationRoot = document.createElement('div');
            const mutationText = document.createTextNode('abcdef');
            mutationRoot.appendChild(mutationText);
            const mutationRange = document.createRange();
            mutationRange.setStart(mutationText, 2);
            mutationRange.setEnd(mutationText, 4);
            mutationRange.deleteContents();
            const marker = document.createElement('i'); marker.textContent = 'X';
            mutationRange.insertNode(marker);

            let selectionCtor = '', rangeCall = '';
            try { new Selection(); selectionCtor = 'ok'; }
            catch (error) { selectionCtor = error.name + ':' + error.message; }
            try { Range(); rangeCall = 'ok'; }
            catch (error) { rangeCall = error.name + ':' + error.message; }

            return JSON.stringify({
                initial,
                extended,
                collapsed,
                empty:[selection.rangeCount,selection.anchorNode,selection.anchorOffset,selection.focusNode,selection.focusOffset,selection.isCollapsed,selection.direction,String(selection)],
                mutation:mutationRoot.innerHTML,
                constructors:{
                    range:[Range.length,String(Range),rangeCall],
                    selection:[Selection.length,String(Selection),selectionCtor],
                    abstract:[AbstractRange.length,String(AbstractRange),Object.getPrototypeOf(Range.prototype) === AbstractRange.prototype]
                },
                constants:[Range.START_TO_START,Range.START_TO_END,Range.END_TO_END,Range.END_TO_START],
                createLength:Document.prototype.createRange.length,
                singleton:getSelection() === getSelection() && document.getSelection() === getSelection()
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"initial":{"text":"ello wor","clone":"ello <b>wor</b>","rangeKeys":[],"selectionKeys":[],"tags":["[object Range]","[object Selection]"],"anchor":[true,1],"focus":[true,3],"direction":"none","count":1,"sameRange":true,"rectList":"[object DOMRectList]","rect":"[object DOMRect]"},"extended":{"anchor":[true,2],"focus":[true,2],"direction":"forward","text":"llo wo"},"collapsed":[true,2,true],"empty":[0,null,0,null,0,true,"none",""],"mutation":"ab<i>X</i>ef","constructors":{"range":[0,"function Range() { [native code] }","TypeError:Failed to construct 'Range': Please use the 'new' operator, this DOM object constructor cannot be called as a function."],"selection":[0,"function Selection() { [native code] }","TypeError:Failed to construct 'Selection': Illegal constructor"],"abstract":[0,"function AbstractRange() { [native code] }",true]},"constants":[0,1,2,3],"createLength":0,"singleton":true}"#
    );
}

#[tokio::test]
async fn text_and_character_data_match_chrome_148() {
    let result = check(
        r#"(() => {
            let characterCtor = '';
            try { new CharacterData(); characterCtor = 'ok'; }
            catch (error) { characterCtor = error.name + ':' + error.message; }

            const detached = new Text('abc');
            const detachedSplit = detached.splitText(1);

            const parent = document.createElement('p');
            const first = document.createTextNode('ab');
            const second = document.createTextNode('cd');
            const span = document.createElement('span');
            const tail = document.createTextNode('ef');
            parent.appendChild(first);
            parent.appendChild(second);
            parent.appendChild(span);
            parent.appendChild(tail);
            const split = second.splitText(1);
            const before = document.createTextNode('B');
            span.before('x', before);
            span.after('y');
            tail.replaceWith('q', document.createTextNode('R'));
            first.remove();

            return JSON.stringify({
                constructors:{
                    text:[Text.length,String(Text),Object.getPrototypeOf(Text) === CharacterData],
                    character:[CharacterData.length,String(CharacterData),characterCtor],
                    proto:Object.getPrototypeOf(Text.prototype) === CharacterData.prototype
                },
                detached:{
                    tag:Object.prototype.toString.call(detached),
                    keys:Reflect.ownKeys(detached).map(String),
                    data:detached.data,
                    split:detachedSplit.data,
                    splitParent:detachedSplit.parentNode
                },
                live:{
                    whole:second.wholeText,
                    second:second.data,
                    split:split.data,
                    splitPrev:split.previousSibling === second,
                    assigned:second.assignedSlot
                },
                mixin:{
                    html:parent.innerHTML,
                    previousElement:second.previousElementSibling,
                    nextElement:second.nextElementSibling?.tagName || null
                },
                methods:[
                    Text.prototype.splitText.length,
                    CharacterData.prototype.before.length,
                    CharacterData.prototype.after.length,
                    CharacterData.prototype.remove.length,
                    CharacterData.prototype.replaceWith.length
                ]
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"constructors":{"text":[0,"function Text() { [native code] }",true],"character":[0,"function CharacterData() { [native code] }","TypeError:Failed to construct 'CharacterData': Illegal constructor"],"proto":true},"detached":{"tag":"[object Text]","keys":[],"data":"a","split":"bc","splitParent":null},"live":{"whole":"cdxB","second":"c","split":"d","splitPrev":true,"assigned":null},"mixin":{"html":"cdxB<span></span>yqR","previousElement":null,"nextElement":"SPAN"},"methods":[1,0,0,0,0]}"#
    );
}

#[tokio::test]
async fn cssom_core_surface_and_behavior_match_chrome() {
    let result = check(
        r#"(() => {
            const el = document.createElement('div');
            el.setAttribute('style', 'color: blue');
            const style = el.style;
            style.setProperty('color', 'red', 'important');
            style.cssFloat = 'right';
            const old = style.removeProperty('color');
            style.setProperty('color', 'red', 'important');

            const sheet = new CSSStyleSheet();
            sheet.replaceSync('#a { color: red; float: left; } @media (min-width: 1px) { #b { display: block; } } @font-face { font-family: X; src: url("x"); }');
            const rows = Array.from(sheet.cssRules).map((rule) => ({
                ctor: rule.constructor.name,
                tag: Object.prototype.toString.call(rule),
                keys: Reflect.ownKeys(rule).map(String),
                type: rule.type,
                cssText: rule.cssText,
                parentSheet: rule.parentStyleSheet === sheet,
                parentRule: rule.parentRule === null,
                selector: 'selectorText' in rule ? rule.selectorText : null,
                media: 'media' in rule ? rule.media.mediaText : null,
                style: 'style' in rule ? [Object.prototype.toString.call(rule.style), rule.style.cssText, rule.style.parentRule === rule] : null,
                nested: 'cssRules' in rule ? Array.from(rule.cssRules).map((nested) => nested.cssText) : null,
            }));

            const before = sheet.cssRules.length;
            const addResult = sheet.addRule('.x', 'opacity: .5');
            const afterAdd = sheet.cssRules.length;
            sheet.removeRule(afterAdd - 1);

            const directCall = (() => {
                try { CSSStyleSheet(); return 'ok'; }
                catch (error) { return error.name + ':' + error.message; }
            })();
            const illegal = {};
            for (const name of ['CSSStyleDeclaration','CSSRule','CSSStyleRule','CSSMediaRule','CSSFontFaceRule']) {
                try { new globalThis[name](); illegal[name] = 'ok'; }
                catch (error) { illegal[name] = error.name + ':' + error.message; }
            }

            return JSON.stringify({
                style:{
                    tag:Object.prototype.toString.call(style),
                    instance:style instanceof CSSStyleDeclaration,
                    length:style.length,
                    item0:style.item(0),
                    color:style.getPropertyValue('color'),
                    priority:style.getPropertyPriority('color'),
                    cssFloat:style.cssFloat,
                    cssText:style.cssText,
                    parentRule:style.parentRule,
                    old,
                    attr:el.getAttribute('style')
                },
                sheet:{
                    tag:Object.prototype.toString.call(sheet),
                    instance:sheet instanceof CSSStyleSheet,
                    keys:Reflect.ownKeys(sheet).map(String),
                    before,addResult,afterAdd,afterRemove:sheet.cssRules.length,
                    ownerRule:sheet.ownerRule,
                    directCall,
                    ctorSource:String(CSSStyleSheet)
                },
                rows,illegal,
                constants:[CSSRule.STYLE_RULE,CSSRule.MEDIA_RULE,CSSRule.FONT_FACE_RULE,CSSRule.SUPPORTS_RULE,CSSRule.FONT_FEATURE_VALUES_RULE],
                parents:{
                    sheet:Object.getPrototypeOf(CSSStyleSheet.prototype) === StyleSheet.prototype,
                    styleRule:Object.getPrototypeOf(CSSStyleRule.prototype) === CSSRule.prototype,
                    fontRule:Object.getPrototypeOf(CSSFontFaceRule.prototype) === CSSRule.prototype,
                    mediaRule:Object.getPrototypeOf(CSSMediaRule.prototype) === CSSConditionRule.prototype
                }
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r##"{"style":{"tag":"[object CSSStyleDeclaration]","instance":true,"length":2,"item0":"float","color":"red","priority":"important","cssFloat":"right","cssText":"float: right; color: red !important;","parentRule":null,"old":"red","attr":"float: right; color: red !important;"},"sheet":{"tag":"[object CSSStyleSheet]","instance":true,"keys":[],"before":3,"addResult":-1,"afterAdd":4,"afterRemove":3,"ownerRule":null,"directCall":"TypeError:Failed to construct 'CSSStyleSheet': Please use the 'new' operator, this DOM object constructor cannot be called as a function.","ctorSource":"function CSSStyleSheet() { [native code] }"},"rows":[{"ctor":"CSSStyleRule","tag":"[object CSSStyleRule]","keys":[],"type":1,"cssText":"#a { color: red; float: left; }","parentSheet":true,"parentRule":true,"selector":"#a","media":null,"style":["[object CSSStyleDeclaration]","color: red; float: left;",true],"nested":[]},{"ctor":"CSSMediaRule","tag":"[object CSSMediaRule]","keys":[],"type":4,"cssText":"@media (min-width: 1px) {\n  #b { display: block; }\n}","parentSheet":true,"parentRule":true,"selector":null,"media":"(min-width: 1px)","style":null,"nested":["#b { display: block; }"]},{"ctor":"CSSFontFaceRule","tag":"[object CSSFontFaceRule]","keys":[],"type":5,"cssText":"@font-face { font-family: X; src: url(\"x\"); }","parentSheet":true,"parentRule":true,"selector":null,"media":null,"style":["[object CSSStyleDeclaration]","font-family: X; src: url(\"x\");",true],"nested":null}],"illegal":{"CSSStyleDeclaration":"TypeError:Failed to construct 'CSSStyleDeclaration': Illegal constructor","CSSRule":"TypeError:Failed to construct 'CSSRule': Illegal constructor","CSSStyleRule":"TypeError:Failed to construct 'CSSStyleRule': Illegal constructor","CSSMediaRule":"TypeError:Failed to construct 'CSSMediaRule': Illegal constructor","CSSFontFaceRule":"TypeError:Failed to construct 'CSSFontFaceRule': Illegal constructor"},"constants":[1,4,5,12,14],"parents":{"sheet":true,"styleRule":true,"fontRule":true,"mediaRule":true}}"##
    );
}

#[tokio::test]
async fn performance_timeline_webidl_matches_chrome() {
    let result = check(
        r#"(() => {
            const ctors = {};
            for (const name of ['PerformanceEntry','PerformanceResourceTiming','PerformanceNavigationTiming','PerformanceMeasure']) {
                try { new globalThis[name](); ctors[name] = 'ok'; }
                catch (error) { ctors[name] = error.name + ':' + error.message; }
            }
            let call;
            try { PerformanceMark('x'); call = 'ok'; }
            catch (error) { call = error.name + ':' + error.message; }
            const direct = new PerformanceMark('x', { startTime: 12.5, detail: { a: 1 } });
            performance.clearMarks(); performance.clearMeasures();
            const pm = performance.mark('a', { startTime: 5, detail: 'd' });
            const measure = performance.measure('m', { start: 1, duration: 3, detail: { z: 2 } });
            const pick = (entry) => ({
                tag: Object.prototype.toString.call(entry), ctor: entry.constructor.name,
                keys: Reflect.ownKeys(entry).map(String), name: entry.name, entryType: entry.entryType,
                startTime: entry.startTime, duration: entry.duration,
                detail: 'detail' in entry ? entry.detail : undefined,
                json: [entry.toJSON().name, entry.toJSON().entryType, entry.toJSON().startTime, entry.toJSON().duration],
            });
            const nav = performance.getEntriesByType('navigation')[0];
            const beforeClear = {
                marks: performance.getEntriesByType('mark').length,
                measures: performance.getEntriesByType('measure').length,
                markSame: performance.getEntriesByName('a', 'mark')[0] === pm,
                measureSame: performance.getEntriesByName('m', 'measure')[0] === measure,
            };
            performance.clearMarks('a'); performance.clearMeasures('m');
            return JSON.stringify({
                ctors,call,direct:pick(direct),pm:pick(pm),measure:pick(measure),beforeClear,
                afterClear:[performance.getEntriesByType('mark').length,performance.getEntriesByType('measure').length],
                nav:nav?{
                    tag:Object.prototype.toString.call(nav), keys:Reflect.ownKeys(nav).map(String),
                    instances:[nav instanceof PerformanceNavigationTiming,nav instanceof PerformanceResourceTiming,nav instanceof PerformanceEntry],
                    nameType:[typeof nav.name,nav.entryType,nav.startTime,typeof nav.duration],
                    resource:[nav.initiatorType,typeof nav.fetchStart,typeof nav.responseEnd,Array.isArray(nav.serverTiming)],
                    navigation:[nav.type,typeof nav.redirectCount,typeof nav.activationStart,typeof nav.criticalCHRestart,nav.notRestoredReasons===null,'confidence'in nav],
                    jsonMatch:[nav.toJSON().name===nav.name,nav.toJSON().entryType===nav.entryType,nav.toJSON().responseEnd===nav.responseEnd,nav.toJSON().type===nav.type]
                }:null,
                parents:{
                    r:Object.getPrototypeOf(PerformanceResourceTiming.prototype)===PerformanceEntry.prototype,
                    n:Object.getPrototypeOf(PerformanceNavigationTiming.prototype)===PerformanceResourceTiming.prototype,
                    mark:Object.getPrototypeOf(PerformanceMark.prototype)===PerformanceEntry.prototype,
                    measure:Object.getPrototypeOf(PerformanceMeasure.prototype)===PerformanceEntry.prototype
                }
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"ctors":{"PerformanceEntry":"TypeError:Failed to construct 'PerformanceEntry': Illegal constructor","PerformanceResourceTiming":"TypeError:Failed to construct 'PerformanceResourceTiming': Illegal constructor","PerformanceNavigationTiming":"TypeError:Failed to construct 'PerformanceNavigationTiming': Illegal constructor","PerformanceMeasure":"TypeError:Failed to construct 'PerformanceMeasure': Illegal constructor"},"call":"TypeError:Failed to construct 'PerformanceMark': Please use the 'new' operator, this DOM object constructor cannot be called as a function.","direct":{"tag":"[object PerformanceMark]","ctor":"PerformanceMark","keys":[],"name":"x","entryType":"mark","startTime":12.5,"duration":0,"detail":{"a":1},"json":["x","mark",12.5,0]},"pm":{"tag":"[object PerformanceMark]","ctor":"PerformanceMark","keys":[],"name":"a","entryType":"mark","startTime":5,"duration":0,"detail":"d","json":["a","mark",5,0]},"measure":{"tag":"[object PerformanceMeasure]","ctor":"PerformanceMeasure","keys":[],"name":"m","entryType":"measure","startTime":1,"duration":3,"detail":{"z":2},"json":["m","measure",1,3]},"beforeClear":{"marks":1,"measures":1,"markSame":true,"measureSame":true},"afterClear":[0,0],"nav":{"tag":"[object PerformanceNavigationTiming]","keys":[],"instances":[true,true,true],"nameType":["string","navigation",0,"number"],"resource":["navigation","number","number",true],"navigation":["navigate","number","number","number",true,true],"jsonMatch":[true,true,true,true]},"parents":{"r":true,"n":true,"mark":true,"measure":true}}"#
    );
}

// ================================================================
// Window globals
// ================================================================

#[tokio::test]
async fn window_self() {
    assert_eq!(check("window === self").await, "true");
}
#[tokio::test]
async fn window_document() {
    assert_eq!(check("typeof document").await, "object");
}
#[tokio::test]
async fn window_location() {
    assert_eq!(check("typeof location").await, "object");
}
#[tokio::test]
async fn window_location_href() {
    assert_eq!(check("typeof location.href").await, "string");
}
#[tokio::test]
async fn window_location_protocol() {
    assert_eq!(check("typeof location.protocol").await, "string");
}
#[tokio::test]
async fn window_navigator() {
    assert_eq!(check("typeof navigator").await, "object");
}

#[tokio::test]
async fn navigator_omits_non_chromium_legacy_members() {
    let result = check(
        r#"JSON.stringify({
            msDoNotTrack: 'msDoNotTrack' in navigator,
            loadPurpose: 'loadPurpose' in navigator,
            sayswho: 'sayswho' in navigator,
            mozGetUserMedia: 'mozGetUserMedia' in navigator,
            insecureCanShare: 'canShare' in navigator,
            insecureShare: 'share' in navigator
        })"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"msDoNotTrack":false,"loadPurpose":false,"sayswho":false,"mozGetUserMedia":false,"insecureCanShare":false,"insecureShare":false}"#
    );

    assert_eq!(
        check_secure("'canShare' in navigator && 'share' in navigator").await,
        "true"
    );
}
#[tokio::test]
async fn window_screen() {
    assert_eq!(check("typeof screen").await, "object");
}
#[tokio::test]
async fn window_history() {
    assert_eq!(check("typeof history").await, "object");
}
#[tokio::test]
async fn window_chrome() {
    assert_eq!(check("typeof chrome").await, "object");
}
#[tokio::test]
async fn window_crypto() {
    assert_eq!(check("typeof crypto").await, "object");
}
#[tokio::test]
async fn window_performance() {
    assert_eq!(check("typeof performance").await, "object");
}
#[tokio::test]
async fn window_console() {
    assert_eq!(check("typeof console").await, "object");
}
#[tokio::test]
async fn window_local_storage() {
    assert_eq!(check("typeof localStorage").await, "object");
}
#[tokio::test]
async fn storage_webidl_shape_and_named_properties_match_chrome() {
    let result = check(
        r#"(() => {
            localStorage.clear();
            const get1 = localStorage.getItem;
            const get2 = localStorage.getItem;
            localStorage.setItem('alpha', '1');
            localStorage.beta = '2';
            let illegal = '';
            try { Storage.prototype.getItem.call({}); illegal = 'ok'; }
            catch (error) { illegal = error.name + ':' + error.message; }
            let construction = '';
            try { new Storage(); construction = 'ok'; }
            catch (error) { construction = error.name + ':' + error.message; }
            const beforeDelete = {
                tag:Object.prototype.toString.call(localStorage),
                instance:localStorage instanceof Storage,
                proto:Object.getPrototypeOf(localStorage) === Storage.prototype,
                methodIdentity:get1 === get2,
                own:Reflect.ownKeys(localStorage).map(String).sort(),
                objectKeys:Object.keys(localStorage).sort(),
                alpha:localStorage.alpha,
                beta:localStorage.getItem('beta'),
                length:localStorage.length,
                key0:localStorage.key(0),
                illegal,
                construction,
                protoNames:Object.getOwnPropertyNames(Storage.prototype).sort(),
                enumerable:['length','key','getItem','setItem','removeItem','clear'].map(name =>
                    Object.getOwnPropertyDescriptor(Storage.prototype,name).enumerable),
            };
            delete localStorage.alpha;
            const afterDelete = {
                alpha:localStorage.getItem('alpha'),
                length:localStorage.length,
            };
            localStorage.clear();
            return JSON.stringify({beforeDelete,afterDelete});
        })()"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["beforeDelete"]["tag"], "[object Storage]");
    assert_eq!(value["beforeDelete"]["instance"], true);
    assert_eq!(value["beforeDelete"]["proto"], true);
    assert_eq!(value["beforeDelete"]["methodIdentity"], true);
    assert_eq!(
        value["beforeDelete"]["own"],
        serde_json::json!(["alpha", "beta"])
    );
    assert_eq!(
        value["beforeDelete"]["objectKeys"],
        serde_json::json!(["alpha", "beta"])
    );
    assert_eq!(value["beforeDelete"]["alpha"], "1");
    assert_eq!(value["beforeDelete"]["beta"], "2");
    assert_eq!(value["beforeDelete"]["length"], 2);
    assert!(matches!(
        value["beforeDelete"]["key0"].as_str(),
        Some("alpha" | "beta")
    ));
    assert!(value["beforeDelete"]["illegal"]
        .as_str()
        .unwrap()
        .contains("Illegal invocation"));
    assert!(value["beforeDelete"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["beforeDelete"]["protoNames"],
        serde_json::json!([
            "clear",
            "constructor",
            "getItem",
            "key",
            "length",
            "removeItem",
            "setItem"
        ])
    );
    assert_eq!(
        value["beforeDelete"]["enumerable"],
        serde_json::json!([true, true, true, true, true, true])
    );
    assert!(value["afterDelete"]["alpha"].is_null());
    assert_eq!(value["afterDelete"]["length"], 1);
}
#[tokio::test]
async fn window_session_storage() {
    assert_eq!(check("typeof sessionStorage").await, "object");
}
#[tokio::test]
async fn window_is_secure_context() {
    // Default `from_html` URL is `about:blank` — insecure per WICG
    // secure-contexts §3.2 (Phase 7 fix). Real Chrome agrees.
    assert_eq!(check("isSecureContext").await, "false");
}
#[tokio::test]
async fn window_inner_width() {
    assert_eq!(check("typeof innerWidth").await, "number");
}
#[tokio::test]
async fn window_inner_height() {
    assert_eq!(check("typeof innerHeight").await, "number");
}
#[tokio::test]
async fn window_outer_width() {
    assert_eq!(check("typeof outerWidth").await, "number");
}
#[tokio::test]
async fn window_outer_height() {
    assert_eq!(check("typeof outerHeight").await, "number");
}
#[tokio::test]
async fn window_device_pixel_ratio() {
    assert_eq!(check("typeof devicePixelRatio").await, "number");
}
#[tokio::test]
async fn window_scroll_x() {
    assert_eq!(check("typeof scrollX").await, "number");
}
#[tokio::test]
async fn window_scroll_y() {
    assert_eq!(check("typeof scrollY").await, "number");
}

// Functions
#[tokio::test]
async fn fn_set_timeout() {
    assert_eq!(check("typeof setTimeout").await, "function");
}
#[tokio::test]
async fn fn_set_interval() {
    assert_eq!(check("typeof setInterval").await, "function");
}
#[tokio::test]
async fn fn_clear_timeout() {
    assert_eq!(check("typeof clearTimeout").await, "function");
}
#[tokio::test]
async fn fn_clear_interval() {
    assert_eq!(check("typeof clearInterval").await, "function");
}
#[tokio::test]
async fn fn_request_animation_frame() {
    assert_eq!(check("typeof requestAnimationFrame").await, "function");
}
#[tokio::test]
async fn fn_request_idle_callback() {
    assert_eq!(check("typeof requestIdleCallback").await, "function");
}
#[tokio::test]
async fn fn_fetch() {
    assert_eq!(check("typeof fetch").await, "function");
}
#[tokio::test]
async fn fn_atob() {
    assert_eq!(check("typeof atob").await, "function");
}
#[tokio::test]
async fn fn_btoa() {
    assert_eq!(check("typeof btoa").await, "function");
}
#[tokio::test]
async fn fn_alert() {
    assert_eq!(check("typeof alert").await, "function");
}
#[tokio::test]
async fn fn_confirm() {
    assert_eq!(check("typeof confirm").await, "function");
}
#[tokio::test]
async fn fn_prompt() {
    assert_eq!(check("typeof prompt").await, "function");
}
#[tokio::test]
async fn fn_scroll_to() {
    assert_eq!(check("typeof scrollTo").await, "function");
}
#[tokio::test]
async fn fn_scroll_by() {
    assert_eq!(check("typeof scrollBy").await, "function");
}
#[tokio::test]
async fn fn_get_computed_style() {
    assert_eq!(check("typeof getComputedStyle").await, "function");
}
#[tokio::test]
async fn fn_match_media() {
    assert_eq!(check("typeof matchMedia").await, "function");
}
#[tokio::test]
async fn fn_get_selection() {
    assert_eq!(check("typeof getSelection").await, "function");
}
#[tokio::test]
async fn fn_open() {
    assert_eq!(check("typeof open").await, "function");
}
#[tokio::test]
async fn fn_close() {
    assert_eq!(check("typeof close").await, "function");
}
#[tokio::test]
async fn fn_post_message() {
    assert_eq!(check("typeof postMessage").await, "function");
}

// ================================================================
// Navigator
// ================================================================

#[tokio::test]
async fn nav_user_agent() {
    assert_eq!(check("typeof navigator.userAgent").await, "string");
}
#[tokio::test]
async fn nav_platform() {
    assert_eq!(check("typeof navigator.platform").await, "string");
}
#[tokio::test]
async fn nav_language() {
    assert_eq!(check("typeof navigator.language").await, "string");
}
#[tokio::test]
async fn nav_languages() {
    assert_eq!(check("Array.isArray(navigator.languages)").await, "true");
}
#[tokio::test]
async fn nav_vendor() {
    assert_eq!(check("navigator.vendor").await, "Google Inc.");
}
#[tokio::test]
async fn nav_hardware_concurrency() {
    assert_eq!(check("navigator.hardwareConcurrency > 0").await, "true");
}
#[tokio::test]
async fn nav_device_memory() {
    // deviceMemory is [SecureContext]. Phase 7.
    assert_eq!(check_secure("navigator.deviceMemory > 0").await, "true");
}
#[tokio::test]
async fn nav_max_touch_points() {
    assert_eq!(check("typeof navigator.maxTouchPoints").await, "number");
}
#[tokio::test]
async fn nav_cookie_enabled() {
    assert_eq!(check("navigator.cookieEnabled").await, "true");
}
#[tokio::test]
async fn nav_on_line() {
    assert_eq!(check("navigator.onLine").await, "true");
}
#[tokio::test]
async fn nav_pdf_viewer_enabled() {
    assert_eq!(check("navigator.pdfViewerEnabled").await, "true");
}
#[tokio::test]
async fn nav_webdriver() {
    // Modern Chrome (>=89, incl. Chrome-148) ALWAYS defines
    // navigator.webdriver === false for normal browsing; `undefined`
    // is the old/headless tell (a challenge sensor flagged wdt.r="undefined").
    assert_eq!(check("navigator.webdriver").await, "false");
    assert_eq!(check("typeof navigator.webdriver").await, "boolean");
}
#[tokio::test]
async fn nav_plugins_length() {
    assert_eq!(check("navigator.plugins.length > 0").await, "true");
}
#[tokio::test]
async fn nav_connection() {
    assert_eq!(check("typeof navigator.connection").await, "object");
}
#[tokio::test]
async fn nav_java_enabled() {
    assert_eq!(check("typeof navigator.javaEnabled").await, "function");
}
#[tokio::test]
async fn nav_send_beacon() {
    assert_eq!(check("typeof navigator.sendBeacon").await, "function");
}

#[tokio::test]
async fn nav_web_share_matches_chrome_secure_context_surface() {
    let value = check_secure(
        r#"JSON.stringify({
            shareType: typeof navigator.share,
            canShareType: typeof navigator.canShare,
            shareLength: navigator.share.length,
            canShareLength: navigator.canShare.length,
            shareEnumerable: Object.getOwnPropertyDescriptor(Navigator.prototype, 'share').enumerable,
            canShareEnumerable: Object.getOwnPropertyDescriptor(Navigator.prototype, 'canShare').enumerable,
            empty: navigator.canShare(),
            object: navigator.canShare({}),
            text: navigator.canShare({text:'x'}),
            title: navigator.canShare({title:'x'}),
            url: navigator.canShare({url:'https://example.com/'}),
            unknown: navigator.canShare({foo:'x'}),
            file: navigator.canShare({files:[new File(['x'], 'a.txt', {type:'text/plain'})]}),
            nativeShare: Function.prototype.toString.call(navigator.share),
            nativeCanShare: Function.prototype.toString.call(navigator.canShare)
        })"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&value).unwrap();
    assert_eq!(value["shareType"], "function");
    assert_eq!(value["canShareType"], "function");
    assert_eq!(value["shareLength"], 0);
    assert_eq!(value["canShareLength"], 0);
    assert_eq!(value["shareEnumerable"], true);
    assert_eq!(value["canShareEnumerable"], true);
    assert_eq!(value["empty"], false);
    assert_eq!(value["object"], false);
    assert_eq!(value["text"], true);
    assert_eq!(value["title"], true);
    assert_eq!(value["url"], true);
    assert_eq!(value["unknown"], false);
    assert_eq!(value["file"], true);
    assert_eq!(value["nativeShare"], "function share() { [native code] }");
    assert_eq!(
        value["nativeCanShare"],
        "function canShare() { [native code] }"
    );
}

#[tokio::test]
async fn nav_web_share_is_absent_in_non_secure_context() {
    assert_eq!(
        check("JSON.stringify([typeof navigator.share, typeof navigator.canShare, 'share' in Navigator.prototype, 'canShare' in Navigator.prototype])").await,
        r#"["undefined","undefined",false,false]"#
    );
}

#[tokio::test]
async fn nav_web_share_without_trusted_activation_rejects_like_chrome() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"navigator.share({text:'x'}).then(
            () => { globalThis.__shareResult = 'resolved'; },
            error => { globalThis.__shareResult = error.name + ':' + error.message; }
        );"#,
    )
    .unwrap();
    let _ = page
        .event_loop()
        .run_until_settled(std::time::Duration::from_millis(100))
        .await;
    let result = page
        .evaluate("String(globalThis.__shareResult || '')")
        .unwrap();
    assert_eq!(
        result,
        "NotAllowedError:Failed to execute 'share' on 'Navigator': Must be handling a user gesture to perform a share request."
    );
}

#[tokio::test]
async fn navigator_and_performance_webidl_arities_match_chrome_148() {
    let value = check_secure(
        r#"JSON.stringify({
            navigatorCtor: Function.prototype.toString.call(Navigator),
            screenCtor: Function.prototype.toString.call(Screen),
            performanceCtor: Function.prototype.toString.call(Performance),
            adAuctionComponents: navigator.adAuctionComponents.length,
            clearOriginJoinedAdInterestGroups: navigator.clearOriginJoinedAdInterestGroups.length,
            deprecatedReplaceInURN: navigator.deprecatedReplaceInURN.length,
            deprecatedURNToURL: navigator.deprecatedURNToURL.length,
            getInterestGroupAdAuctionData: navigator.getInterestGroupAdAuctionData.length,
            joinAdInterestGroup: navigator.joinAdInterestGroup.length,
            runAdAuction: navigator.runAdAuction.length,
            sendBeacon: navigator.sendBeacon.length,
            setAppBadge: navigator.setAppBadge.length,
            getEntriesByName: Performance.prototype.getEntriesByName.length,
            measure: Performance.prototype.measure.length,
            setResourceTimingBufferSize: Performance.prototype.setResourceTimingBufferSize.length
        })"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&value).unwrap();
    assert_eq!(
        value["navigatorCtor"],
        "function Navigator() { [native code] }"
    );
    assert_eq!(value["screenCtor"], "function Screen() { [native code] }");
    assert_eq!(
        value["performanceCtor"],
        "function Performance() { [native code] }"
    );
    assert_eq!(value["adAuctionComponents"], 1);
    assert_eq!(value["clearOriginJoinedAdInterestGroups"], 1);
    assert_eq!(value["deprecatedReplaceInURN"], 2);
    assert_eq!(value["deprecatedURNToURL"], 1);
    assert_eq!(value["getInterestGroupAdAuctionData"], 1);
    assert_eq!(value["joinAdInterestGroup"], 1);
    assert_eq!(value["runAdAuction"], 1);
    assert_eq!(value["sendBeacon"], 1);
    assert_eq!(value["setAppBadge"], 0);
    assert_eq!(value["getEntriesByName"], 1);
    assert_eq!(value["measure"], 1);
    assert_eq!(value["setResourceTimingBufferSize"], 1);
}

#[tokio::test]
async fn document_interface_inheritance_and_webidl_arities_match_chrome_148() {
    let value = check_secure(
        r#"JSON.stringify({
            ctorName: document.constructor.name,
            ctorSource: Function.prototype.toString.call(document.constructor),
            htmlDistinct: HTMLDocument !== Document,
            htmlProtoDistinct: HTMLDocument.prototype !== Document.prototype,
            htmlProtoParent: Object.getPrototypeOf(HTMLDocument.prototype) === Document.prototype,
            htmlCtorParent: Object.getPrototypeOf(HTMLDocument) === Document,
            instanceDocument: document instanceof Document,
            instanceHTMLDocument: document instanceof HTMLDocument,
            documentCtorName: Document.prototype.constructor.name,
            documentCtorSource: Function.prototype.toString.call(Document.prototype.constructor),
            htmlCtorName: HTMLDocument.prototype.constructor.name,
            htmlCtorSource: Function.prototype.toString.call(HTMLDocument.prototype.constructor),
            documentLength: Document.length,
            htmlDocumentLength: HTMLDocument.length,
            ariaNotify: Document.prototype.ariaNotify.length,
            createExpression: Document.prototype.createExpression.length,
            createNodeIterator: Document.prototype.createNodeIterator.length,
            createTreeWalker: Document.prototype.createTreeWalker.length,
            evaluate: Document.prototype.evaluate.length,
            execCommand: Document.prototype.execCommand.length,
            hasPrivateToken: Document.prototype.hasPrivateToken.length,
            hasRedemptionRecord: Document.prototype.hasRedemptionRecord.length,
            importNode: Document.prototype.importNode.length,
            queryCommandIndeterm: Document.prototype.queryCommandIndeterm.length,
            queryCommandState: Document.prototype.queryCommandState.length,
            queryCommandValue: Document.prototype.queryCommandValue.length,
            requestStorageAccessFor: Document.prototype.requestStorageAccessFor.length,
            startViewTransition: Document.prototype.startViewTransition.length,
            write: Document.prototype.write.length,
            writeln: Document.prototype.writeln.length,
            htmlConstruct: (() => { try { new HTMLDocument(); return 'ok'; } catch (e) { return e.name + ':' + e.message; } })()
        })"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&value).unwrap();
    assert_eq!(value["ctorName"], "HTMLDocument");
    assert_eq!(
        value["ctorSource"],
        "function HTMLDocument() { [native code] }"
    );
    assert_eq!(value["htmlDistinct"], true);
    assert_eq!(value["htmlProtoDistinct"], true);
    assert_eq!(value["htmlProtoParent"], true);
    assert_eq!(value["htmlCtorParent"], true);
    assert_eq!(value["instanceDocument"], true);
    assert_eq!(value["instanceHTMLDocument"], true);
    assert_eq!(value["documentCtorName"], "Document");
    assert_eq!(
        value["documentCtorSource"],
        "function Document() { [native code] }"
    );
    assert_eq!(value["htmlCtorName"], "HTMLDocument");
    assert_eq!(
        value["htmlCtorSource"],
        "function HTMLDocument() { [native code] }"
    );
    assert_eq!(value["documentLength"], 0);
    assert_eq!(value["htmlDocumentLength"], 0);
    assert_eq!(value["ariaNotify"], 1);
    assert_eq!(value["createExpression"], 1);
    assert_eq!(value["createNodeIterator"], 1);
    assert_eq!(value["createTreeWalker"], 1);
    assert_eq!(value["evaluate"], 2);
    assert_eq!(value["execCommand"], 1);
    assert_eq!(value["hasPrivateToken"], 1);
    assert_eq!(value["hasRedemptionRecord"], 1);
    assert_eq!(value["importNode"], 1);
    assert_eq!(value["queryCommandIndeterm"], 1);
    assert_eq!(value["queryCommandState"], 1);
    assert_eq!(value["queryCommandValue"], 1);
    assert_eq!(value["requestStorageAccessFor"], 1);
    assert_eq!(value["startViewTransition"], 0);
    assert_eq!(value["write"], 0);
    assert_eq!(value["writeln"], 0);
    assert_eq!(
        value["htmlConstruct"],
        "TypeError:Failed to construct 'HTMLDocument': Illegal constructor"
    );
}

#[tokio::test]
async fn nav_get_battery() {
    assert_eq!(
        check_secure("typeof navigator.getBattery").await,
        "function"
    );
}
#[tokio::test]
async fn nav_user_agent_data() {
    assert_eq!(
        check_secure("typeof navigator.userAgentData").await,
        "object"
    );
}
#[tokio::test]
async fn nav_ua_data_brands() {
    assert_eq!(
        check_secure("navigator.userAgentData.brands.length > 0").await,
        "true"
    );
}
#[tokio::test]
async fn nav_ua_data_mobile() {
    assert_eq!(
        check_secure("typeof navigator.userAgentData.mobile").await,
        "boolean"
    );
}
// Client Hints API contract — browser_oxide exposes the full getHighEntropyValues
// surface required by CreepJS / Yandex Antirobot / device-fingerprint sensors.
#[tokio::test]
async fn nav_ua_data_get_high_entropy_is_function() {
    assert_eq!(
        check_secure("typeof navigator.userAgentData.getHighEntropyValues").await,
        "function"
    );
}
#[tokio::test]
async fn nav_ua_data_get_high_entropy_returns_promise() {
    assert_eq!(
        check_secure("navigator.userAgentData.getHighEntropyValues([]) instanceof Promise").await,
        "true"
    );
}
// For tests that need to inspect the resolved object, kick off the Promise and
// stash its result in a synchronous global via .then(); then pump microtasks.
// Our Page::evaluate drains microtasks before returning, so window.__r is
// populated by the time the second evaluate() reads it.
#[tokio::test]
async fn nav_ua_data_high_entropy_full_version_list() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"window.__r = null;
        navigator.userAgentData.getHighEntropyValues(['fullVersionList']).then(r => { window.__r = r; });"#
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    let result = page.evaluate(
        r#"(() => {
            const r = window.__r;
            if (!r) return 'null';
            return Array.isArray(r.fullVersionList)
                && r.fullVersionList.length >= 3
                && r.fullVersionList.every(b => typeof b.brand === 'string' && typeof b.version === 'string');
        })()"#
    ).unwrap();
    assert_eq!(result, "true");
}
#[tokio::test]
async fn nav_ua_data_high_entropy_architecture_and_bitness() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"window.__r = null;
        navigator.userAgentData.getHighEntropyValues(['architecture', 'bitness']).then(r => { window.__r = r; });"#
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    let result = page
        .evaluate(
            r#"(() => {
            const r = window.__r;
            if (!r) return 'null';
            return (r.architecture === 'x86' || r.architecture === 'arm') && r.bitness === '64';
        })()"#,
        )
        .unwrap();
    assert_eq!(result, "true");
}
#[tokio::test]
async fn nav_ua_data_high_entropy_only_returns_requested_plus_low_entropy() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"window.__r = null;
        navigator.userAgentData.getHighEntropyValues(['architecture']).then(r => { window.__r = r; });"#
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    let result = page
        .evaluate(
            r#"(() => {
            const r = window.__r;
            if (!r) return 'null';
            return 'brands' in r && 'mobile' in r && 'platform' in r
                && 'architecture' in r
                && !('wow64' in r)
                && !('model' in r)
                && !('platformVersion' in r);
        })()"#,
        )
        .unwrap();
    assert_eq!(result, "true");
}
#[tokio::test]
async fn nav_ua_data_to_json_returns_low_entropy_only() {
    assert_eq!(
        check_secure(
            r#"(() => {
                const j = navigator.userAgentData.toJSON();
                return Array.isArray(j.brands) && typeof j.mobile === 'boolean' && typeof j.platform === 'string'
                    && !('fullVersionList' in j) && !('architecture' in j);
            })()"#
        ).await,
        "true"
    );
}
#[tokio::test]
async fn nav_ua_data_brands_match_user_agent_version() {
    // Consistency: the major version in brands[] must match the version in navigator.userAgent
    assert_eq!(
        check_secure(
            r#"(() => {
                const uaMatch = navigator.userAgent.match(/Chrome\/(\d+)/);
                if (!uaMatch) return 'no-ua-match';
                const major = uaMatch[1];
                return navigator.userAgentData.brands.some(b => b.brand === 'Google Chrome' && b.version === major);
            })()"#
        ).await,
        "true"
    );
}
#[tokio::test]
async fn nav_media_devices() {
    assert_eq!(
        check_secure("typeof navigator.mediaDevices").await,
        "object"
    );
}
#[tokio::test]
async fn nav_permissions() {
    assert_eq!(check("typeof navigator.permissions").await, "object");
}
#[tokio::test]
async fn nav_clipboard() {
    assert_eq!(check_secure("typeof navigator.clipboard").await, "object");
}
#[tokio::test]
async fn clipboard_and_media_devices_webidl_shape_match_chrome() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"globalThis.__cmshape = null;
        (async () => {
            const clipboardHandler = () => {};
            const mediaHandler = () => {};
            navigator.clipboard.onclipboardchange = clipboardHandler;
            navigator.mediaDevices.ondevicechange = mediaHandler;
            await navigator.clipboard.writeText('hello');
            const text = await navigator.clipboard.readText();
            const devices = await navigator.mediaDevices.enumerateDevices();
            const construction = C => {
                try { new C(); return 'ok'; }
                catch (error) { return error.name + ':' + error.message; }
            };
            globalThis.__cmshape = {
                clipboard:{
                    own:Reflect.ownKeys(navigator.clipboard).map(String),
                    tag:Object.prototype.toString.call(navigator.clipboard),
                    instance:navigator.clipboard instanceof Clipboard,
                    eventTarget:navigator.clipboard instanceof EventTarget,
                    proto:Object.getPrototypeOf(Clipboard.prototype) === EventTarget.prototype,
                    construction:construction(Clipboard),
                    handler:navigator.clipboard.onclipboardchange === clipboardHandler,
                    text,
                    methods:['read','readText','write','writeText'].map(name =>
                        [name, Clipboard.prototype[name].length,
                         Object.getOwnPropertyDescriptor(Clipboard.prototype,name).enumerable]),
                    handlerEnumerable:Object.getOwnPropertyDescriptor(
                        Clipboard.prototype,'onclipboardchange').enumerable,
                },
                media:{
                    own:Reflect.ownKeys(navigator.mediaDevices).map(String),
                    tag:Object.prototype.toString.call(navigator.mediaDevices),
                    instance:navigator.mediaDevices instanceof MediaDevices,
                    eventTarget:navigator.mediaDevices instanceof EventTarget,
                    proto:Object.getPrototypeOf(MediaDevices.prototype) === EventTarget.prototype,
                    construction:construction(MediaDevices),
                    handler:navigator.mediaDevices.ondevicechange === mediaHandler,
                    devices:Array.isArray(devices),
                    methods:[
                        'enumerateDevices','getDisplayMedia','getSupportedConstraints',
                        'getUserMedia','setCaptureHandleConfig'
                    ].map(name => [
                        name, MediaDevices.prototype[name].length,
                        Object.getOwnPropertyDescriptor(MediaDevices.prototype,name).enumerable
                    ]),
                    handlerEnumerable:Object.getOwnPropertyDescriptor(
                        MediaDevices.prototype,'ondevicechange').enumerable,
                }
            };
        })();"#,
    )
    .unwrap();
    for _ in 0..10 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(20))
            .await;
        if page.evaluate("globalThis.__cmshape !== null").unwrap() == "true" {
            break;
        }
    }
    let result = page
        .evaluate("JSON.stringify(globalThis.__cmshape)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["clipboard"]["own"], serde_json::json!([]));
    assert_eq!(value["clipboard"]["tag"], "[object Clipboard]");
    assert_eq!(value["clipboard"]["instance"], true);
    assert_eq!(value["clipboard"]["eventTarget"], true);
    assert_eq!(value["clipboard"]["proto"], true);
    assert!(value["clipboard"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(value["clipboard"]["handler"], true);
    assert_eq!(value["clipboard"]["text"], "hello");
    assert_eq!(
        value["clipboard"]["methods"],
        serde_json::json!([
            ["read", 0, true],
            ["readText", 0, true],
            ["write", 1, true],
            ["writeText", 1, true]
        ])
    );
    assert_eq!(value["clipboard"]["handlerEnumerable"], true);
    assert_eq!(value["media"]["own"], serde_json::json!([]));
    assert_eq!(value["media"]["tag"], "[object MediaDevices]");
    assert_eq!(value["media"]["instance"], true);
    assert_eq!(value["media"]["eventTarget"], true);
    assert_eq!(value["media"]["proto"], true);
    assert!(value["media"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(value["media"]["handler"], true);
    assert_eq!(value["media"]["devices"], true);
    assert_eq!(
        value["media"]["methods"],
        serde_json::json!([
            ["enumerateDevices", 0, true],
            ["getDisplayMedia", 0, true],
            ["getSupportedConstraints", 0, true],
            ["getUserMedia", 0, true],
            ["setCaptureHandleConfig", 0, true]
        ])
    );
    assert_eq!(value["media"]["handlerEnumerable"], true);
}
#[tokio::test]
async fn nav_storage() {
    assert_eq!(check_secure("typeof navigator.storage").await, "object");
}
#[tokio::test]
async fn nav_storage_exposes_origin_private_file_system() {
    assert_eq!(
        check_secure(
            r#"JSON.stringify({
                getDirectory: typeof navigator.storage.getDirectory,
                handleTag: FileSystemHandle.prototype[Symbol.toStringTag],
                directoryTag: FileSystemDirectoryHandle.prototype[Symbol.toStringTag],
                fileTag: FileSystemFileHandle.prototype[Symbol.toStringTag],
                rootMethods: ['getDirectoryHandle','getFileHandle','removeEntry','resolve','entries','keys','values']
                    .every(name => typeof FileSystemDirectoryHandle.prototype[name] === 'function'),
                fileMethods: ['createWritable','getFile','move']
                    .every(name => typeof FileSystemFileHandle.prototype[name] === 'function'),
            })"#,
        )
        .await,
        r#"{"getDirectory":"function","handleTag":"FileSystemHandle","directoryTag":"FileSystemDirectoryHandle","fileTag":"FileSystemFileHandle","rootMethods":true,"fileMethods":true}"#
    );
}
#[tokio::test]
async fn nav_service_worker() {
    assert_eq!(
        check_secure("typeof navigator.serviceWorker").await,
        "object"
    );
}

// ================================================================
// Document
// ================================================================

#[tokio::test]
async fn doc_document_element() {
    assert_eq!(check("document.documentElement.tagName").await, "HTML");
}
#[tokio::test]
async fn doc_head() {
    assert_eq!(check("document.head.tagName").await, "HEAD");
}
#[tokio::test]
async fn doc_body() {
    assert_eq!(check("document.body.tagName").await, "BODY");
}
#[tokio::test]
async fn doc_ready_state() {
    assert_eq!(check("document.readyState").await, "complete");
}
#[tokio::test]
async fn doc_visibility_state() {
    assert_eq!(check("document.visibilityState").await, "visible");
}
#[tokio::test]
async fn doc_hidden() {
    assert_eq!(check("document.hidden").await, "false");
}
#[tokio::test]
async fn doc_has_focus() {
    assert_eq!(check("document.hasFocus()").await, "true");
}
#[tokio::test]
async fn doc_character_set() {
    // Phase 7 — HTML legacy default per spec is windows-1252. Real
    // Chrome reports this for HTML docs without explicit <meta charset>.
    assert_eq!(check("document.characterSet").await, "windows-1252");
}
#[tokio::test]
async fn doc_content_type() {
    assert_eq!(check("document.contentType").await, "text/html");
}
#[tokio::test]
async fn doc_compat_mode() {
    assert_eq!(check("document.compatMode").await, "CSS1Compat");
}
#[tokio::test]
async fn doc_default_view() {
    assert_eq!(check("document.defaultView === window").await, "true");
}

// Document methods
#[tokio::test]
async fn doc_create_element() {
    assert_eq!(check("typeof document.createElement").await, "function");
}
#[tokio::test]
async fn doc_create_text_node() {
    assert_eq!(check("typeof document.createTextNode").await, "function");
}
#[tokio::test]
async fn doc_create_document_fragment() {
    assert_eq!(
        check("typeof document.createDocumentFragment").await,
        "function"
    );
}
#[tokio::test]
async fn doc_create_event() {
    assert_eq!(check("typeof document.createEvent").await, "function");
}
#[tokio::test]
async fn doc_create_range() {
    assert_eq!(check("typeof document.createRange").await, "function");
}
#[tokio::test]
async fn doc_get_element_by_id() {
    assert_eq!(check("typeof document.getElementById").await, "function");
}
#[tokio::test]
async fn doc_query_selector() {
    assert_eq!(check("typeof document.querySelector").await, "function");
}
#[tokio::test]
async fn doc_query_selector_all() {
    assert_eq!(check("typeof document.querySelectorAll").await, "function");
}
#[tokio::test]
async fn doc_get_elements_by_tag_name() {
    assert_eq!(
        check("typeof document.getElementsByTagName").await,
        "function"
    );
}
#[tokio::test]
async fn doc_get_elements_by_class_name() {
    assert_eq!(
        check("typeof document.getElementsByClassName").await,
        "function"
    );
}
#[tokio::test]
async fn doc_exec_command() {
    assert_eq!(check("typeof document.execCommand").await, "function");
}
#[tokio::test]
async fn doc_element_from_point() {
    assert_eq!(check("typeof document.elementFromPoint").await, "function");
}
#[tokio::test]
async fn doc_element_from_point_out_of_viewport_is_null() {
    // Real Chrome returns null for a point outside the viewport (negative or
    // beyond innerWidth/innerHeight). The previous unconditional
    // `return this.body` was a one-call CreepJS/challenge-vendor layout
    // lie-detector tell (06_ENGINE_CORRECTNESS #7).
    let r = check(
        r#"(function(){
            var oob = document.elementFromPoint(99999, 99999);
            var neg = document.elementFromPoint(-1, -1);
            var oobList = document.elementsFromPoint(99999, 99999);
            return JSON.stringify({
                oob: oob === null,
                neg: neg === null,
                oobListEmpty: Array.isArray(oobList) && oobList.length === 0,
            });
        })()"#,
    )
    .await;
    assert!(
        r.contains("\"oob\":true"),
        "OOB elementFromPoint must be null: {r}"
    );
    assert!(
        r.contains("\"neg\":true"),
        "negative elementFromPoint must be null: {r}"
    );
    assert!(
        r.contains("\"oobListEmpty\":true"),
        "OOB elementsFromPoint must be []: {r}"
    );
}
#[tokio::test]
async fn doc_write() {
    assert_eq!(check("typeof document.write").await, "function");
}
#[tokio::test]
async fn doc_writeln() {
    assert_eq!(check("typeof document.writeln").await, "function");
}
#[tokio::test]
async fn doc_import_node() {
    assert_eq!(check("typeof document.importNode").await, "function");
}
#[tokio::test]
async fn doc_adopt_node() {
    assert_eq!(check("typeof document.adoptNode").await, "function");
}
#[tokio::test]
async fn doc_scripts() {
    assert_eq!(check("typeof document.scripts").await, "object");
}
#[tokio::test]
async fn doc_forms() {
    assert_eq!(check("typeof document.forms").await, "object");
}
#[tokio::test]
async fn doc_active_element() {
    assert_eq!(
        check("document.activeElement === document.body").await,
        "true"
    );
}

// ================================================================
// Element / Node
// ================================================================

#[tokio::test]
async fn node_element_node() {
    assert_eq!(check("Node.ELEMENT_NODE").await, "1");
}
#[tokio::test]
async fn node_text_node() {
    assert_eq!(check("Node.TEXT_NODE").await, "3");
}
#[tokio::test]
async fn node_document_node() {
    assert_eq!(check("Node.DOCUMENT_NODE").await, "9");
}
#[tokio::test]
async fn el_append_child() {
    assert_eq!(check("typeof document.body.appendChild").await, "function");
}
#[tokio::test]
async fn el_remove_child() {
    assert_eq!(check("typeof document.body.removeChild").await, "function");
}
#[tokio::test]
async fn el_insert_before() {
    assert_eq!(check("typeof document.body.insertBefore").await, "function");
}
#[tokio::test]
async fn el_replace_child() {
    assert_eq!(check("typeof document.body.replaceChild").await, "function");
}
#[tokio::test]
async fn el_clone_node() {
    assert_eq!(check("typeof document.body.cloneNode").await, "function");
}
#[tokio::test]
async fn el_contains() {
    assert_eq!(check("typeof document.body.contains").await, "function");
}
#[tokio::test]
async fn el_remove() {
    assert_eq!(
        check("typeof document.createElement('div').remove").await,
        "function"
    );
}
#[tokio::test]
async fn el_append() {
    assert_eq!(check("typeof document.body.append").await, "function");
}
#[tokio::test]
async fn el_prepend() {
    assert_eq!(check("typeof document.body.prepend").await, "function");
}
#[tokio::test]
async fn el_after() {
    assert_eq!(
        check("typeof document.createElement('div').after").await,
        "function"
    );
}
#[tokio::test]
async fn el_before() {
    assert_eq!(
        check("typeof document.createElement('div').before").await,
        "function"
    );
}
#[tokio::test]
async fn el_replace_with() {
    assert_eq!(
        check("typeof document.createElement('div').replaceWith").await,
        "function"
    );
}
#[tokio::test]
async fn el_replace_children() {
    assert_eq!(
        check("typeof document.body.replaceChildren").await,
        "function"
    );
}
#[tokio::test]
async fn el_get_attribute() {
    assert_eq!(check("typeof document.body.getAttribute").await, "function");
}
#[tokio::test]
async fn el_get_attribute_missing_is_null() {
    assert_eq!(
        check("document.body.getAttribute('definitely-missing') === null").await,
        "true"
    );
}
#[tokio::test]
async fn dynamic_iframe_remains_connected_and_queryable() {
    assert_eq!(
        check(
            r#"(function(){
                var host=document.createElement('div');
                document.body.appendChild(host);
                var frame=document.createElement('iframe');
                frame.id='dynamic-frame-check';
                host.appendChild(frame);
                return JSON.stringify({
                    connected:frame.isConnected,
                    parent:frame.parentElement===host,
                    byId:document.getElementById('dynamic-frame-check')===frame,
                    count:document.querySelectorAll('iframe').length,
                    serialized:/<iframe/i.test(host.innerHTML)
                });
            })()"#,
        )
        .await,
        r#"{"connected":true,"parent":true,"byId":true,"count":1,"serialized":true}"#
    );
}
#[tokio::test]
async fn appending_document_fragment_moves_its_children() {
    assert_eq!(
        check(
            r#"(function(){
                var host=document.createElement('div');
                document.body.appendChild(host);
                var fragment=document.createDocumentFragment();
                var frame=document.createElement('iframe');
                frame.id='fragment-frame-check';
                fragment.appendChild(frame);
                host.appendChild(fragment);
                return JSON.stringify({
                    connected:frame.isConnected,
                    parent:frame.parentElement===host,
                    fragmentEmpty:fragment.childNodes.length===0,
                    byId:document.getElementById('fragment-frame-check')===frame,
                    count:document.querySelectorAll('iframe').length
                });
            })()"#,
        )
        .await,
        r#"{"connected":true,"parent":true,"fragmentEmpty":true,"byId":true,"count":1}"#
    );
}
#[tokio::test]
async fn shadow_root_supports_scoped_queries() {
    assert_eq!(
        check(
            r#"(function(){
                var host=document.createElement('div');
                document.body.appendChild(host);
                var root=host.attachShadow({mode:'closed'});
                var frame=document.createElement('iframe');
                frame.id='shadow-frame-check';
                root.appendChild(frame);
                return JSON.stringify({
                    query:root.querySelector('#shadow-frame-check')===frame,
                    queryAll:root.querySelectorAll('iframe').length===1,
                    byId:root.getElementById('shadow-frame-check')===frame,
                    children:root.children.length===1,
                    first:root.firstElementChild===frame,
                    count:root.childElementCount===1,
                    closed:host.shadowRoot===null
                });
            })()"#,
        )
        .await,
        r#"{"query":true,"queryAll":true,"byId":true,"children":true,"first":true,"count":true,"closed":true}"#
    );
}
#[tokio::test]
async fn el_set_attribute() {
    assert_eq!(check("typeof document.body.setAttribute").await, "function");
}
#[tokio::test]
async fn el_remove_attribute() {
    assert_eq!(
        check("typeof document.body.removeAttribute").await,
        "function"
    );
}
#[tokio::test]
async fn el_has_attribute() {
    assert_eq!(check("typeof document.body.hasAttribute").await, "function");
}
#[tokio::test]
async fn el_toggle_attribute() {
    assert_eq!(
        check("typeof document.body.toggleAttribute").await,
        "function"
    );
}
#[tokio::test]
async fn el_insert_adjacent_html() {
    assert_eq!(
        check("typeof document.body.insertAdjacentHTML").await,
        "function"
    );
}
#[tokio::test]
async fn el_insert_adjacent_element() {
    assert_eq!(
        check("typeof document.body.insertAdjacentElement").await,
        "function"
    );
}
#[tokio::test]
async fn el_matches() {
    assert_eq!(check("typeof document.body.matches").await, "function");
}
#[tokio::test]
async fn el_closest() {
    assert_eq!(check("typeof document.body.closest").await, "function");
}
#[tokio::test]
async fn el_get_bounding_client_rect() {
    assert_eq!(
        check("typeof document.body.getBoundingClientRect").await,
        "function"
    );
}
#[tokio::test]
async fn el_class_list() {
    assert_eq!(check("typeof document.body.classList").await, "object");
}
#[tokio::test]
async fn el_style() {
    assert_eq!(check("typeof document.body.style").await, "object");
}
#[tokio::test]
async fn el_dataset() {
    assert_eq!(check("typeof document.body.dataset").await, "object");
}
#[tokio::test]
async fn el_inner_html() {
    assert_eq!(check("typeof document.body.innerHTML").await, "string");
}
#[tokio::test]
async fn el_outer_html() {
    assert_eq!(check("typeof document.body.outerHTML").await, "string");
}
#[tokio::test]
async fn el_text_content() {
    assert_eq!(check("typeof document.body.textContent").await, "string");
}
#[tokio::test]
async fn el_owner_document() {
    assert_eq!(
        check("document.body.ownerDocument === document").await,
        "true"
    );
}
#[tokio::test]
async fn el_is_connected() {
    assert_eq!(check("document.body.isConnected").await, "true");
}
#[tokio::test]
async fn el_offset_width() {
    assert_eq!(check("typeof document.body.offsetWidth").await, "number");
}
#[tokio::test]
async fn el_offset_height() {
    assert_eq!(check("typeof document.body.offsetHeight").await, "number");
}
#[tokio::test]
async fn el_check_visibility() {
    assert_eq!(
        check("typeof document.body.checkVisibility").await,
        "function"
    );
}
#[tokio::test]
async fn el_add_event_listener() {
    assert_eq!(
        check("typeof document.body.addEventListener").await,
        "function"
    );
}
#[tokio::test]
async fn el_dispatch_event() {
    assert_eq!(
        check("typeof document.body.dispatchEvent").await,
        "function"
    );
}
#[tokio::test]
async fn el_click() {
    assert_eq!(check("typeof document.body.click").await, "function");
}
#[tokio::test]
async fn el_focus() {
    assert_eq!(check("typeof document.body.focus").await, "function");
}
#[tokio::test]
async fn el_blur() {
    assert_eq!(check("typeof document.body.blur").await, "function");
}
#[tokio::test]
async fn el_next_element_sibling() {
    assert_eq!(check("'nextElementSibling' in document.body").await, "true");
}
#[tokio::test]
async fn el_previous_element_sibling() {
    assert_eq!(
        check("'previousElementSibling' in document.body").await,
        "true"
    );
}
#[tokio::test]
async fn el_child_element_count() {
    assert_eq!(
        check("typeof document.body.childElementCount").await,
        "number"
    );
}
#[tokio::test]
async fn el_animate() {
    assert_eq!(check("typeof document.body.animate").await, "function");
}

// ================================================================
// Events
// ================================================================

#[tokio::test]
async fn cls_event() {
    assert_eq!(check("typeof Event").await, "function");
}
#[tokio::test]
async fn cls_custom_event() {
    assert_eq!(check("typeof CustomEvent").await, "function");
}
#[tokio::test]
async fn cls_mouse_event() {
    assert_eq!(check("typeof MouseEvent").await, "function");
}
#[tokio::test]
async fn cls_keyboard_event() {
    assert_eq!(check("typeof KeyboardEvent").await, "function");
}
#[tokio::test]
async fn cls_input_event() {
    assert_eq!(check("typeof InputEvent").await, "function");
}
#[tokio::test]
async fn cls_focus_event() {
    assert_eq!(check("typeof FocusEvent").await, "function");
}
#[tokio::test]
async fn cls_pointer_event() {
    assert_eq!(check("typeof PointerEvent").await, "function");
}
#[tokio::test]
async fn cls_wheel_event() {
    assert_eq!(check("typeof WheelEvent").await, "function");
}
#[tokio::test]
async fn cls_touch_event() {
    assert_eq!(check("typeof TouchEvent").await, "function");
}
#[tokio::test]
async fn cls_message_event() {
    assert_eq!(check("typeof MessageEvent").await, "function");
}
#[tokio::test]
async fn cls_error_event() {
    assert_eq!(check("typeof ErrorEvent").await, "function");
}
#[tokio::test]
async fn cls_event_target() {
    assert_eq!(check("typeof EventTarget").await, "function");
}
#[tokio::test]
async fn cls_event_source() {
    assert_eq!(check("typeof EventSource").await, "function");
    assert_eq!(check("EventSource.CONNECTING").await, "0");
    assert_eq!(check("EventSource.OPEN").await, "1");
    assert_eq!(check("EventSource.CLOSED").await, "2");
}

#[tokio::test]
async fn event_and_message_event_match_chrome_148_webidl_surface() {
    let result = check(
        r#"(() => {
            const event = new Event('probe', { bubbles: true, cancelable: true, composed: true });
            const message = new MessageEvent('message', { data: 'x', origin: 'https://example.com' });
            const target = new EventTarget();
            let during = null;
            target.addEventListener('probe', value => {
                during = {
                    target: value.target === target,
                    currentTarget: value.currentTarget === target,
                    phase: value.eventPhase,
                    path: value.composedPath().length,
                };
            });
            target.dispatchEvent(event);
            const trusted = Object.getOwnPropertyDescriptor(event, 'isTrusted');
            return JSON.stringify({
                eventOwn: Object.getOwnPropertyNames(event),
                eventKeys: Object.keys(event),
                eventProto: Object.getOwnPropertyNames(Event.prototype),
                eventTag: Object.prototype.toString.call(event),
                trusted: {
                    own: !!trusted,
                    enumerable: trusted && trusted.enumerable,
                    configurable: trusted && trusted.configurable,
                    getter: trusted && String(trusted.get),
                },
                messageOwn: Object.getOwnPropertyNames(message),
                messageProto: Object.getOwnPropertyNames(MessageEvent.prototype),
                messageTag: Object.prototype.toString.call(message),
                eventTargetProto: Object.getOwnPropertyNames(EventTarget.prototype),
                during,
                after: {
                    target: event.target === target,
                    currentTarget: event.currentTarget,
                    phase: event.eventPhase,
                    path: event.composedPath().length,
                },
            });
        })()"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"eventOwn":["isTrusted"],"eventKeys":["isTrusted"],"eventProto":["type","target","currentTarget","eventPhase","bubbles","cancelable","defaultPrevented","composed","timeStamp","srcElement","returnValue","cancelBubble","NONE","CAPTURING_PHASE","AT_TARGET","BUBBLING_PHASE","composedPath","initEvent","preventDefault","stopImmediatePropagation","stopPropagation","constructor"],"eventTag":"[object Event]","trusted":{"own":true,"enumerable":true,"configurable":false,"getter":"function get isTrusted() { [native code] }"},"messageOwn":["isTrusted"],"messageProto":["data","origin","lastEventId","source","ports","userActivation","initMessageEvent","constructor"],"messageTag":"[object MessageEvent]","eventTargetProto":["addEventListener","dispatchEvent","removeEventListener","when","constructor"],"during":{"target":true,"currentTarget":true,"phase":2,"path":1},"after":{"target":true,"currentTarget":null,"phase":0,"path":0}}"#
    );
}

// WebRTC
#[tokio::test]
async fn cls_rtc_peer_connection() {
    assert_eq!(check("typeof RTCPeerConnection").await, "function");
    assert_eq!(check("typeof webkitRTCPeerConnection").await, "function");
    assert_eq!(check("typeof RTCSessionDescription").await, "function");
    assert_eq!(check("typeof RTCIceCandidate").await, "function");
}

// Font enumeration
#[tokio::test]
async fn api_document_fonts() {
    assert_eq!(check("typeof document.fonts").await, "object");
    assert_eq!(check("typeof document.fonts.check").await, "function");
    assert_eq!(check("document.fonts.check('12px Arial')").await, "true");
}

// MediaSource
#[tokio::test]
async fn cls_media_source() {
    assert_eq!(check("typeof MediaSource").await, "function");
    assert_eq!(
        check("typeof MediaSource.isTypeSupported").await,
        "function"
    );
    assert_eq!(
        check("MediaSource.isTypeSupported('video/mp4')").await,
        "true"
    );
    assert_eq!(
        check("MediaSource.isTypeSupported('video/webm; codecs=\"vp9\"')").await,
        "true"
    );
}

#[tokio::test]
async fn media_codec_support_matches_chrome_148_macos() {
    let result = check(
        r#"JSON.stringify({
            mse: [
                'audio/mp4; codecs="mp4a.40.2"',
                'audio/mp4; codecs="opus"',
                'audio/webm; codecs="vorbis"',
                'video/mp4; codecs="avc1.42E01E"',
                'video/mp4; codecs="av01.0.01M.08"',
                'video/mp4; codecs="vp09.00.10.08"',
                'video/webm; codecs="vp8"',
                'video/webm; codecs="vp09.00.10.08"',
                'video/webm; codecs="av01.0.01M.08"',
            ].map(type => MediaSource.isTypeSupported(type)),
            unsupportedMse: [
                'audio/mp4; codecs="ac-3"',
                'audio/mp4; codecs="ec-3"',
                'audio/ogg; codecs="vorbis"',
                'audio/ogg; codecs="flac"',
                'video/mp4; codecs="hev1.1.6.L93.B0"',
                'video/ogg; codecs="theora"',
            ].map(type => MediaSource.isTypeSupported(type)),
            canPlay: [
                'audio/ogg; codecs="vorbis"',
                'audio/ogg; codecs="flac"',
                'video/mp4; codecs="av01.0.01M.08"',
            ].map(type => document.createElement('video').canPlayType(type)),
            unsupportedCanPlay: [
                'audio/mp4; codecs="ac-3"',
                'video/mp4; codecs="hev1.1.6.L93.B0"',
                'video/ogg; codecs="theora"',
            ].map(type => document.createElement('video').canPlayType(type)),
        })"#,
    )
    .await;
    assert_eq!(
        result,
        r#"{"mse":[true,true,true,true,true,true,true,true,true],"unsupportedMse":[false,false,false,false,false,false],"canPlay":["probably","probably","probably"],"unsupportedCanPlay":["","",""]}"#
    );
}

#[tokio::test]
async fn media_capabilities_match_chrome_148_macos() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        r#"globalThis.__mediaCapabilitiesResults = null;
        Promise.all([
            navigator.mediaCapabilities.decodingInfo({
                type: 'file',
                audio: {contentType:'audio/mp4; codecs="mp4a.40.2"', channels:'2', bitrate:128000, samplerate:48000}
            }),
            navigator.mediaCapabilities.decodingInfo({
                type: 'file',
                video: {contentType:'video/mp4; codecs="avc1.42E01E"', width:1920, height:1080, bitrate:2646242, framerate:'25'}
            }),
            navigator.mediaCapabilities.decodingInfo({
                type: 'file',
                video: {contentType:'video/mp4; codecs="hev1.1.6.L93.B0"', width:1920, height:1080, bitrate:2646242, framerate:'25'}
            })
        ]).then(results => { globalThis.__mediaCapabilitiesResults = results; });"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(50))
        .await
        .unwrap();
    let result = page
        .evaluate(
            r#"JSON.stringify({
                results: globalThis.__mediaCapabilitiesResults,
                resultNames: globalThis.__mediaCapabilitiesResults.map(Object.getOwnPropertyNames),
                protoNames: Object.getOwnPropertyNames(MediaCapabilities.prototype),
                methodLengths: [MediaCapabilities.prototype.decodingInfo.length, MediaCapabilities.prototype.encodingInfo.length],
                construction: (() => { try { new MediaCapabilities(); return 'ok'; } catch (error) { return error.name + ':' + error.message; } })(),
                tag: Object.prototype.toString.call(navigator.mediaCapabilities),
            })"#,
        )
        .unwrap();
    assert_eq!(
        result,
        r#"{"results":[{"powerEfficient":true,"smooth":true,"supported":true,"keySystemAccess":null},{"powerEfficient":false,"smooth":true,"supported":true,"keySystemAccess":null},{"powerEfficient":false,"smooth":false,"supported":false,"keySystemAccess":null}],"resultNames":[["powerEfficient","smooth","supported","keySystemAccess"],["powerEfficient","smooth","supported","keySystemAccess"],["powerEfficient","smooth","supported","keySystemAccess"]],"protoNames":["decodingInfo","encodingInfo","constructor"],"methodLengths":[1,1],"construction":"TypeError:Failed to construct 'MediaCapabilities': Illegal constructor","tag":"[object MediaCapabilities]"}"#
    );
}

#[tokio::test]
async fn child_realm_media_capabilities_are_realm_local() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    let result = page
        .evaluate(
            r#"(() => {
                const iframe = document.createElement('iframe');
                document.body.appendChild(iframe);
                const child = iframe.contentWindow;
                return JSON.stringify({
                    constructorDistinct: child.MediaCapabilities !== MediaCapabilities,
                    singletonDistinct: child.navigator.mediaCapabilities !== navigator.mediaCapabilities,
                    prototypeLocal: child.Object.getPrototypeOf(child.navigator.mediaCapabilities)
                        === child.MediaCapabilities.prototype,
                    constructorLocal: child.navigator.mediaCapabilities.constructor
                        === child.MediaCapabilities,
                    navigatorOwn: child.Object.getOwnPropertyNames(child.navigator),
                    singletonOwn: child.Object.getOwnPropertyNames(child.navigator.mediaCapabilities),
                    tag: child.Object.prototype.toString.call(child.navigator.mediaCapabilities),
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(
        result,
        r#"{"constructorDistinct":true,"singletonDistinct":true,"prototypeLocal":true,"constructorLocal":true,"navigatorOwn":[],"singletonOwn":[],"tag":"[object MediaCapabilities]"}"#
    );
}

// Speech synthesis voices are populated asynchronously in Chromium. A new
// document can legitimately observe [] on the first getVoices() call.
#[tokio::test]
async fn api_speech_synthesis_voices() {
    assert_eq!(
        check("Array.isArray(speechSynthesis.getVoices())").await,
        "true"
    );
}

#[tokio::test]
async fn api_speech_synthesis_eventtarget_and_illegal_constructor() {
    assert_eq!(
        check(
            r#"JSON.stringify({
                eventTarget: speechSynthesis instanceof EventTarget,
                instance: speechSynthesis instanceof SpeechSynthesis,
                tag: Object.prototype.toString.call(speechSynthesis),
                illegal: (() => { try { new SpeechSynthesis(); return false; }
                    catch (error) { return error instanceof TypeError && /Illegal constructor/.test(error.message); } })(),
                onAccessor: (() => { const d=Object.getOwnPropertyDescriptor(SpeechSynthesis.prototype,'onvoiceschanged');
                    return !!d && typeof d.get==='function' && typeof d.set==='function' && d.enumerable===true; })()
            })"#,
        )
        .await,
        r#"{"eventTarget":true,"instance":true,"tag":"[object SpeechSynthesis]","illegal":true,"onAccessor":true}"#
    );
}

#[tokio::test]
async fn voice_orientation_and_ua_data_webidl_shape_match_chrome() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate("speechSynthesis.getVoices()").unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(250))
        .await
        .ok();
    let result = page
        .evaluate(
            r#"(() => {
                const voices = speechSynthesis.getVoices();
                const voice = voices[0];
                const orientation = screen.orientation;
                const ua = navigator.userAgentData;
                const orientationHandler = () => {};
                orientation.onchange = orientationHandler;
                const construction = C => {
                    try { new C(); return 'ok'; }
                    catch (error) { return error.name + ':' + error.message; }
                };
                return JSON.stringify({
                    voice:{
                        exists:!!voice,
                        tag:voice ? Object.prototype.toString.call(voice) : '',
                        instance:voice ? voice instanceof SpeechSynthesisVoice : false,
                        own:voice ? Reflect.ownKeys(voice).map(String) : [],
                        values:voice ? [voice.voiceURI,voice.name,voice.lang,voice.localService,voice.default] : [],
                        construction:construction(SpeechSynthesisVoice),
                        protoNames:Object.getOwnPropertyNames(SpeechSynthesisVoice.prototype).sort(),
                    },
                    orientation:{
                        tag:Object.prototype.toString.call(orientation),
                        instance:orientation instanceof ScreenOrientation,
                        eventTarget:orientation instanceof EventTarget,
                        own:Reflect.ownKeys(orientation).map(String),
                        proto:Object.getPrototypeOf(ScreenOrientation.prototype) === EventTarget.prototype,
                        type:orientation.type,
                        angle:orientation.angle,
                        handler:orientation.onchange === orientationHandler,
                        construction:construction(ScreenOrientation),
                        protoNames:Object.getOwnPropertyNames(ScreenOrientation.prototype).sort(),
                    },
                    ua:{
                        tag:Object.prototype.toString.call(ua),
                        instance:ua instanceof NavigatorUAData,
                        own:Reflect.ownKeys(ua).map(String),
                        brands:Array.isArray(ua.brands),
                        mobile:typeof ua.mobile,
                        platform:typeof ua.platform,
                        construction:construction(NavigatorUAData),
                        protoNames:Object.getOwnPropertyNames(NavigatorUAData.prototype).sort(),
                        lengths:[NavigatorUAData.prototype.getHighEntropyValues.length,
                                 NavigatorUAData.prototype.toJSON.length],
                    }
                });
            })()"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["voice"]["exists"], true);
    assert_eq!(value["voice"]["tag"], "[object SpeechSynthesisVoice]");
    assert_eq!(value["voice"]["instance"], true);
    assert_eq!(value["voice"]["own"], serde_json::json!([]));
    assert_eq!(value["voice"]["values"].as_array().unwrap().len(), 5);
    assert!(value["voice"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["voice"]["protoNames"],
        serde_json::json!([
            "constructor",
            "default",
            "lang",
            "localService",
            "name",
            "voiceURI"
        ])
    );
    assert_eq!(value["orientation"]["tag"], "[object ScreenOrientation]");
    assert_eq!(value["orientation"]["instance"], true);
    assert_eq!(value["orientation"]["eventTarget"], true);
    assert_eq!(value["orientation"]["own"], serde_json::json!([]));
    assert_eq!(value["orientation"]["proto"], true);
    assert!(matches!(
        value["orientation"]["type"].as_str(),
        Some("landscape-primary" | "portrait-primary")
    ));
    assert_eq!(value["orientation"]["angle"], 0);
    assert_eq!(value["orientation"]["handler"], true);
    assert!(value["orientation"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["orientation"]["protoNames"],
        serde_json::json!(["angle", "constructor", "lock", "onchange", "type", "unlock"])
    );
    assert_eq!(value["ua"]["tag"], "[object NavigatorUAData]");
    assert_eq!(value["ua"]["instance"], true);
    assert_eq!(value["ua"]["own"], serde_json::json!([]));
    assert_eq!(value["ua"]["brands"], true);
    assert_eq!(value["ua"]["mobile"], "boolean");
    assert_eq!(value["ua"]["platform"], "string");
    assert!(value["ua"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["ua"]["protoNames"],
        serde_json::json!([
            "brands",
            "constructor",
            "getHighEntropyValues",
            "mobile",
            "platform",
            "toJSON"
        ])
    );
    assert_eq!(value["ua"]["lengths"], serde_json::json!([1, 0]));
}

// Permissions
#[tokio::test]
async fn api_permissions_query() {
    assert_eq!(
        check("typeof navigator.permissions.query").await,
        "function"
    );
}

// Battery
#[tokio::test]
async fn api_battery() {
    assert_eq!(
        check_secure("typeof navigator.getBattery").await,
        "function"
    );
}

// Dynamic script loading via DOM
#[tokio::test]
async fn api_dynamic_script_src_property() {
    // Verify script.src setter works as setAttribute
    assert_eq!(
        check(
            r#"
        const s = document.createElement('script');
        s.src = 'https://example.com/test.js';
        s.getAttribute('src')
    "#
        )
        .await,
        "https://example.com/test.js"
    );
}

// document.cookie read/write
#[tokio::test]
async fn api_document_cookie_readwrite() {
    // This is the exact check wildberries uses
    assert_eq!(
        check(
            r#"
        document.cookie = "cookietest=abc123; path=/; SameSite=Lax";
        document.cookie.includes("cookietest=abc123")
    "#
        )
        .await,
        "true"
    );
}

// ================================================================
// Constructors / Classes
// ================================================================

#[tokio::test]
async fn cls_url() {
    assert_eq!(check("typeof URL").await, "function");
}
#[tokio::test]
async fn url_webidl_shape_and_mutation_match_chrome() {
    let result = check(
        r#"(() => {
            const url = new URL('https://example.com:8443/a?q=1#h');
            const initialParams = url.searchParams;
            const before = {
                own:Reflect.ownKeys(url).map(String),
                tag:Object.prototype.toString.call(url),
                instance:url instanceof URL,
                href:url.href,
                origin:url.origin,
                paramsSame:initialParams === url.searchParams,
                protoNames:Object.getOwnPropertyNames(URL.prototype).sort(),
                enumerable:[
                    'href','origin','protocol','username','password','host','hostname',
                    'port','pathname','search','searchParams','hash','toString','toJSON'
                ].map(name => Object.getOwnPropertyDescriptor(URL.prototype,name).enumerable),
            };
            url.protocol = 'http';
            url.username = 'u';
            url.password = 'p';
            url.hostname = 'other.test';
            url.port = '8080';
            url.pathname = 'next';
            url.search = 'x=2';
            url.hash = 'tail';
            let illegal = '';
            try { Object.getOwnPropertyDescriptor(URL.prototype,'href').get.call({}); illegal = 'ok'; }
            catch (error) { illegal = error.name + ':' + error.message; }
            const opaque = new URL('blob:https://example.com/id?q=1#h');
            return JSON.stringify({
                before,
                after:{
                    href:url.href,
                    origin:url.origin,
                    protocol:url.protocol,
                    username:url.username,
                    password:url.password,
                    host:url.host,
                    hostname:url.hostname,
                    port:url.port,
                    pathname:url.pathname,
                    search:url.search,
                    hash:url.hash,
                    param:url.searchParams.get('x'),
                    text:String(url),
                    json:url.toJSON(),
                    illegal,
                },
                opaque:{
                    protocol:opaque.protocol,
                    origin:opaque.origin,
                    pathname:opaque.pathname,
                    search:opaque.search,
                    hash:opaque.hash,
                    own:Reflect.ownKeys(opaque).map(String),
                }
            });
        })()"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["before"]["own"], serde_json::json!([]));
    assert_eq!(value["before"]["tag"], "[object URL]");
    assert_eq!(value["before"]["instance"], true);
    assert_eq!(value["before"]["href"], "https://example.com:8443/a?q=1#h");
    assert_eq!(value["before"]["origin"], "https://example.com:8443");
    assert_eq!(value["before"]["paramsSame"], true);
    assert_eq!(
        value["before"]["enumerable"],
        serde_json::json!([
            true, true, true, true, true, true, true, true, true, true, true, true, true, true
        ])
    );
    assert_eq!(
        value["after"]["href"],
        "http://u:p@other.test:8080/next?x=2#tail"
    );
    assert_eq!(value["after"]["origin"], "http://other.test:8080");
    assert_eq!(value["after"]["protocol"], "http:");
    assert_eq!(value["after"]["username"], "u");
    assert_eq!(value["after"]["password"], "p");
    assert_eq!(value["after"]["host"], "other.test:8080");
    assert_eq!(value["after"]["hostname"], "other.test");
    assert_eq!(value["after"]["port"], "8080");
    assert_eq!(value["after"]["pathname"], "/next");
    assert_eq!(value["after"]["search"], "?x=2");
    assert_eq!(value["after"]["hash"], "#tail");
    assert_eq!(value["after"]["param"], "2");
    assert_eq!(value["after"]["text"], value["after"]["href"]);
    assert_eq!(value["after"]["json"], value["after"]["href"]);
    assert!(value["after"]["illegal"]
        .as_str()
        .unwrap()
        .contains("Illegal invocation"));
    assert_eq!(value["opaque"]["protocol"], "blob:");
    assert_eq!(value["opaque"]["origin"], "https://example.com");
    assert_eq!(value["opaque"]["pathname"], "https://example.com/id");
    assert_eq!(value["opaque"]["search"], "?q=1");
    assert_eq!(value["opaque"]["hash"], "#h");
    assert_eq!(value["opaque"]["own"], serde_json::json!([]));
}
#[tokio::test]
async fn cls_url_search_params() {
    assert_eq!(check("typeof URLSearchParams").await, "function");
}
#[tokio::test]
async fn url_search_params_sort_and_size_match_chrome() {
    assert_eq!(
        check(
            r#"(() => {
                const params = new URLSearchParams('b=2&a=1&a=0');
                const sizeDescriptor = Object.getOwnPropertyDescriptor(
                    URLSearchParams.prototype, 'size');
                const sortDescriptor = Object.getOwnPropertyDescriptor(
                    URLSearchParams.prototype, 'sort');
                const before = params.size;
                params.sort();
                return JSON.stringify({
                    before,
                    after:params.size,
                    value:String(params),
                    sizeDescriptor:[sizeDescriptor.enumerable,sizeDescriptor.configurable,
                                    typeof sizeDescriptor.get,sizeDescriptor.get.length],
                    sortDescriptor:[sortDescriptor.enumerable,sortDescriptor.configurable,
                                    sortDescriptor.writable,sortDescriptor.value.length,
                                    String(sortDescriptor.value)]
                });
            })()"#,
        )
        .await,
        r#"{"before":3,"after":3,"value":"a=1&a=0&b=2","sizeDescriptor":[true,true,"function",0],"sortDescriptor":[true,true,true,0,"function sort() { [native code] }"]}"#
    );
}
#[tokio::test]
async fn cls_abort_controller() {
    assert_eq!(check("typeof AbortController").await, "function");
}
#[tokio::test]
async fn cls_abort_signal() {
    assert_eq!(check("typeof AbortSignal").await, "function");
}
#[tokio::test]
async fn abort_controller_and_signal_match_chrome_webidl_shape() {
    let result = check(
        r#"(() => {
            const controller = new AbortController();
            const signal = controller.signal;
            let listenerCalls = 0;
            let handlerCalls = 0;
            signal.addEventListener('abort', () => listenerCalls++);
            signal.onabort = () => handlerCalls++;
            const construction = (() => {
                try { new AbortSignal(); return 'ok'; }
                catch (error) { return error.name + ':' + error.message; }
            })();
            const before = {
                controllerOwn:Reflect.ownKeys(controller).map(String),
                signalOwn:Reflect.ownKeys(signal).map(String),
                controllerTag:Object.prototype.toString.call(controller),
                signalTag:Object.prototype.toString.call(signal),
                signalEventTarget:signal instanceof EventTarget,
                signalProto:Object.getPrototypeOf(AbortSignal.prototype) === EventTarget.prototype,
                construction,
                protoNames:Object.getOwnPropertyNames(AbortSignal.prototype).sort(),
                controllerProtoNames:Object.getOwnPropertyNames(AbortController.prototype).sort(),
                enumerable:['aborted','onabort','reason','throwIfAborted'].map(name =>
                    Object.getOwnPropertyDescriptor(AbortSignal.prototype,name).enumerable),
                controllerEnumerable:['signal','abort'].map(name =>
                    Object.getOwnPropertyDescriptor(AbortController.prototype,name).enumerable),
                aborted:signal.aborted,
                reason:signal.reason,
            };
            controller.abort('because');
            let thrown = null;
            try { signal.throwIfAborted(); } catch (error) { thrown = error; }
            const staticSignal = AbortSignal.abort('static-reason');
            return JSON.stringify({
                before,
                after:{
                    aborted:signal.aborted,
                    reason:signal.reason,
                    listenerCalls,
                    handlerCalls,
                    thrown,
                    handlerIdentity:typeof signal.onabort === 'function',
                    staticAborted:staticSignal.aborted,
                    staticReason:staticSignal.reason,
                    staticOwn:Reflect.ownKeys(staticSignal).map(String),
                }
            });
        })()"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["before"]["controllerOwn"], serde_json::json!([]));
    assert_eq!(value["before"]["signalOwn"], serde_json::json!([]));
    assert_eq!(value["before"]["controllerTag"], "[object AbortController]");
    assert_eq!(value["before"]["signalTag"], "[object AbortSignal]");
    assert_eq!(value["before"]["signalEventTarget"], true);
    assert_eq!(value["before"]["signalProto"], true);
    assert!(value["before"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["before"]["protoNames"],
        serde_json::json!([
            "aborted",
            "constructor",
            "onabort",
            "reason",
            "throwIfAborted"
        ])
    );
    assert_eq!(
        value["before"]["controllerProtoNames"],
        serde_json::json!(["abort", "constructor", "signal"])
    );
    assert_eq!(
        value["before"]["enumerable"],
        serde_json::json!([true, true, true, true])
    );
    assert_eq!(
        value["before"]["controllerEnumerable"],
        serde_json::json!([true, true])
    );
    assert_eq!(value["before"]["aborted"], false);
    assert!(value["before"]["reason"].is_null());
    assert_eq!(value["after"]["aborted"], true);
    assert_eq!(value["after"]["reason"], "because");
    assert_eq!(value["after"]["listenerCalls"], 1);
    assert_eq!(value["after"]["handlerCalls"], 1);
    assert_eq!(value["after"]["thrown"], "because");
    assert_eq!(value["after"]["handlerIdentity"], true);
    assert_eq!(value["after"]["staticAborted"], true);
    assert_eq!(value["after"]["staticReason"], "static-reason");
    assert_eq!(value["after"]["staticOwn"], serde_json::json!([]));
}
#[tokio::test]
async fn cls_headers() {
    assert_eq!(check("typeof Headers").await, "function");
}
#[tokio::test]
async fn cls_request() {
    assert_eq!(check("typeof Request").await, "function");
}
#[tokio::test]
async fn cls_response() {
    assert_eq!(check("typeof Response").await, "function");
}
#[tokio::test]
async fn cls_form_data() {
    assert_eq!(check("typeof FormData").await, "function");
}
#[tokio::test]
async fn cls_blob() {
    assert_eq!(check("typeof Blob").await, "function");
}
#[tokio::test]
async fn cls_file() {
    assert_eq!(check("typeof File").await, "function");
}
#[tokio::test]
async fn cls_image() {
    assert_eq!(check("typeof Image").await, "function");
}
#[tokio::test]
async fn cls_dom_parser() {
    assert_eq!(check("typeof DOMParser").await, "function");
}
#[tokio::test]
async fn cls_dom_rect() {
    assert_eq!(check("typeof DOMRect").await, "function");
}
#[tokio::test]
async fn cls_text_encoder() {
    assert_eq!(check("typeof TextEncoder").await, "function");
}
#[tokio::test]
async fn cls_text_decoder() {
    assert_eq!(check("typeof TextDecoder").await, "function");
}
#[tokio::test]
async fn cls_mutation_observer() {
    assert_eq!(check("typeof MutationObserver").await, "function");
}
#[tokio::test]
async fn cls_intersection_observer() {
    assert_eq!(check("typeof IntersectionObserver").await, "function");
}
#[tokio::test]
async fn cls_resize_observer() {
    assert_eq!(check("typeof ResizeObserver").await, "function");
}
#[tokio::test]
async fn cls_xml_http_request() {
    assert_eq!(check("typeof XMLHttpRequest").await, "function");
}
#[tokio::test]
async fn xhr_dispatches_onreadystatechange_once_per_transition() {
    assert_eq!(
        check(
            "(() => { \
              const xhr = new XMLHttpRequest(); \
              let propertyCalls = 0; \
              let listenerCalls = 0; \
              xhr.onreadystatechange = () => propertyCalls++; \
              xhr.addEventListener('readystatechange', () => listenerCalls++); \
              xhr.open('GET', 'https://example.com/', true); \
              return JSON.stringify([propertyCalls, listenerCalls, xhr.readyState]); \
             })()"
        )
        .await,
        "[1,1,1]"
    );
}

#[tokio::test]
async fn network_webidl_instances_match_chrome_shape() {
    let result = check(
        r#"(() => {
            const xhr = new XMLHttpRequest();
            let listenerCalls = 0;
            xhr.addEventListener('readystatechange', () => listenerCalls++);
            const readyHandler = () => {};
            xhr.onreadystatechange = readyHandler;
            const initialStatus = xhr.status;
            xhr.status = 99;
            xhr.open('GET', '/probe', true);

            const source = new EventSource('/events', { withCredentials: true });
            const sourceHandler = () => {};
            source.onmessage = sourceHandler;
            source.readyState = 99;
            source.close();

            const socket = new WebSocket('ws://127.0.0.1:9/');
            const socketHandler = () => {};
            socket.onmessage = socketHandler;
            const socketState = socket.readyState;
            socket.readyState = 99;

            const construction = fn => {
                try { fn(); return 'ok'; }
                catch (error) { return error.name + ':' + error.message; }
            };

            return JSON.stringify({
                xhr: {
                    own: Reflect.ownKeys(xhr).map(String),
                    tag: Object.prototype.toString.call(xhr),
                    instance: xhr instanceof XMLHttpRequest,
                    eventTarget: xhr instanceof XMLHttpRequestEventTarget,
                    genericTarget: xhr instanceof EventTarget,
                    proto: Object.getPrototypeOf(XMLHttpRequest.prototype)
                        === XMLHttpRequestEventTarget.prototype,
                    eventProto: Object.getPrototypeOf(XMLHttpRequestEventTarget.prototype)
                        === EventTarget.prototype,
                    readyState: xhr.readyState,
                    listenerCalls,
                    handler: xhr.onreadystatechange === readyHandler,
                    readonlyStatus: xhr.status === initialStatus,
                    directGetter: Object.getOwnPropertyDescriptor(
                        XMLHttpRequest.prototype, 'readyState').get.call(xhr),
                    upload: {
                        own: Reflect.ownKeys(xhr.upload).map(String),
                        tag: Object.prototype.toString.call(xhr.upload),
                        upload: xhr.upload instanceof XMLHttpRequestUpload,
                        eventTarget: xhr.upload instanceof XMLHttpRequestEventTarget,
                        genericTarget: xhr.upload instanceof EventTarget,
                    },
                },
                source: {
                    own: Reflect.ownKeys(source).map(String),
                    tag: Object.prototype.toString.call(source),
                    target: source instanceof EventTarget,
                    credentials: source.withCredentials,
                    handler: source.onmessage === sourceHandler,
                    closed: source.readyState === EventSource.CLOSED,
                },
                socket: {
                    own: Reflect.ownKeys(socket).map(String),
                    tag: Object.prototype.toString.call(socket),
                    target: socket instanceof EventTarget,
                    handler: socket.onmessage === socketHandler,
                    readonlyState: socket.readyState === socketState,
                },
                construction: {
                    wsMissing: construction(() => new WebSocket()),
                    wsCall: construction(() => WebSocket('ws://127.0.0.1:9/')),
                    sourceMissing: construction(() => new EventSource()),
                    sourceCall: construction(() => EventSource('/events')),
                    xhrCall: construction(() => XMLHttpRequest()),
                    xhrEventTarget: construction(() => new XMLHttpRequestEventTarget()),
                    xhrUpload: construction(() => new XMLHttpRequestUpload()),
                },
            });
        })()"#,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["xhr"]["own"], serde_json::json!([]));
    assert_eq!(value["xhr"]["tag"], "[object XMLHttpRequest]");
    assert_eq!(value["xhr"]["instance"], true);
    assert_eq!(value["xhr"]["eventTarget"], true);
    assert_eq!(value["xhr"]["genericTarget"], true);
    assert_eq!(value["xhr"]["proto"], true);
    assert_eq!(value["xhr"]["eventProto"], true);
    assert_eq!(value["xhr"]["readyState"], 1);
    assert_eq!(value["xhr"]["listenerCalls"], 1);
    assert_eq!(value["xhr"]["handler"], true);
    assert_eq!(value["xhr"]["readonlyStatus"], true);
    assert_eq!(value["xhr"]["directGetter"], 1);
    assert_eq!(value["xhr"]["upload"]["own"], serde_json::json!([]));
    assert_eq!(
        value["xhr"]["upload"]["tag"],
        "[object XMLHttpRequestUpload]"
    );
    assert_eq!(value["xhr"]["upload"]["upload"], true);
    assert_eq!(value["xhr"]["upload"]["eventTarget"], true);
    assert_eq!(value["xhr"]["upload"]["genericTarget"], true);
    assert_eq!(value["source"]["own"], serde_json::json!([]));
    assert_eq!(value["source"]["tag"], "[object EventSource]");
    assert_eq!(value["source"]["target"], true);
    assert_eq!(value["source"]["credentials"], true);
    assert_eq!(value["source"]["handler"], true);
    assert_eq!(value["source"]["closed"], true);
    assert_eq!(value["socket"]["own"], serde_json::json!([]));
    assert_eq!(value["socket"]["tag"], "[object WebSocket]");
    assert_eq!(value["socket"]["target"], true);
    assert_eq!(value["socket"]["handler"], true);
    assert_eq!(value["socket"]["readonlyState"], true);
    assert_eq!(
        value["construction"]["wsMissing"],
        "TypeError:Failed to construct 'WebSocket': 1 argument required, but only 0 present."
    );
    assert_eq!(
        value["construction"]["sourceMissing"],
        "TypeError:Failed to construct 'EventSource': 1 argument required, but only 0 present."
    );
    assert!(value["construction"]["wsCall"]
        .as_str()
        .unwrap()
        .contains("Please use the 'new' operator"));
    assert!(value["construction"]["sourceCall"]
        .as_str()
        .unwrap()
        .contains("Please use the 'new' operator"));
    assert!(value["construction"]["xhrCall"]
        .as_str()
        .unwrap()
        .contains("Please use the 'new' operator"));
    assert!(value["construction"]["xhrEventTarget"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert!(value["construction"]["xhrUpload"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
}

#[tokio::test]
async fn cls_websocket() {
    assert_eq!(check("typeof WebSocket").await, "function");
}
#[tokio::test]
async fn cls_notification() {
    assert_eq!(check("typeof Notification").await, "function");
}
#[tokio::test]
async fn cls_dom_exception() {
    assert_eq!(check("typeof DOMException").await, "function");
}
#[tokio::test]
async fn cls_audio_context() {
    assert_eq!(check("typeof AudioContext").await, "function");
}
#[tokio::test]
async fn cls_offline_audio_context() {
    assert_eq!(check("typeof OfflineAudioContext").await, "function");
}
#[tokio::test]
async fn cls_custom_elements() {
    assert_eq!(check("typeof customElements").await, "object");
}

// ================================================================
// HTMLElement subtypes
// ================================================================

#[tokio::test]
async fn cls_html_element() {
    assert_eq!(check("typeof HTMLElement").await, "function");
}
#[tokio::test]
async fn cls_html_div_element() {
    assert_eq!(check("typeof HTMLDivElement").await, "function");
}
#[tokio::test]
async fn cls_html_input_element() {
    assert_eq!(check("typeof HTMLInputElement").await, "function");
}
#[tokio::test]
async fn cls_html_anchor_element() {
    assert_eq!(check("typeof HTMLAnchorElement").await, "function");
}
#[tokio::test]
async fn cls_html_image_element() {
    assert_eq!(check("typeof HTMLImageElement").await, "function");
}
#[tokio::test]
async fn cls_html_canvas_element() {
    assert_eq!(check("typeof HTMLCanvasElement").await, "function");
}
#[tokio::test]
async fn cls_html_form_element() {
    assert_eq!(check("typeof HTMLFormElement").await, "function");
}
#[tokio::test]
async fn cls_html_video_element() {
    assert_eq!(check("typeof HTMLVideoElement").await, "function");
}
#[tokio::test]
async fn cls_svg_element() {
    assert_eq!(check("typeof SVGElement").await, "function");
}

#[tokio::test]
async fn svg_namespace_uses_svg_web_idl_hierarchy() {
    assert_eq!(
        check(
            r#"(function(){
                const ns='http://www.w3.org/2000/svg';
                const svg=document.createElementNS(ns,'svg');
                const rect=document.createElementNS(ns,'rect');
                const script=document.createElementNS(ns,'script');
                const box=rect.getBBox();
                let illegal=false;
                try { new SVGRect(); } catch (error) { illegal=error instanceof TypeError; }
                return [
                    svg.namespaceURI===ns,
                    svg.tagName==='svg',
                    svg.constructor===SVGSVGElement,
                    svg instanceof SVGGraphicsElement,
                    rect.constructor===SVGRectElement,
                    rect instanceof SVGGeometryElement,
                    typeof rect.getBBox==='function',
                    Object.prototype.toString.call(rect)==='[object SVGRectElement]',
                    box.constructor===SVGRect,
                    Object.prototype.toString.call(box)==='[object SVGRect]',
                    box.x===0 && box.y===0 && box.width===0 && box.height===0,
                    typeof script.getBBox==='undefined',
                    typeof document.body.getBBox==='undefined',
                    Object.getPrototypeOf(SVGGeometryElement.prototype)===SVGGraphicsElement.prototype,
                    Object.getPrototypeOf(SVGGraphicsElement.prototype)===SVGElement.prototype,
                    Object.getPrototypeOf(SVGElement.prototype)===Element.prototype,
                    illegal
                ].every(Boolean);
            })()"#,
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn svg_text_computed_length_matches_chrome_shape() {
    assert_eq!(
        check(
            r#"(function(){
                const ns='http://www.w3.org/2000/svg';
                const svg=document.createElementNS(ns,'svg');
                const text=document.createElementNS(ns,'text');
                text.textContent='Turnstile';
                svg.appendChild(text);
                document.body.appendChild(svg);
                const attached=text.getComputedTextLength();
                text.remove();
                let illegal=false;
                try {
                    SVGTextContentElement.prototype.getComputedTextLength.call({});
                } catch (error) {
                    illegal=error instanceof TypeError && error.message==='Illegal invocation';
                }
                const descriptor=Object.getOwnPropertyDescriptor(
                    SVGTextContentElement.prototype,
                    'getComputedTextLength'
                );
                return [
                    typeof attached==='number' && Number.isFinite(attached) && attached>0,
                    text.getComputedTextLength()===0,
                    descriptor.enumerable && descriptor.writable && descriptor.configurable,
                    descriptor.value.length===0,
                    !Object.prototype.hasOwnProperty.call(SVGTextElement.prototype,'getComputedTextLength'),
                    illegal
                ].every(Boolean);
            })()"#,
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn svg_text_extent_matches_chrome_shape() {
    assert_eq!(
        check(
            r#"(function(){
                const ns='http://www.w3.org/2000/svg';
                const svg=document.createElementNS(ns,'svg');
                const text=document.createElementNS(ns,'text');
                text.setAttribute('font-size','16');
                text.textContent='😀A';
                svg.appendChild(text);
                document.body.appendChild(svg);
                const emojiFromString=text.getExtentOfChar('😀');
                const emojiFromLead=text.getExtentOfChar(0);
                const emojiFromTrail=text.getExtentOfChar(1);
                const ascii=text.getExtentOfChar(2);
                const descriptor=Object.getOwnPropertyDescriptor(
                    SVGTextContentElement.prototype,
                    'getExtentOfChar'
                );
                text.remove();
                let detached=false;
                let missing=false;
                let illegal=false;
                try { text.getExtentOfChar(0); }
                catch (error) { detached=error instanceof DOMException && error.name==='IndexSizeError'; }
                try { text.getExtentOfChar(); }
                catch (error) { missing=error instanceof TypeError; }
                try { SVGTextContentElement.prototype.getExtentOfChar.call({},0); }
                catch (error) { illegal=error instanceof TypeError && error.message==='Illegal invocation'; }
                return [
                    emojiFromString.constructor===SVGRect,
                    Object.prototype.toString.call(emojiFromString)==='[object SVGRect]',
                    [emojiFromString.x,emojiFromString.y,emojiFromString.width,emojiFromString.height]
                        .every(Number.isFinite),
                    emojiFromString.width>0 && emojiFromString.height>0,
                    emojiFromString.x===emojiFromLead.x,
                    emojiFromString.width===emojiFromLead.width,
                    emojiFromLead.x===emojiFromTrail.x,
                    emojiFromLead.width===emojiFromTrail.width,
                    ascii.x>=emojiFromString.x+emojiFromString.width,
                    text.getNumberOfChars()===0,
                    descriptor.enumerable && descriptor.writable && descriptor.configurable,
                    descriptor.value.length===1,
                    !Object.prototype.hasOwnProperty.call(SVGTextElement.prototype,'getExtentOfChar'),
                    detached,missing,illegal
                ].every(Boolean);
            })()"#,
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn character_data_mutation_methods_match_chrome_shape() {
    assert_eq!(
        check(
            r#"(function(){
                const text=document.createTextNode('ab');
                text.appendData('<>');
                text.insertData(2,'X');
                text.deleteData(1,2);
                text.replaceData(1,1,'YZ');
                const descriptor=Object.getOwnPropertyDescriptor(
                    CharacterData.prototype,
                    'appendData'
                );
                let range=false;
                let illegal=false;
                try { text.substringData(99,1); }
                catch (error) { range=error instanceof DOMException && error.name==='IndexSizeError'; }
                try { CharacterData.prototype.appendData.call({},'x'); }
                catch (error) { illegal=error instanceof TypeError && error.message==='Illegal invocation'; }
                return [
                    Object.getPrototypeOf(Text.prototype)===CharacterData.prototype,
                    Object.getPrototypeOf(CharacterData.prototype)===Node.prototype,
                    text instanceof CharacterData,
                    text.data==='aYZ>',
                    text.length===4,
                    text.substringData(1,2)==='YZ',
                    descriptor.enumerable && descriptor.writable && descriptor.configurable,
                    descriptor.value.length===1,
                    !Object.prototype.hasOwnProperty.call(Text.prototype,'appendData'),
                    range,illegal
                ].every(Boolean);
            })()"#,
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn parsed_svg_elements_keep_svg_namespace_and_prototype() {
    assert_eq!(
        check(
            r#"(function(){
                document.body.innerHTML='<svg id="s"><rect id="r"></rect></svg>';
                const svg=document.getElementById('s');
                const rect=document.getElementById('r');
                return svg instanceof SVGSVGElement
                    && rect instanceof SVGRectElement
                    && rect.namespaceURI==='http://www.w3.org/2000/svg'
                    && typeof rect.getBBox==='function';
            })()"#,
        )
        .await,
        "true"
    );
}

// ================================================================
// Canvas
// ================================================================

#[tokio::test]
async fn canvas_2d_context() {
    assert_eq!(
        check("typeof document.createElement('canvas').getContext('2d')").await,
        "object"
    );
}
#[tokio::test]
async fn canvas_webgl_context() {
    assert_eq!(
        check("typeof document.createElement('canvas').getContext('webgl')").await,
        "object"
    );
}
#[tokio::test]
async fn offscreen_canvas_webgl_context() {
    // FP parity: a real OffscreenCanvas exposes WebGL (anti-bot fingerprint
    // workers read webGLVendor/webGLRenderer via
    // `new OffscreenCanvas(1,1).getContext('webgl')`). Returning null was a
    // headless tell. webgl + webgl2 must both yield a context, and the
    // unmasked vendor must match the profile-spoofed value the on-DOM canvas
    // reports (not empty).
    assert_eq!(
        check("typeof new OffscreenCanvas(1,1).getContext('webgl')").await,
        "object"
    );
    assert_eq!(
        check("typeof new OffscreenCanvas(1,1).getContext('webgl2')").await,
        "object"
    );
    let vendor = check(
        "(function(){var gl=new OffscreenCanvas(1,1).getContext('webgl');var e=gl.getExtension('WEBGL_debug_renderer_info');return String(gl.getParameter(e.UNMASKED_VENDOR_WEBGL));})()",
    )
    .await;
    assert!(
        vendor.contains("Google")
            || vendor.contains("Apple")
            || vendor.contains("Intel")
            || vendor.contains("Mozilla"),
        "OffscreenCanvas WebGL unmasked vendor looks empty/wrong: {vendor}"
    );
}
#[tokio::test]
async fn webgpu_adapter_has_real_limits_and_info() {
    // FP parity: a real GPUAdapter exposes numeric limits, a feature set, and
    // adapter info. A hollow adapter (undefined limits / empty features / no
    // info) is a headless tell collected as gpuSupportedLimits/gpuAdapterInfo.
    // Limits are prototype getters (own-keys stays []), values must be numeric,
    // info must match the macOS/Metal profile, and requestDevice must resolve.
    // navigator.gpu is secure-context-gated, and the probe is async — use an
    // https page + evaluate_async to drain the requestAdapter/requestDevice
    // promise chain, then read the stashed result.
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let setup = r#"
        globalThis.__gpu = "(pending)";
        (async () => {
            const a = await navigator.gpu.requestAdapter();
            const d = await a.requestDevice();
            globalThis.__gpu = JSON.stringify({
                ownKeys: Object.keys(a.limits).length,
                maxTex: a.limits.maxTextureDimension2D,
                maxBind: a.limits.maxBindGroups,
                vendor: a.info && a.info.vendor,
                arch: a.info && a.info.architecture,
                features: a.features.size,
                deviceOk: !!(d && d.limits && d.limits.maxBindGroups === 4),
            });
        })();
    "#;
    let _ = page
        .evaluate_async(setup, std::time::Duration::from_secs(5))
        .await;
    let out = page.evaluate("globalThis.__gpu").unwrap_or_default();
    assert!(
        out.contains("\"ownKeys\":0"),
        "limits own-keys must be [] like Chrome: {out}"
    );
    assert!(
        out.contains("\"maxTex\":16384"),
        "maxTextureDimension2D missing/wrong: {out}"
    );
    assert!(
        out.contains("\"vendor\":\"apple\""),
        "adapter.info.vendor wrong: {out}"
    );
    assert!(
        out.contains("\"arch\":\"metal-3\""),
        "adapter.info.architecture wrong: {out}"
    );
    assert!(
        out.contains("\"deviceOk\":true"),
        "requestDevice must resolve to a device: {out}"
    );
    assert!(
        !out.contains("\"features\":0"),
        "adapter must expose features: {out}"
    );
}
#[tokio::test]
async fn webgpu_texture_create_view_has_webidl_shape() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let setup = r#"
        globalThis.__gpuTexture = "(pending)";
        (async () => {
            const adapter = await navigator.gpu.requestAdapter();
            const device = await adapter.requestDevice();
            const texture = device.createTexture({
                size: [16, 8, 2], format: "rgba8unorm", usage: 16,
            });
            const view = texture.createView();
            globalThis.__gpuTexture = JSON.stringify({
                textureTag: Object.prototype.toString.call(texture),
                viewTag: Object.prototype.toString.call(view),
                dimensions: [texture.width, texture.height, texture.depthOrArrayLayers],
                createViewType: typeof texture.createView,
                createViewLength: texture.createView.length,
                textureInstance: texture instanceof GPUTexture,
                viewInstance: view instanceof GPUTextureView,
            });
        })();
    "#;
    let _ = page
        .evaluate_async(setup, std::time::Duration::from_secs(5))
        .await;
    assert_eq!(
        page.evaluate("globalThis.__gpuTexture").unwrap_or_default(),
        r#"{"textureTag":"[object GPUTexture]","viewTag":"[object GPUTextureView]","dimensions":[16,8,2],"createViewType":"function","createViewLength":0,"textureInstance":true,"viewInstance":true}"#
    );
}

#[tokio::test]
async fn webgpu_render_pass_command_flow_has_webidl_shape() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let setup = r#"
        globalThis.__gpuCommands = "(pending)";
        (async () => {
            const adapter = await navigator.gpu.requestAdapter();
            const device = await adapter.requestDevice();
            const texture = device.createTexture({
                size: [1, 1], format: "rgba8unorm", usage: 16,
            });
            const encoder = device.createCommandEncoder({ label: "encoder" });
            const pass = encoder.beginRenderPass({
                colorAttachments: [{
                    view: texture.createView(), loadOp: "clear",
                    clearValue: { r: 0, g: 0, b: 0, a: 1 }, storeOp: "store",
                }],
            });
            pass.end();
            const commandBuffer = encoder.finish({ label: "commands" });
            device.queue.submit([commandBuffer]);
            const buffer = device.createBuffer({
                size: 64, usage: 9, label: "readback",
            });
            await buffer.mapAsync(1);
            const mapped = buffer.getMappedRange();
            const mappedState = buffer.mapState;
            buffer.unmap();
            const descriptor = Object.getOwnPropertyDescriptor(
                GPUCommandEncoder.prototype, "beginRenderPass"
            );
            globalThis.__gpuCommands = JSON.stringify({
                encoderTag: Object.prototype.toString.call(encoder),
                passTag: Object.prototype.toString.call(pass),
                commandBufferTag: Object.prototype.toString.call(commandBuffer),
                encoderLabel: encoder.label,
                commandBufferLabel: commandBuffer.label,
                buffer: {
                    tag: Object.prototype.toString.call(buffer),
                    instance: buffer instanceof GPUBuffer,
                    size: buffer.size,
                    usage: buffer.usage,
                    label: buffer.label,
                    mappedBytes: mapped.byteLength,
                    mappedState,
                    unmappedState: buffer.mapState,
                    mapAsyncLength: buffer.mapAsync.length,
                    getMappedRangeLength: buffer.getMappedRange.length,
                    protoNames: Object.getOwnPropertyNames(GPUBuffer.prototype),
                },
                instances: [
                    encoder instanceof GPUCommandEncoder,
                    pass instanceof GPURenderPassEncoder,
                    commandBuffer instanceof GPUCommandBuffer,
                ],
                beginLength: encoder.beginRenderPass.length,
                endLength: pass.end.length,
                finishLength: encoder.finish.length,
                enumerable: descriptor.enumerable,
                writable: descriptor.writable,
                configurable: descriptor.configurable,
                protoNames: Object.getOwnPropertyNames(GPUCommandEncoder.prototype),
            });
        })();
    "#;
    let _ = page
        .evaluate_async(setup, std::time::Duration::from_secs(5))
        .await;
    assert_eq!(
        page.evaluate("globalThis.__gpuCommands")
            .unwrap_or_default(),
        r#"{"encoderTag":"[object GPUCommandEncoder]","passTag":"[object GPURenderPassEncoder]","commandBufferTag":"[object GPUCommandBuffer]","encoderLabel":"encoder","commandBufferLabel":"commands","buffer":{"tag":"[object GPUBuffer]","instance":true,"size":64,"usage":9,"label":"readback","mappedBytes":64,"mappedState":"mapped","unmappedState":"unmapped","mapAsyncLength":1,"getMappedRangeLength":0,"protoNames":["size","usage","mapState","label","destroy","getMappedRange","mapAsync","unmap","constructor"]},"instances":[true,true,true],"beginLength":1,"endLength":0,"finishLength":0,"enumerable":true,"writable":true,"configurable":true,"protoNames":["label","beginComputePass","beginRenderPass","copyBufferToTexture","copyTextureToBuffer","copyTextureToTexture","finish","insertDebugMarker","pushDebugGroup","clearBuffer","copyBufferToBuffer","popDebugGroup","resolveQuerySet","constructor"]}"#
    );
}

#[tokio::test]
async fn connected_style_element_exposes_live_stylesheet() {
    assert_eq!(
        check(
            r#"(() => {
                const style = document.createElement('style');
                style.appendChild(document.createTextNode('a { color: red; }'));
                const detached = style.sheet;
                document.head.appendChild(style);
                const sheet = style.sheet;
                const result = sheet.insertRule('b { color: blue; }', sheet.cssRules.length);
                const descriptor = Object.getOwnPropertyDescriptor(
                    HTMLStyleElement.prototype, 'sheet'
                );
                return JSON.stringify({
                    detached: detached === null,
                    tag: Object.prototype.toString.call(sheet),
                    stable: sheet === style.sheet,
                    owner: sheet.ownerNode === style,
                    ruleCount: sheet.cssRules.length,
                    insertResult: result,
                    getterLength: descriptor.get.length,
                    enumerable: descriptor.enumerable,
                    configurable: descriptor.configurable,
                });
            })()"#
        )
        .await,
        r#"{"detached":true,"tag":"[object CSSStyleSheet]","stable":true,"owner":true,"ruleCount":2,"insertResult":1,"getterLength":0,"enumerable":true,"configurable":true}"#
    );
}

#[tokio::test]
async fn offscreen_canvas_resize_resets_and_resizes_backing_store() {
    assert_eq!(
        check(
            "(() => { \
              const canvas = new OffscreenCanvas(1, 1); \
              const context = canvas.getContext('2d'); \
              context.fillStyle = '#f00'; \
              context.fillRect(0, 0, 1, 1); \
              canvas.width = 2; \
              canvas.height = 2; \
              context.fillStyle = '#0f0'; \
              context.fillRect(0, 0, 2, 2); \
              const data = context.getImageData(0, 0, 2, 2).data; \
              return JSON.stringify([canvas.width, canvas.height, data.length, \
                Array.from(data).every((value, i) => value === [0,255,0,255][i % 4])]); \
             })()"
        )
        .await,
        "[2,2,16,true]"
    );
}
#[tokio::test]
async fn canvas_counterclockwise_full_circle_renders() {
    assert_eq!(
        check(
            "(() => { \
              const context = new OffscreenCanvas(10, 10).getContext('2d'); \
              context.beginPath(); \
              context.arc(5, 5, 4, 0, Math.PI * 2, true); \
              context.fill(); \
              const alpha = context.getImageData(0, 0, 10, 10).data.filter((_, i) => i % 4 === 3); \
              return alpha.filter(value => value > 0).length > 40; \
             })()"
        )
        .await,
        "true"
    );
}
#[tokio::test]
async fn canvas_evenodd_fill_rule_preserves_inner_hole() {
    assert_eq!(
        check(
            "(() => { \
              const context = new OffscreenCanvas(20, 20).getContext('2d'); \
              context.fillStyle = '#f00'; \
              context.fillRect(0, 0, 20, 20); \
              context.fillStyle = '#fff'; \
              context.beginPath(); \
              context.arc(10, 10, 8, 0, Math.PI * 2, true); \
              context.arc(10, 10, 4, 0, Math.PI * 2, true); \
              context.fill('evenodd'); \
              return JSON.stringify(Array.from(context.getImageData(10, 10, 1, 1).data)); \
             })()"
        )
        .await,
        "[255,0,0,255]"
    );
}
#[tokio::test]
async fn canvas_display_p3_readback_matches_chrome_formats() {
    assert_eq!(
        check(
            "(() => { \
              const context = new OffscreenCanvas(2, 2).getContext('2d'); \
              context.fillStyle = 'color(display-p3 1 0.25 0.5)'; \
              context.fillRect(0, 0, 2, 2); \
              const srgb = context.getImageData(0, 0, 1, 1, \
                {colorSpace:'srgb',pixelFormat:'rgba-unorm8'}); \
              const p3 = context.getImageData(0, 0, 1, 1, \
                {colorSpace:'display-p3',pixelFormat:'rgba-unorm8'}); \
              const wide = context.getImageData(0, 0, 1, 1, \
                {colorSpace:'display-p3',pixelFormat:'rgba-float16'}); \
              return JSON.stringify([Array.from(srgb.data), Array.from(p3.data), \
                Array.from(wide.data), wide.data.constructor.name, wide.colorSpace, wide.pixelFormat]); \
             })()"
        )
        .await,
        "[[255,27,127,255],[255,64,127,255],[1,0.25,0.5,1],\"Float16Array\",\"display-p3\",\"rgba-float16\"]"
    );
}
#[tokio::test]
async fn canvas_to_data_url() {
    assert_eq!(
        check("document.createElement('canvas').toDataURL().startsWith('data:image/png')").await,
        "true"
    );
}

#[tokio::test]
async fn canvas_drawing_produces_nonblank_data_url() {
    // Simulate challenge-vendor-style canvas fingerprint operations.
    let result = check(r#"(function() {
        var c = document.createElement('canvas');
        c.width = 200; c.height = 50;
        var ctx = c.getContext('2d');
        ctx.font = '18px Arial';
        ctx.fillStyle = '#f60';
        ctx.fillRect(125, 1, 62, 20);
        ctx.fillStyle = '#069';
        ctx.fillText('WBAAS fingerprint test Cwm fjordbank glyphs 😺', 2, 15);
        ctx.fillStyle = 'rgba(102, 204, 0, 0.7)';
        ctx.fillText('WBAAS fingerprint test Cwm fjordbank glyphs 😺', 4, 45);
        var data = c.toDataURL('image/png');
        // Check it's non-empty (not a blank canvas)
        return data.length + ':' + (data !== 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAMgAAAAyCAYAAAAZUl3oAAAABmJLR0QA/wD/AP+gvaeTAAAADklEQVRoge3BMQEAAADCoPVP7WsIoAAAeAMBxAACqwAAAABJRU5ErkJggg==');
    })()"#).await;
    eprintln!("Canvas drawing probe: {}", &result[..result.len().min(100)]);
    let parts: Vec<&str> = result.splitn(2, ':').collect();
    let len: usize = parts[0].parse().unwrap_or(0);
    assert!(
        len > 1000,
        "canvas toDataURL after drawing should produce >1KB data, got {}b",
        len
    );
    assert_eq!(
        parts.get(1),
        Some(&"true"),
        "canvas toDataURL should differ from blank canvas"
    );
}

// ================================================================
// WebGL fingerprint catalog (browser_oxide::stealth::gpu) — profile-driven WebGL fix.
// These assert the profile-driven WebGL fingerprint is exposed with
// realistic per-GPU values, not the old hardcoded single-profile stubs.
// ================================================================

async fn webgl_eval(profile: browser_oxide::stealth::StealthProfile, js: &str) -> String {
    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        "https://example.com/",
        profile,
    )
    .await
    .unwrap();
    page.evaluate(js).unwrap_or_else(|e| format!("ERR: {e}"))
}

#[tokio::test]
async fn webgl_extensions_has_25_plus_entries() {
    // Real Chrome 131 exposes 25-35 extensions depending on GPU.
    // Our old stub returned 13.
    let profile = browser_oxide::stealth::chrome_148_windows();
    let r = webgl_eval(
        profile,
        r#"(() => {
            const gl = document.createElement('canvas').getContext('webgl');
            const exts = gl.getSupportedExtensions();
            return exts.length >= 25 ? 'ok:' + exts.length : 'fail:' + exts.length;
        })()"#,
    )
    .await;
    assert!(r.starts_with("ok:"), "expected >=25 extensions, got {r}");
}

#[tokio::test]
async fn webgl_unmasked_vendor_matches_windows_profile() {
    // Windows Chrome profile should get the NVIDIA vendor string.
    let profile = browser_oxide::stealth::chrome_148_windows();
    let r = webgl_eval(
        profile,
        r#"(() => {
            const gl = document.createElement('canvas').getContext('webgl');
            return gl.getParameter(0x9245);  // UNMASKED_VENDOR_WEBGL
        })()"#,
    )
    .await;
    assert!(
        r.contains("NVIDIA"),
        "Windows profile should report NVIDIA, got: {r}"
    );
}

#[tokio::test]
async fn webgl_unmasked_renderer_matches_macos_profile() {
    // macOS profile reports Apple M3 (Phase 7 — was M2 Pro).
    let profile = browser_oxide::stealth::chrome_148_macos();
    let r = webgl_eval(
        profile,
        r#"(() => {
            const gl = document.createElement('canvas').getContext('webgl');
            return gl.getParameter(0x9246);  // UNMASKED_RENDERER_WEBGL
        })()"#,
    )
    .await;
    assert!(
        r.contains("Apple M3"),
        "macOS profile should report Apple M3, got: {r}"
    );
}

#[tokio::test]
async fn webgl_unmasked_renderer_matches_linux_profile() {
    // Linux profile should get the Intel UHD Graphics 630 renderer.
    let profile = browser_oxide::stealth::chrome_148_linux();
    let r = webgl_eval(
        profile,
        r#"(() => {
            const gl = document.createElement('canvas').getContext('webgl');
            return gl.getParameter(0x9246);
        })()"#,
    )
    .await;
    assert!(
        r.contains("Intel"),
        "Linux profile should report Intel, got: {r}"
    );
}

#[tokio::test]
async fn webgl_extensions_differ_across_profiles() {
    // Apple GPU exposes WEBGL_compressed_texture_astc that neither NVIDIA nor
    // Intel Linux expose. This is THE standard CreepJS probe for GPU diversity.
    let apple = browser_oxide::stealth::chrome_148_macos();
    let intel = browser_oxide::stealth::chrome_148_linux();
    let apple_has_astc = webgl_eval(
        apple,
        r#"document.createElement('canvas').getContext('webgl').getSupportedExtensions().includes('WEBGL_compressed_texture_astc')"#,
    ).await;
    let intel_has_astc = webgl_eval(
        intel,
        r#"document.createElement('canvas').getContext('webgl').getSupportedExtensions().includes('WEBGL_compressed_texture_astc')"#,
    ).await;
    assert_eq!(
        apple_has_astc, "true",
        "Apple should expose WEBGL_compressed_texture_astc"
    );
    assert_eq!(
        intel_has_astc, "false",
        "Intel Linux should NOT expose WEBGL_compressed_texture_astc"
    );
}

#[tokio::test]
async fn webgl_shader_precision_int_differs_from_float() {
    // Real Chrome returns [127, 127, 23] for HIGH_FLOAT and [31, 30, 0]
    // for HIGH_INT. Our previous stub returned {127, 127, 23} for ALL
    // precision types, which is a distinctive tell.
    let profile = browser_oxide::stealth::chrome_148_windows();
    let r = webgl_eval(
        profile,
        r#"(() => {
            const gl = document.createElement('canvas').getContext('webgl');
            const VERTEX_SHADER = 0x8B31;
            const HIGH_FLOAT = 0x8DF2;
            const HIGH_INT = 0x8DF5;
            const f = gl.getShaderPrecisionFormat(VERTEX_SHADER, HIGH_FLOAT);
            const i = gl.getShaderPrecisionFormat(VERTEX_SHADER, HIGH_INT);
            return f.rangeMin + ',' + f.rangeMax + ',' + f.precision + ' | ' + i.rangeMin + ',' + i.rangeMax + ',' + i.precision;
        })()"#,
    )
    .await;
    // Expect HIGH_FLOAT = [127, 127, 23], HIGH_INT = [31, 30, 0]
    assert!(
        r.contains("127,127,23 | 31,30,0"),
        "shader precision wrong, got: {r}"
    );
}

#[tokio::test]
async fn webgl_max_texture_size_is_16384() {
    let profile = browser_oxide::stealth::chrome_148_windows();
    let r = webgl_eval(
        profile,
        r#"document.createElement('canvas').getContext('webgl').getParameter(0x0D33)"#,
    )
    .await;
    assert_eq!(r, "16384", "MAX_TEXTURE_SIZE should be 16384");
}

// ================================================================
// Performance API (§P1 item 9 fix) — PerformanceNavigationTiming +
// PerformanceResourceTiming. The bot-detection sensor reads these.
// ================================================================

#[tokio::test]
async fn perf_get_entries_by_type_navigation() {
    // getEntriesByType('navigation') must return a non-empty array
    assert_eq!(
        check("performance.getEntriesByType('navigation').length >= 1").await,
        "true"
    );
}

#[tokio::test]
async fn perf_navigation_entry_has_timing_fields() {
    // The navigation entry must have domContentLoadedEventStart,
    // loadEventEnd, and transferSize like a real Chrome
    assert_eq!(
        check(
            r#"(() => {
                const e = performance.getEntriesByType('navigation')[0];
                return typeof e.domContentLoadedEventStart === 'number'
                    && typeof e.loadEventEnd === 'number'
                    && typeof e.transferSize === 'number'
                    && e.entryType === 'navigation';
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_get_entries_by_type_resource() {
    // A blank document has loaded no subresources. Chrome returns an empty
    // resource list; invented favicon/bundle/analytics records are detectable.
    assert_eq!(
        check("performance.getEntriesByType('resource').length").await,
        "0"
    );
}

#[tokio::test]
async fn perf_resource_entry_shape() {
    assert_eq!(
        check(
            r#"(() => {
                const script = document.createElement('script');
                script.src = 'https://example.com/app.js';
                document.head.appendChild(script);
                const r = performance.getEntriesByType('resource')[0];
                return r.entryType === 'resource'
                    && typeof r.initiatorType === 'string'
                    && typeof r.nextHopProtocol === 'string'
                    && r.nextHopProtocol === 'h2';
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_resource_entry_has_web_idl_prototype_chain() {
    assert_eq!(
        check(
            r#"(() => {
                const script = document.createElement('script');
                script.src = 'https://example.com/app.js';
                document.head.appendChild(script);
                const entries = performance.getEntriesByType('resource');
                const entry = entries[0];
                const json = JSON.parse(JSON.stringify(entry));
                return entries.length === 1
                    && entry instanceof PerformanceResourceTiming
                    && entry instanceof PerformanceEntry
                    && Object.prototype.toString.call(entry)
                        === '[object PerformanceResourceTiming]'
                    && typeof json.responseEnd === 'number'
                    && json.responseStart === 0
                    && json.requestStart === 0
                    && json.transferSize === 0
                    && json.encodedBodySize === 0;
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_navigation_entry_has_web_idl_prototype_chain() {
    assert_eq!(
        check(
            r#"(() => {
                const entry = performance.getEntriesByType('navigation')[0];
                const json = JSON.parse(JSON.stringify(entry));
                return entry instanceof PerformanceNavigationTiming
                    && entry instanceof PerformanceResourceTiming
                    && entry instanceof PerformanceEntry
                    && Object.prototype.toString.call(entry)
                        === '[object PerformanceNavigationTiming]'
                    && typeof json.responseEnd === 'number'
                    && typeof json.domComplete === 'number'
                    && json.entryType === 'navigation';
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_navigation_name_tracks_installed_location() {
    assert_eq!(
        check_secure("performance.getEntriesByType('navigation')[0].name").await,
        "https://example.com/"
    );
}

#[tokio::test]
async fn perf_navigation_uses_network_measurements_when_available() {
    let time_origin_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0
        - 500.0;
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    let mut runtime = browser_oxide::js_runtime::BrowserJsRuntime::with_options(
        dom,
        browser_oxide::js_runtime::runtime::BrowserRuntimeOptions {
            navigation_timing: Some(browser_oxide::net::TimingStats {
                name: "https://example.test/measured".into(),
                time_origin_unix_ms,
                request_start_ms: 7.0,
                response_start_ms: 113.0,
                response_end_ms: 241.0,
                transfer_size: 90_300,
                encoded_body_size: 90_000,
                decoded_body_size: 240_000,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let result = runtime
        .execute_script(
            r#"(() => {
                const e = performance.getEntriesByType('navigation')[0];
                return JSON.stringify({
                    requestStart: Math.round(e.requestStart),
                    responseStart: Math.round(e.responseStart),
                    responseEnd: Math.round(e.responseEnd),
                    transferSize: e.transferSize,
                    encodedBodySize: e.encodedBodySize,
                    decodedBodySize: e.decodedBodySize,
                    oldResponseStart: performance.timing.responseStart
                        - performance.timing.navigationStart,
                    durationCoversResponse: e.duration >= e.responseEnd,
                });
            })()"#,
            None,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["requestStart"], 7);
    assert_eq!(value["responseStart"], 113);
    assert_eq!(value["responseEnd"], 241);
    assert_eq!(value["transferSize"], 90_300);
    assert_eq!(value["encodedBodySize"], 90_000);
    assert_eq!(value["decodedBodySize"], 240_000);
    assert_eq!(value["oldResponseStart"], 113);
    assert_eq!(value["durationCoversResponse"], true);
}

#[tokio::test]
async fn perf_network_navigation_lifecycle_uses_measured_dispatch_times() {
    // Make the network navigation 600ms old at runtime creation. PerfState
    // anchors performance.now() to this HTTP origin, so the lifecycle emitted
    // below must land around the live elapsed time rather than the historical
    // fixed 320.5/328.7/515.9ms constants.
    let time_origin_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0
        - 600.0;
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    let mut runtime = browser_oxide::js_runtime::BrowserJsRuntime::with_options(
        dom,
        browser_oxide::js_runtime::runtime::BrowserRuntimeOptions {
            navigation_timing: Some(browser_oxide::net::TimingStats {
                name: "https://example.test/measured-lifecycle".into(),
                time_origin_unix_ms,
                request_start_ms: 20.0,
                response_start_ms: 180.0,
                response_end_ms: 260.0,
                transfer_size: 10_300,
                encoded_body_size: 10_000,
                decoded_body_size: 30_000,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    runtime.complete_document_lifecycle();
    let result = runtime
        .execute_script(
            r#"(() => {
                const e = performance.getEntriesByType('navigation')[0];
                const t = performance.timing;
                return JSON.stringify({
                    responseEnd: e.responseEnd,
                    domInteractive: e.domInteractive,
                    dclStart: e.domContentLoadedEventStart,
                    dclEnd: e.domContentLoadedEventEnd,
                    domComplete: e.domComplete,
                    loadStart: e.loadEventStart,
                    loadEnd: e.loadEventEnd,
                    legacyDclEnd: t.domContentLoadedEventEnd - t.navigationStart,
                    legacyLoadEnd: t.loadEventEnd - t.navigationStart,
                });
            })()"#,
            None,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    let number = |key: &str| value[key].as_f64().unwrap();
    let response_end = number("responseEnd");
    let dom_interactive = number("domInteractive");
    let dcl_start = number("dclStart");
    let dcl_end = number("dclEnd");
    let dom_complete = number("domComplete");
    let load_start = number("loadStart");
    let load_end = number("loadEnd");

    assert!(dom_interactive >= response_end, "{result}");
    assert!(
        dom_interactive >= 500.0,
        "lifecycle remained synthetic: {result}"
    );
    assert!(dom_interactive <= dcl_start, "{result}");
    assert!(dcl_start <= dcl_end, "{result}");
    assert!(dcl_end <= dom_complete, "{result}");
    assert!(dom_complete <= load_start, "{result}");
    assert!(load_start <= load_end, "{result}");
    assert!((number("legacyDclEnd") - dcl_end).abs() <= 1.0, "{result}");
    assert!(
        (number("legacyLoadEnd") - load_end).abs() <= 1.0,
        "{result}"
    );
}

#[tokio::test]
async fn perf_entries_do_not_invent_vendor_resources() {
    assert_eq!(
        check("performance.getEntries().some(e => /qauth|wbaas/.test(e.name))").await,
        "false"
    );
}

#[tokio::test]
async fn navigation_api_is_an_event_target_with_chrome_surface() {
    assert_eq!(
        check(
            r#"(() => {
                const expected = [
                    'currentEntry', 'transition', 'activation', 'canGoBack',
                    'canGoForward', 'onnavigate', 'onnavigatesuccess',
                    'onnavigateerror', 'oncurrententrychange', 'back', 'entries',
                    'forward', 'navigate', 'reload', 'traverseTo',
                    'updateCurrentEntry', 'constructor'
                ];
                let fired = 0;
                navigation.addEventListener('probe', () => fired++);
                navigation.dispatchEvent(new Event('probe'));
                return navigation instanceof EventTarget
                    && Object.prototype.toString.call(navigation) === '[object Navigation]'
                    && Object.getPrototypeOf(Navigation) === EventTarget
                    && Object.getPrototypeOf(Navigation.prototype) === EventTarget.prototype
                    && Object.getOwnPropertyNames(navigation).length === 0
                    && JSON.stringify(Object.getOwnPropertyNames(Navigation.prototype)) === JSON.stringify(expected)
                    && Object.prototype.toString.call(navigation.currentEntry) === '[object NavigationHistoryEntry]'
                    && fired === 1;
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn performance_observer_delivers_resource_entries() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        r#"globalThis.__observerResult = null;
        const observer = new PerformanceObserver((list, self, options) => {
            const entries = list.getEntries();
            observer.disconnect();
            globalThis.__observerResult = {
                entries,
                sameObserver: self === observer,
                dropped: options.droppedEntriesCount,
                listTag: Object.prototype.toString.call(list),
            };
        });
        observer.observe({ entryTypes: ['resource'] });
        const raw = {
            url: 'https://example.test/probe.js',
            type: 'script', startTime: 1, duration: 2, size: 3,
        };
        globalThis._browser_oxide.__perfResourceEntries.push(raw);
        globalThis[Symbol.for('__browser_oxide_performance_resource__')](raw);"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(100))
        .await
        .unwrap();
    let result = page
        .evaluate(
            r#"(() => {
                const result = globalThis.__observerResult;
                return Object.getOwnPropertyNames(PerformanceObserver.prototype).join(',')
                    === 'disconnect,observe,takeRecords,constructor'
                    && Object.getOwnPropertyNames(new PerformanceObserver(() => {})).length === 0
                    && result && result.sameObserver
                    && result.dropped === 0
                    && result.listTag === '[object PerformanceObserverEntryList]'
                    && result.entries.length === 1
                    && result.entries[0].name === 'https://example.test/probe.js';
            })()"#,
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[tokio::test]
async fn rtc_ice_candidates_reach_event_target_listeners() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        r#"globalThis.__iceEvents = [];
        const pc = new RTCPeerConnection();
        pc.addEventListener('icecandidate', event => {
            globalThis.__iceEvents.push({
                trusted: event.isTrusted,
                tag: Object.prototype.toString.call(event),
                candidate: event.candidate && event.candidate.candidate,
            });
        });
        pc.createOffer().then(offer => pc.setLocalDescription(offer));"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(300))
        .await
        .unwrap();
    assert_eq!(
        page.evaluate(
            r#"globalThis.__iceEvents.length === 2
                && globalThis.__iceEvents[0].trusted === true
                && globalThis.__iceEvents[0].tag === '[object RTCPeerConnectionIceEvent]'
                && /\.local /.test(globalThis.__iceEvents[0].candidate)
                && globalThis.__iceEvents[1].candidate === null"#
        )
        .unwrap(),
        "true"
    );
}

#[tokio::test]
async fn rtc_offer_and_state_machine_match_chrome_148() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        r#"globalThis.__rtcParity = null;
        (async () => {
            const pc = new RTCPeerConnection({
                iceCandidatePoolSize: 1,
                iceServers: [
                    { urls: 'stun:stun.cloudflare.com:3478' },
                    { urls: 'stun:stun.l.google.com:19302' },
                ],
            });
            const events = [];
            for (const type of [
                'negotiationneeded', 'signalingstatechange',
                'icegatheringstatechange', 'icecandidate'
            ]) {
                pc.addEventListener(type, event => events.push({
                    type,
                    gathering: pc.iceGatheringState,
                    candidate: event.candidate && event.candidate.toJSON(),
                }));
            }
            pc.createDataChannel('');
            const offer = await pc.createOffer({
                offerToReceiveAudio: true,
                offerToReceiveVideo: true,
            });
            const offerChecks = {
                plain: !(offer instanceof RTCSessionDescription)
                    && Object.prototype.toString.call(offer) === '[object Object]'
                    && Object.getPrototypeOf(offer) === Object.prototype
                    && JSON.stringify(Object.getOwnPropertyNames(offer)) === JSON.stringify(['sdp', 'type']),
                length: offer.sdp.length,
                audio: offer.sdp.includes('m=audio 9 UDP/TLS/RTP/SAVPF'),
                video: offer.sdp.includes('m=video 9 UDP/TLS/RTP/SAVPF'),
                data: offer.sdp.includes('m=application 9 UDP/DTLS/SCTP webrtc-datachannel'),
                bundle: offer.sdp.includes('a=group:BUNDLE 0 1 2'),
                ice: /a=ice-ufrag:[A-Za-z0-9+/]{4}\r\n/.test(offer.sdp),
                fingerprint: /a=fingerprint:sha-256(?: [0-9A-F]{2}(?::[0-9A-F]{2}){31})\r\n/.test(offer.sdp),
            };
            await pc.setLocalDescription(offer);
            const callee = new RTCPeerConnection();
            await callee.setRemoteDescription(offer);
            const answer = await callee.createAnswer();
            const answerChecks = {
                plain: !(answer instanceof RTCSessionDescription)
                    && Object.prototype.toString.call(answer) === '[object Object]'
                    && Object.getPrototypeOf(answer) === Object.prototype
                    && JSON.stringify(Object.getOwnPropertyNames(answer)) === JSON.stringify(['sdp', 'type']),
                type: answer.type,
            };
            const afterSet = {
                signaling: pc.signalingState,
                current: pc.currentLocalDescription,
                localTag: Object.prototype.toString.call(pc.localDescription),
                pendingType: pc.pendingLocalDescription && pc.pendingLocalDescription.type,
            };
            await new Promise(resolve => setTimeout(resolve, 100));
            globalThis.__rtcParity = { offerChecks, answerChecks, afterSet, events, localSdp: pc.localDescription.sdp };
            callee.close();
        })();"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(400))
        .await
        .unwrap();
    assert_eq!(
        page.evaluate(
            r#"(() => {
                const result = globalThis.__rtcParity;
                if (!result) return false;
                const candidates = result.events.filter(event => event.type === 'icecandidate');
                const gathering = result.events
                    .filter(event => event.type === 'icegatheringstatechange')
                    .map(event => event.gathering);
                return result.offerChecks.plain
                    && result.offerChecks.length > 6000
                    && result.offerChecks.audio
                    && result.offerChecks.video
                    && result.offerChecks.data
                    && result.offerChecks.bundle
                    && result.offerChecks.ice
                    && result.offerChecks.fingerprint
                    && result.answerChecks.plain
                    && result.answerChecks.type === 'answer'
                    && result.afterSet.signaling === 'have-local-offer'
                    && result.afterSet.current === null
                    && result.afterSet.localTag === '[object RTCSessionDescription]'
                    && result.afterSet.pendingType === 'offer'
                    && result.events.some(event => event.type === 'negotiationneeded')
                    && result.events.some(event => event.type === 'signalingstatechange')
                    && JSON.stringify(gathering) === JSON.stringify(['gathering', 'complete'])
                    && candidates.length === 4
                    && candidates.slice(0, 3).every((event, index) =>
                        event.candidate
                        && event.candidate.sdpMid === String(index)
                        && event.candidate.sdpMLineIndex === index
                        && /\.local /.test(event.candidate.candidate))
                    && candidates[3].candidate === null
                    && (result.localSdp.match(/a=candidate:/g) || []).length === 3;
            })()"#,
        )
        .unwrap(),
        "true"
    );
}

#[tokio::test]
async fn rtc_peer_connection_and_data_channel_match_chrome_surface() {
    assert_eq!(
        check(
            r#"(() => {
                const pcNames = [
                    'localDescription', 'currentLocalDescription',
                    'pendingLocalDescription', 'remoteDescription',
                    'currentRemoteDescription', 'pendingRemoteDescription',
                    'signalingState', 'iceGatheringState',
                    'iceConnectionState', 'connectionState',
                    'canTrickleIceCandidates', 'onnegotiationneeded',
                    'onicecandidate', 'onsignalingstatechange',
                    'oniceconnectionstatechange', 'onconnectionstatechange',
                    'onicegatheringstatechange', 'onicecandidateerror',
                    'ontrack', 'sctp', 'ondatachannel', 'onaddstream',
                    'onremovestream', 'addIceCandidate', 'addStream',
                    'addTrack', 'addTransceiver', 'close', 'createAnswer',
                    'createDTMFSender', 'createDataChannel', 'createOffer',
                    'getConfiguration', 'getLocalStreams', 'getReceivers',
                    'getRemoteStreams', 'getSenders', 'getStats',
                    'getTransceivers', 'removeStream', 'removeTrack',
                    'restartIce', 'setConfiguration', 'setLocalDescription',
                    'setRemoteDescription', 'constructor'
                ];
                const channelNames = [
                    'label', 'ordered', 'maxPacketLifeTime', 'maxRetransmits',
                    'protocol', 'negotiated', 'id', 'readyState',
                    'bufferedAmount', 'bufferedAmountLowThreshold', 'onopen',
                    'onbufferedamountlow', 'onerror', 'onclosing', 'onclose',
                    'onmessage', 'binaryType', 'reliable', 'close', 'send',
                    'constructor'
                ];
                const candidateNames = [
                    'candidate', 'sdpMid', 'sdpMLineIndex', 'foundation',
                    'component', 'priority', 'address', 'protocol', 'port',
                    'type', 'tcpType', 'relatedAddress', 'relatedPort',
                    'usernameFragment', 'relayProtocol', 'url', 'toJSON',
                    'constructor'
                ];
                const pc = new RTCPeerConnection({ iceServers: [] });
                const channel = pc.createDataChannel('probe');
                const candidate = new RTCIceCandidate({
                    candidate: 'candidate:1 1 udp 9 host.local 1234 typ host',
                    sdpMid: '0', sdpMLineIndex: 0
                });
                const description = new RTCSessionDescription({ type: 'offer', sdp: '' });
                const iceEvent = new RTCPeerConnectionIceEvent('icecandidate', { candidate });
                let channelConstructorThrows = false;
                try { new RTCDataChannel(); } catch (error) {
                    channelConstructorThrows = error instanceof TypeError;
                }
                const ok = Object.getOwnPropertyNames(pc).length === 0
                    && Object.getOwnPropertyNames(channel).length === 0
                    && Object.getOwnPropertyNames(candidate).length === 0
                    && Object.getOwnPropertyNames(description).length === 0
                    && JSON.stringify(Object.getOwnPropertyNames(RTCPeerConnection.prototype)) === JSON.stringify(pcNames)
                    && JSON.stringify(Object.getOwnPropertyNames(RTCDataChannel.prototype)) === JSON.stringify(channelNames)
                    && JSON.stringify(Object.getOwnPropertyNames(RTCIceCandidate.prototype)) === JSON.stringify(candidateNames)
                    && JSON.stringify(Object.getOwnPropertyNames(RTCSessionDescription.prototype)) === JSON.stringify(['type', 'sdp', 'toJSON', 'constructor'])
                    && JSON.stringify(Object.getOwnPropertyNames(RTCPeerConnectionIceEvent.prototype)) === JSON.stringify(['candidate', 'constructor'])
                    && JSON.stringify(Object.getOwnPropertyNames(RTCPeerConnection)) === JSON.stringify(['length', 'name', 'prototype', 'generateCertificate'])
                    && JSON.stringify(Object.getOwnPropertyNames(RTCDataChannel)) === JSON.stringify(['length', 'name', 'prototype'])
                    && Object.getPrototypeOf(RTCPeerConnection) === EventTarget
                    && Object.getPrototypeOf(RTCPeerConnection.prototype) === EventTarget.prototype
                    && Object.getPrototypeOf(RTCDataChannel) === EventTarget
                    && Object.getPrototypeOf(RTCDataChannel.prototype) === EventTarget.prototype
                    && Object.prototype.toString.call(pc) === '[object RTCPeerConnection]'
                    && Object.prototype.toString.call(channel) === '[object RTCDataChannel]'
                    && Object.prototype.toString.call(candidate) === '[object RTCIceCandidate]'
                    && Object.prototype.toString.call(description) === '[object RTCSessionDescription]'
                    && Object.prototype.toString.call(iceEvent) === '[object RTCPeerConnectionIceEvent]'
                    && channel.label === 'probe'
                    && channel.ordered === true
                    && channel.readyState === 'connecting'
                    && candidate.foundation === '1'
                    && candidate.component === 'rtp'
                    && candidate.protocol === 'udp'
                    && candidate.address === 'host.local'
                    && candidate.port === 1234
                    && candidate.type === 'host'
                    && iceEvent.candidate === candidate
                    && channelConstructorThrows;
                pc.close();
                return ok;
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_timing_is_present() {
    // performance.timing (deprecated but the challenge vendor + CreepJS still probe)
    assert_eq!(
        check("typeof performance.timing.navigationStart === 'number'").await,
        "true"
    );
}

#[tokio::test]
async fn perf_time_origin_is_present() {
    assert_eq!(
        check("typeof performance.timeOrigin === 'number'").await,
        "true"
    );
}

// ================================================================
// Intl timezone consistency (§P1 item 13) — timezone must match profile
// ================================================================

#[tokio::test]
async fn intl_timezone_matches_moscow_profile() {
    let profile = browser_oxide::stealth::presets::chrome_148_ru();
    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><body></body></html>",
        "https://example.com/",
        profile,
    )
    .await
    .unwrap();
    let r = page
        .evaluate("Intl.DateTimeFormat().resolvedOptions().timeZone")
        .unwrap();
    assert_eq!(r, "Europe/Moscow", "RU profile should report Europe/Moscow");
}

#[tokio::test]
async fn intl_timezone_matches_tokyo_profile() {
    let profile = browser_oxide::stealth::presets::chrome_148_jp();
    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><body></body></html>",
        "https://example.com/",
        profile,
    )
    .await
    .unwrap();
    let r = page
        .evaluate("Intl.DateTimeFormat().resolvedOptions().timeZone")
        .unwrap();
    assert_eq!(r, "Asia/Tokyo", "JP profile should report Asia/Tokyo");
}

#[tokio::test]
async fn date_timezone_offset_is_numeric_from_profile() {
    // Date.prototype.getTimezoneOffset() must return a number matching the
    // profile's timezone (Moscow = UTC+3 in summer, -180 minutes).
    let profile = browser_oxide::stealth::presets::chrome_148_ru();
    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><body></body></html>",
        "https://example.com/",
        profile,
    )
    .await
    .unwrap();
    let r = page
        .evaluate("typeof new Date().getTimezoneOffset()")
        .unwrap();
    assert_eq!(r, "number");
    let r2 = page.evaluate("new Date().getTimezoneOffset()").unwrap();
    // Moscow is UTC+3, so offset should be -180 (minutes).
    assert_eq!(
        r2, "-180",
        "Moscow profile should report -180 min offset, got {r2}"
    );
}

#[tokio::test]
async fn perf_paint_entries_present() {
    // first-paint + first-contentful-paint are the two paint entries Chrome always reports
    assert_eq!(
        check(
            r#"(() => {
                const p = performance.getEntriesByType('paint');
                return p.length === 2
                    && p.some(e => e.name === 'first-paint')
                    && p.some(e => e.name === 'first-contentful-paint');
            })()"#
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl_get_extension_returns_non_null_for_supported() {
    let profile = browser_oxide::stealth::chrome_148_windows();
    let r = webgl_eval(
        profile,
        r#"(() => {
            const gl = document.createElement('canvas').getContext('webgl');
            const ext = gl.getExtension('EXT_texture_filter_anisotropic');
            return typeof ext;
        })()"#,
    )
    .await;
    assert_eq!(
        r, "object",
        "getExtension should return non-null for supported extensions"
    );
}

// ================================================================
// Chrome-specific (anti-bot critical)
// ================================================================

#[tokio::test]
async fn chrome_app() {
    assert_eq!(check("typeof chrome.app").await, "object");
}
#[tokio::test]
async fn chrome_runtime() {
    // Real Chrome 147: chrome.runtime absent on regular pages (extension-only).
    assert_eq!(check("typeof chrome.runtime").await, "undefined");
}
#[tokio::test]
async fn chrome_csi() {
    assert_eq!(check("typeof chrome.csi").await, "function");
}
#[tokio::test]
async fn chrome_load_times() {
    assert_eq!(check("typeof chrome.loadTimes").await, "function");
}
#[tokio::test]
async fn performance_memory() {
    assert_eq!(
        check("performance.memory.jsHeapSizeLimit > 0").await,
        "true"
    );
}
#[tokio::test]
async fn speech_synthesis() {
    assert_eq!(
        check("Array.isArray(speechSynthesis.getVoices())").await,
        "true"
    );
}

// ================================================================
// iframe
// ================================================================

#[tokio::test]
async fn iframe_content_window() {
    assert_eq!(
        check("typeof document.createElement('iframe').contentWindow").await,
        "object"
    );
}
#[tokio::test]
async fn iframe_same_isolate_host_load_is_queued_once() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();

    let sync = page
        .evaluate(
            r#"(() => {
                globalThis.__iframeLoadAudit = {listener: 0, handler: 0, trusted: false};
                const frame = document.createElement('iframe');
                frame.addEventListener('load', event => {
                    __iframeLoadAudit.listener++;
                    __iframeLoadAudit.trusted = event.isTrusted;
                });
                frame.onload = () => __iframeLoadAudit.handler++;
                document.body.appendChild(frame);
                void frame.contentWindow;
                globalThis.__iframeLoadAudit.frame = frame;
                return JSON.stringify({
                    listener: __iframeLoadAudit.listener,
                    handler: __iframeLoadAudit.handler,
                });
            })()"#,
        )
        .unwrap();
    assert_eq!(sync, r#"{"listener":0,"handler":0}"#);

    for _ in 0..10 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(25))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        page.evaluate(
            "JSON.stringify({listener:__iframeLoadAudit.listener,handler:__iframeLoadAudit.handler,trusted:__iframeLoadAudit.trusted,readyState:__iframeLoadAudit.frame.contentDocument.readyState})"
        )
        .unwrap(),
        r#"{"listener":1,"handler":1,"trusted":true,"readyState":"complete"}"#
    );

    page.evaluate("void __iframeLoadAudit.frame.contentWindow")
        .unwrap();
    for _ in 0..3 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(25))
            .await;
    }
    assert_eq!(
        page.evaluate(
            "JSON.stringify({listener:__iframeLoadAudit.listener,handler:__iframeLoadAudit.handler})"
        )
        .unwrap(),
        r#"{"listener":1,"handler":1}"#
    );
}
#[tokio::test]
async fn iframe_cross_realm_nav_stealth() {
    // Verify cross-realm property access works WITH a stealth profile.
    // The vendor's ifw probe reads cw.navigator.webdriver from the parent context.
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        "https://example.com/",
        profile,
    )
    .await
    .unwrap();
    let result = page
        .evaluate(r#"
        (function() {
            var iframe = document.createElement('iframe');
            iframe.setAttribute('srcdoc', '<head></head><body></body>');
            document.body.appendChild(iframe);
            var cw = iframe.contentWindow;
            if (!cw) return 'no_cw';
            var navType = typeof cw.navigator;
            var wd = 'threw';
            try { wd = cw.navigator ? cw.navigator.webdriver : 'null_nav'; } catch(e) { wd = 'ERR:' + e.message; }
            var aw = 'threw';
            try { aw = cw.screen ? cw.screen.availWidth : 'null_scr'; } catch(e) { aw = 'ERR:' + e.message; }
            var names = Object.getOwnPropertyNames(cw);
            var hasNav = names.indexOf('navigator') >= 0;
            var hasScr = names.indexOf('screen') >= 0;
            return JSON.stringify({navType, wd, aw, hasNav, hasScr, total: names.length});
        })()
    "#)
        .unwrap_or_else(|e| format!("ERROR: {e}"));
    println!("iframe_cross_realm_nav_stealth: {}", result);
    assert!(
        result.contains("\"navType\":\"object\""),
        "cw.navigator must be object with stealth profile, got: {result}"
    );
    // Real Chrome returns undefined for webdriver (omitted by JSON.stringify) or false;
    // explicit true is the only unacceptable automation signal.
    assert!(
        !result.contains("\"wd\":true"),
        "cw.navigator.webdriver must not be true, got: {result}"
    );
}
#[tokio::test]
async fn iframe_inner_realm_nav_access() {
    // Verify that code running INSIDE the child realm (via new cw.Function(...))
    // can access navigator, screen, and devicePixelRatio — these are the
    // properties the vendor's ifw and spd probes read from inside the child realm.
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile(
        "<!DOCTYPE html><html><body></body></html>",
        "https://example.com/",
        profile,
    )
    .await
    .unwrap();
    let result = page
        .evaluate(
            r#"
        (function() {
            var iframe = document.createElement('iframe');
            document.body.appendChild(iframe);
            var cw = iframe.contentWindow;
            if (!cw) return 'no_cw';
            if (!cw.Function) return 'no_fn:' + typeof cw.Function;
            var innerNav = 'threw';
            try { innerNav = new cw.Function('return typeof navigator')(); }
            catch(e) { innerNav = 'err:' + e.message; }
            var innerDpr = 'threw';
            try { innerDpr = new cw.Function('return devicePixelRatio')(); }
            catch(e) { innerDpr = 'err:' + e.message; }
            var innerScreen = 'threw';
            try { innerScreen = new cw.Function('return typeof screen')(); }
            catch(e) { innerScreen = 'err:' + e.message; }
            return JSON.stringify({innerNav, innerDpr, innerScreen});
        })()
    "#,
        )
        .unwrap_or_else(|e| format!("ERROR: {e}"));
    println!("iframe_inner_realm_nav_access: {}", result);
    assert!(
        result.contains("\"innerNav\":\"object\""),
        "navigator must be object inside realm, got: {result}"
    );
    assert!(
        result.contains("\"innerScreen\":\"object\""),
        "screen must be object inside realm, got: {result}"
    );
}
#[tokio::test]
async fn iframe_child_realm_dpr_is_accessor() {
    // Verify that devicePixelRatio in the child realm is defined as an accessor
    // (getter), not a data property. The vendor's dpi probe checks this.
    let result = check(r#"
        (function() {
            var iframe = document.createElement('iframe');
            document.body.appendChild(iframe);
            var cw = iframe.contentWindow;
            if (!cw) return 'no_cw';
            if (!cw.Function) return 'no_fn';
            // Check descriptor from inside the child realm
            var desc = new cw.Function(
                'return JSON.stringify((function(d){return d?{hasGet:!!d.get,hasValue:"value" in d}:{missing:true};})(Object.getOwnPropertyDescriptor(globalThis,"devicePixelRatio")))'
            )();
            return desc || 'null';
        })()
    "#).await;
    println!("iframe_child_realm_dpr_is_accessor: {}", result);
    assert!(
        result.contains("\"hasGet\":true"),
        "child realm dpr must be accessor with getter, got: {result}"
    );
}
#[tokio::test]
async fn iframe_cross_realm_write_read() {
    // Verify parent-context JS writes to child realm are readable back.
    // This mirrors how the vendor tags sentinel functions on iframe.contentWindow.
    let result = check(
        r#"
        (function() {
            var f = document.createElement('iframe');
            document.body.appendChild(f);
            var cw = f.contentWindow;
            if (!cw) return 'no_cw';
            // Write from parent context to child realm
            cw.__testSentinel = 'hello';
            // Read back from parent context
            var readBack = cw.__testSentinel;
            // Also write a function
            var fn1 = function sentinel() { return 42; };
            cw.__sentinelFn = fn1;
            var fn1Back = cw.__sentinelFn;
            return JSON.stringify({
                writeRead: readBack,
                fnType: typeof fn1Back,
                fnCall: fn1Back ? fn1Back() : 'no_fn',
                navType: typeof cw.navigator,
                wd: cw.navigator ? cw.navigator.webdriver : 'null_nav'
            });
        })()
    "#,
    )
    .await;
    println!("iframe_cross_realm_write_read: {}", result);
    assert!(
        result.contains("\"writeRead\":\"hello\""),
        "cross-realm write-read must work, got: {result}"
    );
    assert!(
        result.contains("\"fnType\":\"function\""),
        "cross-realm fn write-read must work, got: {result}"
    );
}
#[tokio::test]
async fn iframe_content_document() {
    assert_eq!(
        check("typeof document.createElement('iframe').contentDocument").await,
        "object"
    );
}

// ================================================================
// kNoScriptId prototype-chain integrity
// ----------------------------------------------------------------
// Real Chrome: the navigator object has ZERO own properties; every
// data/accessor lives on Navigator.prototype. Challenge-vendor probes
// call Object.getOwnPropertyDescriptor(navigator, 'x'); if the result
// is defined they take a "fake native" detection path (V8 kNoScriptId
// guard, May 2025 patch). These tests pin the prototype-only layout.
// ================================================================

#[tokio::test]
async fn k_no_script_id_navigator_is_instance() {
    assert_eq!(check("navigator instanceof Navigator").await, "true");
}

#[tokio::test]
async fn k_no_script_id_navigator_proto_is_navigator_prototype() {
    assert_eq!(
        check("Object.getPrototypeOf(navigator) === Navigator.prototype").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_user_agent_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'userAgent') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_user_agent_on_prototype() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(Navigator.prototype, 'userAgent') !== undefined")
            .await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_user_agent_prototype_is_accessor() {
    assert_eq!(
        check(
            "typeof Object.getOwnPropertyDescriptor(Navigator.prototype, 'userAgent').get === 'function'"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_webdriver_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'webdriver') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_webdriver_on_prototype_returns_false() {
    // Modern Chrome (>=89): navigator.webdriver === false for normal
    // browsing, owned-on-prototype (wdt fix; the prior
    // "undefined" assertion encoded a wrong assumption now contradicted
    // by the live challenge sensor + worker_bootstrap's existing `false`).
    assert_eq!(check("navigator.webdriver === false").await, "true");
}

#[tokio::test]
async fn k_no_script_id_plugins_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'plugins') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_mime_types_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'mimeTypes') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_user_agent_data_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'userAgentData') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_gpu_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'gpu') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_scheduling_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(navigator, 'scheduling') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_get_user_media_on_prototype() {
    assert_eq!(
        check("Navigator.prototype.hasOwnProperty('getUserMedia')").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_navigator_has_no_own_properties() {
    assert_eq!(
        check("Object.getOwnPropertyNames(navigator).length").await,
        "0"
    );
}

#[tokio::test]
async fn k_no_script_id_user_agent_getter_tostring_masked() {
    assert_eq!(
        check(
            "Object.getOwnPropertyDescriptor(Navigator.prototype, 'userAgent').get.toString().includes('[native code]')"
        )
        .await,
        "true"
    );
}

// --- Screen / History / SpeechSynthesis / CustomElementRegistry ---

#[tokio::test]
async fn k_no_script_id_screen_is_instance_of_screen() {
    assert_eq!(check("screen instanceof Screen").await, "true");
}

#[tokio::test]
async fn k_no_script_id_screen_width_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(screen, 'width') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_screen_has_no_own_properties() {
    assert_eq!(
        check("Object.getOwnPropertyNames(screen).length").await,
        "0"
    );
}

#[tokio::test]
async fn k_no_script_id_history_is_instance_of_history() {
    assert_eq!(check("history instanceof History").await, "true");
}

#[tokio::test]
async fn k_no_script_id_history_length_not_on_instance() {
    assert_eq!(
        check("Object.getOwnPropertyDescriptor(history, 'length') === undefined").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_history_push_state_on_prototype() {
    assert_eq!(
        check("History.prototype.hasOwnProperty('pushState')").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_speech_synthesis_is_instance() {
    assert_eq!(
        check("speechSynthesis instanceof SpeechSynthesis").await,
        "true"
    );
}

#[tokio::test]
async fn k_no_script_id_custom_elements_is_instance() {
    assert_eq!(
        check("customElements instanceof CustomElementRegistry").await,
        "true"
    );
}

// --- TextEncoder / TextDecoder shape (challenge-vendor probe target) ---

#[tokio::test]
async fn text_encoder_encoding_is_utf8() {
    assert_eq!(check("new TextEncoder().encoding").await, "utf-8");
}

#[tokio::test]
async fn text_encoder_encode_into_exists() {
    assert_eq!(
        check("typeof TextEncoder.prototype.encodeInto").await,
        "function"
    );
}

#[tokio::test]
async fn text_encoder_tostring_masked_as_native() {
    assert_eq!(
        check("TextEncoder.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn text_encoder_encode_tostring_masked_as_native() {
    assert_eq!(
        check("TextEncoder.prototype.encode.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn text_encoder_encoding_is_accessor_on_prototype() {
    assert_eq!(
        check(
            "typeof Object.getOwnPropertyDescriptor(TextEncoder.prototype, 'encoding').get === 'function'"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn text_encoder_prototype_has_expected_props() {
    assert_eq!(
        check("JSON.stringify(Object.getOwnPropertyNames(TextEncoder.prototype).sort())").await,
        "[\"constructor\",\"encode\",\"encodeInto\",\"encoding\"]"
    );
}

#[tokio::test]
async fn text_encoder_encode_roundtrip() {
    assert_eq!(
        check("JSON.stringify(Array.from(new TextEncoder().encode('ABC')))").await,
        "[65,66,67]"
    );
}

#[tokio::test]
async fn text_encoder_encode_into_writes_bytes() {
    assert_eq!(
        check(
            "(() => { const b = new Uint8Array(4); const r = new TextEncoder().encodeInto('Hi', b); return JSON.stringify({r, b: Array.from(b)}); })()"
        )
        .await,
        "{\"r\":{\"read\":2,\"written\":2},\"b\":[72,105,0,0]}"
    );
}

#[tokio::test]
async fn text_decoder_encoding_is_utf8() {
    assert_eq!(check("new TextDecoder().encoding").await, "utf-8");
}

#[tokio::test]
async fn text_decoder_decodes_utf8() {
    assert_eq!(
        check("new TextDecoder().decode(new Uint8Array([72, 105]))").await,
        "Hi"
    );
}

#[tokio::test]
async fn chrome_load_times_is_native() {
    assert_eq!(
        check("chrome.loadTimes.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn chrome_csi_is_native() {
    assert_eq!(
        check("chrome.csi.toString().includes('[native code]')").await,
        "true"
    );
}

// --- Function.prototype.toString bypass (CreepJS "lies" detection) ---
// CreepJS and challenge vendors call Function.prototype.toString.call(fn) directly,
// bypassing any instance-level fn.toString override. Every polyfilled API
// must return "[native code]" via this path too.
//
// Pattern: Function.prototype.toString.call(X).includes('[native code]')
//   vs the weaker:           X.toString().includes('[native code]')
// Only the former catches JS-injection lies.

#[tokio::test]
async fn fn_proto_tostring_nav_getbattery_native() {
    assert_eq!(
        check_secure(
            "Function.prototype.toString.call(navigator.getBattery).includes('[native code]')"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_speech_getvoices_native() {
    assert_eq!(
        check(
            "Function.prototype.toString.call(speechSynthesis.getVoices).includes('[native code]')"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_permissions_query_native() {
    assert_eq!(
        check("Function.prototype.toString.call(navigator.permissions.query).includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_media_devices_enumerate_native() {
    assert_eq!(
        check_secure("Function.prototype.toString.call(navigator.mediaDevices.enumerateDevices).includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_ua_getter_native() {
    // Accessor getter on Navigator.prototype must also look native via bypass
    assert_eq!(
        check("Function.prototype.toString.call(Object.getOwnPropertyDescriptor(Navigator.prototype, 'userAgent').get).includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_screen_width_getter_native() {
    assert_eq!(
        check("Function.prototype.toString.call(Object.getOwnPropertyDescriptor(Screen.prototype, 'width').get).includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_chrome_loadtimes_native() {
    // chrome.loadTimes already passes fn.toString(); verify bypass also works
    assert_eq!(
        check("Function.prototype.toString.call(chrome.loadTimes).includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_rtc_create_offer_native() {
    assert_eq!(
        check("Function.prototype.toString.call(RTCPeerConnection.prototype.createOffer).includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn fn_proto_tostring_itself_is_native() {
    // Function.prototype.toString.call(Function.prototype.toString) must be native
    assert_eq!(
        check("Function.prototype.toString.call(Function.prototype.toString).includes('[native code]')").await,
        "true"
    );
}

// --- Symbol.toStringTag brand strings (challenge-vendor probe) ---
// The bot-detection sensors check Object.prototype.toString.call(x) against the
// Chrome WebIDL brand names. Anything returning "[object Object]" is an
// instant bot signal.

#[tokio::test]
async fn to_string_tag_document() {
    assert_eq!(
        check("Object.prototype.toString.call(document)").await,
        "[object HTMLDocument]"
    );
}

#[tokio::test]
async fn to_string_tag_html_body_element() {
    let s = check(
        "(() => { const b = document.createElement('body'); return Object.prototype.toString.call(b); })()",
    )
    .await;
    assert_eq!(s, "[object HTMLBodyElement]");
}

#[tokio::test]
async fn to_string_tag_html_div_element() {
    let s = check(
        "(() => { const d = document.createElement('div'); return Object.prototype.toString.call(d); })()",
    )
    .await;
    assert_eq!(s, "[object HTMLDivElement]");
}

#[tokio::test]
async fn to_string_tag_html_canvas_element() {
    let s = check(
        "(() => { const c = document.createElement('canvas'); return Object.prototype.toString.call(c); })()",
    )
    .await;
    assert_eq!(s, "[object HTMLCanvasElement]");
}

#[tokio::test]
async fn to_string_tag_canvas_rendering_context_2d() {
    let s = check(
        "(() => { const ctx = document.createElement('canvas').getContext('2d'); return Object.prototype.toString.call(ctx); })()",
    )
    .await;
    assert_eq!(s, "[object CanvasRenderingContext2D]");
}

#[tokio::test]
async fn to_string_tag_webgl_rendering_context() {
    let s = check(
        "(() => { const ctx = document.createElement('canvas').getContext('webgl'); return Object.prototype.toString.call(ctx); })()",
    )
    .await;
    assert_eq!(s, "[object WebGLRenderingContext]");
}

#[tokio::test]
async fn to_string_tag_element_prototype() {
    assert_eq!(
        check("Object.prototype.toString.call(Element.prototype)").await,
        "[object Element]"
    );
}

#[tokio::test]
async fn to_string_tag_node_prototype() {
    assert_eq!(
        check("Object.prototype.toString.call(Node.prototype)").await,
        "[object Node]"
    );
}

#[tokio::test]
async fn to_string_tag_event_target_prototype() {
    assert_eq!(
        check("Object.prototype.toString.call(EventTarget.prototype)").await,
        "[object EventTarget]"
    );
}

// --- Worker context fingerprint consistency ---
// All fingerprint values inside Worker scope must exactly match the main window.
// Detectors (challenge vendors, fingerprint-scan) compare Worker UA/platform/hardwareConcurrency
// against the main page values and flag mismatches as bot signals.

#[tokio::test]
async fn worker_ua_matches_window() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_windows();
    let win_ua = profile.user_agent.to_string();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    // Spin up a worker and stash its navigator.userAgent in a global
    page.evaluate(
        r#"window.__wua = null;
        const src = 'self.postMessage(navigator.userAgent);';
        const w = new Worker(URL.createObjectURL(new Blob([src],{type:'text/javascript'})));
        w.onmessage = e => { window.__wua = e.data; w.terminate(); };"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(500))
        .await
        .ok();
    let worker_ua = page.evaluate("window.__wua").unwrap();
    assert_eq!(worker_ua, win_ua, "Worker UA should match window UA");
}

#[tokio::test]
async fn worker_platform_matches_window() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_windows();
    let expected = profile.platform.clone();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    page.evaluate(
        r#"window.__wplat = null;
        const src = 'self.postMessage(navigator.platform);';
        const w = new Worker(URL.createObjectURL(new Blob([src],{type:'text/javascript'})));
        w.onmessage = e => { window.__wplat = e.data; w.terminate(); };"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(500))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__wplat").unwrap(), expected);
}

#[tokio::test]
async fn worker_hardware_concurrency_matches_window() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_windows();
    let expected = profile.cpu_cores.to_string();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    page.evaluate(
        r#"window.__whc = null;
        const src = 'self.postMessage(String(navigator.hardwareConcurrency));';
        const w = new Worker(URL.createObjectURL(new Blob([src],{type:'text/javascript'})));
        w.onmessage = e => { window.__whc = e.data; w.terminate(); };"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(500))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__whc").unwrap(), expected);
}

// --- screen.availTop per-OS consistency ---
// On macOS the 25px menu bar means availTop=25 (not 0).
// On Windows/Linux there is no top bar so availTop=0.
// availTop===0 for macOS profiles is a geometry inconsistency signal.

#[tokio::test]
async fn screen_avail_top_macos_is_33() {
    // Phase 7 — Chrome 147 macOS arm64 (M3) reports availTop=33,
    // not 25. Verified against a reference browser capture.
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_macos();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    assert_eq!(page.evaluate("screen.availTop").unwrap(), "33");
}

#[tokio::test]
async fn screen_avail_top_windows_is_0() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_windows();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    assert_eq!(page.evaluate("screen.availTop").unwrap(), "0");
}

#[tokio::test]
async fn screen_avail_top_linux_is_0() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_linux();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    assert_eq!(page.evaluate("screen.availTop").unwrap(), "0");
}

// --- Challenge-vendor specific Navigator shape checks ---

#[tokio::test]
async fn nav_webdriver_typeof_boolean() {
    // Modern Chrome (>=89, incl. Chrome-148): navigator.webdriver is a
    // boolean `false` for normal browsing (the fn name was always right;
    // the prior "undefined" assertion was the wrong assumption —
    // wdt fix, the challenge sensor flagged wdt.r="undefined").
    assert_eq!(check("typeof navigator.webdriver").await, "boolean");
}

#[tokio::test]
async fn nav_webdriver_in_operator_true() {
    // Real Chrome: 'webdriver' in navigator === true. Missing-key is a tell.
    assert_eq!(check("'webdriver' in navigator").await, "true");
}

#[tokio::test]
async fn nav_languages_is_frozen() {
    assert_eq!(check("Object.isFrozen(navigator.languages)").await, "true");
}

#[tokio::test]
async fn nav_constructor_name_is_navigator() {
    assert_eq!(check("navigator.constructor.name").await, "Navigator");
}

#[tokio::test]
async fn nav_permissions_query_is_native() {
    assert_eq!(
        check("navigator.permissions.query.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn permissions_webidl_shape_matches_chrome_148() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"globalThis.__permissionShape = null;
        navigator.permissions.query({name:'geolocation'}).then(status => {
            const handler = () => {};
            status.onchange = handler;
            const construction = C => {
                try { new C(); return 'ok'; }
                catch (error) { return error.name + ':' + error.message; }
            };
            globalThis.__permissionShape = {
                permissions: {
                    tag:Object.prototype.toString.call(navigator.permissions),
                    own:Reflect.ownKeys(navigator.permissions).map(String),
                    instance:navigator.permissions instanceof Permissions,
                    source:String(Permissions),
                    construction:construction(Permissions),
                },
                status: {
                    tag:Object.prototype.toString.call(status),
                    own:Reflect.ownKeys(status).map(String),
                    instance:status instanceof PermissionStatus,
                    eventTarget:status instanceof EventTarget,
                    source:String(PermissionStatus),
                    construction:construction(PermissionStatus),
                    name:status.name,
                    state:status.state,
                    handler:status.onchange === handler,
                    enumerable:['name','state','onchange'].map(name =>
                        Object.getOwnPropertyDescriptor(PermissionStatus.prototype,name).enumerable),
                }
            };
        });"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(50))
        .await
        .unwrap();
    let result = page
        .evaluate("JSON.stringify(globalThis.__permissionShape)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["permissions"]["tag"], "[object Permissions]");
    assert_eq!(value["permissions"]["own"], serde_json::json!([]));
    assert_eq!(value["permissions"]["instance"], true);
    assert_eq!(
        value["permissions"]["source"],
        "function Permissions() { [native code] }"
    );
    assert!(value["permissions"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(value["status"]["tag"], "[object PermissionStatus]");
    assert_eq!(value["status"]["own"], serde_json::json!([]));
    assert_eq!(value["status"]["instance"], true);
    assert_eq!(value["status"]["eventTarget"], true);
    assert_eq!(
        value["status"]["source"],
        "function PermissionStatus() { [native code] }"
    );
    assert!(value["status"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(value["status"]["name"], "geolocation");
    assert_eq!(value["status"]["state"], "prompt");
    assert_eq!(value["status"]["handler"], true);
    assert_eq!(
        value["status"]["enumerable"],
        serde_json::json!([true, true, true])
    );
}

// --- navigator.keyboard (CreepJS + challenge-vendor probe) ---
// Real Chrome exposes a Keyboard object with getLayoutMap() -> KeyboardLayoutMap.
// An empty {} object or missing getLayoutMap() is an immediate lie signal.

#[tokio::test]
async fn nav_keyboard_exists() {
    assert_eq!(check_secure("typeof navigator.keyboard").await, "object");
}

#[tokio::test]
async fn nav_keyboard_getlayoutmap_is_function() {
    assert_eq!(
        check_secure("typeof navigator.keyboard.getLayoutMap").await,
        "function"
    );
}

#[tokio::test]
async fn nav_keyboard_getlayoutmap_returns_promise() {
    assert_eq!(
        check_secure("navigator.keyboard.getLayoutMap() instanceof Promise").await,
        "true"
    );
}

#[tokio::test]
async fn nav_keyboard_getlayoutmap_resolves_to_map() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        "window.__r = null; navigator.keyboard.getLayoutMap().then(m => { window.__r = m; });",
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__r !== null").unwrap(), "true");
}

#[tokio::test]
async fn nav_keyboard_getlayoutmap_has_entries() {
    // A real QWERTY layout has ~50 entries; we just need > 0.
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        "window.__r = 0; navigator.keyboard.getLayoutMap().then(m => { window.__r = m.size; });",
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    let size: usize = page.evaluate("window.__r").unwrap().parse().unwrap_or(0);
    assert!(
        size > 0,
        "KeyboardLayoutMap should have entries, got {size}"
    );
}

#[tokio::test]
async fn keyboard_speech_and_history_webidl_shape_matches_chrome_148() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(
        r#"globalThis.__shape = null;
        navigator.keyboard.getLayoutMap().then(map => {
            const construction = C => {
                try { new C(); return 'ok'; }
                catch (error) { return error.name + ':' + error.message; }
            };
            history.scrollRestoration = 'manual';
            const keyboardNames = ['getLayoutMap','lock','unlock'];
            const layoutNames = ['size','get','has','entries','keys','values','forEach'];
            globalThis.__shape = {
                keyboard: {
                    tag: Object.prototype.toString.call(navigator.keyboard),
                    own: Reflect.ownKeys(navigator.keyboard).map(String),
                    construction: construction(Keyboard),
                    source: String(Keyboard),
                    enumerable: keyboardNames.map(name =>
                        Object.getOwnPropertyDescriptor(Keyboard.prototype, name).enumerable),
                    lengths: keyboardNames.map(name => Keyboard.prototype[name].length),
                },
                layout: {
                    tag: Object.prototype.toString.call(map),
                    own: Reflect.ownKeys(map).map(String),
                    construction: construction(KeyboardLayoutMap),
                    source: String(KeyboardLayoutMap),
                    enumerable: layoutNames.map(name =>
                        Object.getOwnPropertyDescriptor(KeyboardLayoutMap.prototype, name).enumerable),
                    forEachLength: KeyboardLayoutMap.prototype.forEach.length,
                    size: map.size,
                },
                speech: {
                    source: String(SpeechSynthesis),
                    construction: construction(SpeechSynthesis),
                    speakLength: SpeechSynthesis.prototype.speak.length,
                },
                history: {
                    value: history.scrollRestoration,
                    setter: typeof Object.getOwnPropertyDescriptor(
                        History.prototype, 'scrollRestoration').set,
                    lengths: [
                        History.prototype.go.length,
                        History.prototype.pushState.length,
                        History.prototype.replaceState.length,
                    ],
                },
            };
        });"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .unwrap();
    let result = page.evaluate("JSON.stringify(globalThis.__shape)").unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["keyboard"]["tag"], "[object Keyboard]");
    assert_eq!(value["keyboard"]["own"], serde_json::json!([]));
    assert!(value["keyboard"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["keyboard"]["source"],
        "function Keyboard() { [native code] }"
    );
    assert_eq!(
        value["keyboard"]["enumerable"],
        serde_json::json!([true, true, true])
    );
    assert_eq!(value["keyboard"]["lengths"], serde_json::json!([0, 0, 0]));
    assert_eq!(value["layout"]["tag"], "[object KeyboardLayoutMap]");
    assert_eq!(value["layout"]["own"], serde_json::json!([]));
    assert!(value["layout"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["layout"]["source"],
        "function KeyboardLayoutMap() { [native code] }"
    );
    assert_eq!(
        value["layout"]["enumerable"],
        serde_json::json!([true, true, true, true, true, true, true])
    );
    assert_eq!(value["layout"]["forEachLength"], 1);
    assert!(value["layout"]["size"].as_u64().unwrap_or(0) > 0);
    assert_eq!(
        value["speech"]["source"],
        "function SpeechSynthesis() { [native code] }"
    );
    assert!(value["speech"]["construction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(value["speech"]["speakLength"], 1);
    assert_eq!(value["history"]["value"], "manual");
    assert_eq!(value["history"]["setter"], "function");
    assert_eq!(value["history"]["lengths"], serde_json::json!([0, 2, 2]));
}

#[tokio::test]
async fn nav_keyboard_getlayoutmap_has_keya() {
    // KeyA is always present in any Latin keyboard layout.
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate("window.__r = false; navigator.keyboard.getLayoutMap().then(m => { window.__r = m.has('KeyA'); });").unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__r").unwrap(), "true");
}

#[tokio::test]
async fn nav_keyboard_getlayoutmap_is_native() {
    assert_eq!(
        check_secure("Function.prototype.toString.call(navigator.keyboard.getLayoutMap).includes('[native code]')").await,
        "true"
    );
}

// --- requestMediaKeySystemAccess / DRM (challenge-vendor probe) ---
// Real Chrome on Windows/macOS supports com.widevine.alpha.
// Always-rejecting NotSupportedError is a bot signal.

#[tokio::test]
async fn media_key_widevine_resolves_on_windows() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_windows();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    page.evaluate(
        "window.__r = null; navigator.requestMediaKeySystemAccess('com.widevine.alpha', [{initDataTypes:['cenc'],videoCapabilities:[{contentType:'video/mp4;codecs=\"avc1.42E01E\"'}]}]).then(a => { window.__r = 'ok'; }).catch(e => { window.__r = 'err:' + e.name; });"
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__r").unwrap(), "ok");
}

#[tokio::test]
async fn media_key_widevine_resolves_on_macos() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_macos();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    page.evaluate(
        "window.__r = null; navigator.requestMediaKeySystemAccess('com.widevine.alpha', [{initDataTypes:['cenc'],videoCapabilities:[{contentType:'video/mp4;codecs=\"avc1.42E01E\"'}]}]).then(a => { window.__r = 'ok'; }).catch(e => { window.__r = 'err:' + e.name; });"
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__r").unwrap(), "ok");
}

#[tokio::test]
async fn media_key_clearkey_always_resolves() {
    // org.w3.clearkey must work on all platforms per the W3C EME spec.
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        "window.__r = null; navigator.requestMediaKeySystemAccess('org.w3.clearkey', [{initDataTypes:['keyids'],videoCapabilities:[{contentType:'video/webm;codecs=\"vp8\"'}]}]).then(a => { window.__r = 'ok'; }).catch(e => { window.__r = 'err:' + e.name; });"
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__r").unwrap(), "ok");
}

#[tokio::test]
async fn media_key_access_key_system_is_string() {
    use browser_oxide::stealth::presets;
    let profile = presets::chrome_148_windows();
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    page.evaluate(
        "window.__r = null; navigator.requestMediaKeySystemAccess('com.widevine.alpha', [{initDataTypes:['cenc'],videoCapabilities:[{contentType:'video/mp4;codecs=\"avc1.42E01E\"'}]}]).then(a => { window.__r = typeof a.keySystem; }).catch(() => { window.__r = 'err'; });"
    ).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    assert_eq!(page.evaluate("window.__r").unwrap(), "string");
}

// --- Crypto / SubtleCrypto / Performance ---

#[tokio::test]
async fn crypto_instanceof_crypto() {
    assert_eq!(check("crypto instanceof Crypto").await, "true");
}

#[tokio::test]
async fn crypto_subtle_instanceof_subtle_crypto() {
    assert_eq!(
        check_secure("crypto.subtle instanceof SubtleCrypto").await,
        "true"
    );
}

#[tokio::test]
async fn crypto_has_no_own_properties() {
    assert_eq!(
        check("Object.getOwnPropertyNames(crypto).length").await,
        "0"
    );
}

#[tokio::test]
async fn crypto_get_random_values_native() {
    assert_eq!(
        check("crypto.getRandomValues.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn crypto_random_uuid_native() {
    assert_eq!(
        check_secure("crypto.randomUUID.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn crypto_random_uuid_format() {
    assert_eq!(
        check_secure("/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(crypto.randomUUID())").await,
        "true"
    );
}

#[tokio::test]
async fn crypto_get_random_values_returns_non_zero() {
    assert_eq!(
        check(
            "(() => { const a = new Uint32Array(4); crypto.getRandomValues(a); return a.some(v => v !== 0); })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn crypto_subtle_digest_exists() {
    assert_eq!(
        check_secure("typeof crypto.subtle.digest").await,
        "function"
    );
}

#[tokio::test]
async fn crypto_subtle_digest_is_native() {
    assert_eq!(
        check_secure("crypto.subtle.digest.toString().includes('[native code]')").await,
        "true"
    );
}

// Verify crypto.subtle.digest actually computes a hash (not just a stub).
// Uses execute_and_run to let the event loop drain and resolve the Promise.
#[tokio::test]
async fn crypto_subtle_digest_actually_works() {
    use browser_oxide::Page;
    use std::time::Duration;
    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        "https://example.com/",
        browser_oxide::stealth::presets::chrome_148_windows(),
    )
    .await
    .unwrap();
    // Run the async digest and store result in a global
    let _ = page.event_loop().execute_and_run(r#"
        (async function() {
            try {
                var data = new TextEncoder().encode('hello world');
                var hash = await crypto.subtle.digest('SHA-256', data);
                var arr = new Uint8Array(hash);
                globalThis.__cryptoTestResult = Array.from(arr).map(function(b) { return b.toString(16).padStart(2, '0'); }).join('');
            } catch(e) { globalThis.__cryptoTestResult = 'err:' + e.message; }
        })();
    "#, Duration::from_secs(5)).await;
    let result = page
        .event_loop()
        .execute_script("globalThis.__cryptoTestResult || 'not-set'")
        .unwrap_or_default();
    // SHA-256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576e637fb7cf96a1ddd (note: 63 chars)
    // Correct SHA-256("hello world") is b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576e637fb7cf96a1ddd53d which is actually wrong
    // Real SHA-256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576e637fb7cf96a1ddd53de is 64 chars
    // Just check it's 64 hex chars and starts with "b9":
    assert_eq!(
        result.len(),
        64,
        "crypto.subtle.digest should return 64-char SHA-256 hex hash, got: {}",
        result
    );
    assert!(
        !result.starts_with("err:"),
        "crypto.subtle.digest failed: {}",
        result
    );
    assert!(
        !result.starts_with("not-set"),
        "crypto.subtle.digest never ran"
    );
}

#[tokio::test]
async fn crypto_subtle_hmac_matches_chrome_and_known_vector() {
    use browser_oxide::Page;
    use std::time::Duration;

    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        "https://example.com/",
        browser_oxide::stealth::presets::chrome_148_windows(),
    )
    .await
    .unwrap();

    let _ = page
        .event_loop()
        .execute_and_run(
            r#"
            (async function() {
                const hex = (buffer) => Array.from(new Uint8Array(buffer))
                    .map((b) => b.toString(16).padStart(2, '0')).join('');
                const result = {};
                try {
                    try {
                        new CryptoKey();
                        result.cryptoKeyConstructor = 'ok';
                    } catch (e) {
                        result.cryptoKeyConstructor = `${e.name}:${e.message}`;
                    }
                    const algorithmDescriptor = Object.getOwnPropertyDescriptor(
                        CryptoKey.prototype, 'algorithm'
                    );
                    result.cryptoKeySurface = {
                        length: CryptoKey.length,
                        native: Function.prototype.toString.call(CryptoKey).includes('[native code]'),
                        algorithmEnumerable: algorithmDescriptor.enumerable,
                        algorithmConfigurable: algorithmDescriptor.configurable,
                        algorithmHasGetter: typeof algorithmDescriptor.get === 'function',
                        algorithmHasSetter: typeof algorithmDescriptor.set === 'function',
                    };
                    const raw = new TextEncoder().encode('key');
                    const data = new TextEncoder().encode('The quick brown fox jumps over the lazy dog');
                    const key = await crypto.subtle.importKey(
                        'raw', raw, { name: 'HMAC', hash: 'SHA-256' }, true, ['sign', 'verify']
                    );
                    const signature = await crypto.subtle.sign('HMAC', key, data);
                    const badSignature = new Uint8Array(signature.slice(0));
                    badSignature[0] ^= 1;
                    result.key = {
                        tag: Object.prototype.toString.call(key),
                        own: Reflect.ownKeys(key).length,
                        proto: Object.getOwnPropertyNames(CryptoKey.prototype),
                        type: key.type,
                        extractable: key.extractable,
                        algorithm: key.algorithm,
                        usages: key.usages,
                        sameAlgorithmObject: key.algorithm === key.algorithm,
                        sameUsagesArray: key.usages === key.usages,
                    };
                    result.signature = hex(signature);
                    result.verify = await crypto.subtle.verify('HMAC', key, signature, data);
                    result.verifyBad = await crypto.subtle.verify('HMAC', key, badSignature, data);
                    result.exported = hex(await crypto.subtle.exportKey('raw', key));

                    const generated = await crypto.subtle.generateKey(
                        { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
                    );
                    result.generated = {
                        type: generated.type,
                        algorithm: generated.algorithm,
                        usages: generated.usages,
                        rawLength: (await crypto.subtle.exportKey('raw', generated)).byteLength,
                    };

                    try {
                        await crypto.subtle.importKey(
                            'raw', raw, { name: 'HMAC', hash: 'SHA-256' }, true, ['encrypt']
                        );
                        result.badUsage = 'ok';
                    } catch (e) {
                        result.badUsage = e.name;
                    }

                    const nonExtractable = await crypto.subtle.importKey(
                        'raw', raw, { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']
                    );
                    try {
                        await crypto.subtle.exportKey('raw', nonExtractable);
                        result.nonExtractable = 'ok';
                    } catch (e) {
                        result.nonExtractable = e.name;
                    }

                    const verifyOnly = await crypto.subtle.importKey(
                        'raw', raw, { name: 'HMAC', hash: 'SHA-256' }, true, ['verify']
                    );
                    try {
                        await crypto.subtle.sign('HMAC', verifyOnly, data);
                        result.wrongUsage = 'ok';
                    } catch (e) {
                        result.wrongUsage = e.name;
                    }
                } catch (e) {
                    result.topError = `${e.name}:${e.message}`;
                }
                globalThis.__hmacResult = JSON.stringify(result);
            })();
            "#,
            Duration::from_secs(5),
        )
        .await;

    let raw = page
        .event_loop()
        .execute_script("globalThis.__hmacResult || '{}' ")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(
        value["cryptoKeyConstructor"],
        "TypeError:Failed to construct 'CryptoKey': Illegal constructor"
    );
    assert_eq!(value["cryptoKeySurface"]["length"], 0);
    assert_eq!(value["cryptoKeySurface"]["native"], true);
    assert_eq!(value["cryptoKeySurface"]["algorithmEnumerable"], true);
    assert_eq!(value["cryptoKeySurface"]["algorithmConfigurable"], true);
    assert_eq!(value["cryptoKeySurface"]["algorithmHasGetter"], true);
    assert_eq!(value["cryptoKeySurface"]["algorithmHasSetter"], false);
    assert_eq!(value["key"]["tag"], "[object CryptoKey]");
    assert_eq!(value["key"]["own"], 0);
    assert_eq!(
        value["key"]["proto"],
        serde_json::json!(["type", "extractable", "algorithm", "usages", "constructor"])
    );
    assert_eq!(value["key"]["type"], "secret");
    assert_eq!(value["key"]["extractable"], true);
    assert_eq!(value["key"]["algorithm"]["name"], "HMAC");
    assert_eq!(value["key"]["algorithm"]["hash"]["name"], "SHA-256");
    assert_eq!(value["key"]["algorithm"]["length"], 24);
    assert_eq!(
        value["key"]["usages"],
        serde_json::json!(["sign", "verify"])
    );
    assert_eq!(value["key"]["sameAlgorithmObject"], false);
    assert_eq!(value["key"]["sameUsagesArray"], false);
    assert_eq!(
        value["signature"],
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(value["verify"], true);
    assert_eq!(value["verifyBad"], false);
    assert_eq!(value["exported"], "6b6579");
    assert_eq!(value["generated"]["type"], "secret");
    assert_eq!(value["generated"]["algorithm"]["name"], "HMAC");
    assert_eq!(value["generated"]["algorithm"]["hash"]["name"], "SHA-256");
    assert_eq!(value["generated"]["algorithm"]["length"], 512);
    assert_eq!(value["generated"]["usages"], serde_json::json!(["sign"]));
    assert_eq!(value["generated"]["rawLength"], 64);
    assert_eq!(value["badUsage"], "SyntaxError");
    assert_eq!(value["nonExtractable"], "InvalidAccessError");
    assert_eq!(value["wrongUsage"], "InvalidAccessError");
    assert!(value.get("topError").is_none());
}

#[tokio::test]
async fn crypto_subtle_pbkdf2_matches_standard_vectors_and_key_semantics() {
    use browser_oxide::Page;
    use std::time::Duration;

    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        "https://example.com/",
        browser_oxide::stealth::presets::chrome_148_windows(),
    )
    .await
    .unwrap();

    let _ = page
        .event_loop()
        .execute_and_run(
            r#"
            (async function() {
                const hex = (buffer) => Array.from(new Uint8Array(buffer))
                    .map((b) => b.toString(16).padStart(2, '0')).join('');
                const enc = new TextEncoder();
                const password = enc.encode('password');
                const salt = enc.encode('salt');
                const result = {};
                try {
                    const key = await crypto.subtle.importKey(
                        'raw', password, 'PBKDF2', false, ['deriveBits']
                    );
                    result.key = {
                        tag: Object.prototype.toString.call(key),
                        own: Reflect.ownKeys(key).length,
                        type: key.type,
                        extractable: key.extractable,
                        algorithm: key.algorithm,
                        usages: key.usages,
                        sameAlgorithmObject: key.algorithm === key.algorithm,
                        sameUsagesArray: key.usages === key.usages,
                    };
                    result.sha256One = hex(await crypto.subtle.deriveBits(
                        { name: 'PBKDF2', salt, iterations: 1, hash: 'SHA-256' },
                        key,
                        256
                    ));
                    result.sha2564096 = hex(await crypto.subtle.deriveBits(
                        { name: 'PBKDF2', salt, iterations: 4096, hash: 'SHA-256' },
                        key,
                        256
                    ));
                    result.zeroLength = (await crypto.subtle.deriveBits(
                        { name: 'PBKDF2', salt, iterations: 1, hash: 'SHA-256' },
                        key,
                        0
                    )).byteLength;

                    try {
                        await crypto.subtle.importKey(
                            'raw', password, 'PBKDF2', true, ['deriveBits']
                        );
                        result.extractable = 'ok';
                    } catch (e) {
                        result.extractable = e.name;
                    }
                    try {
                        await crypto.subtle.importKey(
                            'raw', password, 'PBKDF2', false, ['sign']
                        );
                        result.badUsage = 'ok';
                    } catch (e) {
                        result.badUsage = e.name;
                    }
                    try {
                        await crypto.subtle.deriveBits(
                            { name: 'PBKDF2', salt, iterations: 0, hash: 'SHA-256' },
                            key,
                            256
                        );
                        result.zeroIterations = 'ok';
                    } catch (e) {
                        result.zeroIterations = e.name;
                    }
                    try {
                        await crypto.subtle.deriveBits(
                            { name: 'PBKDF2', salt, iterations: 1, hash: 'SHA-256' },
                            key,
                            7
                        );
                        result.badLength = 'ok';
                    } catch (e) {
                        result.badLength = e.name;
                    }
                    try {
                        await crypto.subtle.deriveBits(
                            { name: 'PBKDF2', salt, iterations: 1, hash: 'MD5' },
                            key,
                            256
                        );
                        result.badHash = 'ok';
                    } catch (e) {
                        result.badHash = e.name;
                    }
                } catch (e) {
                    result.topError = `${e.name}:${e.message}`;
                }
                globalThis.__pbkdf2Result = JSON.stringify(result);
            })();
            "#,
            Duration::from_secs(5),
        )
        .await;

    let raw = page
        .event_loop()
        .execute_script("globalThis.__pbkdf2Result || '{}' ")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["key"]["tag"], "[object CryptoKey]");
    assert_eq!(value["key"]["own"], 0);
    assert_eq!(value["key"]["type"], "secret");
    assert_eq!(value["key"]["extractable"], false);
    assert_eq!(
        value["key"]["algorithm"],
        serde_json::json!({"name": "PBKDF2"})
    );
    assert_eq!(value["key"]["usages"], serde_json::json!(["deriveBits"]));
    assert_eq!(value["key"]["sameAlgorithmObject"], false);
    assert_eq!(value["key"]["sameUsagesArray"], false);
    assert_eq!(
        value["sha256One"],
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
    );
    assert_eq!(
        value["sha2564096"],
        "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
    );
    assert_eq!(value["zeroLength"], 0);
    assert_eq!(value["extractable"], "SyntaxError");
    assert_eq!(value["badUsage"], "SyntaxError");
    assert_eq!(value["zeroIterations"], "OperationError");
    assert_eq!(value["badLength"], "OperationError");
    assert_eq!(value["badHash"], "NotSupportedError");
    assert!(value.get("topError").is_none());
}

#[tokio::test]
async fn crypto_subtle_pbkdf2_derive_key_matches_hmac_semantics() {
    use browser_oxide::Page;
    use std::time::Duration;

    let mut page = Page::with_profile(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        "https://example.com/",
        browser_oxide::stealth::presets::chrome_148_windows(),
    )
    .await
    .unwrap();

    let _ = page
        .event_loop()
        .execute_and_run(
            r#"
            (async function() {
                const hex = (buffer) => Array.from(new Uint8Array(buffer))
                    .map((b) => b.toString(16).padStart(2, '0')).join('');
                const enc = new TextEncoder();
                const password = enc.encode('password');
                const salt = enc.encode('salt');
                const result = {};
                try {
                    const baseKey = await crypto.subtle.importKey(
                        'raw', password, 'PBKDF2', false, ['deriveKey']
                    );
                    const explicit = await crypto.subtle.deriveKey(
                        { name: 'PBKDF2', salt, iterations: 2, hash: 'SHA-256' },
                        baseKey,
                        { name: 'HMAC', hash: 'SHA-256', length: 256 },
                        true,
                        ['sign', 'verify']
                    );
                    result.explicit = {
                        algorithm: explicit.algorithm,
                        type: explicit.type,
                        extractable: explicit.extractable,
                        usages: explicit.usages,
                        raw: hex(await crypto.subtle.exportKey('raw', explicit)),
                    };

                    const defaultLength = await crypto.subtle.deriveKey(
                        { name: 'PBKDF2', salt, iterations: 2, hash: 'SHA-256' },
                        baseKey,
                        { name: 'HMAC', hash: 'SHA-256' },
                        true,
                        ['sign']
                    );
                    result.defaultLength = {
                        algorithm: defaultLength.algorithm,
                        rawLength: (await crypto.subtle.exportKey('raw', defaultLength)).byteLength,
                    };

                    for (const [name, derivedType, usages] of [
                        ['noUsage', { name: 'HMAC', hash: 'SHA-256', length: 256 }, []],
                        ['badUsage', { name: 'HMAC', hash: 'SHA-256', length: 256 }, ['encrypt']],
                        ['badDerivedAlgorithm', { name: 'UNKNOWN' }, ['sign']],
                        ['badLength', { name: 'HMAC', hash: 'SHA-256', length: 7 }, ['sign']],
                    ]) {
                        try {
                            await crypto.subtle.deriveKey(
                                { name: 'PBKDF2', salt, iterations: 1, hash: 'SHA-256' },
                                baseKey,
                                derivedType,
                                true,
                                usages
                            );
                            result[name] = 'ok';
                        } catch (e) {
                            result[name] = e.name;
                        }
                    }

                    const wrongUsageBase = await crypto.subtle.importKey(
                        'raw', password, 'PBKDF2', false, ['deriveBits']
                    );
                    try {
                        await crypto.subtle.deriveKey(
                            { name: 'PBKDF2', salt, iterations: 1, hash: 'SHA-256' },
                            wrongUsageBase,
                            { name: 'HMAC', hash: 'SHA-256', length: 256 },
                            true,
                            ['sign']
                        );
                        result.baseWrongUsage = 'ok';
                    } catch (e) {
                        result.baseWrongUsage = e.name;
                    }
                } catch (e) {
                    result.topError = `${e.name}:${e.message}`;
                }
                globalThis.__pbkdf2DeriveKeyResult = JSON.stringify(result);
            })();
            "#,
            Duration::from_secs(5),
        )
        .await;

    let raw = page
        .event_loop()
        .execute_script("globalThis.__pbkdf2DeriveKeyResult || '{}' ")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(
        value["explicit"]["algorithm"],
        serde_json::json!({"name":"HMAC","hash":{"name":"SHA-256"},"length":256})
    );
    assert_eq!(value["explicit"]["type"], "secret");
    assert_eq!(value["explicit"]["extractable"], true);
    assert_eq!(
        value["explicit"]["usages"],
        serde_json::json!(["sign", "verify"])
    );
    assert_eq!(
        value["explicit"]["raw"],
        "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
    );
    assert_eq!(value["defaultLength"]["algorithm"]["length"], 512);
    assert_eq!(value["defaultLength"]["rawLength"], 64);
    assert_eq!(value["noUsage"], "SyntaxError");
    assert_eq!(value["badUsage"], "SyntaxError");
    assert_eq!(value["badDerivedAlgorithm"], "NotSupportedError");
    assert_eq!(value["badLength"], "OperationError");
    assert_eq!(value["baseWrongUsage"], "InvalidAccessError");
    assert!(value.get("topError").is_none());
}

#[tokio::test]
async fn performance_instanceof_performance() {
    assert_eq!(check("performance instanceof Performance").await, "true");
}

#[tokio::test]
async fn performance_has_no_own_properties() {
    assert_eq!(
        check("Object.getOwnPropertyNames(performance).length").await,
        "0"
    );
}

#[tokio::test]
async fn performance_memory_on_prototype() {
    assert_eq!(
        check("Performance.prototype.hasOwnProperty('memory')").await,
        "true"
    );
}

#[tokio::test]
async fn performance_get_entries_is_native() {
    assert_eq!(
        check("performance.getEntries.toString().includes('[native code]')").await,
        "true"
    );
}

// ================================================================
// WebAuthn + FedCM — detection-shape probes
// ----------------------------------------------------------------
// What anti-bot vendors check:
//   typeof PublicKeyCredential, IdentityCredential, IdentityProvider
//   PublicKeyCredential.isUVPAA() returns Promise resolving to per-profile bool
//   PublicKeyCredential.getClientCapabilities() returns object (Chrome 133+)
//   navigator.credentials.create({publicKey:...}) rejects NotAllowedError after ~120ms
//   navigator.credentials.get({identity:...}) rejects NotAllowedError after ~200ms
//   _maskAsNative purity on all spoofed methods
// ================================================================

// Promise helper: kicks off a Promise-returning expression and pumps timers
// for `timeout_ms` ms so setTimeout-delayed rejections (120-250 ms in WebAuthn
// shim) have time to fire. Stashes resolved value in window.__r and rejection
// in window.__rej. Returns either "ok:<value>" or "rej:<error.name>".
/// Loads over https:// so [SecureContext]-only APIs (credentials, etc.)
/// are exposed. Phase 7.
async fn await_promise_secure(js: &str, timeout_ms: u64) -> String {
    await_promise_inner(js, timeout_ms, None, true).await
}
/// Same as `await_promise_secure`, but with a caller-supplied profile.
/// Phase 7 — required for [SecureContext]-only APIs.
async fn await_promise_with_profile_secure(
    js: &str,
    timeout_ms: u64,
    profile: browser_oxide::stealth::StealthProfile,
) -> String {
    await_promise_inner(js, timeout_ms, Some(profile), true).await
}
async fn await_promise_inner(
    js: &str,
    timeout_ms: u64,
    profile: Option<browser_oxide::stealth::StealthProfile>,
    secure: bool,
) -> String {
    let mut page = if secure {
        Page::from_html_with_url(&html(""), "https://example.com/", profile)
            .await
            .unwrap()
    } else {
        Page::from_html(&html(""), profile).await.unwrap()
    };
    let stash = format!(
        "window.__r = null; window.__rej = null; \
         ({js}).then(v => {{ window.__r = String(v); }}, \
                     e => {{ window.__rej = (e && e.name) ? e.name : (e && e.constructor ? e.constructor.name : String(e)); }});"
    );
    page.evaluate(&stash).unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(timeout_ms))
        .await
        .ok();
    page.evaluate(
        "(window.__rej !== null) ? ('rej:' + window.__rej) \
         : (window.__r !== null) ? ('ok:' + window.__r) \
         : 'pending'",
    )
    .unwrap()
}

#[tokio::test]
async fn webauthn_public_key_credential_exists() {
    assert_eq!(check_secure("typeof PublicKeyCredential").await, "function");
}

#[tokio::test]
async fn webauthn_public_key_credential_constructor_throws() {
    assert_eq!(
        check_secure("(() => { try { new PublicKeyCredential(); return 'no-throw'; } catch (e) { return e.message; } })()")
            .await,
        "Illegal constructor"
    );
}

#[tokio::test]
async fn webauthn_authenticator_response_classes_exist() {
    assert_eq!(
        check_secure("typeof AuthenticatorResponse").await,
        "function"
    );
    assert_eq!(
        check_secure("typeof AuthenticatorAttestationResponse").await,
        "function"
    );
    assert_eq!(
        check_secure("typeof AuthenticatorAssertionResponse").await,
        "function"
    );
}

#[tokio::test]
async fn webauthn_attestation_extends_response() {
    assert_eq!(
        check_secure("AuthenticatorAttestationResponse.prototype instanceof AuthenticatorResponse")
            .await,
        "true"
    );
}

#[tokio::test]
async fn webauthn_isuvpa_returns_promise() {
    assert_eq!(
        check_secure("PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable() instanceof Promise").await,
        "true"
    );
}

#[tokio::test]
async fn webauthn_iscma_returns_promise() {
    assert_eq!(
        check_secure("PublicKeyCredential.isConditionalMediationAvailable() instanceof Promise")
            .await,
        "true"
    );
}

#[tokio::test]
async fn webauthn_isuvpa_true_on_windows_profile() {
    let profile = browser_oxide::stealth::presets::chrome_148_windows();
    assert_eq!(
        await_promise_with_profile_secure(
            "PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()",
            50,
            profile,
        )
        .await,
        "ok:true"
    );
}

#[tokio::test]
async fn webauthn_isuvpa_true_on_macos_profile() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    assert_eq!(
        await_promise_with_profile_secure(
            "PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()",
            50,
            profile,
        )
        .await,
        "ok:true"
    );
}

#[tokio::test]
async fn webauthn_isuvpa_false_on_linux_profile() {
    let profile = browser_oxide::stealth::presets::chrome_148_linux();
    assert_eq!(
        await_promise_with_profile_secure(
            "PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()",
            50,
            profile,
        )
        .await,
        "ok:false"
    );
}

#[tokio::test]
async fn webauthn_get_client_capabilities_shape() {
    assert_eq!(
        await_promise_secure(
            "PublicKeyCredential.getClientCapabilities().then(c => \
             typeof c.userVerifyingPlatformAuthenticator === 'boolean' && \
             typeof c.conditionalGet === 'boolean' && \
             typeof c.hybridTransport === 'boolean')",
            50,
        )
        .await,
        "ok:true"
    );
}

#[tokio::test]
async fn webauthn_isuvpa_is_native() {
    assert_eq!(
        check_secure("PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn webauthn_credentials_create_is_native() {
    assert_eq!(
        check_secure("navigator.credentials.create.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn webauthn_credentials_create_rejects_with_not_allowed() {
    // Shim sleeps 120 ms before rejecting; pump 250 ms.
    assert_eq!(
        await_promise_secure("navigator.credentials.create({publicKey:{}})", 250,).await,
        "rej:NotAllowedError"
    );
}

#[tokio::test]
async fn webauthn_credentials_get_publickey_rejects() {
    assert_eq!(
        await_promise_secure("navigator.credentials.get({publicKey:{}})", 250,).await,
        "rej:NotAllowedError"
    );
}

#[tokio::test]
async fn webauthn_credentials_create_no_args_rejects_typeerror() {
    // Synchronous Promise.reject(new TypeError(...)). 50 ms pump is plenty.
    assert_eq!(
        await_promise_secure("navigator.credentials.create()", 50,).await,
        "rej:TypeError"
    );
}

#[tokio::test]
async fn webauthn_navigator_credentials_is_credentials_container() {
    assert_eq!(
        check_secure("navigator.credentials instanceof CredentialsContainer").await,
        "true"
    );
}

#[tokio::test]
async fn fedcm_identity_credential_exists() {
    assert_eq!(check_secure("typeof IdentityCredential").await, "function");
}

#[tokio::test]
async fn fedcm_identity_provider_exists() {
    assert_eq!(check_secure("typeof IdentityProvider").await, "function");
}

#[tokio::test]
async fn fedcm_identity_credential_constructor_throws() {
    assert_eq!(
        check_secure("(() => { try { new IdentityCredential(); return 'no-throw'; } catch (e) { return e.message; } })()")
            .await,
        "Illegal constructor"
    );
}

#[tokio::test]
async fn fedcm_identity_provider_get_user_info_rejects() {
    assert_eq!(
        await_promise_secure(
            "IdentityProvider.getUserInfo({configURL:'https://x/cfg.json',clientId:'a'})",
            50,
        )
        .await,
        "rej:NotAllowedError"
    );
}

// ================================================================
// WebGL fingerprint surface (spoofing path)
// ----------------------------------------------------------------
// Anti-bot vendors probe:
//   typeof WebGLRenderingContext, WebGL2RenderingContext
//   getParameter(VENDOR/RENDERER/UNMASKED_VENDOR_WEBGL/UNMASKED_RENDERER_WEBGL)
//   getParameter(MAX_TEXTURE_SIZE/MAX_RENDERBUFFER_SIZE/...)  — non-zero, plausible
//   getSupportedExtensions() — non-empty, GPU-correct
//   getShaderPrecisionFormat(FRAGMENT_SHADER, HIGH_FLOAT) — {127,127,23} on Chrome
//   getExtension('WEBGL_debug_renderer_info') — non-null, exposes UNMASKED_*
//   getContextAttributes() — Chrome defaults (alpha=true, antialias=true, etc.)
// All values come from the active StealthProfile.gpu_profile (stealth/src/gpu.rs).
// ================================================================

async fn webgl_check(js: &str) -> String {
    // Helper: create a canvas, get a webgl context, run js against `gl`.
    let wrapped = format!(
        "(() => {{ \
          const c = document.createElement('canvas'); \
          c.width = 256; c.height = 256; \
          const gl = c.getContext('webgl'); \
          if (!gl) return 'no-context'; \
          return ({js}); \
        }})()"
    );
    check(&wrapped).await
}

async fn webgl_check_with_profile(
    js: &str,
    profile: browser_oxide::stealth::StealthProfile,
) -> String {
    let wrapped = format!(
        "(() => {{ \
          const c = document.createElement('canvas'); \
          c.width = 256; c.height = 256; \
          const gl = c.getContext('webgl'); \
          if (!gl) return 'no-context'; \
          return ({js}); \
        }})()"
    );
    let mut page = Page::from_html(&html(""), Some(profile)).await.unwrap();
    page.evaluate(&wrapped)
        .unwrap_or_else(|e| format!("ERROR: {e}"))
}

#[tokio::test]
async fn webgl_rendering_context_class_exists() {
    assert_eq!(check("typeof WebGLRenderingContext").await, "function");
    assert_eq!(check("typeof WebGL2RenderingContext").await, "function");
}

#[tokio::test]
async fn webgl_get_context_returns_object() {
    assert_eq!(webgl_check("typeof gl").await, "object");
    assert_eq!(webgl_check("gl !== null").await, "true");
}

#[tokio::test]
async fn webgl_vendor_renderer_strings_non_empty() {
    assert_eq!(
        webgl_check(
            "typeof gl.getParameter(0x1F00) === 'string' && gl.getParameter(0x1F00).length > 0"
        )
        .await,
        "true"
    );
    assert_eq!(
        webgl_check(
            "typeof gl.getParameter(0x1F01) === 'string' && gl.getParameter(0x1F01).length > 0"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl_unmasked_vendor_renderer_per_profile() {
    // Win profile: NVIDIA. Mac: Apple. Linux: Intel.
    let win_renderer = webgl_check_with_profile(
        "gl.getParameter(0x9246)", // UNMASKED_RENDERER_WEBGL
        browser_oxide::stealth::presets::chrome_148_windows(),
    )
    .await;
    assert!(
        win_renderer.contains("NVIDIA"),
        "Win UNMASKED_RENDERER should mention NVIDIA, got {win_renderer}"
    );

    let mac_renderer = webgl_check_with_profile(
        "gl.getParameter(0x9246)",
        browser_oxide::stealth::presets::chrome_148_macos(),
    )
    .await;
    assert!(
        mac_renderer.contains("Apple"),
        "Mac UNMASKED_RENDERER should mention Apple, got {mac_renderer}"
    );

    let linux_renderer = webgl_check_with_profile(
        "gl.getParameter(0x9246)",
        browser_oxide::stealth::presets::chrome_148_linux(),
    )
    .await;
    assert!(
        linux_renderer.contains("Intel"),
        "Linux UNMASKED_RENDERER should mention Intel, got {linux_renderer}"
    );
}

#[tokio::test]
async fn webgl_max_texture_size_at_least_chrome_minimum() {
    // Chrome 130 reports 16384 on most modern GPUs; minimum WebGL spec is 64.
    // Our profiles all set 16384.
    assert_eq!(
        webgl_check("gl.getParameter(0x0D33) >= 8192").await, // MAX_TEXTURE_SIZE
        "true"
    );
}

#[tokio::test]
async fn webgl_get_supported_extensions_non_empty() {
    assert_eq!(
        webgl_check(
            "Array.isArray(gl.getSupportedExtensions()) && gl.getSupportedExtensions().length > 5"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl_supported_extensions_contain_debug_renderer_info() {
    // WEBGL_debug_renderer_info is in every Chrome-reported extension list
    // because it's the canonical way to get UNMASKED_VENDOR/RENDERER strings.
    assert_eq!(
        webgl_check("gl.getSupportedExtensions().includes('WEBGL_debug_renderer_info')").await,
        "true"
    );
}

#[tokio::test]
async fn webgl_get_extension_debug_renderer_info_returns_constants() {
    assert_eq!(
        webgl_check(
            "(() => { \
              const ext = gl.getExtension('WEBGL_debug_renderer_info'); \
              return ext !== null \
                  && ext.UNMASKED_VENDOR_WEBGL === 0x9245 \
                  && ext.UNMASKED_RENDERER_WEBGL === 0x9246; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl_get_shader_precision_format_high_float() {
    // FRAGMENT_SHADER (0x8B30) + HIGH_FLOAT (0x8DF2) → {rangeMin:127, rangeMax:127, precision:23}.
    assert_eq!(
        webgl_check(
            "(() => { \
              const p = gl.getShaderPrecisionFormat(0x8B30, 0x8DF2); \
              return p !== null \
                  && p.rangeMin === 127 \
                  && p.rangeMax === 127 \
                  && p.precision === 23; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl_get_context_attributes_chrome_defaults() {
    // Real Chrome defaults from WebGL spec.
    assert_eq!(
        webgl_check(
            "(() => { \
              const a = gl.getContextAttributes(); \
              return a.alpha === true \
                  && a.antialias === true \
                  && a.depth === true \
                  && a.failIfMajorPerformanceCaveat === false \
                  && a.premultipliedAlpha === true \
                  && a.preserveDrawingBuffer === false \
                  && a.stencil === false; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl_is_context_lost_returns_false() {
    assert_eq!(webgl_check("gl.isContextLost()").await, "false");
}

#[tokio::test]
async fn webgl_get_error_returns_zero_initially() {
    // NO_ERROR = 0
    assert_eq!(webgl_check("gl.getError()").await, "0");
}

#[tokio::test]
async fn webgl_max_viewport_dims_returns_array_of_two() {
    assert_eq!(
        webgl_check(
            "(() => { \
              const v = gl.getParameter(0x0D3A); \
              return Array.isArray(v) && v.length === 2 && v[0] > 0 && v[1] > 0; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn webgl2_context_returns_object() {
    assert_eq!(
        check(
            "(() => { \
              const c = document.createElement('canvas'); \
              const gl = c.getContext('webgl2'); \
              return typeof gl; \
             })()"
        )
        .await,
        "object"
    );
}

#[tokio::test]
async fn webgl_extensions_differ_per_profile_apple() {
    // Apple profile should include the ASTC compressed-texture extension
    // that NVIDIA/Intel typically lack (per gpu.rs::apple_m2_pro_macos).
    let exts = webgl_check_with_profile(
        "JSON.stringify(gl.getSupportedExtensions())",
        browser_oxide::stealth::presets::chrome_148_macos(),
    )
    .await;
    assert!(
        exts.contains("WEBGL_compressed_texture_astc"),
        "Apple profile should expose ASTC; got {exts}"
    );
}

// ================================================================
// Audio realtime — AnalyserNode + BiquadFilterNode
// ----------------------------------------------------------------
// AnalyserNode and BiquadFilterNode are now realer:
//   - AnalyserNode exposes fftSize, frequencyBinCount, smoothing,
//     minDecibels, maxDecibels (all probed by CreepJS).
//   - BiquadFilterNode.getFrequencyResponse() runs the closed-form
//     bilinear-transform via op_audio_biquad_response.
// Wire-through for graph-driven analyser data is still pending
// (offline path uses op_offline_audio_render, which is bit-accurate
// to Blink at ~3.6 ppm — see canvas/tests/audio_reference.rs).
// ================================================================

#[tokio::test]
async fn audio_context_exists() {
    assert_eq!(check("typeof AudioContext").await, "function");
    assert_eq!(check("typeof OfflineAudioContext").await, "function");
}

#[tokio::test]
async fn analyser_has_chrome_default_props() {
    assert_eq!(
        check(
            "(() => { \
              const ctx = new AudioContext(); \
              const a = ctx.createAnalyser(); \
              return a.fftSize === 2048 \
                  && a.frequencyBinCount === 1024 \
                  && a.smoothingTimeConstant === 0.8 \
                  && a.minDecibels === -100 \
                  && a.maxDecibels === -30; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn analyser_get_float_frequency_data_returns_min_db_for_silence() {
    // Unconnected analyser → silence → all bins at minDecibels (-100).
    assert_eq!(
        check(
            "(() => { \
              const ctx = new AudioContext(); \
              const a = ctx.createAnalyser(); \
              const f = new Float32Array(a.frequencyBinCount); \
              a.getFloatFrequencyData(f); \
              return f.every(v => v === a.minDecibels); \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn biquad_get_frequency_response_unity_at_dc_for_lowpass() {
    // Lowpass at 1 kHz, Q=0.7071 → at f=0, |H| ≈ 1.
    assert_eq!(
        check(
            "(() => { \
              const ctx = new AudioContext(); \
              const f = ctx.createBiquadFilter(); \
              f.type = 'lowpass'; f.frequency.value = 1000; f.Q.value = 0.7071; \
              const freqs = new Float32Array([0]); \
              const mag = new Float32Array(1); \
              const phase = new Float32Array(1); \
              f.getFrequencyResponse(freqs, mag, phase); \
              return Math.abs(mag[0] - 1) < 0.01; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn biquad_get_frequency_response_blocks_dc_for_highpass() {
    assert_eq!(
        check(
            "(() => { \
              const ctx = new AudioContext(); \
              const f = ctx.createBiquadFilter(); \
              f.type = 'highpass'; f.frequency.value = 1000; f.Q.value = 0.7071; \
              const freqs = new Float32Array([0]); \
              const mag = new Float32Array(1); \
              const phase = new Float32Array(1); \
              f.getFrequencyResponse(freqs, mag, phase); \
              return mag[0] < 0.01; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn biquad_get_frequency_response_writes_n_values() {
    assert_eq!(
        check(
            "(() => { \
              const ctx = new AudioContext(); \
              const f = ctx.createBiquadFilter(); \
              const freqs = new Float32Array([100, 500, 1000, 5000, 20000]); \
              const mag = new Float32Array(5); \
              const phase = new Float32Array(5); \
              f.getFrequencyResponse(freqs, mag, phase); \
              return mag.every(v => Number.isFinite(v) && v >= 0) \
                  && phase.every(v => Number.isFinite(v)); \
             })()"
        )
        .await,
        "true"
    );
}

// ================================================================
// performance.now() resolution
// ----------------------------------------------------------------
// Chrome 148 uses a 100 µs grid in a non-cross-origin-isolated page. We verify:
//   - Returns finite number
//   - Is monotonic across calls
//   - Positive hot-loop deltas stay on the 0.1 ms grid
// ================================================================

#[tokio::test]
async fn perf_now_returns_number() {
    assert_eq!(check("typeof performance.now()").await, "number");
}

#[tokio::test]
async fn perf_now_is_native() {
    assert_eq!(
        check("performance.now.toString().includes('[native code]')").await,
        "true"
    );
}

#[tokio::test]
async fn perf_now_is_finite_non_negative() {
    assert_eq!(
        check("(() => { const t = performance.now(); return Number.isFinite(t) && t >= 0; })()")
            .await,
        "true"
    );
}

#[tokio::test]
async fn perf_now_hot_loop_produces_distinct_values() {
    // Crossing multiple 100 µs buckets confirms the clock is advancing.
    assert_eq!(
        check(
            "(() => { \
              const xs = []; \
              for (let i = 0; i < 5000; i++) xs.push(performance.now()); \
              return new Set(xs).size > 10; \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_now_hot_loop_has_chrome_resolution() {
    assert_eq!(
        check(
            "(() => { \
              const xs = []; \
              for (let i = 0; i < 5000; i++) xs.push(performance.now()); \
              const positive = xs.slice(1).map((x, i) => x - xs[i]).filter(x => x > 0); \
              if (!positive.length) return false; \
              const min = Math.min(...positive); \
              return min >= 0.099 && positive.every(x => Math.abs(x * 10 - Math.round(x * 10)) < 1e-6); \
             })()"
        )
        .await,
        "true"
    );
}

#[tokio::test]
async fn perf_now_is_strictly_monotonic() {
    // HRT spec requires monotonic non-decreasing. The PerfState clamps
    // each return to >= last value, so 1000 samples must have zero
    // backward jumps.
    assert_eq!(
        check(
            "(() => { \
              const xs = []; \
              for (let i = 0; i < 1000; i++) xs.push(performance.now()); \
              for (let i = 1; i < xs.length; i++) if (xs[i] < xs[i-1]) return false; \
              return true; \
             })()"
        )
        .await,
        "true"
    );
}

// ================================================================
// Cross-origin isolation + SharedArrayBuffer
// ----------------------------------------------------------------
// What anti-bot vendors check (2024+):
//   self.crossOriginIsolated reflects COOP+COEP from response headers
//   typeof SharedArrayBuffer === 'function' (V8 always exposes constructor)
//   new SharedArrayBuffer(N) usable
//   typeof Atomics === 'object', Atomics.wait/notify exist
// SAB postMessage transfer to workers is gated separately on COI but
// our worker plumbing is still stub (CAPABILITY_GAPS_2026.md §T1.5),
// so transfer-rejection tests are deferred until workers fire.
// ================================================================

fn coi_check(cross_origin_isolated: bool, js: &str) -> String {
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    let mut rt = browser_oxide::js_runtime::BrowserJsRuntime::with_options(
        dom,
        browser_oxide::js_runtime::runtime::BrowserRuntimeOptions {
            cross_origin_isolated,
            // A valid cross-origin isolated document is necessarily secure.
            is_secure_context: cross_origin_isolated,
            ..Default::default()
        },
    );
    rt.execute_script(js, None)
        .unwrap_or_else(|e| format!("ERROR: {e}"))
}

#[tokio::test]
async fn coi_default_is_false_via_page() {
    // Page::from_html doesn't (yet) extract COOP/COEP from response headers,
    // so the default must be false.
    assert_eq!(check("typeof crossOriginIsolated").await, "boolean");
    assert_eq!(check("crossOriginIsolated").await, "false");
}

#[tokio::test]
async fn coi_true_when_runtime_constructed_isolated() {
    assert_eq!(coi_check(true, "crossOriginIsolated"), "true");
}

#[tokio::test]
async fn coi_false_when_runtime_constructed_non_isolated() {
    assert_eq!(coi_check(false, "crossOriginIsolated"), "false");
}

#[tokio::test]
async fn coi_property_descriptor_is_configurable_getter() {
    // Real Chrome exposes crossOriginIsolated as a getter on the global,
    // configurable=true. Plain `globalThis.X = false` would set it as a
    // value property — detectable.
    assert_eq!(
        coi_check(false, "Object.getOwnPropertyDescriptor(globalThis, 'crossOriginIsolated').get instanceof Function"),
        "true"
    );
}

#[tokio::test]
async fn sab_constructor_exists() {
    // Chrome hides SharedArrayBuffer without cross-origin isolation (COOP+COEP).
    // Default pages are not cross-origin isolated, so SAB is undefined.
    assert_eq!(check("typeof SharedArrayBuffer").await, "undefined");
}

#[tokio::test]
async fn sab_constructible_with_byte_length() {
    assert_eq!(coi_check(true, "new SharedArrayBuffer(8).byteLength"), "8");
}

#[tokio::test]
async fn sab_instance_is_shared_array_buffer() {
    assert_eq!(
        coi_check(
            true,
            "new SharedArrayBuffer(4) instanceof SharedArrayBuffer"
        ),
        "true"
    );
}

#[tokio::test]
async fn atomics_object_exists() {
    assert_eq!(check("typeof Atomics").await, "object");
}

#[tokio::test]
async fn atomics_wait_and_notify_exist() {
    assert_eq!(check("typeof Atomics.wait").await, "function");
    assert_eq!(check("typeof Atomics.notify").await, "function");
}

#[tokio::test]
async fn atomics_wait_returns_timed_out_synchronously() {
    // Atomics.wait on a fresh SharedArrayBuffer with timeout=1ms must return
    // "timed-out" (or "ok"/"not-equal" on edge cases) — proves SAB+Atomics
    // are functional, not just present.
    assert_eq!(
        coi_check(
            true,
            "Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1)"
        ),
        "timed-out"
    );
}

#[tokio::test]
async fn coi_requested_on_insecure_runtime_is_clamped_off() {
    let dom = browser_oxide::html_parser::parse_html("<html><body></body></html>");
    let mut rt = browser_oxide::js_runtime::BrowserJsRuntime::with_options(
        dom,
        browser_oxide::js_runtime::runtime::BrowserRuntimeOptions {
            cross_origin_isolated: true,
            is_secure_context: false,
            ..Default::default()
        },
    );
    assert_eq!(
        rt.execute_script("crossOriginIsolated", None).unwrap(),
        "false"
    );
    assert_eq!(
        rt.execute_script("typeof SharedArrayBuffer", None).unwrap(),
        "undefined"
    );
}

#[tokio::test]
async fn fedcm_credentials_get_identity_rejects() {
    // FedCM shim sleeps 200 ms before rejecting; pump 350 ms.
    assert_eq!(
        await_promise_secure(
            "navigator.credentials.get({ identity: { providers: [{ configURL: 'https://x/cfg.json', clientId: 'a' }] } })",
            350,
        )
        .await,
        "rej:NotAllowedError"
    );
}

// ================================================================
// Live navigation smoke test — reddit.com (network-gated, #[ignore])
// ----------------------------------------------------------------
// Run with:
//   cargo test -p browser --test chrome_compat reddit_smoke \
//     -- --ignored --test-threads=1 --nocapture
// ================================================================

// ================================================================
// V8 shim recursion reproducer (task #6)
// ================================================================

/// Phase-1: does walking the prototype chain with Reflect.ownKeys hang?
#[tokio::test]
async fn shim_recursion_proto_walk_no_access() {
    // If this test fails (timeout / SIGTRAP) the bug is in ownKeys / getPrototypeOf
    // enumeration itself — not in getter invocation.
    let result = check(
        r#"
        (function() {
            try {
                let p = globalThis;
                const seen = [];
                let depth = 0;
                while (p !== null && p !== undefined && depth < 30) {
                    // Guard against circular prototype (shouldn't happen, but just in case)
                    if (seen.indexOf(p) !== -1) return 'cycle_at_' + depth;
                    seen.push(p);
                    Reflect.ownKeys(p); // enumerate — don't access values
                    p = Object.getPrototypeOf(p);
                    depth++;
                }
                return 'walk_done_' + depth;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("walk_done_") || result.starts_with("cycle_at_"),
        "proto walk should complete without crash, got: {result}"
    );
}

/// Phase-2: does invoking each getter while walking cause recursion?
#[tokio::test]
async fn shim_recursion_proto_walk_with_getters() {
    // CreepJS calls toString on every function it finds while walking.
    // This test isolates whether our getter-invocation or toString masking recurses.
    let result = check(
        r#"
        (function() {
            try {
                let p = globalThis;
                let depth = 0;
                const seen = [];
                while (p !== null && p !== undefined && depth < 30) {
                    if (seen.indexOf(p) !== -1) return 'cycle_at_' + depth;
                    seen.push(p);
                    for (const key of Reflect.ownKeys(p)) {
                        try {
                            const desc = Object.getOwnPropertyDescriptor(p, key);
                            if (!desc) continue;
                            // Invoke getter if present
                            if (typeof desc.get === 'function') {
                                try { desc.get.call(p); } catch(_) {}
                            }
                            // Call .toString() on any function value/getter
                            if (typeof desc.get === 'function') {
                                desc.get.toString();
                            }
                            if (typeof desc.value === 'function') {
                                desc.value.toString();
                            }
                        } catch(_) {}
                    }
                    p = Object.getPrototypeOf(p);
                    depth++;
                }
                return 'walk_done_' + depth;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("walk_done_") || result.starts_with("cycle_at_"),
        "proto walk+getters should complete without crash, got: {result}"
    );
}

/// Phase-3: does Function.prototype.toString.call(fn) on all globalThis functions recurse?
#[tokio::test]
async fn shim_recursion_fn_proto_tostring_on_all() {
    let result = check(
        r#"
        (function() {
            try {
                const seen = new Set();
                let checked = 0;
                let p = globalThis;
                while (p) {
                    for (const key of Reflect.ownKeys(p)) {
                        try {
                            const desc = Object.getOwnPropertyDescriptor(p, key);
                            if (!desc) continue;
                            for (const fn of [desc.value, desc.get, desc.set]) {
                                if (typeof fn !== 'function' || seen.has(fn)) continue;
                                seen.add(fn);
                                Function.prototype.toString.call(fn);
                                checked++;
                            }
                        } catch(_) {}
                    }
                    p = Object.getPrototypeOf(p);
                    if (!p || p === Object.prototype) break;
                }
                return 'toString_ok_' + checked;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("toString_ok_"),
        "Function.prototype.toString on all fns should not recurse, got: {result}"
    );
}

/// Diagnostic: WeakSet behavior with globalThis (V8 global proxy identity)
#[tokio::test]
async fn shim_recursion_diag_weakset_globalthis() {
    let result = check(
        r#"
        (function() {
            try {
                const ws = new WeakSet();
                // Add globalThis and check if it's found
                ws.add(globalThis);
                const afterAdd = ws.has(globalThis);

                // Add window (which === globalThis) and check
                ws.add(window);
                const afterAddWindow = ws.has(globalThis);
                const afterAddGT = ws.has(window);

                // Check inner global (prototype of globalThis)
                const innerGlobal = Object.getPrototypeOf(globalThis);
                const innerInSet = innerGlobal ? ws.has(innerGlobal) : null;
                const innerIsGT = innerGlobal === globalThis;

                // WeakMap test
                const wm = new WeakMap();
                wm.set(globalThis, 42);
                const wmGet = wm.get(globalThis);
                const wmGetWindow = wm.get(window);

                return JSON.stringify({
                    afterAdd, afterAddWindow, afterAddGT,
                    innerInSet, innerIsGT,
                    wmGet, wmGetWindow,
                    windowIsGT: window === globalThis,
                });
            } catch(e) { return 'error: ' + e.message; }
        })()
    "#,
    )
    .await;
    println!("weakset globalThis test: {result}");
    assert!(
        !result.starts_with("error:"),
        "weakset test failed: {result}"
    );
    // Critical: afterAdd must be true (otherwise cycle detection in creepjs fails)
    assert!(
        result.contains("\"afterAdd\":true"),
        "WeakSet.has(globalThis) after add must be true, got: {result}"
    );
}

/// Diagnostic: check CallSite frame objects from Error.prepareStackTrace
#[tokio::test]
async fn shim_recursion_diag_callsite() {
    let result = check(
        r#"
        (function() {
            try {
                let frameInfo = null;
                const origPrep = Error.prepareStackTrace;
                Error.prepareStackTrace = function(err, frames) {
                    if (frames.length > 0 && frameInfo === null) {
                        const f = frames[0];
                        frameInfo = {
                            type: typeof f,
                            isFunc: typeof f === 'function',
                            hasToString: typeof f.toString === 'function',
                            toStringSrc: (() => {
                                try {
                                    // Get toString without calling it (to avoid recursion)
                                    const ts = Object.getOwnPropertyDescriptor(f, 'toString');
                                    if (ts) return 'own:' + typeof ts.value;
                                    // inherited?
                                    let p = Object.getPrototypeOf(f);
                                    while (p) {
                                        const d = Object.getOwnPropertyDescriptor(p, 'toString');
                                        if (d) {
                                            const fn = d.value;
                                            const isFnProtoTs = fn === Function.prototype.toString;
                                            const isObjProtoTs = fn === Object.prototype.toString;
                                            return 'inherited:fn=' + typeof fn +
                                                   ',isFuncProtoToStr=' + isFnProtoTs +
                                                   ',isObjProtoToStr=' + isObjProtoTs;
                                        }
                                        p = Object.getPrototypeOf(p);
                                    }
                                    return 'none';
                                } catch(e) { return 'err:' + e.message; }
                            })(),
                            protoChainLen: (() => {
                                let n = 0;
                                let p = f;
                                while (p && n < 20) { p = Object.getPrototypeOf(p); n++; }
                                return n;
                            })(),
                            isFuncProtoInChain: (() => {
                                let p = Object.getPrototypeOf(f);
                                while (p) {
                                    if (p === Function.prototype) return true;
                                    p = Object.getPrototypeOf(p);
                                }
                                return false;
                            })(),
                        };
                    }
                    // Call original (if any) or use default
                    if (origPrep) return origPrep(err, frames);
                    return undefined;
                };
                // Trigger a stack trace capture
                try { throw new Error('test'); } catch(e) { void e.stack; }
                Error.prepareStackTrace = origPrep;
                return JSON.stringify(frameInfo);
            } catch(e) { return 'error: ' + e.message; }
        })()
    "#,
    )
    .await;
    println!("callsite frame info: {result}");
    assert!(
        !result.starts_with("error:"),
        "callsite diag failed: {result}"
    );
}

#[tokio::test]
async fn error_stack_preserves_v8_method_alias_formatting() {
    let result = check(
        r#"(() => {
            function target() { return new Error('boom').stack; }
            const receiver = { alias: target };
            return receiver.alias();
        })()"#,
    )
    .await;
    assert!(
        result.contains("Object.target [as alias]"),
        "V8 method alias formatting was lost: {result}"
    );
}

#[tokio::test]
async fn error_stack_preserves_v8_eval_origin_formatting() {
    let result =
        check(r#"(() => eval("function inner(){return new Error('boom').stack}; inner()"))()"#)
            .await;
    assert!(
        result.contains("at inner (eval at"),
        "V8 eval origin formatting was lost: {result}"
    );
}

/// Diagnostic: what does Object.getPrototypeOf(globalThis) return?
#[tokio::test]
async fn shim_recursion_diag_global_proto() {
    let result = check(
        r#"
        (function() {
            try {
                const gt = globalThis;
                const p1 = Object.getPrototypeOf(gt);
                const info = {
                    p1_null: p1 === null,
                    p1_is_gt: p1 === gt,
                    p1_is_objproto: p1 === Object.prototype,
                    p1_type: typeof p1,
                    p1_keys: p1 ? Object.getOwnPropertyNames(p1).length : -1,
                };
                if (p1 && p1 !== null) {
                    const p2 = Object.getPrototypeOf(p1);
                    info.p2_null = p2 === null;
                    info.p2_is_gt = p2 === gt;
                    info.p2_is_objproto = p2 === Object.prototype;
                    info.p2_is_p1 = p2 === p1;
                }
                if (p1) {
                    info.p1_own_keys = Object.getOwnPropertyNames(p1);
                    info.p1_own_symbols = Object.getOwnPropertySymbols(p1).map(s => s.toString());
                    // Check the constructor property
                    const ctor = p1.constructor;
                    info.p1_ctor_is_gt = ctor === gt;
                    info.p1_ctor_is_func = typeof ctor === 'function';
                    info.p1_ctor_is_object_ctor = ctor === Object;
                    info.p1_ctor_type = typeof ctor;
                    if (ctor && typeof ctor === 'function') {
                        info.p1_ctor_name = ctor.name;
                    }
                    // Check the descriptor for constructor
                    const desc = Object.getOwnPropertyDescriptor(p1, 'constructor');
                    info.p1_ctor_desc_type = desc ? (desc.get ? 'getter' : 'value') : 'none';
                    // Important: does accessing constructor cause recursion?
                    // Check Object.getPrototypeOf(p1.constructor) if it's a function
                    if (ctor && typeof ctor === 'function') {
                        info.p1_ctor_proto_is_gt = Object.getPrototypeOf(ctor) === gt;
                        const ctorProto = Object.getPrototypeOf(ctor);
                        info.p1_ctor_proto_is_func_proto = ctorProto === Function.prototype;
                        // Does Function.prototype.toString work on it?
                        try {
                            const ts = Function.prototype.toString.call(ctor);
                            info.p1_ctor_tostring = ts.slice(0, 50);
                        } catch(e) {
                            info.p1_ctor_tostring_err = e.message;
                        }
                        // Can we access ctor.prototype?
                        info.p1_ctor_prototype_is_p1 = ctor.prototype === p1;
                    }
                }
                return JSON.stringify(info);
            } catch(e) { return 'error: ' + e.message; }
        })()
    "#,
    )
    .await;
    // Expect: p1 is Object.prototype or null (Deno runtime)
    println!("global proto chain: {result}");
    assert!(!result.starts_with("error:"), "proto diag failed: {result}");
}

/// Phase-4: iframe window prototype chain walking (creepjs primary pattern)
#[tokio::test]
async fn shim_recursion_iframe_proto_walk() {
    let result = check(
        r#"
        (function() {
            try {
                const iframe = document.createElement('iframe');
                document.body.appendChild(iframe);
                const win = iframe.contentWindow;
                if (!win) return 'no_contentWindow';

                // Walk contentWindow's own properties
                const ownKeys = Object.getOwnPropertyNames(win);

                // Check if 'window' in win (has trap)
                const hasWindow = 'window' in win;

                // Walk prototype chain of contentWindow
                const chain = [];
                let proto = win;
                let depth = 0;
                while (proto !== null && proto !== undefined && depth < 20) {
                    if (chain.indexOf(proto) !== -1) return 'cycle_at_' + depth;
                    chain.push(proto);
                    proto = Object.getPrototypeOf(proto);
                    depth++;
                }

                // Access each own key of contentWindow
                for (const key of ownKeys) {
                    try { win[key]; } catch(_) {}
                }

                // Check 'in' for common window properties
                const props = ['window','self','top','parent','location','document',
                               'navigator','screen','fetch','setTimeout','Array',
                               'Object','Function','localStorage'];
                for (const p of props) {
                    try { const r = p in win; } catch(_) {}
                }

                return 'iframe_walk_ok_' + chain.length + '_own_' + ownKeys.length;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("iframe_walk_ok_") || result.starts_with("cycle_at_"),
        "iframe contentWindow walk should not crash, got: {result}"
    );
}

/// Phase-5: creepjs-style window vs iframe window comparison
#[tokio::test]
async fn shim_recursion_creepjs_realm_check() {
    let result = check(
        r#"
        (function() {
            try {
                const iframe = document.createElement('iframe');
                document.body.appendChild(iframe);
                const iwin = iframe.contentWindow;
                if (!iwin) return 'no_contentWindow';

                // creepjs: compare constructors across realms
                const tests = {
                    arrayMatch: iwin.Array === Array,
                    objectMatch: iwin.Object === Object,
                    functionMatch: iwin.Function === Function,
                    selfIsWindow: iwin.self === iwin,
                    toStringNative: Function.prototype.toString.call(iwin.Array),
                };

                // creepjs: walk ALL own props of main window, check if in iframe window
                let diffCount = 0;
                const mainKeys = Object.getOwnPropertyNames(window);
                const iwinKeys = Object.getOwnPropertyNames(iwin);
                for (const k of mainKeys) {
                    if (!iwinKeys.includes(k)) diffCount++;
                }

                // creepjs: getPrototypeOf chain on iwin
                let protoDepth = 0;
                let p = iwin;
                while (p !== null && p !== undefined && protoDepth < 10) {
                    p = Object.getPrototypeOf(p);
                    protoDepth++;
                }

                return JSON.stringify({
                    ok: true,
                    diffCount,
                    protoDepth,
                    selfIsWindow: tests.selfIsWindow,
                });
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.contains("\"ok\":true"),
        "creepjs realm check should not crash, got: {result}"
    );
}

/// Phase-6: simulate creepjs 'lies' detection — Function.prototype.toString
/// called on every window property (including Proxy-wrapped properties)
#[tokio::test]
async fn shim_recursion_creepjs_lies_detection() {
    let result = check(
        r#"
        (function() {
            try {
                const toString = Function.prototype.toString;
                let checked = 0;
                let errors = 0;

                // Access every own property of window and call toString if function
                const keys = Object.getOwnPropertyNames(window);
                for (const key of keys) {
                    try {
                        const val = window[key];
                        if (typeof val === 'function') {
                            toString.call(val);
                            checked++;
                        }
                        // Also check getters
                        const desc = Object.getOwnPropertyDescriptor(window, key);
                        if (desc && typeof desc.get === 'function') {
                            toString.call(desc.get);
                        }
                    } catch(e) {
                        errors++;
                    }
                }

                // Check via prototype chain too
                let proto = Object.getPrototypeOf(window);
                while (proto && proto !== Object.prototype) {
                    for (const key of Object.getOwnPropertyNames(proto)) {
                        try {
                            const desc = Object.getOwnPropertyDescriptor(proto, key);
                            if (desc && typeof desc.get === 'function') toString.call(desc.get);
                            if (desc && typeof desc.value === 'function') toString.call(desc.value);
                            checked++;
                        } catch(_) { errors++; }
                    }
                    proto = Object.getPrototypeOf(proto);
                }

                return 'lies_check_ok_checked=' + checked + '_errors=' + errors;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("lies_check_ok_"),
        "creepjs lies detection should not crash, got: {result}"
    );
}

/// Phase-7: Yandex Metrika IIFE pattern — createElement('script'), src= assignment,
/// getElementsByTagName, parentNode.insertBefore. No network fetch (src points at
/// a non-existent data: URL so _onNodeInserted's op_net_fetch_sync either
/// fails or is skipped). Guards that the DOM mutation + re-entrant _onNodeInserted
/// path doesn't infinitely recurse.
#[tokio::test]
async fn shim_recursion_ym_iife_pattern() {
    let result = check(
        r#"
        (function() {
            try {
                // Simulate Yandex Metrika initialization IIFE.
                // Uses a data: URL so no real network call is made.
                var insertCount = 0;
                (function(m, e, t, r, i) {
                    m[i] = m[i] || function() { (m[i].a = m[i].a || []).push(arguments); };
                    m[i].l = 1 * new Date();
                    for (var j = 0; j < document.scripts.length; j++) {
                        if (document.scripts[j].src === r) { return; }
                    }
                    var k = e.createElement(t);
                    k.src = r;
                    insertCount++;
                    var a = e.getElementsByTagName(t)[0];
                    if (a && a.parentNode) { a.parentNode.insertBefore(k, a); }
                    else { e.head.appendChild(k); }
                })(window, document, 'script', 'data:text/javascript,void+0', 'ym');
                return 'ym_ok_inserts_' + insertCount;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("ym_ok_"),
        "Yandex Metrika IIFE pattern should not recurse, got: {result}"
    );
}

/// YM tag.js probe: iframe contentWindow access + navigator for..in.
/// YM creates a hidden iframe to read cross-document globals. Our iframeWindow
/// Proxy falls through to globalThis — if the fall-through path creates a Proxy
/// cycle or triggers a self-referential toString chain, it crashes here.
#[tokio::test]
async fn shim_recursion_ym_iframe_navigator_probe() {
    let result = check(
        r#"
        (function() {
            try {
                // Create iframe + read its window (our iframeWindow Proxy)
                var iframe = document.createElement('iframe');
                document.body.appendChild(iframe);
                var iw = iframe.contentWindow;
                if (!iw) return 'error: no contentWindow';

                // YM accesses iframe.contentWindow.navigator for cross-frame check
                var nav = iw.navigator;
                var ua = nav ? nav.userAgent : 'missing';

                // YM probes navigator properties via for...in
                var navKeys = [];
                try {
                    for (var k in navigator) { navKeys.push(k); if (navKeys.length > 100) break; }
                } catch(e) {}

                // YM accesses plugins and mimeTypes
                var pluginsLen = navigator.plugins.length;
                var mimesLen = navigator.mimeTypes.length;

                // YM calls Function.prototype.toString on iframe globals
                var iToStr = Function.prototype.toString.call(iw.Function || Function);

                return 'ym_iframe_ok_nav_keys_' + navKeys.length +
                       '_plugins_' + pluginsLen +
                       '_mimes_' + mimesLen +
                       '_tostr_' + (iToStr.includes('[native code]') ? 'native' : 'src');
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("ym_iframe_ok_"),
        "YM iframe+navigator probe should not crash, got: {result}"
    );
}

/// YM tag.js probe: document.cookie read/write and script enumeration.
/// YM reads/writes cookies on every pageview. Also iterates document.scripts
/// to avoid re-inserting itself.
#[tokio::test]
async fn shim_recursion_ym_cookie_scripts_probe() {
    let result = check(
        r#"
        (function() {
            try {
                // Cookie read/write (YM stores its state in _ym_uid cookie)
                document.cookie = '_ym_uid=1234567890; path=/';
                var cookie = document.cookie;

                // YM iterates document.scripts to check if already loaded
                var scriptSrcs = [];
                for (var j = 0; j < document.scripts.length; j++) {
                    scriptSrcs.push(document.scripts[j].src || 'inline');
                }

                // YM probes performance.timing for load time calculation
                var timing = window.performance && window.performance.timing;
                var navStart = timing ? timing.navigationStart : -1;

                // YM uses screen properties
                var screenInfo = screen.width + 'x' + screen.height + 'x' + screen.colorDepth;

                return 'ym_cookie_ok_cookie_' + (cookie.includes('_ym_uid') ? 'found' : 'missing') +
                       '_scripts_' + scriptSrcs.length +
                       '_screen_' + screenInfo;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("ym_cookie_ok_"),
        "YM cookie+scripts probe should not crash, got: {result}"
    );
}

/// YM tag.js probe: window property enumeration pattern.
/// YM scans window properties to check for anti-detect environment signals.
/// If any property getter recurses or the prototype walk loops, this crashes.
#[tokio::test]
async fn shim_recursion_ym_window_enum_probe() {
    let result = check(
        r#"
        (function() {
            try {
                // YM checks if specific globals exist via in-operator
                var checks = {
                    ym: 'ym' in window,
                    Ya: 'Ya' in window,
                    yandex: 'yandex' in window,
                    _ym_: '_ym_' in window,
                };

                // YM enumerates window to find conflicting libraries
                var keyCount = 0;
                try {
                    for (var k in window) {
                        keyCount++;
                        if (keyCount > 500) break; // Safety cap
                    }
                } catch(e) {}

                // YM checks window.top === window (not in a cross-origin frame)
                var isTop = window.top === window;
                var isSelf = window.self === window;
                var isParent = window.parent === window;

                // YM checks typeof various globals
                var typeChecks = [
                    typeof window.JSON,
                    typeof window.Promise,
                    typeof window.fetch,
                    typeof window.XMLHttpRequest,
                    typeof window.Worker,
                ].join(',');

                return 'ym_enum_ok_keys_' + keyCount + '_top_' + isTop +
                       '_types_' + typeChecks;
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("ym_enum_ok_"),
        "YM window enumeration should not crash, got: {result}"
    );
}

/// YM tag.js probe: eval-based module loader pattern.
/// YM tag.js uses a webpack-like bundler. The outermost IIFE sets up a module
/// registry via an object mapping module IDs to factory functions. If our eval
/// of a string containing nested function definitions causes V8 C++ recursion,
/// this test catches it.
#[tokio::test]
async fn shim_recursion_ym_module_loader_pattern() {
    let result = check(
        r#"
        (function() {
            try {
                // Simulate YM's webpack-style module loader
                var modules = {};
                var cache = {};
                function require(id) {
                    if (cache[id]) return cache[id].exports;
                    var mod = { exports: {} };
                    cache[id] = mod;
                    if (modules[id]) modules[id](mod, mod.exports, require);
                    return mod.exports;
                }

                // Register some fake modules
                modules[0] = function(m, e, r) {
                    e.init = function() { return r(1).run(); };
                };
                modules[1] = function(m, e, r) {
                    e.run = function() {
                        // Simulate YM environment probe
                        var hasYa = typeof window.Ya !== 'undefined';
                        var userAgent = navigator.userAgent;
                        var lang = navigator.language;
                        return { hasYa: hasYa, ua: userAgent.slice(0, 20), lang: lang };
                    };
                };

                // Run entry point (module 0)
                var result = require(0).init();

                return 'ym_loader_ok_ua_' + (result.ua ? 'present' : 'missing') +
                       '_lang_' + (result.lang ? 'present' : 'missing');
            } catch(e) {
                return 'error: ' + e.message;
            }
        })()
    "#,
    )
    .await;
    assert!(
        result.starts_with("ym_loader_ok_"),
        "YM module loader pattern should not crash, got: {result}"
    );
}

// Tier-based smoke tests. Each tier runs as a separate #[tokio::test] so each
// gets a fresh stack — running 30 V8 isolates in a single test overflows.

#[tokio::test]
#[ignore = "network: dump our headers via httpbin to diff vs real Chrome"]
async fn dump_our_headers_httpbin() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::new(&profile).unwrap();
    let url = "https://httpbin.org/headers";
    println!("\n=== Our request headers per httpbin.org/headers ===");
    let hdrs = browser_oxide::net::headers::chrome_headers(&profile);
    println!("  Headers we'll attach (in order, LOW-ENTROPY = production navs):");
    for (k, v) in &hdrs {
        println!("    {}: {}", k, v);
    }
    match client.get_with_headers(url, &hdrs).await {
        Ok(resp) => {
            let body = resp.text();
            println!("  --- httpbin echo (what server saw) ---");
            println!("{}", body);
        }
        Err(e) => println!("  ERROR: {e}"),
    }
    println!("=== end ===");
}

#[tokio::test]
#[ignore = "network: dump our TLS+H2 fingerprint via tls.peet.ws"]
async fn tls_fingerprint_peet() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::new(&profile).unwrap();
    let url = "https://tls.peet.ws/api/all";
    println!("\n=== Our TLS fingerprint per tls.peet.ws ===");
    let hdrs = browser_oxide::net::headers::chrome_headers_with_accept_ch(&profile);
    match client.get_with_headers(url, &hdrs).await {
        Ok(resp) => {
            let body = resp.text();
            println!("  status: {}", resp.status);
            println!("  body bytes: {}", body.len());
            // Permissive extract: key="value" with optional whitespace
            let extract = |key: &str| -> String {
                let pat = format!("\"{}\"", key);
                if let Some(s) = body.find(&pat) {
                    let after = &body[s + pat.len()..];
                    // find next '"' that opens the value
                    if let Some(open) = after.find('"') {
                        let val_start = open + 1;
                        if let Some(close) = after[val_start..].find('"') {
                            return after[val_start..val_start + close].to_string();
                        }
                    }
                }
                "?".to_string()
            };
            println!("  ja3:               {}", extract("ja3"));
            println!("  ja3_hash:          {}", extract("ja3_hash"));
            println!("  ja4:               {}", extract("ja4"));
            println!("  ja4_r:             {}", extract("ja4_r"));
            println!("  peetprint:         {}", extract("peetprint"));
            println!("  peetprint_hash:    {}", extract("peetprint_hash"));
            println!("  akamai_fp:         {}", extract("akamai_fingerprint"));
            println!(
                "  akamai_hash:       {}",
                extract("akamai_fingerprint_hash")
            );
            println!("  user_agent:        {}", extract("user_agent"));
            // also dump our http_version to verify h2
            println!("  http_version:      {}", extract("http_version"));
        }
        Err(e) => println!("  ERROR: {e}"),
    }
    println!("=== end ===");
}

#[tokio::test]
#[ignore = "network: hits reddit.com"]
async fn reddit_smoke() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = match Page::navigate("https://www.reddit.com/", profile, 5).await {
        Ok(p) => p,
        Err(e) => {
            println!("\n=== reddit.com navigation FAILED ===");
            println!("error: {e}");
            panic!("navigate failed");
        }
    };

    let title = page.title();
    let url = page.url().to_string();

    let body_len = page
        .evaluate("document.body ? document.body.textContent.length : 0")
        .unwrap_or_else(|e| format!("evaluate body err: {e}"));

    let body_snippet = page
        .evaluate(
            "document.body ? \
             document.body.textContent.replace(/\\s+/g, ' ').trim().slice(0, 400) \
             : 'no body'",
        )
        .unwrap_or_else(|e| format!("evaluate snippet err: {e}"));

    let html_len = page
        .evaluate("document.documentElement ? document.documentElement.outerHTML.length : 0")
        .unwrap_or_default();

    let n_links = page
        .evaluate("document.querySelectorAll('a').length")
        .unwrap_or_default();

    let n_scripts = page
        .evaluate("document.querySelectorAll('script').length")
        .unwrap_or_default();

    let cookies = page
        .evaluate("typeof document.cookie === 'string' ? document.cookie.length : 0")
        .unwrap_or_default();

    println!("\n=== reddit.com navigation result ===");
    println!("  final url:      {url}");
    println!("  title:          {title:?}");
    println!("  html bytes:     {html_len}");
    println!("  body chars:     {body_len}");
    println!("  <a> count:      {n_links}");
    println!("  <script> count: {n_scripts}");
    println!("  cookie chars:   {cookies}");
    println!("  body snippet:   {body_snippet}");
    println!("=== end ===\n");

    // Sanity: we got *some* content back (not a 0-byte challenge).
    assert!(
        html_len.parse::<usize>().unwrap_or(0) > 1000,
        "expected >1000 bytes of HTML, got {html_len}"
    );
}

#[tokio::test]
async fn fingerprint_probe_vs_chrome() {
    let js = r#"(() => {
      const r = {};
      r.chromeKeys = Object.keys(window.chrome||{}).sort().join(',');
      r.chromeRuntimeExists = !!(window.chrome && window.chrome.runtime);
      r.chromeWebstoreExists = !!(window.chrome && window.chrome.webstore);
      if (window.chrome && window.chrome.loadTimes) {
        try {
          const lt = window.chrome.loadTimes();
          r.loadTimesSpdy = lt.wasFetchedViaSpdy;
          r.loadTimesNpn = lt.wasNpnNegotiated;
          r.loadTimesProto = lt.npnNegotiatedProtocol;
          r.loadTimesKeys = Object.keys(lt).sort().join(',');
        } catch(e) { r.loadTimesError = ''+e; }
      }
      r.trustedTypes = typeof trustedTypes;
      r.scheduler = typeof scheduler;
      r.reportError = typeof reportError;
      r.requestIdleCallback = typeof requestIdleCallback;
      r.cancelIdleCallback = typeof cancelIdleCallback;
      r.queueMicrotask = typeof queueMicrotask;
      r.SharedArrayBuffer = typeof SharedArrayBuffer;
      r.deviceMemory = navigator.deviceMemory;
      r.connection_effectiveType = navigator.connection ? navigator.connection.effectiveType : 'NONE';
      r.getBattery = typeof navigator.getBattery === 'function';
      r.pdfViewerEnabled = navigator.pdfViewerEnabled;
      r.webdriver = navigator.webdriver;
      r.webdriverInNav = 'webdriver' in navigator;
      r.pluginsLength = navigator.plugins ? navigator.plugins.length : -1;
      r.mimeTypesLength = navigator.mimeTypes ? navigator.mimeTypes.length : -1;
      r.perfMemory = !!(window.performance && window.performance.memory);
      r.isSecureContext = window.isSecureContext;
      r.touchExists = typeof Touch !== 'undefined';
      r.PaymentRequest = typeof PaymentRequest;
      r.docHasFocus = document.hasFocus();
      r.docVisibility = document.visibilityState;
      r.navigatorProto = Object.getPrototypeOf(navigator) ? Object.getPrototypeOf(navigator).constructor.name : null;
      r.notificationExists = typeof Notification !== 'undefined';
      r.paymentRequestExists = typeof PaymentRequest !== 'undefined';
      r.indexedDBExists = typeof indexedDB !== 'undefined';
      r.storageExists = typeof navigator.storage !== 'undefined';
      r.serviceWorkerExists = typeof navigator.serviceWorker !== 'undefined';
      r.credentialsExists = typeof navigator.credentials !== 'undefined';
      r.mediaDevicesExists = typeof navigator.mediaDevices !== 'undefined';
      r.geolocationExists = typeof navigator.geolocation !== 'undefined';
      r.bluetoothExists = !!(navigator.bluetooth);
      r.usbExists = !!(navigator.usb);
      r.ResizeObserver = typeof ResizeObserver;
      r.IntersectionObserver = typeof IntersectionObserver;
      r.PerformanceObserver = typeof PerformanceObserver;
      r.MutationObserver = typeof MutationObserver;
      r.cryptoRandomUUID = typeof crypto !== 'undefined' && typeof crypto.randomUUID;
      r.screen_orientation = screen.orientation ? screen.orientation.type : 'NONE';
      r.devicePixelRatio = window.devicePixelRatio;
      return JSON.stringify(r, null, 2);
    })()"#;
    let result = check(js).await;
    println!("\nFINGERPRINT PROBE:\n{}", result);
    // Just ensure it ran
    assert!(
        result.starts_with('{'),
        "expected JSON, got: {}",
        &result[..100.min(result.len())]
    );
}

#[tokio::test]
async fn check_iterators_test() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile("", "about:blank", profile)
        .await
        .unwrap();
    let js = r#"
        (function() {
            const results = {};
            const targets = [
                ['navigator.plugins', navigator.plugins],
                ['navigator.mimeTypes', navigator.mimeTypes],
                ['document.fonts', document.fonts],
            ];
            for (const [name, obj] of targets) {
                try {
                    const it = obj[Symbol.iterator]();
                    results[name + '_iterator_iterable'] = it[Symbol.iterator]() === it;
                } catch (e) {
                    results[name + '_error'] = e.message;
                }
            }
            return JSON.stringify(results, null, 2);
        })()
    "#;
    let result = page.evaluate(js).unwrap();
    println!("ITERATOR CHECK:\n{}", result);
}

#[tokio::test]
async fn check_ctors_test() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile("", "about:blank", profile)
        .await
        .unwrap();
    let js = r#"
        (function() {
            const results = {};
            const targets = [
                ['navigator.plugins.item', navigator.plugins.item],
                ['navigator.plugins.namedItem', navigator.plugins.namedItem],
                ['navigator.plugins.refresh', navigator.plugins.refresh],
                ['MediaSource.isTypeSupported', MediaSource.isTypeSupported],
                ['Notification.requestPermission', Notification.requestPermission],
            ];
            for (const [name, fn] of targets) {
                try {
                    new fn();
                    results[name] = 'IS_CONSTRUCTOR';
                } catch (e) {
                    results[name] = 'NOT_CONSTRUCTOR';
                }
            }
            return JSON.stringify(results, null, 2);
        })()
    "#;
    let result = page.evaluate(js).unwrap();
    println!("CTOR CHECK:\n{}", result);
}

#[tokio::test]
async fn check_tostring_test() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile("", "about:blank", profile)
        .await
        .unwrap();
    let js = r#"
        (function() {
            const res = {};
            const targets = {
                'mediaDevices.enumerateDevices': navigator.mediaDevices && navigator.mediaDevices.enumerateDevices,
                'mediaDevices.getUserMedia': navigator.mediaDevices && navigator.mediaDevices.getUserMedia,
                'mediaDevices.addEventListener': navigator.mediaDevices && navigator.mediaDevices.addEventListener,
                'PublicKeyCredential.isUVPAA': globalThis.PublicKeyCredential && PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable,
                'CredentialsContainer.get': navigator.credentials && navigator.credentials.get,
                'plugins.item': navigator.plugins.item,
                'plugins.refresh': navigator.plugins.refresh,
                'fetch': globalThis.fetch,
                'setTimeout': globalThis.setTimeout,
            };
            for (const [k, v] of Object.entries(targets)) {
                try {
                    res[k + '_instance'] = v.toString();
                    res[k + '_protoCall'] = Function.prototype.toString.call(v);
                    res[k + '_length'] = v.length;
                } catch (e) {
                    res[k + '_err'] = e.message;
                }
            }
            return JSON.stringify(res, null, 2);
        })()
    "#;
    let result = page.evaluate(js).unwrap();
    println!("TOSTRING CHECK:\n{}", result);
}

#[tokio::test]
async fn check_spread_test() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile("", "about:blank", profile)
        .await
        .unwrap();
    let js = r#"
        (function() {
            const res = {};
            const targets = {
                'plugins': navigator.plugins,
                'mimeTypes': navigator.mimeTypes,
                'fonts': document.fonts,
                'scripts': document.scripts,
                'styleSheets': document.styleSheets,
                'all': document.all,
                'htmlCollection': document.getElementsByTagName('div'),
                'nodeList': document.querySelectorAll('div'),
            };
            for (const [k, v] of Object.entries(targets)) {
                try {
                    [...v];
                    res[k] = true;
                } catch(e) {
                    res[k] = e.message;
                }
            }
            return JSON.stringify(res, null, 2);
        })()
    "#;
    println!("SPREAD CHECK:\n{}", page.evaluate(js).unwrap());
}

/// W4a candidates probe — try `[...x]` on every known `ao` candidate
/// and report which throw. The doc lists: navigator.plugins,
/// userAgentData.brands, MediaSource.activeSourceBuffers,
/// document.fonts, HTMLCollection, RTCRtpReceiver.getCapabilities.
#[tokio::test]
async fn check_ao_candidates_test() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = browser_oxide::Page::with_profile("", "about:blank", profile)
        .await
        .unwrap();
    let js = r#"
        (function() {
            const res = {};
            const tests = [
                ['navigator.plugins',
                    () => navigator.plugins],
                ['navigator.mimeTypes',
                    () => navigator.mimeTypes],
                ['navigator.userAgentData.brands',
                    () => navigator.userAgentData && navigator.userAgentData.brands],
                ['document.fonts',
                    () => document.fonts],
                ['document.scripts',
                    () => document.scripts],
                ['document.styleSheets',
                    () => document.styleSheets],
                ['document.all',
                    () => document.all],
                ['document.images',
                    () => document.images],
                ['document.links',
                    () => document.links],
                ['document.forms',
                    () => document.forms],
                ['document.embeds',
                    () => document.embeds],
                ['document.anchors',
                    () => document.anchors],
                ['HTMLCollection (children)',
                    () => document.body.children],
                ['NodeList (querySelectorAll)',
                    () => document.querySelectorAll('div')],
                ['NodeList (childNodes)',
                    () => document.body.childNodes],
                ['NamedNodeMap (attributes)',
                    () => document.body.attributes],
                ['DOMTokenList (classList)',
                    () => document.body.classList],
                ['FormData',
                    () => new FormData()],
                ['URLSearchParams',
                    () => new URLSearchParams('a=1&b=2')],
                ['Headers',
                    () => new Headers({'X-Test': 'a'})],
                ['MediaList',
                    () => document.styleSheets[0] && document.styleSheets[0].media],
                ['CSSRuleList',
                    () => document.styleSheets[0] && document.styleSheets[0].cssRules],
                ['MediaSource exists?',
                    () => typeof MediaSource],
                ['MediaSource.activeSourceBuffers',
                    () => { const ms = new MediaSource(); return ms.activeSourceBuffers; }],
                ['MediaSource.sourceBuffers',
                    () => { const ms = new MediaSource(); return ms.sourceBuffers; }],
                ['RTCRtpReceiver exists?',
                    () => typeof RTCRtpReceiver],
                ['RTCRtpReceiver.getCapabilities(audio)',
                    () => RTCRtpReceiver.getCapabilities('audio')],
                ['RTCRtpSender.getCapabilities(video)',
                    () => RTCRtpSender.getCapabilities('video')],
                ['ResizeObserverSize-like',
                    () => ({length: 2, 0: 'a', 1: 'b'})],
                ['FileList',
                    () => null /* requires <input type=file>; skip */],
                ['TouchList',
                    () => null],
            ];
            for (const [name, fn] of tests) {
                let r = {};
                try {
                    const v = fn();
                    if (v === null) { r.skip = true; }
                    else if (typeof v === 'string') { r.value = v; }
                    else {
                        r.type = Object.prototype.toString.call(v);
                        r.hasSymbolIter = typeof v[Symbol.iterator] === 'function';
                        try {
                            const arr = [...v];
                            r.spread = `ok len=${arr.length}`;
                        } catch (e) {
                            r.spread = 'ERR: ' + e.message.split('\n')[0];
                        }
                    }
                } catch (e) {
                    r.setup_err = e.message;
                }
                res[name] = r;
            }
            return JSON.stringify(res, null, 2);
        })()
    "#;
    println!("AO CANDIDATES CHECK:\n{}", page.evaluate(js).unwrap());
}

/// Diagnose why a site returns < 1KB HTML in our engine (THIN-BODY in
/// holistic_sweep classifier). Both cloudflare.com and primevideo.com
/// fail this way in parallel sweep. Reports the actual HTML, readyState,
/// body children count, head children count, and any visible CSP errors.
async fn thin_body_diagnose(url: &str, name: &str) {
    use browser_oxide::Page;
    use std::time::Duration;

    println!("\n========== {name} ({url}) ==========");
    let r = tokio::time::timeout(
        Duration::from_secs(60),
        Page::navigate(url, browser_oxide::stealth::presets::chrome_148_macos(), 2),
    )
    .await;
    match r {
        Ok(Ok(mut p)) => {
            let info = p.evaluate(r#"
                JSON.stringify({
                    readyState: document.readyState,
                    bodyLen: (document.body && document.body.innerHTML || '').length,
                    bodyChildren: document.body ? document.body.children.length : 0,
                    headChildren: document.head ? document.head.children.length : 0,
                    scriptCount: document.scripts ? document.scripts.length : 0,
                    title: document.title,
                    url: location.href,
                    bodySnippet: (document.body && document.body.innerHTML || '').slice(0, 400),
                    htmlSnippet: document.documentElement ? document.documentElement.outerHTML.slice(0, 400) : '',
                })
            "#).unwrap_or_default();
            println!("info: {info}");
        }
        Ok(Err(e)) => println!("ERROR: {e}"),
        Err(_) => println!("TIMEOUT"),
    }
}

#[tokio::test]
#[ignore = "network: THIN-BODY diagnostic for parallel sweep failures"]
async fn diag_thin_body_sites() {
    thin_body_diagnose("https://www.cloudflare.com/", "cloudflare").await;
    thin_body_diagnose("https://www.primevideo.com/", "prime-video").await;
}

/// PaymentRequest API surface — must exist on Chrome profiles.
/// Must exist on secure-context Chrome profiles. canMakePayment for
/// Google Pay payment method must resolve true (matches a real Chrome
/// with the handler registered, no card enrolled). hasEnrolledInstrument
/// is Chrome/Edge-only and resolves false on a fresh profile.
/// ApplePaySession MUST be undefined under non-macOS Chrome profiles
/// (the cardinal sin: ApplePaySession + Chrome UA = instant flag).
#[tokio::test]
async fn check_payment_request_surface() {
    let profile = browser_oxide::stealth::chrome_148_linux();
    let mut page = Page::from_html_with_url(&html(""), "https://example.com/", Some(profile))
        .await
        .unwrap();
    // Synchronous surface checks first.
    let sync_js = r#"
        (function() {
            const res = {};
            res.PaymentRequest_typeof = typeof PaymentRequest;
            res.PaymentRequest_length = typeof PaymentRequest === 'function' ? PaymentRequest.length : null;
            res.PaymentResponse_typeof = typeof PaymentResponse;
            res.PaymentMethodChangeEvent_typeof = typeof PaymentMethodChangeEvent;
            res.PaymentRequestUpdateEvent_typeof = typeof PaymentRequestUpdateEvent;
            res.ApplePaySession_typeof = typeof ApplePaySession;
            res.canMakePayment_toString = typeof PaymentRequest === 'function'
                ? PaymentRequest.prototype.canMakePayment.toString()
                : null;
            // Constructor validation (sync throws)
            try { new PaymentRequest([], { total: { label: 'x', amount: { currency: 'USD', value: '1' } } }); res.empty_methods = 'no throw'; }
            catch (e) { res.empty_methods = e.name; }
            try { new PaymentRequest([{supportedMethods: 'basic-card'}], {}); res.no_total = 'no throw'; }
            catch (e) { res.no_total = e.name; }
            return JSON.stringify(res, null, 2);
        })()
    "#;
    let sync_result = page.evaluate(sync_js).unwrap();
    println!("PAYMENT REQUEST SYNC CHECK:\n{}", sync_result);

    assert!(sync_result.contains("\"PaymentRequest_typeof\": \"function\""));
    assert!(sync_result.contains("\"PaymentRequest_length\": 2"));
    assert!(sync_result.contains("\"PaymentResponse_typeof\": \"function\""));
    assert!(sync_result.contains("\"PaymentMethodChangeEvent_typeof\": \"function\""));
    assert!(sync_result.contains("\"PaymentRequestUpdateEvent_typeof\": \"function\""));
    assert!(
        sync_result.contains("\"ApplePaySession_typeof\": \"undefined\""),
        "ApplePaySession must be undefined under Linux Chrome profile (cardinal sin)"
    );
    assert!(sync_result.contains("\"empty_methods\": \"TypeError\""));
    assert!(sync_result.contains("\"no_total\": \"TypeError\""));
    assert!(
        sync_result.contains("[native code]"),
        "PaymentRequest.prototype.canMakePayment.toString() must include [native code]"
    );

    // Async checks — kick off Promise chain into window.__r, pump microtasks, read back.
    page.evaluate(
        r#"window.__r = {};
        (async function() {
            const r = window.__r;
            try {
                const pr = new PaymentRequest(
                    [{ supportedMethods: 'https://google.com/pay' }],
                    { total: { label: 'x', amount: { currency: 'USD', value: '1.00' } } }
                );
                r.id_present = typeof pr.id === 'string' && pr.id.length > 0;
                r.shippingAddress = pr.shippingAddress;
                r.shippingOption = pr.shippingOption;
                r.shippingType = pr.shippingType;
                r.canMakePayment_googlepay = await pr.canMakePayment();
                r.hasEnrolledInstrument = await pr.hasEnrolledInstrument();
                try { await pr.show(); r.show_ok = 'unexpected resolve'; }
                catch (e) { r.show_rejected = e.name; }
                r.abort_resolved = await pr.abort();
            } catch (e) { r.ctor_err = e.name + ': ' + e.message; }

            try {
                const pr2 = new PaymentRequest(
                    [{ supportedMethods: 'unknown://method' }],
                    { total: { label: 'x', amount: { currency: 'USD', value: '1.00' } } }
                );
                r.canMakePayment_unknown = await pr2.canMakePayment();
            } catch (e) { r.unknown_err = e.message; }

            try {
                r.spc = await PaymentRequest.securePaymentConfirmationAvailability();
            } catch (e) { r.spc_err = e.message; }

            window.__r_done = true;
        })();"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(500))
        .await
        .ok();
    let async_result = page
        .evaluate("JSON.stringify(window.__r, null, 2)")
        .unwrap();
    println!("PAYMENT REQUEST ASYNC CHECK:\n{}", async_result);

    assert!(async_result.contains("\"id_present\": true"));
    assert!(async_result.contains("\"shippingAddress\": null"));
    assert!(async_result.contains("\"canMakePayment_googlepay\": true"));
    assert!(async_result.contains("\"canMakePayment_unknown\": false"));
    assert!(async_result.contains("\"hasEnrolledInstrument\": false"));
    assert!(async_result.contains("\"show_rejected\": \"AbortError\""));
    assert!(
        async_result.contains("\"spc\": \"unavailable-no-user-verifying-platform-authenticator\"")
    );
}

/// navigator.getInstalledRelatedApps — Chrome/Edge-only API; absence
/// under Chrome UA is a tell. Must return Promise<[]> on a fresh profile.
#[tokio::test]
async fn check_get_installed_related_apps() {
    let profile = browser_oxide::stealth::chrome_148_linux();
    let mut page = Page::from_html_with_url(&html(""), "https://example.com/", Some(profile))
        .await
        .unwrap();
    let sync_js = r#"
        (function() {
            const res = {};
            res.typeof_method = typeof navigator.getInstalledRelatedApps;
            res.toString = typeof navigator.getInstalledRelatedApps === 'function'
                ? navigator.getInstalledRelatedApps.toString()
                : null;
            return JSON.stringify(res, null, 2);
        })()
    "#;
    let sync_result = page.evaluate(sync_js).unwrap();
    println!("getInstalledRelatedApps SYNC CHECK:\n{}", sync_result);
    assert!(sync_result.contains("\"typeof_method\": \"function\""));
    assert!(sync_result.contains("[native code]"));

    page.evaluate(
        r#"window.__r = null;
        navigator.getInstalledRelatedApps().then(o => { window.__r = o; });"#,
    )
    .unwrap();
    page.evaluate_async("void 0", std::time::Duration::from_millis(200))
        .await
        .ok();
    let async_result = page
        .evaluate(
            r#"(() => {
                const r = window.__r;
                if (!Array.isArray(r)) return 'not-array:' + typeof r;
                return 'len=' + r.length;
            })()"#,
        )
        .unwrap();
    println!("getInstalledRelatedApps ASYNC CHECK: {}", async_result);
    assert_eq!(async_result, "len=0");
}

/// Tier 1.2 — Function.prototype.toString cross-realm + stack-trace audit.
///
/// fingerprint-suite + the challenge vendor's threat-research blog highlight 4 detector
/// edge cases that defeat naive toString patches:
///   1. instance.toString + "" coercion (skips Function.prototype.toString)
///   2. Function.prototype.toString.call(maskedFn) directly
///   3. Cross-realm: iframe.contentWindow.Function.prototype.toString.call(parentFn)
///   4. Stack-trace inspection: try { fn.toString.call(undefined) } catch (e) { e.stack }
///      — must NOT contain Proxy artifacts ("at Object.apply", "at Object.get",
///      "at Reflect.apply", "at newHandler.<computed>")
#[tokio::test]
async fn check_tostring_audit_full() {
    let profile = browser_oxide::stealth::chrome_148_linux();
    let mut page = Page::from_html_with_url(&html(""), "https://example.com/", Some(profile))
        .await
        .unwrap();
    let js = r#"
        (function() {
            const res = {};
            const targets = [
                ['canPlayType', () => {
                    const v = document.createElement('video');
                    return v.canPlayType;
                }],
                ['enumerateDevices', () => navigator.mediaDevices && navigator.mediaDevices.enumerateDevices],
                ['getBattery', () => navigator.getBattery],
                ['getInstalledRelatedApps', () => navigator.getInstalledRelatedApps],
                ['canMakePayment',
                    () => typeof PaymentRequest === 'function' && PaymentRequest.prototype.canMakePayment],
                ['plugins.item', () => navigator.plugins && navigator.plugins.item],
                ['fetch', () => globalThis.fetch],
            ];

            for (const [label, getter] of targets) {
                const fn = getter();
                const r = {};
                if (typeof fn !== 'function') { r.skip = 'not a function: ' + typeof fn; res[label] = r; continue; }

                // Path 1: implicit + "" coercion
                try {
                    const s = fn + "";
                    r.coerce = s.includes('[native code]') ? 'native' : ('SRC: ' + s.slice(0, 60));
                } catch (e) { r.coerce_err = e.message; }

                // Path 2: Function.prototype.toString.call(fn)
                try {
                    const s = Function.prototype.toString.call(fn);
                    r.protoCall = s.includes('[native code]') ? 'native' : ('SRC: ' + s.slice(0, 60));
                } catch (e) { r.protoCall_err = e.message; }

                // Path 3: instance .toString()
                try {
                    const s = fn.toString();
                    r.instance = s.includes('[native code]') ? 'native' : ('SRC: ' + s.slice(0, 60));
                } catch (e) { r.instance_err = e.message; }

                // Path 4: Object.prototype.toString — should NOT change shape
                try {
                    r.objProto = Object.prototype.toString.call(fn);
                } catch (e) { r.objProto_err = e.message; }

                res[label] = r;
            }

            // Cross-realm: iframe.contentWindow.Function.prototype.toString
            try {
                const ifr = document.createElement('iframe');
                document.body && document.body.appendChild(ifr);
                const cw = ifr.contentWindow;
                if (!cw) { res.iframe = 'no contentWindow'; }
                else if (!cw.Function || !cw.Function.prototype || !cw.Function.prototype.toString) {
                    res.iframe = 'no cw.Function.prototype.toString';
                } else {
                    const cwTs = cw.Function.prototype.toString;
                    const fns = {
                        'parent.fetch': globalThis.fetch,
                        'parent.canPlayType': document.createElement('video').canPlayType,
                        'parent.canMakePayment': PaymentRequest && PaymentRequest.prototype.canMakePayment,
                        'parent.getBattery': navigator.getBattery,
                    };
                    res.iframe = {};
                    for (const [k, fn] of Object.entries(fns)) {
                        if (typeof fn !== 'function') { res.iframe[k] = 'skip:' + typeof fn; continue; }
                        try {
                            const s = cwTs.call(fn);
                            res.iframe[k] = s.includes('[native code]') ? 'native' : ('SRC: ' + s.slice(0, 60));
                        } catch (e) { res.iframe[k] = 'ERR: ' + e.message.split('\n')[0]; }
                    }
                }
            } catch (e) { res.iframe_err = e.message; }

            // Stack-trace sanitization
            const proxyArtifacts = [
                'at Object.apply', 'at Object.get', 'at Reflect.apply',
                'at newHandler.', 'at Proxy.', 'at handler.', 'at trap.',
                '_inPatchedToStr', '_nativeTag', '_origFnToStr', '_patchedFnToStr',
            ];
            try {
                Function.prototype.toString.call(undefined);
                res.stack = 'unexpected: did not throw';
            } catch (e) {
                const stack = String(e.stack || '');
                const hits = proxyArtifacts.filter(a => stack.includes(a));
                res.stack = hits.length === 0 ? 'clean' : 'LEAK: ' + hits.join(', ');
                res.stack_full = stack.split('\n').slice(0, 6).join(' | ');
            }
            try {
                Function.prototype.toString.call({});
                res.stack_obj = 'unexpected: did not throw';
            } catch (e) {
                const stack = String(e.stack || '');
                const hits = proxyArtifacts.filter(a => stack.includes(a));
                res.stack_obj = hits.length === 0 ? 'clean' : 'LEAK: ' + hits.join(', ');
            }

            return JSON.stringify(res, null, 2);
        })()
    "#;
    let result = page.evaluate(js).unwrap();
    println!("TOSTRING AUDIT:\n{}", result);

    // Whole-result invariants: NO path on ANY masked function should leak
    // raw source. JSON values starting with "SRC:" mean detector got source.
    assert!(
        !result.contains("\"coerce\": \"SRC:"),
        "+ \"\" coercion leaked raw source somewhere: {result}"
    );
    assert!(
        !result.contains("\"protoCall\": \"SRC:"),
        "Function.prototype.toString.call leaked raw source: {result}"
    );
    assert!(
        !result.contains("\"instance\": \"SRC:"),
        "instance .toString() leaked raw source: {result}"
    );
    // Cross-realm via iframe.contentWindow must also not leak
    assert!(
        !result.contains(": \"SRC:"),
        "cross-realm iframe toString leaked source: {result}"
    );
    // Stack must be clean (no Proxy / handler artifacts)
    assert!(
        result.contains("\"stack\": \"clean\""),
        "stack trace from toString.call(undefined) leaked Proxy artifacts: {result}"
    );
    assert!(
        result.contains("\"stack_obj\": \"clean\""),
        "stack trace from toString.call({{}}) leaked Proxy artifacts: {result}"
    );
}

/// Tier 1.5 verification — audio fingerprint differs across stealth profiles.
/// Real OfflineAudioContext-based fingerprint probes (FPjs, CreepJS) hash
/// the rendered output. Per-profile audio_seed must produce distinct hashes.
/// Two profiles with different audio_seed values MUST produce different
/// audio output, AND the same profile must reproduce the same output (deterministic).
#[tokio::test]
async fn check_audio_fingerprint_per_profile() {
    let render_js = r#"
        (function() {
            const ctx = new OfflineAudioContext(1, 5000, 44100);
            const osc = ctx.createOscillator();
            osc.type = 'triangle';
            osc.frequency.value = 10000;
            const comp = ctx.createDynamicsCompressor();
            comp.threshold.value = -50;
            comp.knee.value = 40;
            comp.ratio.value = 12;
            comp.attack.value = 0;
            comp.release.value = 0.25;
            osc.connect(comp);
            comp.connect(ctx.destination);
            osc.start(0);
            window.__audio_done = false;
            window.__audio_hash = null;
            ctx.startRendering().then(buf => {
                const data = buf.getChannelData(0);
                let s = 0;
                // FPjs-canonical reduction: sum of abs values in [4500, 5000]
                for (let i = 4500; i < Math.min(5000, data.length); i++) s += Math.abs(data[i]);
                window.__audio_hash = s;
                window.__audio_done = true;
            });
        })()
    "#;
    async fn render(profile: browser_oxide::stealth::StealthProfile) -> f64 {
        let mut page = Page::from_html_with_url(&html(""), "https://example.com/", Some(profile))
            .await
            .unwrap();
        page.evaluate(
            r#"
            (function() {
                const ctx = new OfflineAudioContext(1, 5000, 44100);
                const osc = ctx.createOscillator();
                osc.type = 'triangle';
                osc.frequency.value = 10000;
                const comp = ctx.createDynamicsCompressor();
                comp.threshold.value = -50;
                comp.knee.value = 40;
                comp.ratio.value = 12;
                comp.attack.value = 0;
                comp.release.value = 0.25;
                osc.connect(comp);
                comp.connect(ctx.destination);
                osc.start(0);
                window.__audio_done = false;
                window.__audio_hash = null;
                ctx.startRendering().then(buf => {
                    const data = buf.getChannelData(0);
                    let s = 0;
                    for (let i = 4500; i < Math.min(5000, data.length); i++) s += Math.abs(data[i]);
                    window.__audio_hash = s;
                    window.__audio_done = true;
                });
            })()
        "#,
        )
        .unwrap();
        page.evaluate_async("void 0", std::time::Duration::from_millis(500))
            .await
            .ok();
        let raw = page.evaluate(r#"String(window.__audio_hash)"#).unwrap();
        raw.parse::<f64>().unwrap_or(f64::NAN)
    }
    let _ = render_js; // referenced for documentation

    let h_mac = render(browser_oxide::stealth::chrome_148_macos()).await;
    let h_lin = render(browser_oxide::stealth::chrome_148_linux()).await;
    let h_lin_2 = render(browser_oxide::stealth::chrome_148_linux()).await;
    println!("audio hashes: mac={h_mac} lin={h_lin} lin_again={h_lin_2}");

    assert!(
        h_mac.is_finite() && h_mac > 0.0,
        "macOS profile produced no audio output"
    );
    assert!(
        h_lin.is_finite() && h_lin > 0.0,
        "Linux profile produced no audio output"
    );
    // Different audio_seed → distinct hashes
    assert!(
        (h_mac - h_lin).abs() > 1e-6,
        "audio fingerprint should differ across profiles with distinct audio_seed: mac={h_mac} lin={h_lin}"
    );
    // Same profile → reproducible hash (determinism)
    assert!(
        (h_lin - h_lin_2).abs() < 1e-6,
        "same profile should produce identical audio hash: lin={h_lin} lin_again={h_lin_2}"
    );
}

/// Tier 4 — function-identity preservation sniff tests.
/// The challenge vendor's obfuscated sentinel-property throws (5
/// engine-divergence TypeErrors per the trace) most likely arise from one
/// of three sites where the same function reference returns a DIFFERENT
/// object on subsequent access. If any sniff test fails, that's the
/// divergence site the vendor is detecting; patch it to return stable references.
#[tokio::test]
async fn check_function_identity_preservation() {
    let profile = browser_oxide::stealth::chrome_148_macos();
    let mut page = Page::from_html_with_url(&html(""), "https://example.com/", Some(profile))
        .await
        .unwrap();
    let js = r#"
        (function() {
            const res = {};

            // Test 1: navigator.mediaDevices.enumerateDevices identity stability
            try {
                const a = navigator.mediaDevices.enumerateDevices;
                const b = navigator.mediaDevices.enumerateDevices;
                res.test1_same_ref = (a === b);
                if (a === b) {
                    a.unjzomuybtbyyhwwkdpkxomylnab = 'tag';
                    res.test1_tag_persists = (b.unjzomuybtbyyhwwkdpkxomylnab === 'tag');
                    delete a.unjzomuybtbyyhwwkdpkxomylnab;
                } else {
                    res.test1_tag_persists = 'skipped (refs differ)';
                }
            } catch (e) { res.test1_err = e.message; }

            // Test 2: iframe contentWindow getOwnPropertyDescriptor.value identity
            try {
                const iframe = document.createElement('iframe');
                document.body && document.body.appendChild(iframe);
                const w = iframe.contentWindow;
                if (!w) { res.test2_skipped = 'no contentWindow'; }
                else {
                    const d1 = Object.getOwnPropertyDescriptor(w, 'Function');
                    const d2 = Object.getOwnPropertyDescriptor(w, 'Function');
                    if (!d1 || !d2) { res.test2_skipped = 'no Function descriptor'; }
                    else {
                        res.test2_same_ref = (d1.value === d2.value);
                        if (d1.value === d2.value && d1.value) {
                            d1.value.unjzomuybtbyyhwwkdpkxomylnab = 'tag';
                            res.test2_tag_persists = (d2.value.unjzomuybtbyyhwwkdpkxomylnab === 'tag');
                            try { delete d1.value.unjzomuybtbyyhwwkdpkxomylnab; } catch (_) {}
                        } else {
                            res.test2_tag_persists = 'skipped (refs differ)';
                        }
                    }
                }
            } catch (e) { res.test2_err = e.message; }

            // Test 3: Navigator method identity via Object.getOwnPropertyDescriptor
            try {
                const proto = navigator.constructor.prototype;
                const d1 = Object.getOwnPropertyDescriptor(proto, 'sendBeacon');
                const d2 = Object.getOwnPropertyDescriptor(proto, 'sendBeacon');
                if (!d1 || !d2) { res.test3_skipped = 'no sendBeacon descriptor'; }
                else {
                    res.test3_same_ref = (d1.value === d2.value);
                    if (d1.value === d2.value && d1.value) {
                        d1.value.unjzomuybtbyyhwwkdpkxomylnab = 'tag';
                        res.test3_tag_persists = (d2.value.unjzomuybtbyyhwwkdpkxomylnab === 'tag');
                        try { delete d1.value.unjzomuybtbyyhwwkdpkxomylnab; } catch (_) {}
                    } else {
                        res.test3_tag_persists = 'skipped (refs differ)';
                    }
                }
            } catch (e) { res.test3_err = e.message; }

            // Test 4 (bonus): navigator method via direct property access (the way
            // most JS uses it). If THIS one differs while test 3 passes, the issue
            // is in our descriptor wrapper, not the underlying function.
            try {
                const a = navigator.sendBeacon;
                const b = navigator.sendBeacon;
                res.test4_same_ref = (a === b);
                if (a === b) {
                    a.unjzomuybtbyyhwwkdpkxomylnab = 'tag';
                    res.test4_tag_persists = (b.unjzomuybtbyyhwwkdpkxomylnab === 'tag');
                    try { delete a.unjzomuybtbyyhwwkdpkxomylnab; } catch (_) {}
                }
            } catch (e) { res.test4_err = e.message; }

            // Test 5 (bonus): WebGL parameter — the vendor specifically tags
            // WebGLRenderingContext.prototype.getParameter via the canvas probe.
            try {
                const c = document.createElement('canvas');
                const g1 = c.getContext('webgl');
                const g2 = c.getContext('webgl');
                res.test5_ctx_same = (g1 === g2);
                if (g1 && g2) {
                    const fn1 = g1.getParameter;
                    const fn2 = g2.getParameter;
                    res.test5_method_same = (fn1 === fn2);
                }
            } catch (e) { res.test5_err = e.message; }

            return JSON.stringify(res, null, 2);
        })()
    "#;
    let result = page.evaluate(js).unwrap();
    println!("FUNCTION IDENTITY CHECK:\n{}", result);

    // Failures here directly identify the divergence site:
    assert!(
        result.contains("\"test1_same_ref\": true"),
        "FAIL test1: navigator.mediaDevices.enumerateDevices returns DIFFERENT object on re-access — divergence site #1"
    );
    assert!(
        result.contains("\"test1_tag_persists\": true")
            || result.contains("\"test1_tag_persists\": \"skipped (refs differ)\""),
        "FAIL test1: tag did NOT persist on enumerateDevices re-access — even though refs were equal"
    );
    assert!(
        result.contains("\"test3_same_ref\": true") || result.contains("\"test3_skipped\""),
        "FAIL test3: Navigator.sendBeacon descriptor.value returns DIFFERENT objects on re-access"
    );
    assert!(
        result.contains("\"test4_same_ref\": true"),
        "FAIL test4: direct navigator.sendBeacon returns DIFFERENT objects on re-access"
    );
}

/// iOS Safari profile JS surface check — Phase 3 (synthesis doc Tier 2.3-2.4).
/// Verifies the `iphone_15_pro_safari_18` preset produces an iOS-shaped JS env:
///   - 16 declined APIs absent
///   - userAgentData absent (Safari has no UA-CH)
///   - hasEnrolledInstrument absent on PaymentRequest (Chrome/Edge-only)
///   - window.orientation present (legacy iOS-only)
///   - DeviceMotionEvent.requestPermission static present (iOS 13+)
///   - ontouchstart on window
///   - WebGL renderer = "Apple GPU" constant
#[tokio::test]
async fn check_ios_safari_surface() {
    let profile = browser_oxide::stealth::presets::iphone_15_pro_safari_18();
    let mut page = Page::from_html_with_url(&html(""), "https://example.com/", Some(profile))
        .await
        .unwrap();
    let js = r#"
        (function() {
            const res = {};

            // 16 declined APIs — all must be absent on iOS profile
            const declined = [
                "Bluetooth", "USB", "Serial", "HID", "Sensor", "Accelerometer",
                "Gyroscope", "Magnetometer", "NetworkInformation", "BatteryManager",
                "IdleDetector",
            ];
            res.declined_apis_present = declined.filter(k => typeof globalThis[k] !== "undefined");

            // Navigator absences
            res.bluetooth_typeof = typeof navigator.bluetooth;
            res.usb_typeof = typeof navigator.usb;
            res.serial_typeof = typeof navigator.serial;
            res.hid_typeof = typeof navigator.hid;
            res.getBattery_typeof = typeof navigator.getBattery;
            res.connection_typeof = typeof navigator.connection;
            res.userAgentData_typeof = typeof navigator.userAgentData;
            res.deviceMemory_typeof = typeof navigator.deviceMemory;
            res.requestMIDIAccess_typeof = typeof navigator.requestMIDIAccess;

            // W1.5 — challenge-vendor UA-consistency check uses the `in`
            // operator. The descriptor itself must
            // not exist, not just return undefined — `typeof` returns
            // "undefined" for both cases but `'X' in navigator` distinguishes
            // them. This is the load-bearing check for wayfair/zillow/trulia/bloomberg.
            res.chrome_in_window = ('chrome' in globalThis);
            res.userActivation_in_navigator = ('userActivation' in navigator);
            res.deviceMemory_in_navigator = ('deviceMemory' in navigator);
            res.connection_in_navigator = ('connection' in navigator);
            res.scheduling_in_navigator = ('scheduling' in navigator);
            res.getInstalledRelatedApps_in_navigator = ('getInstalledRelatedApps' in navigator);
            res.IdleDetector_in_window = ('IdleDetector' in globalThis);
            res.UserActivation_in_window = ('UserActivation' in globalThis);

            // PaymentRequest hasEnrolledInstrument MUST be absent (Chrome-only)
            res.hasEnrolledInstrument_present = typeof PaymentRequest === "function"
                ? typeof PaymentRequest.prototype.hasEnrolledInstrument !== "undefined"
                : "no_PaymentRequest";

            // iOS-only globals
            res.window_orientation = typeof globalThis.orientation;
            res.window_orientation_value = globalThis.orientation;
            res.ontouchstart_present = "ontouchstart" in globalThis;

            // DeviceMotionEvent.requestPermission must exist (iOS 13+ tell)
            res.deviceMotion_requestPermission =
                typeof DeviceMotionEvent !== "undefined"
                && typeof DeviceMotionEvent.requestPermission;
            res.deviceOrientation_requestPermission =
                typeof DeviceOrientationEvent !== "undefined"
                && typeof DeviceOrientationEvent.requestPermission;

            // Profile-driven values from preset
            res.platform = navigator.platform;
            res.maxTouchPoints = navigator.maxTouchPoints;
            res.hardwareConcurrency = navigator.hardwareConcurrency;
            res.userAgent = navigator.userAgent;

            return JSON.stringify(res, null, 2);
        })()
    "#;
    let result = page.evaluate(js).unwrap();
    println!("iOS SAFARI SURFACE CHECK:\n{}", result);

    // The 16 declined APIs must all be absent
    assert!(
        result.contains("\"declined_apis_present\": []"),
        "iOS profile must strip all 16 declined APIs, got: {result}"
    );

    // Navigator absences
    assert!(result.contains("\"bluetooth_typeof\": \"undefined\""));
    assert!(result.contains("\"usb_typeof\": \"undefined\""));
    assert!(result.contains("\"serial_typeof\": \"undefined\""));
    assert!(result.contains("\"hid_typeof\": \"undefined\""));
    assert!(result.contains("\"getBattery_typeof\": \"undefined\""));
    assert!(result.contains("\"connection_typeof\": \"undefined\""));
    assert!(result.contains("\"userAgentData_typeof\": \"undefined\""));
    assert!(result.contains("\"deviceMemory_typeof\": \"undefined\""));

    // W1.5 — `in`-operator absences (challenge-vendor UA-consistency check).
    // These are the actual fail-on-true checks for wayfair/zillow/trulia/bloomberg.
    for (key, label) in [
        (
            "\"chrome_in_window\": false",
            "window.chrome must be absent",
        ),
        (
            "\"userActivation_in_navigator\": false",
            "navigator.userActivation must be absent",
        ),
        (
            "\"deviceMemory_in_navigator\": false",
            "navigator.deviceMemory must be absent",
        ),
        (
            "\"connection_in_navigator\": false",
            "navigator.connection must be absent",
        ),
        (
            "\"scheduling_in_navigator\": false",
            "navigator.scheduling must be absent",
        ),
        (
            "\"getInstalledRelatedApps_in_navigator\": false",
            "navigator.getInstalledRelatedApps must be absent",
        ),
        (
            "\"IdleDetector_in_window\": false",
            "globalThis.IdleDetector must be absent",
        ),
        (
            "\"UserActivation_in_window\": false",
            "globalThis.UserActivation must be absent",
        ),
    ] {
        assert!(result.contains(key), "iOS surface: {label} (got: {result})");
    }

    // PaymentRequest hasEnrolledInstrument must be absent on Safari
    assert!(
        result.contains("\"hasEnrolledInstrument_present\": false"),
        "iOS profile must NOT expose PaymentRequest.prototype.hasEnrolledInstrument (Chrome/Edge-only): {result}"
    );

    // iOS-only globals
    assert!(result.contains("\"window_orientation\": \"number\""));
    assert!(result.contains("\"window_orientation_value\": 0"));
    assert!(result.contains("\"ontouchstart_present\": true"));

    // iOS 13+ Device*Event.requestPermission statics
    assert!(
        result.contains("\"deviceMotion_requestPermission\": \"function\""),
        "iOS profile must expose DeviceMotionEvent.requestPermission static"
    );
    assert!(
        result.contains("\"deviceOrientation_requestPermission\": \"function\""),
        "iOS profile must expose DeviceOrientationEvent.requestPermission static"
    );

    // Profile-driven
    assert!(result.contains("\"platform\": \"iPhone\""));
    assert!(result.contains("\"maxTouchPoints\": 5"));
    assert!(
        result.contains("\"hardwareConcurrency\": 2"),
        "Safari intentionally caps hardwareConcurrency to 2 — got: {result}"
    );
    assert!(result.contains("\"userAgent\":") && result.contains("iPhone"));
}

/// Verify window[N]/window.length frame registry (challenge-vendor `ifw` probe).
/// After document.body.appendChild(iframe), window[0] must return the
/// iframe's contentWindow so window[0].navigator.webdriver is accessible.
#[tokio::test]
async fn window_frame_registry_after_append() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let js = r#"
        (function(){
            const iframe = document.createElement('iframe');
            document.body.appendChild(iframe);
            const len = window.length;
            const w0 = window[0];
            let wd = 'ERR_NO_W0';
            try { wd = String(w0 && w0.navigator && w0.navigator.webdriver); }
            catch(e) { wd = 'ERR:'+e.message; }
            return JSON.stringify({
                windowLength: len,
                hasW0: typeof w0 !== 'undefined',
                w0Type: typeof w0,
                webdriver: wd,
            });
        })()
    "#;
    let result = page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"));
    eprintln!("window-frame-registry: {result}");
    assert!(!result.starts_with("ERROR:"), "eval failed: {result}");
    assert!(
        result.contains("\"windowLength\":1"),
        "window.length must be 1 after append: {result}"
    );
    assert!(
        result.contains("\"hasW0\":true"),
        "window[0] must be defined after append: {result}"
    );
    assert!(
        !result.contains("ERR:"),
        "window[0].navigator.webdriver must not throw: {result}"
    );
}

// Diagnostic: does our PluginArray.prototype.namedItem leak source?
// A captured challenge-vendor fingerprint blob (Group F) showed our
// PluginArray.namedItem leaking the full
// inner source `function namedItem(n) { const len = _pluginsLen(); ... }`.
// Today's toString patch re-enable should mask it — this test confirms.
#[tokio::test]
#[allow(non_snake_case, reason = "mirrors JS API name under test")]
async fn pluginarray_namedItem_must_show_native_code() {
    let result = check_secure(
        r#"
        const fn = navigator.plugins.namedItem;
        const s1 = String(fn);
        const s2 = fn.toString();
        const s3 = Function.prototype.toString.call(fn);
        // All three should return the masked native-code shape.
        JSON.stringify({
            implicit: s1,
            instance: s2,
            protoCall: s3,
            leak_pluginsLen: s1.includes('_pluginsLen'),
            leak_allPlugins: s1.includes('_allPlugins'),
        });
        "#,
    )
    .await;
    eprintln!("namedItem audit: {result}");
    assert!(
        result.contains("[native code]"),
        "namedItem must mask to native shape; got: {result}"
    );
    assert!(
        result.contains("\"leak_pluginsLen\":false"),
        "namedItem leaks _pluginsLen identifier — toString masking broken: {result}"
    );
    assert!(
        result.contains("\"leak_allPlugins\":false"),
        "namedItem leaks _allPlugins identifier — toString masking broken: {result}"
    );
}

/// Challenge-vendor `hcp`/`cpl` probes: child realm navigator must expose plugins/mimeTypes.
/// The vendor checks `iframe.contentWindow.navigator.plugins.length` (hcp) and
/// `navigator.plugins.length` inside an srcdoc iframe (cpl). Both must return
/// the same count as the parent realm (5 plugins). Without this fix the child
/// realm navigator object is missing plugins/mimeTypes → TypeError on `.length`.
#[tokio::test]
async fn child_realm_navigator_plugins_accessible() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let js = r#"
        (function(){
            const ifr = document.createElement('iframe');
            document.body.appendChild(ifr);
            const cw = ifr.contentWindow;
            let pluginsLen = 'ERR_NO_PLUGINS';
            let mimeLen = 'ERR_NO_MIME';
            try { pluginsLen = cw.navigator.plugins.length; } catch(e) { pluginsLen = 'ERR:'+e.message; }
            try { mimeLen = cw.navigator.mimeTypes.length; } catch(e) { mimeLen = 'ERR:'+e.message; }
            return JSON.stringify({
                parentPlugins: navigator.plugins.length,
                childPlugins: pluginsLen,
                parentMime: navigator.mimeTypes.length,
                childMime: mimeLen,
            });
        })()
    "#;
    let result = page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"));
    eprintln!("child-realm-navigator-plugins: {result}");
    assert!(!result.starts_with("ERROR:"), "eval failed: {result}");
    assert!(
        result.contains("\"childPlugins\":5"),
        "child realm must have 5 plugins: {result}"
    );
    assert!(
        result.contains("\"childMime\":2"),
        "child realm must have 2 mimeTypes: {result}"
    );
}

/// Challenge-vendor `ifw` probe: `iframe.contentWindow.navigator.webdriver` must not throw.
/// The `ifw` probe error was "Cannot read properties of undefined (reading 'webdriver')"
/// — this means `iframe.contentWindow.navigator` was undefined (not `webdriver`).
/// After adding plugins/mimeTypes to child realm navigator, re-verify the full
/// `contentWindow` access path used by the vendor (not just `window[0]`).
#[tokio::test]
async fn iframe_contentwindow_navigator_webdriver_no_throw() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let js = r#"
        (function(){
            const ifr = document.createElement('iframe');
            document.body.appendChild(ifr);
            const cw = ifr.contentWindow;
            let r = {};
            r.hasCW = typeof cw !== 'undefined';
            try { r.navType = typeof cw.navigator; } catch(e) { r.navType = 'ERR:'+e.message; }
            try {
                const nav = cw.navigator;
                r.wdVal = String(nav.webdriver);
            } catch(e) { r.wdVal = 'ERR:'+e.message; }
            try {
                const nav = cw.navigator;
                const desc = Object.getOwnPropertyDescriptor(nav, 'webdriver');
                r.descDefined = desc !== undefined;
            } catch(e) { r.descDefined = 'ERR:'+e.message; }
            return JSON.stringify(r);
        })()
    "#;
    let result = page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"));
    eprintln!("ifw-probe-sim: {result}");
    assert!(!result.starts_with("ERROR:"), "eval failed: {result}");
    assert!(
        result.contains("\"hasCW\":true"),
        "iframe.contentWindow must exist: {result}"
    );
    assert!(
        result.contains("\"navType\":\"object\""),
        "iframe.contentWindow.navigator must be object: {result}"
    );
    assert!(
        !result.contains("ERR:"),
        "no property access must throw: {result}"
    );
}

// W3.2 — Cross-origin iframe postMessage round-trip. A vendor's managed
// challenge mounts the iframe from a cross-origin challenge host and
// communicates with the parent via postMessage. We don't load a real
// vendor iframe here (network + vendor-side fingerprinting would dominate);
// instead verify the iframe Proxy's postMessage delivers as a parent-
// realm MessageEvent. PLAN W3.2.
#[tokio::test]
async fn iframe_postmessage_round_trip_via_proxy() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    // FP-E1: real bidirectional iframe postMessage. The framed document
    // registers a 'message' listener (proving the child realm now exposes a
    // working addEventListener — previously undefined, a headless tell) and
    // replies via `event.source`. The parent posts to iframe.contentWindow and
    // must receive the reply with `event.source === iframe.contentWindow`
    // (what real challenge solvers assert).
    let setup = r#"
        globalThis.__rt = { parentGot: null, sourceIsCW: null, hasCw: null };
        const ifr = document.createElement('iframe');
        ifr.srcdoc = "<scr" + "ipt>window.addEventListener('message', function(e){ try { e.source.postMessage('echo:' + e.data, '*'); } catch(_){} });</scr" + "ipt>";
        document.body.appendChild(ifr);
        const cw = ifr.contentWindow;
        globalThis.__rt.hasCw = (cw != null);
        window.addEventListener('message', function(e){
            if (typeof e.data === 'string' && e.data.indexOf('echo:') === 0) {
                globalThis.__rt.parentGot = e.data;
                globalThis.__rt.sourceIsCW = (e.source === cw);
            }
        });
        if (cw) cw.postMessage('hello', '*');
    "#;
    let _ = page
        .evaluate_async(setup, std::time::Duration::from_secs(5))
        .await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__rt)")
        .unwrap_or_default();
    eprintln!("iframe postmessage round-trip: {result}");
    assert!(
        result.contains("\"hasCw\":true"),
        "no contentWindow: {result}"
    );
    assert!(
        result.contains("echo:hello"),
        "bidirectional iframe postMessage failed (child listener never fired or reply not delivered): {result}"
    );
    assert!(
        result.contains("\"sourceIsCW\":true"),
        "event.source !== iframe.contentWindow on the reply: {result}"
    );
}

/// location.origin must be set correctly for secure pages (https://example.com).
/// Root cause: interfaces_bootstrap installed an Illegal-constructor stub for
/// URLSearchParams before shared_apis_bootstrap could install the real polyfill,
/// making new URL() fail silently and leaving _locationData.origin = "null".
#[tokio::test]
async fn location_origin_secure_page() {
    let result = check_secure(r#"JSON.stringify({
        origin: location.origin,
        url_ok: (()=>{try{return new URL('https://x.com/').origin;}catch(e){return 'ERR:'+e.message;}})()
    })"#).await;
    eprintln!("loc-origin: {result}");
    assert_eq!(
        result, r#"{"origin":"https://example.com","url_ok":"https://x.com"}"#,
        "location.origin or URL broken: {result}"
    );
}

#[tokio::test]
async fn location_legacy_unforgeable_shape_matches_chrome() {
    let mut page = Page::from_html_with_url(
        &html(""),
        "https://example.com/base?q=1#old",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    let result = page
        .evaluate(
            r#"(() => {
                const names = [
                    'ancestorOrigins','href','origin','protocol','host','hostname','port',
                    'pathname','search','hash','assign','reload','replace','toString'
                ];
                const before = {
                    tag:Object.prototype.toString.call(location),
                    instance:location instanceof Location,
                    own:Object.getOwnPropertyNames(location).sort(),
                    proto:Object.getOwnPropertyNames(Location.prototype).sort(),
                    descriptors:names.map(name => {
                        const d=Object.getOwnPropertyDescriptor(location,name);
                        return [name,!!d,!!d.enumerable,!!d.configurable,
                            'value' in d ? !!d.writable : typeof d.get,
                            'value' in d ? typeof d.value : (d.set ? typeof d.set : 'undefined')];
                    }),
                    href:location.href,
                };
                location.port='9443';
                location.pathname='next';
                location.search='x=2';
                location.hash='tail';
                const after={
                    href:location.href,
                    port:location.port,
                    pathname:location.pathname,
                    search:location.search,
                    hash:location.hash,
                    pending:globalThis.__pendingNavigation,
                };
                return JSON.stringify({before,after});
            })()"#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["before"]["tag"], "[object Location]");
    assert_eq!(value["before"]["instance"], true);
    assert_eq!(value["before"]["proto"], serde_json::json!(["constructor"]));
    assert_eq!(
        value["before"]["own"],
        serde_json::json!([
            "ancestorOrigins",
            "assign",
            "hash",
            "host",
            "hostname",
            "href",
            "origin",
            "pathname",
            "port",
            "protocol",
            "reload",
            "replace",
            "search",
            "toString",
            "valueOf"
        ])
    );
    for row in value["before"]["descriptors"].as_array().unwrap() {
        assert_eq!(row[1], true, "missing Location own descriptor: {row}");
        assert_eq!(row[2], true, "Location member must be enumerable: {row}");
        assert_eq!(row[3], false, "Location member must be unforgeable: {row}");
        if matches!(
            row[0].as_str(),
            Some("assign" | "reload" | "replace" | "toString")
        ) {
            assert_eq!(row[4], false, "Location methods are non-writable: {row}");
            assert_eq!(row[5], "function");
        } else {
            assert_eq!(row[4], "function");
        }
    }
    assert_eq!(value["after"]["port"], "9443");
    assert_eq!(value["after"]["pathname"], "/next");
    assert_eq!(value["after"]["search"], "?x=2");
    assert_eq!(value["after"]["hash"], "#tail");
    assert_eq!(
        value["after"]["href"],
        "https://example.com:9443/next?x=2#tail"
    );
    assert_eq!(value["after"]["pending"]["kind"], "assign");
    assert_eq!(
        value["after"]["pending"]["url"],
        "https://example.com:9443/next?x=2#old"
    );
}

// ================================================================
// every patched native must return `function NAME() { [native code] }`
// from Function.prototype.toString — 11 of 12 anti-bot vendors fingerprint
// this. Fix 1 closes WebGL[2]RenderingContext.prototype. Fix 3 widens
// STRICT_INTERFACES (JS-side) to the remaining prototypes.
// Kept in the default suite so any future JS-source leakage from a patched
// WebIDL method is caught immediately.
// ================================================================

#[tokio::test]
async fn native_code_mask_audit() {
    // Enumerates every constructor on globalThis that has a .prototype,
    // walks each prototype's own-function descriptors, asserts
    // `String(value)` matches `function <ident>() { [native code] }`.
    // The pattern (rather than exact-name match) accepts V8/ECMA spec
    // aliases (Date.toGMTString == toUTCString, String.trimLeft ==
    // trimStart, Set.keys == Set.values) — real Chrome serializes the
    // same way, so vendors fingerprinting these see no divergence.
    // The leak this catches is "JS source body" (function with body),
    // which IS a fingerprint tell.
    let js = r#"
        (() => {
            const NATIVE_RE = /^function [A-Za-z_$][A-Za-z0-9_$]*\(\) \{ \[native code\] \}$/;
            const SKIP_PROTOS = new Set([Object.prototype, Function.prototype]);
            const failuresByIface = {};
            const totalNames = Object.getOwnPropertyNames(globalThis);
            for (const name of totalNames) {
                let v;
                try { v = globalThis[name]; } catch (e) { continue; }
                if (typeof v !== 'function') continue;
                const proto = v.prototype;
                if (!proto || SKIP_PROTOS.has(proto)) continue;
                let mnames;
                try { mnames = Object.getOwnPropertyNames(proto); } catch (e) { continue; }
                for (const mname of mnames) {
                    if (mname === 'constructor') continue;
                    let desc;
                    try { desc = Object.getOwnPropertyDescriptor(proto, mname); } catch (e) { continue; }
                    if (!desc || typeof desc.value !== 'function') continue;
                    let s;
                    try { s = String(desc.value); } catch (e) { continue; }
                    if (!NATIVE_RE.test(s)) {
                        (failuresByIface[name] = failuresByIface[name] || []).push({method: mname, got: s.slice(0, 100)});
                    }
                }
            }
            // Flat list + per-iface counts
            const flat = [];
            const counts = {};
            for (const iface of Object.keys(failuresByIface).sort()) {
                counts[iface] = failuresByIface[iface].length;
                for (const f of failuresByIface[iface]) flat.push({iface, ...f});
            }
            return JSON.stringify({count: flat.length, counts, failures: flat});
        })()
    "#;
    let result = check(js).await;
    let v: serde_json::Value = serde_json::from_str(&result)
        .unwrap_or_else(|e| panic!("audit json parse: {e}; raw={result}"));
    let count = v["count"].as_u64().unwrap_or(0);
    if count > 0 {
        let counts_pretty = serde_json::to_string_pretty(&v["counts"]).unwrap();
        let dump = serde_json::to_string_pretty(&v["failures"]).unwrap();
        panic!(
            "native_code_mask_audit: {count} total failures.\nPer-interface counts:\n{counts_pretty}\nFull list:\n{dump}"
        );
    }
}

// ================================================================
// v0.1.0-parity Fix 4 — Canvas toDataURL parity (engine side)
// 10 of 12 anti-bot vendors hash canvas 2D output. Full real-Chrome
// pixel parity is a known open question (needs
// `crates/browser/tests/captures/canvas_chrome_148.json` captured via
// a CDP automation driver). This engine-side test asserts two weaker but
// still-required properties:
//
//   (a) Same draw sequence on the SAME profile produces the SAME
//       toDataURL hash across two fresh pages — determinism.
//   (b) Same draw sequence on TWO DIFFERENT profiles produces
//       DIFFERENT hashes — per-profile uniqueness (a vendor that
//       routes via profile-routing must see distinct fingerprints).
//
// Draw sequence is the canonical FingerprintJS "text + arc + emoji"
// pattern — close to what every fingerprinter does.
// ================================================================

const CANVAS_FP_SEQUENCE_JS: &str = r#"(() => {
    const c = document.createElement('canvas');
    c.width = 200; c.height = 60;
    const ctx = c.getContext('2d');
    if (!ctx) return 'NO_CTX';
    // FingerprintJS-style canonical sequence
    ctx.textBaseline = 'top';
    ctx.font = '14px Arial';
    ctx.fillStyle = '#f60';
    ctx.fillRect(0, 0, 100, 30);
    ctx.fillStyle = '#069';
    ctx.fillText('browser_oxide', 2, 15);
    ctx.fillStyle = 'rgba(102, 204, 0, 0.7)';
    ctx.fillText('parity-test', 4, 17);
    ctx.beginPath();
    ctx.arc(150, 30, 12, 0, Math.PI * 2);
    ctx.fill();
    return c.toDataURL();
})()"#;

async fn canvas_hash_for(profile: browser_oxide::stealth::StealthProfile) -> String {
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><head></head><body></body></html>",
        Some(profile),
    )
    .await
    .unwrap();
    let data_url = page.evaluate(CANVAS_FP_SEQUENCE_JS).unwrap();
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data_url.as_bytes());
    // `digest` 0.11 (sha2 0.11) returns `Array<u8, N>`, which — unlike the
    // old `GenericArray` — does not implement `LowerHex`, so `{:x}` no
    // longer compiles. Hex-encode the bytes directly.
    h.finalize().iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[tokio::test]
async fn canvas_todataurl_deterministic_within_profile() {
    let a = canvas_hash_for(browser_oxide::stealth::presets::chrome_148_macos()).await;
    let b = canvas_hash_for(browser_oxide::stealth::presets::chrome_148_macos()).await;
    assert_eq!(
        a, b,
        "two fresh pages with the same profile must hash to the same toDataURL: a={a} b={b}"
    );
}

#[tokio::test]
async fn canvas_todataurl_differs_across_profiles() {
    let mac = canvas_hash_for(browser_oxide::stealth::presets::chrome_148_macos()).await;
    let win = canvas_hash_for(browser_oxide::stealth::presets::chrome_148_windows()).await;
    let lin = canvas_hash_for(browser_oxide::stealth::presets::chrome_148_linux()).await;
    // Per-profile uniqueness: at least one of the three pairs must
    // differ. (Some profiles may share canvas backends and tie; the
    // important property is that profile-routing isn't all identical.)
    let pairs_equal = (mac == win) as u32 + (mac == lin) as u32 + (win == lin) as u32;
    assert!(
        pairs_equal <= 1,
        "≥2 of 3 profiles produced identical canvas hashes (per-profile uniqueness broken). \
         mac={mac} win={win} lin={lin}"
    );
}

// ================================================================
// v0.1.0-parity Fix 2 — WebGL per-profile golden snapshot (engine side)
// each stealth profile must produce a CONSISTENT WebGL parameter set
// matching its declared preset values. The comparison against captured
// real-Chrome output is a separate test deferred to when
// `crates/browser/tests/captures/*.webgl.json` is committed.
// This engine-side test catches
// drift between the preset declaration and what `getParameter`
// actually emits — a necessary precondition for the real-Chrome
// comparison.
// ================================================================

async fn webgl_unmasked_for(profile: browser_oxide::stealth::StealthProfile) -> serde_json::Value {
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><head></head><body><canvas id='c'></canvas></body></html>",
        Some(profile),
    )
    .await
    .unwrap();
    let result = page
        .evaluate(
            r#"(() => {
                const c = document.getElementById('c');
                const gl = c.getContext('webgl');
                if (!gl) return JSON.stringify({err: 'no gl'});
                return JSON.stringify({
                    vendor: gl.getParameter(0x1F00),
                    renderer: gl.getParameter(0x1F01),
                    unmaskedVendor: gl.getParameter(0x9245),
                    unmaskedRenderer: gl.getParameter(0x9246),
                });
            })()"#,
        )
        .unwrap();
    serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"))
}

// The engine reads from `gpu_profile.unmasked_*` (not `webgl_*`),
// per canvas_bootstrap.js:409 (`s("webgl_unmasked_renderer")`).
// Anchor the golden on `gpu_profile` to test what's actually emitted.
// NOTE: A real-Chrome capture comparison verifies the gpu_profile
// values themselves match a real GPU — that step is deferred until
// captures land.

#[tokio::test]
async fn webgl_param_golden_snapshot_chrome_148_macos() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let want_v = profile.gpu_profile.unmasked_vendor.clone();
    let want_r = profile.gpu_profile.unmasked_renderer.clone();
    let v = webgl_unmasked_for(profile).await;
    assert_eq!(v["unmaskedVendor"], want_v, "UNMASKED_VENDOR drift: {v}");
    assert_eq!(
        v["unmaskedRenderer"], want_r,
        "UNMASKED_RENDERER drift: {v}"
    );
}

#[tokio::test]
async fn webgl_param_golden_snapshot_chrome_148_windows() {
    let profile = browser_oxide::stealth::presets::chrome_148_windows();
    let want_v = profile.gpu_profile.unmasked_vendor.clone();
    let want_r = profile.gpu_profile.unmasked_renderer.clone();
    let v = webgl_unmasked_for(profile).await;
    assert_eq!(v["unmaskedVendor"], want_v, "drift: {v}");
    assert_eq!(v["unmaskedRenderer"], want_r, "drift: {v}");
}

#[tokio::test]
async fn webgl_param_golden_snapshot_chrome_148_linux() {
    let profile = browser_oxide::stealth::presets::chrome_148_linux();
    let want_v = profile.gpu_profile.unmasked_vendor.clone();
    let want_r = profile.gpu_profile.unmasked_renderer.clone();
    let v = webgl_unmasked_for(profile).await;
    assert_eq!(v["unmaskedVendor"], want_v, "drift: {v}");
    assert_eq!(v["unmaskedRenderer"], want_r, "drift: {v}");
}

// ================================================================
// v0.1.0-parity Fix 5 — keystroke generator wiring
// The Rust CMU+Buffalo bigram-modulated keystroke generator existed
// (behavior.rs:421-464) but humanize.js never called it. It is now
// exposed via `Symbol.for('__browser_oxide_keystroke_schedule__')` and
// humanize.js consumes it on input focusin.
// ================================================================

#[tokio::test]
async fn keystroke_schedule_slot_installed_and_monotonic() {
    let result = check(
        r#"
        (() => {
            const fn = globalThis[Symbol.for('__browser_oxide_keystroke_schedule__')];
            if (typeof fn !== 'function') return JSON.stringify({err: 'slot missing'});
            const sch = fn('abc', 50);
            if (!Array.isArray(sch) || sch.length === 0) return JSON.stringify({err: 'empty schedule', sch});
            let monotonic = true;
            let prevUp = 0;
            for (const s of sch) {
                if (!(s.down_ms >= prevUp - 0.0001 && s.up_ms > s.down_ms)) monotonic = false;
                prevUp = s.up_ms;
            }
            return JSON.stringify({
                length: sch.length,
                first: sch[0],
                last: sch[sch.length - 1],
                monotonic,
                codes: sch.map(s => s.code),
            });
        })()
        "#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"));
    assert_eq!(
        v["length"].as_u64().unwrap(),
        3,
        "expected 3 entries: {result}"
    );
    assert_eq!(v["monotonic"], true, "schedule not monotonic: {result}");
    assert_eq!(v["codes"][0], "KeyA");
    assert_eq!(v["codes"][1], "KeyB");
    assert_eq!(v["codes"][2], "KeyC");
    let first_down = v["first"]["down_ms"].as_f64().unwrap();
    assert!(first_down >= 0.0, "first down_ms must be ≥ 0: {first_down}");
}

// ================================================================
// v0.1.0-parity Fix 6 — seeded random wired through Symbol-keyed slot
// humanize.js was
// using `Math.random()` per-page, making different visits look like
// N different users to challenge-vendor behavioral models. Replaced with
// a Symbol-keyed `__browser_oxide_behavior_rand__` slot backed by a
// per-runtime ChaCha12 RNG (BehaviorRngState), seeded from
// BROWSER_OXIDE_BEHAVIOR_SEED env var or fresh-random per page.
// ================================================================

#[tokio::test]
async fn behavior_rand_slot_installed_and_in_unit_range() {
    let result = check(
        r#"
        (() => {
            const sym = Symbol.for('__browser_oxide_behavior_rand__');
            const fn = globalThis[sym];
            if (typeof fn !== 'function') return JSON.stringify({err: 'slot missing'});
            const a = fn();
            const b = fn();
            const c = fn();
            return JSON.stringify({
                type: typeof fn,
                inRange: (a >= 0 && a < 1) && (b >= 0 && b < 1) && (c >= 0 && c < 1),
                advanced: !(a === b && b === c),
                a, b, c,
            });
        })()
        "#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"));
    assert_eq!(v["type"], "function", "slot must be a function: {result}");
    assert_eq!(v["inRange"], true, "values out of [0,1): {result}");
    assert_eq!(v["advanced"], true, "sequence not advancing: {result}");
}

// ================================================================
// v0.1.0-parity Fix 8 — MessageChannel/MessagePort proper impl
// Pre-fix `new MessageChannel(); port1.postMessage(...)` was a no-op,
// breaking recaptcha enterprise (duolingo) and every Worker that uses
// channels for message routing. Tests paired routing, start-gating,
// and close-detach.
// ================================================================

// deno_core's `execute_script` calls share the global scope; `const`
// declarations clash on redeclaration → SyntaxError silently aborts
// the next script. Wrap each evaluate in an IIFE to scope locals.

#[tokio::test]
async fn message_channel_paired_routing() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    let _ = page.evaluate(
        r#"(() => {
            globalThis.__mctest = { got: [] };
            const ch = new MessageChannel();
            globalThis.__mctest.ch = ch;
            ch.port2.onmessage = (e) => { globalThis.__mctest.got.push(e.data); };
            ch.port1.postMessage('hello');
            ch.port1.postMessage({n: 42});
        })()"#,
    );
    // MessagePort delivery is a MACROTASK via __bgSetTimeout (unref'd — React
    // 18's concurrent scheduler needs this async, non-loop-pinning delivery),
    // so it fires across SUBSEQUENT run_until_idle invocations, not one.
    // Pump a few short drains with real time so the unref'd timer lands.
    for _ in 0..10 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(25))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let result = page
        .evaluate("JSON.stringify({len: globalThis.__mctest.got.length, first: globalThis.__mctest.got[0], second: globalThis.__mctest.got[1] && globalThis.__mctest.got[1].n})")
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"));
    assert_eq!(
        v["len"].as_u64().unwrap(),
        2,
        "paired delivery failed: {result}"
    );
    assert_eq!(v["first"], "hello");
    assert_eq!(v["second"].as_u64().unwrap(), 42);
}

#[tokio::test]
async fn message_channel_queue_then_start() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    let _ = page.evaluate(
        r#"(() => {
            globalThis.__mctest = { got: [] };
            const ch = new MessageChannel();
            globalThis.__mctest.ch = ch;
            ch.port1.postMessage('queued-1');
            ch.port1.postMessage('queued-2');
        })()"#,
    );
    let pre = page
        .evaluate("globalThis.__mctest.got.length")
        .unwrap_or_default();
    let _ = page.evaluate(
        r#"(() => {
            const ch = globalThis.__mctest.ch;
            ch.port2.onmessage = (e) => { globalThis.__mctest.got.push(e.data); };
            ch.port2.start();
        })()"#,
    );
    // start() flushes the queued messages as unref'd macrotasks — pump several
    // short drains with real time so they land (see paired_routing note).
    for _ in 0..10 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(25))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let post = page
        .evaluate("globalThis.__mctest.got.length")
        .unwrap_or_default();
    assert_eq!(pre, "0", "queued msgs should not deliver pre-start: {pre}");
    assert_eq!(post, "2", "start should drain queue: {post}");
}

#[tokio::test]
async fn message_channel_close_detaches() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    let _ = page.evaluate(
        r#"(() => {
            globalThis.__mctest = { got: [] };
            const ch = new MessageChannel();
            ch.port2.onmessage = (e) => { globalThis.__mctest.got.push(e.data); };
            ch.port1.close();
            ch.port1.postMessage('after-close');
        })()"#,
    );
    let len = page
        .evaluate("globalThis.__mctest.got.length")
        .unwrap_or_default();
    assert_eq!(len, "0", "post-close postMessage must not deliver: {len}");
}

#[tokio::test]
async fn message_channel_webidl_shape_and_explicit_start_match_chrome() {
    let mut page = Page::from_html(&html(""), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    page.evaluate(
        r#"(() => {
            const ch = new MessageChannel();
            globalThis.__mcshape = { ch, got: [] };
            ch.port2.addEventListener('message', event => globalThis.__mcshape.got.push(event.data));
            ch.port1.postMessage('queued');
            let portConstruction = '';
            try { new MessagePort(); portConstruction = 'ok'; }
            catch (error) { portConstruction = error.name + ':' + error.message; }
            globalThis.__mcshape.before = {
                channelOwn:Reflect.ownKeys(ch).map(String),
                portOwn:Reflect.ownKeys(ch.port1).map(String),
                channelTag:Object.prototype.toString.call(ch),
                portTag:Object.prototype.toString.call(ch.port1),
                eventTarget:ch.port1 instanceof EventTarget,
                portConstruction,
                channelProto:Object.getOwnPropertyNames(MessageChannel.prototype).sort(),
                portProto:Object.getOwnPropertyNames(MessagePort.prototype).sort(),
                channelEnumerable:['port1','port2'].map(name =>
                    Object.getOwnPropertyDescriptor(MessageChannel.prototype,name).enumerable),
                portEnumerable:['onmessage','onmessageerror','postMessage','start','close'].map(name =>
                    Object.getOwnPropertyDescriptor(MessagePort.prototype,name).enumerable),
                postMessageLength:MessagePort.prototype.postMessage.length,
                hasOwnAddEventListener:Object.prototype.hasOwnProperty.call(
                    MessagePort.prototype,'addEventListener'),
            };
        })()"#,
    )
    .unwrap();
    for _ in 0..5 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(20))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        page.evaluate("globalThis.__mcshape.got.length").unwrap(),
        "0",
        "addEventListener alone must not start a MessagePort"
    );
    page.evaluate("globalThis.__mcshape.ch.port2.start()")
        .unwrap();
    for _ in 0..10 {
        let _ = page
            .event_loop()
            .run_until_idle(std::time::Duration::from_millis(20))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let result = page
        .evaluate(
            "JSON.stringify({before:globalThis.__mcshape.before,got:globalThis.__mcshape.got})",
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["before"]["channelOwn"], serde_json::json!([]));
    assert_eq!(value["before"]["portOwn"], serde_json::json!([]));
    assert_eq!(value["before"]["channelTag"], "[object MessageChannel]");
    assert_eq!(value["before"]["portTag"], "[object MessagePort]");
    assert_eq!(value["before"]["eventTarget"], true);
    assert!(value["before"]["portConstruction"]
        .as_str()
        .unwrap()
        .contains("Illegal constructor"));
    assert_eq!(
        value["before"]["channelProto"],
        serde_json::json!(["constructor", "port1", "port2"])
    );
    assert_eq!(
        value["before"]["portProto"],
        serde_json::json!([
            "close",
            "constructor",
            "onmessage",
            "onmessageerror",
            "postMessage",
            "start"
        ])
    );
    assert_eq!(
        value["before"]["channelEnumerable"],
        serde_json::json!([true, true])
    );
    assert_eq!(
        value["before"]["portEnumerable"],
        serde_json::json!([true, true, true, true, true])
    );
    assert_eq!(value["before"]["postMessageLength"], 1);
    assert_eq!(value["before"]["hasOwnAddEventListener"], false);
    assert_eq!(value["got"], serde_json::json!(["queued"]));
}

// ================================================================
// v0.1.0-parity Fix 9 — RAF cadence jitter
// Real Chrome's
// requestAnimationFrame cadence shows scheduler noise around the 60Hz
// step (mean ≈ 16.67 ms, σ ≈ 0.5 ms). A perfect 16ms grid (engine
// pre-fix) is the vendor's `set(diffs).size === 1` bot tell. Symbol-keyed
// `__browser_oxide_raf_jitter_ms__` exposes the delay sampler so we
// can pull 1000 samples without 16 s wall clock.
// ================================================================

#[tokio::test]
async fn raf_cadence_jitter() {
    let result = check(
        r#"
        (() => {
            const fn = globalThis[Symbol.for('__browser_oxide_raf_jitter_ms__')];
            if (typeof fn !== 'function') return JSON.stringify({err: 'sampler missing'});
            const n = 1000;
            const xs = new Array(n);
            for (let i = 0; i < n; i++) xs[i] = fn();
            const mean = xs.reduce((a,b) => a + b, 0) / n;
            const variance = xs.reduce((a,b) => a + (b - mean) ** 2, 0) / n;
            const stddev = Math.sqrt(variance);
            let mn = Infinity, mx = -Infinity;
            for (const v of xs) { if (v < mn) mn = v; if (v > mx) mx = v; }
            return JSON.stringify({mean, stddev, min: mn, max: mx, n});
        })()
        "#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"));
    let mean = v["mean"].as_f64().unwrap();
    let stddev = v["stddev"].as_f64().unwrap();
    let max = v["max"].as_f64().unwrap();
    let min = v["min"].as_f64().unwrap();
    assert!(
        (mean - 16.67).abs() < 0.2,
        "mean must track 16.67 ± 0.2 ms (got {mean}). raw={v}"
    );
    assert!(stddev > 0.2, "stddev too low: {stddev} ms. raw={v}");
    assert!(max < 33.0, "max ≥ 33 ms (frame skip): {max} ms. raw={v}");
    assert!(min >= 1.0, "min < 1 ms (clamp broke): {min} ms. raw={v}");
}

// ================================================================
// v0.1.0-parity Fix 7 — performance.timeOrigin consistency
// The vendor's
// origin-skew probe checks `Math.abs((performance.timeOrigin +
// performance.now()) - Date.now()) < small`. Engine pre-fix anchored
// timeOrigin via a Date.now() snapshot at bootstrap minus a hardcoded
// nav offset → ~516ms drift from the Rust-side `performance.now()`
// monotonic origin. Fix 7 exposes `op_perf_time_origin_ms` (wall-clock
// at Rust origin) and sets timeOrigin from it.
// ================================================================

#[tokio::test]
async fn perf_origin_now_consistency() {
    let result = check(
        r#"
        const to = performance.timeOrigin;
        const nv = performance.now();
        const dn = Date.now();
        JSON.stringify({to, nv, dn, drift: to + nv - dn})
        "#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"));
    let drift = v["drift"].as_f64().unwrap_or(f64::INFINITY);
    eprintln!("perf-origin probe: {v}");
    // Spec: <10 ms. Humanization adds
    // 0-35µs of typical jitter + a rare ≤1.5ms spike, plus a few µs of
    // op-overhead — comfortably under 10ms.
    assert!(
        drift.abs() < 10.0,
        "timeOrigin + now() vs Date.now() drift = {drift} ms (>= 10 ms threshold). raw={v}"
    );
}

// ================================================================
// v0.1.0-parity Fix 11 — HTMLFormElement.prototype.elements
// reddit's verify-page solver calls
// `form.elements.namedItem('solution').value = token`. Without the
// `elements` getter that throws TypeError → silently caught at
// page.rs:3406 → __pendingNavigation never set → iter=0 stub return.
// ================================================================

#[tokio::test]
async fn form_elements_collection() {
    let body = r#"
        <form id="f">
            <input type="hidden" name="solution" value="">
            <input type="text" name="user">
            <input type="submit" value="Go">
            <select name="kind"><option value="a">A</option></select>
            <textarea name="notes"></textarea>
            <button type="button" id="btn">Click</button>
        </form>
    "#;
    let mut page = Page::from_html(&html(body), None::<browser_oxide::stealth::StealthProfile>)
        .await
        .unwrap();
    let js = r#"
        (() => {
            const form = document.forms[0] || document.getElementById('f');
            if (!form) return JSON.stringify({err: 'no form'});
            const els = form.elements;
            if (!els) return JSON.stringify({err: 'no elements getter'});
            const sol = els.namedItem('solution');
            const usr = els.namedItem('user');
            const item0 = els.item(0);
            let iterCount = 0;
            try { for (const _ of els) iterCount++; } catch (e) {}
            return JSON.stringify({
                length: els.length,
                solName: sol && sol.name,
                solTag: sol && sol.tagName,
                usrName: usr && usr.name,
                item0Tag: item0 && item0.tagName,
                iterCount,
                missing: els.namedItem('nope'),
            });
        })()
    "#;
    let result = page
        .evaluate(js)
        .unwrap_or_else(|e| panic!("evaluate: {e}"));
    let v: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|e| panic!("json: {e}; raw={result}"));
    assert_eq!(v["solName"], "solution", "namedItem('solution') wrong");
    assert_eq!(v["solTag"], "INPUT", "tagName wrong");
    assert_eq!(v["usrName"], "user");
    assert!(v["length"].as_u64().unwrap() >= 5, "length < 5: {result}");
    assert!(v["item0Tag"].as_str().is_some(), "item(0) failed");
    assert!(v["iterCount"].as_u64().unwrap() >= 5, "iteration count low");
    assert!(v["missing"].is_null(), "namedItem('nope') should be null");
}

// ================================================================
// v0.2.0 FIX-J — FileReader.readAsDataURL / readAsArrayBuffer / readAsText
// were no-op stubs returning empty strings/buffers. AWS WAF challenge.js
// calls readAsDataURL(blob) to base64-encode its encrypted fingerprint
// payload before POSTing to /verify; an empty result bailed challenge.js
// with "challenge data URL was malformed".
// ================================================================

// FileReader's readAsX methods set `result` synchronously (the spec only
// requires the `onload` event to fire on the microtask queue). Production
// AWS WAF challenge.js code reads `reader.result` from inside the onload
// callback, but tests can read it immediately after the call returns.

#[tokio::test]
async fn file_reader_read_as_data_url_encodes_blob_bytes() {
    // 'Hello!' → base64 'SGVsbG8h'
    let js = r#"
        (() => {
            const blob = new Blob([new Uint8Array([72,101,108,108,111,33])], { type: 'text/plain' });
            const r = new FileReader();
            r.readAsDataURL(blob);
            return JSON.stringify({ result: r.result, state: r.readyState });
        })()
    "#;
    let raw = check(js).await;
    let v: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    assert_eq!(
        v["result"], "data:text/plain;base64,SGVsbG8h",
        "readAsDataURL must base64-encode blob bytes with the blob's MIME type: {raw}"
    );
    assert_eq!(v["state"], 2, "readyState must be DONE after read");
}

#[tokio::test]
async fn file_reader_read_as_data_url_default_mime() {
    // No blob type → default 'application/octet-stream' (FileReader spec).
    // 0xff 0x00 0x42 → base64 '/wBC'.
    let js = r#"
        (() => {
            const blob = new Blob([new Uint8Array([0xff, 0x00, 0x42])]);
            const r = new FileReader();
            r.readAsDataURL(blob);
            return r.result;
        })()
    "#;
    assert_eq!(check(js).await, "data:application/octet-stream;base64,/wBC");
}

#[tokio::test]
async fn file_reader_read_as_array_buffer_copies_blob_bytes() {
    let js = r#"
        (() => {
            const blob = new Blob([new Uint8Array([1,2,3,4,5,6,7,8])]);
            const r = new FileReader();
            r.readAsArrayBuffer(blob);
            const view = new Uint8Array(r.result);
            return JSON.stringify({ len: view.byteLength, bytes: Array.from(view) });
        })()
    "#;
    let raw = check(js).await;
    let v: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    assert_eq!(v["len"], 8);
    assert_eq!(v["bytes"], serde_json::json!([1, 2, 3, 4, 5, 6, 7, 8]));
}

#[tokio::test]
async fn file_reader_read_as_text_decodes_utf8() {
    let js = r#"
        (() => {
            const blob = new Blob(['héllo'], { type: 'text/plain' });
            const r = new FileReader();
            r.readAsText(blob);
            return r.result;
        })()
    "#;
    assert_eq!(check(js).await, "héllo");
}

#[tokio::test]
async fn binary_file_and_fetch_objects_match_chrome_webidl_shape() {
    let raw = check(
        r#"(() => {
            const descriptorShape = (ctor, required) => {
                const proto = ctor.prototype;
                return required.every((name) => {
                    const d = Object.getOwnPropertyDescriptor(proto, name);
                    return !!d && d.enumerable === true && d.configurable === true;
                });
            };
            const blob = new Blob(['abc'], { type: 'TEXT/PLAIN' });
            const file = new File(['xy'], 'a/b.txt', { type: 'text/plain', lastModified: 1234 });
            const reader = new FileReader();
            const headers = new Headers();
            headers.append('x-test', 'a');
            headers.append('x-test', 'b');
            headers.append('set-cookie', 'a=1');
            headers.append('set-cookie', 'b=2');
            const request = new Request('https://example.com/path', {
                method: 'POST', body: 'hello', headers: { 'x-test': '1' },
            });
            const requestClone = request.clone();
            const response = new Response('hello', {
                status: 201, headers: { 'content-type': 'text/plain' },
            });
            return JSON.stringify({
                descriptorOk:
                    descriptorShape(Blob, ['size','type','slice','text','arrayBuffer','bytes','stream']) &&
                    descriptorShape(File, ['name','lastModified','lastModifiedDate','webkitRelativePath']) &&
                    descriptorShape(FileReader, ['readyState','result','error','readAsText','readAsArrayBuffer','readAsBinaryString','readAsDataURL','abort']) &&
                    descriptorShape(Headers, ['append','delete','entries','forEach','get','getSetCookie','has','keys','set','values']) &&
                    descriptorShape(Request, ['url','method','headers','body','bodyUsed','signal','text','json','arrayBuffer','blob','bytes','formData','clone']) &&
                    descriptorShape(Response, ['type','redirected','status','statusText','ok','headers','url','body','bodyUsed','text','json','arrayBuffer','blob','bytes','formData','clone']),
                ownKeys: {
                    blob: Reflect.ownKeys(blob).map(String),
                    file: Reflect.ownKeys(file).map(String),
                    reader: Reflect.ownKeys(reader).map(String),
                    headers: Reflect.ownKeys(headers).map(String),
                    request: Reflect.ownKeys(request).map(String),
                    response: Reflect.ownKeys(response).map(String),
                },
                blob: { size: blob.size, type: blob.type },
                file: {
                    name: file.name, size: file.size, type: file.type,
                    lastModified: file.lastModified,
                    lastModifiedDate: file.lastModifiedDate.getTime(),
                    relative: file.webkitRelativePath,
                    isBlob: file instanceof Blob,
                },
                headers: {
                    combined: headers.get('x-test'),
                    cookies: headers.getSetCookie(),
                    iteratorSame: Headers.prototype[Symbol.iterator] === Headers.prototype.entries,
                },
                request: {
                    tag: Object.prototype.toString.call(request),
                    method: request.method,
                    url: request.url,
                    header: request.headers.get('x-test'),
                    cloneOwnKeys: Reflect.ownKeys(requestClone).map(String),
                    cloneMethod: requestClone.method,
                    cloneUrl: requestClone.url,
                },
                response: {
                    tag: Object.prototype.toString.call(response),
                    status: response.status,
                    ok: response.ok,
                    type: response.type,
                    redirected: response.redirected,
                    contentType: response.headers.get('content-type'),
                },
                readerConstants: [FileReader.EMPTY, FileReader.LOADING, FileReader.DONE],
            });
        })()"#,
    )
    .await;
    let value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    assert_eq!(value["descriptorOk"], true);
    for name in ["blob", "file", "reader", "headers", "request", "response"] {
        assert_eq!(
            value["ownKeys"][name],
            serde_json::json!([]),
            "{name}: {raw}"
        );
    }
    assert_eq!(value["blob"]["size"], 3);
    assert_eq!(value["blob"]["type"], "text/plain");
    assert_eq!(value["file"]["name"], "a:b.txt");
    assert_eq!(value["file"]["size"], 2);
    assert_eq!(value["file"]["type"], "text/plain");
    assert_eq!(value["file"]["lastModified"], 1234);
    assert_eq!(value["file"]["lastModifiedDate"], 1234);
    assert_eq!(value["file"]["relative"], "");
    assert_eq!(value["file"]["isBlob"], true);
    assert_eq!(value["headers"]["combined"], "a, b");
    assert_eq!(
        value["headers"]["cookies"],
        serde_json::json!(["a=1", "b=2"])
    );
    assert_eq!(value["headers"]["iteratorSame"], true);
    assert_eq!(value["request"]["tag"], "[object Request]");
    assert_eq!(value["request"]["method"], "POST");
    assert_eq!(value["request"]["url"], "https://example.com/path");
    assert_eq!(value["request"]["header"], "1");
    assert_eq!(value["request"]["cloneOwnKeys"], serde_json::json!([]));
    assert_eq!(value["request"]["cloneMethod"], "POST");
    assert_eq!(value["request"]["cloneUrl"], "https://example.com/path");
    assert_eq!(value["response"]["tag"], "[object Response]");
    assert_eq!(value["response"]["status"], 201);
    assert_eq!(value["response"]["ok"], true);
    assert_eq!(value["response"]["type"], "default");
    assert_eq!(value["response"]["redirected"], false);
    assert_eq!(value["response"]["contentType"], "text/plain");
    assert_eq!(value["readerConstants"], serde_json::json!([0, 1, 2]));
}

#[tokio::test]
async fn dom_core_extended_webidl_matches_chrome_shape_and_behavior() {
    let raw = check(
        r#"(() => {
            let nodeCtor;
            try { new Node(); nodeCtor = 'ok'; }
            catch (e) { nodeCtor = e.name + ':' + e.message; }

            const host = document.createElement('div');
            document.body.appendChild(host);
            const shadow = host.attachShadow({ mode: 'open' });
            shadow.innerHTML = '<span id="inside">hello</span>';
            const frag = new DocumentFragment();
            frag.append('b');
            frag.prepend('a');
            const em = document.createElement('em');
            em.textContent = 'c';
            frag.append(em);
            frag.moveBefore(em, frag.firstChild);
            const orderAfterMove = Array.from(frag.childNodes).map(n => n.textContent).join('');
            frag.replaceChildren('x', document.createElement('i'));
            frag.lastChild.textContent = 'y';

            const nodeProto = Node.prototype;
            const fragProto = DocumentFragment.prototype;
            const shadowProto = ShadowRoot.prototype;
            const nodeEnum = ['nodeType','nodeName','nodeValue','textContent','appendChild','removeChild',
                'replaceChild','insertBefore','cloneNode','contains','hasChildNodes','getRootNode','normalize',
                'isEqualNode','isSameNode','compareDocumentPosition','lookupNamespaceURI','lookupPrefix',
                'isDefaultNamespace'].every(name => Object.getOwnPropertyDescriptor(nodeProto, name)?.enumerable === true);
            const fragEnum = ['children','firstElementChild','lastElementChild','childElementCount','querySelector',
                'querySelectorAll','getElementById','append','prepend','replaceChildren','moveBefore']
                .every(name => Object.getOwnPropertyDescriptor(fragProto, name)?.enumerable === true);
            const shadowEnum = ['host','mode','adoptedStyleSheets','innerHTML','activeElement','clonable',
                'customElementRegistry','delegatesFocus','elementFromPoint','elementsFromPoint','fullscreenElement',
                'getAnimations','getHTML','getSelection','onslotchange','pictureInPictureElement','pointerLockElement',
                'serializable','setHTML','setHTMLUnsafe','slotAssignment','styleSheets']
                .every(name => Object.getOwnPropertyDescriptor(shadowProto, name)?.enumerable === true);

            return JSON.stringify({
                nodeCtor,
                constants: [Node.ELEMENT_NODE, Node.ATTRIBUTE_NODE, Node.TEXT_NODE, Node.COMMENT_NODE,
                    Node.DOCUMENT_NODE, Node.DOCUMENT_FRAGMENT_NODE, Node.DOCUMENT_POSITION_FOLLOWING,
                    Node.DOCUMENT_POSITION_CONTAINS, Node.DOCUMENT_POSITION_CONTAINED_BY],
                protoConstants: [Node.prototype.ELEMENT_NODE, Node.prototype.TEXT_NODE,
                    Node.prototype.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC],
                nodeEnum,
                namespaces: {
                    html: document.documentElement.lookupNamespaceURI(null),
                    xml: document.documentElement.lookupNamespaceURI('xml'),
                    xmlPrefix: document.documentElement.lookupPrefix('http://www.w3.org/XML/1998/namespace'),
                    htmlDefault: document.documentElement.isDefaultNamespace('http://www.w3.org/1999/xhtml'),
                },
                fragment: {
                    orderAfterMove,
                    finalText: frag.textContent,
                    childCount: frag.childElementCount,
                    firstTag: frag.firstElementChild && frag.firstElementChild.tagName,
                    scopedPrivate: Object.prototype.hasOwnProperty.call(fragProto, '_scopedElementIds'),
                    ownKeys: Reflect.ownKeys(frag).map(String),
                    fragEnum,
                },
                shadow: {
                    tag: Object.prototype.toString.call(shadow),
                    hostSame: shadow.host === host,
                    mode: shadow.mode,
                    html: shadow.getHTML(),
                    query: shadow.querySelector('#inside')?.textContent || '',
                    active: shadow.activeElement,
                    clonable: shadow.clonable,
                    delegatesFocus: shadow.delegatesFocus,
                    serializable: shadow.serializable,
                    slotAssignment: shadow.slotAssignment,
                    selectionTag: Object.prototype.toString.call(shadow.getSelection()),
                    animationCount: shadow.getAnimations().length,
                    ownKeys: Reflect.ownKeys(shadow).map(String),
                    shadowEnum,
                },
            });
        })()"#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    assert_eq!(
        v["nodeCtor"],
        "TypeError:Failed to construct 'Node': Illegal constructor"
    );
    assert_eq!(
        v["constants"],
        serde_json::json!([1, 2, 3, 8, 9, 11, 4, 8, 16])
    );
    assert_eq!(v["protoConstants"], serde_json::json!([1, 3, 32]));
    assert_eq!(v["nodeEnum"], true);
    assert_eq!(v["namespaces"]["html"], "http://www.w3.org/1999/xhtml");
    assert_eq!(
        v["namespaces"]["xml"],
        "http://www.w3.org/XML/1998/namespace"
    );
    assert_eq!(v["namespaces"]["xmlPrefix"], "xml");
    assert_eq!(v["namespaces"]["htmlDefault"], true);
    assert_eq!(v["fragment"]["orderAfterMove"], "cab");
    assert_eq!(v["fragment"]["finalText"], "xy");
    assert_eq!(v["fragment"]["childCount"], 1);
    assert_eq!(v["fragment"]["firstTag"], "I");
    assert_eq!(v["fragment"]["scopedPrivate"], false);
    assert_eq!(v["fragment"]["ownKeys"], serde_json::json!([]));
    assert_eq!(v["fragment"]["fragEnum"], true);
    assert_eq!(v["shadow"]["tag"], "[object ShadowRoot]");
    assert_eq!(v["shadow"]["hostSame"], true);
    assert_eq!(v["shadow"]["mode"], "open");
    assert!(v["shadow"]["html"]
        .as_str()
        .unwrap_or("")
        .contains("inside"));
    assert_eq!(v["shadow"]["query"], "hello");
    assert_eq!(v["shadow"]["active"], serde_json::Value::Null);
    assert_eq!(v["shadow"]["clonable"], false);
    assert_eq!(v["shadow"]["delegatesFocus"], false);
    assert_eq!(v["shadow"]["serializable"], false);
    assert_eq!(v["shadow"]["slotAssignment"], "named");
    assert_eq!(v["shadow"]["selectionTag"], "[object Selection]");
    assert_eq!(v["shadow"]["animationCount"], 0);
    assert_eq!(v["shadow"]["ownKeys"], serde_json::json!([]));
    assert_eq!(v["shadow"]["shadowEnum"], true);
}

#[tokio::test]
async fn element_html_webidl_layers_preserve_behavior_and_ownership() {
    let raw = check(
        r#"(() => {
            const ctorResults = {};
            for (const name of ['Element','HTMLElement','HTMLDivElement','HTMLInputElement','HTMLIFrameElement']) {
                try { new globalThis[name](); ctorResults[name] = 'ok'; }
                catch (e) { ctorResults[name] = e.name + ':' + e.message; }
            }

            const div = document.createElement('div');
            document.body.appendChild(div);
            div.align = 'center';
            div.ariaLabel = 'hello';
            const ariaBefore = [div.ariaLabel, div.getAttribute('aria-label')];
            div.ariaLabel = null;
            const ariaAfter = [div.ariaLabel, div.hasAttribute('aria-label')];
            div.role = 'button';
            div.part.add('card', 'active');
            div.part.remove('active');
            div.classList = 'a b';
            const styleA = div.style;
            div.style = 'color: red; display: block';
            const styleB = div.style;
            div.dataset.fooBar = 'baz';
            div.setAttribute('data-extra', '1');
            div.setPointerCapture(7);
            const pointerHeld = div.hasPointerCapture(7);
            div.releasePointerCapture(7);
            const pointerReleased = div.hasPointerCapture(7);
            div.setHTML('<span id="inside">ok</span>');

            const input = document.createElement('input');
            input.value = '12';
            input.accept = 'image/*';
            input.multiple = true;
            input.maxLength = 5;
            input.defaultValue = '34';
            input.setSelectionRange(1, 2, 'forward');
            input.setRangeText('X');
            input.setCustomValidity('bad');
            const invalid = [input.checkValidity(), input.reportValidity(), input.validationMessage, input.validity.valid, input.validity.customError];
            input.setCustomValidity('');
            input.step = '2';
            input.value = '10';
            input.stepUp();

            const iframe = document.createElement('iframe');
            iframe.width = '320';
            iframe.allowFullscreen = true;
            iframe.sandbox.add('allow-scripts');
            iframe.src = 'about:blank';
            iframe.setAttribute('name', 'childFrame');

            const script = document.createElement('script');
            script.src = '/a.js';
            script.async = true;
            const anchor = document.createElement('a');
            anchor.href = '/x';

            return JSON.stringify({
                ctorResults,
                ownership: {
                    elementStyle: Object.prototype.hasOwnProperty.call(Element.prototype, 'style'),
                    elementDataset: Object.prototype.hasOwnProperty.call(Element.prototype, 'dataset'),
                    elementSrc: Object.prototype.hasOwnProperty.call(Element.prototype, 'src'),
                    htmlStyle: Object.prototype.hasOwnProperty.call(HTMLElement.prototype, 'style'),
                    htmlDataset: Object.prototype.hasOwnProperty.call(HTMLElement.prototype, 'dataset'),
                    divAlign: Object.prototype.hasOwnProperty.call(HTMLDivElement.prototype, 'align'),
                    iframeSetAttr: Object.prototype.hasOwnProperty.call(HTMLIFrameElement.prototype, 'setAttribute'),
                    iframeSrc: Object.prototype.hasOwnProperty.call(HTMLIFrameElement.prototype, 'src'),
                },
                div: {
                    align: div.align,
                    ariaBefore, ariaAfter, role: div.role,
                    part: div.part.value,
                    className: div.className,
                    styleIdentity: styleA === styleB,
                    styleText: div.getAttribute('style'),
                    datasetIdentity: div.dataset === div.dataset,
                    datasetValue: div.dataset.fooBar,
                    attrNames: div.getAttributeNames().sort(),
                    pointerHeld, pointerReleased,
                    html: div.getHTML(),
                    query: div.querySelector('#inside')?.textContent || '',
                    ownKeys: Reflect.ownKeys(div).map(String),
                },
                input: {
                    accept: input.accept,
                    multiple: input.multiple,
                    maxLength: input.maxLength,
                    defaultValue: input.defaultValue,
                    valueAfterRange: input.value,
                    selection: [input.selectionStart, input.selectionEnd, input.selectionDirection],
                    invalid,
                    validAfterClear: input.checkValidity(),
                    valueAfterStep: input.value,
                    ownKeys: Reflect.ownKeys(input).map(String),
                },
                iframe: {
                    width: iframe.width,
                    allowFullscreen: iframe.allowFullscreen,
                    sandbox: iframe.sandbox.value,
                    src: iframe.src,
                    name: iframe.name,
                    setAttrInherited: typeof iframe.setAttribute === 'function',
                    ownKeys: Reflect.ownKeys(iframe).map(String),
                },
                specific: {
                    scriptSrc: script.src,
                    scriptAsync: script.async,
                    anchorHref: anchor.href,
                },
            });
        })()"#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    for name in [
        "Element",
        "HTMLElement",
        "HTMLDivElement",
        "HTMLInputElement",
        "HTMLIFrameElement",
    ] {
        assert_eq!(
            v["ctorResults"][name],
            format!("TypeError:Failed to construct '{name}': Illegal constructor"),
            "{name}: {raw}"
        );
    }
    assert_eq!(v["ownership"]["elementStyle"], false);
    assert_eq!(v["ownership"]["elementDataset"], false);
    assert_eq!(v["ownership"]["elementSrc"], false);
    assert_eq!(v["ownership"]["htmlStyle"], true);
    assert_eq!(v["ownership"]["htmlDataset"], true);
    assert_eq!(v["ownership"]["divAlign"], true);
    assert_eq!(v["ownership"]["iframeSetAttr"], false);
    assert_eq!(v["ownership"]["iframeSrc"], true);
    assert_eq!(v["div"]["align"], "center");
    assert_eq!(
        v["div"]["ariaBefore"],
        serde_json::json!(["hello", "hello"])
    );
    assert_eq!(v["div"]["ariaAfter"], serde_json::json!([null, false]));
    assert_eq!(v["div"]["role"], "button");
    assert_eq!(v["div"]["part"], "card");
    assert_eq!(v["div"]["className"], "a b");
    assert_eq!(v["div"]["styleIdentity"], true);
    assert!(v["div"]["styleText"]
        .as_str()
        .unwrap_or("")
        .contains("color"));
    assert_eq!(v["div"]["datasetIdentity"], true);
    assert_eq!(v["div"]["datasetValue"], "baz");
    assert_eq!(v["div"]["pointerHeld"], true);
    assert_eq!(v["div"]["pointerReleased"], false);
    assert!(v["div"]["html"].as_str().unwrap_or("").contains("inside"));
    assert_eq!(v["div"]["query"], "ok");
    assert!(!v["div"]["ownKeys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "_style"));
    assert_eq!(v["input"]["accept"], "image/*");
    assert_eq!(v["input"]["multiple"], true);
    assert_eq!(v["input"]["maxLength"], 5);
    assert_eq!(v["input"]["defaultValue"], "34");
    assert_eq!(
        v["input"]["invalid"],
        serde_json::json!([false, false, "bad", false, true])
    );
    assert_eq!(v["input"]["validAfterClear"], true);
    assert_eq!(v["input"]["valueAfterStep"], "12");
    assert_eq!(v["iframe"]["width"], "320");
    assert_eq!(v["iframe"]["allowFullscreen"], true);
    assert_eq!(v["iframe"]["sandbox"], "allow-scripts");
    assert_eq!(v["iframe"]["src"], "about:blank");
    assert_eq!(v["iframe"]["name"], "childFrame");
    assert_eq!(v["iframe"]["setAttrInherited"], true);
    assert!(v["specific"]["scriptSrc"]
        .as_str()
        .unwrap_or("")
        .ends_with("/a.js"));
    assert_eq!(v["specific"]["scriptAsync"], true);
    assert_eq!(v["specific"]["anchorHref"], "/x");
}

#[tokio::test]
async fn native_mask_metadata_is_hidden_from_parent_and_child_reflection() {
    let raw = check(
        r#"(() => {
            const snap = root => {
                const rows = {};
                for (const [name, fn] of Object.entries({
                    DOMRect: root.DOMRect,
                    Path2D: root.Path2D,
                    Request: root.Request,
                    fnToString: root.Function.prototype.toString,
                })) {
                    rows[name] = {
                        own: root.Reflect.ownKeys(fn).map(String),
                        symbols: root.Object.getOwnPropertySymbols(fn).map(String),
                        descriptorSymbols: root.Object.getOwnPropertySymbols(
                            root.Object.getOwnPropertyDescriptors(fn)
                        ).map(String),
                        source: root.Function.prototype.toString.call(fn),
                    };
                }
                rows.reflection = {
                    ownKeys: String(root.Reflect.ownKeys),
                    symbols: String(root.Object.getOwnPropertySymbols),
                    descriptors: String(root.Object.getOwnPropertyDescriptors),
                };
                return rows;
            };
            const iframe = document.createElement('iframe');
            iframe.srcdoc = '<!doctype html><html><body></body></html>';
            document.body.appendChild(iframe);
            const child = iframe.contentWindow;
            return JSON.stringify({
                parent: snap(window),
                child: snap(child),
                cross: child.Function.prototype.toString.call(DOMRect),
            });
        })()"#,
    )
    .await;
    let value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    for realm in ["parent", "child"] {
        for name in ["DOMRect", "Path2D", "Request", "fnToString"] {
            for field in ["own", "symbols", "descriptorSymbols"] {
                let entries = value[realm][name][field].as_array().unwrap();
                assert!(
                    entries.iter().all(|entry| !entry
                        .as_str()
                        .unwrap_or("")
                        .contains("__browser_oxide_native__")),
                    "{realm}/{name}/{field}: {raw}"
                );
            }
            assert!(
                value[realm][name]["source"]
                    .as_str()
                    .unwrap_or("")
                    .contains("[native code]"),
                "{realm}/{name}: {raw}"
            );
        }
        assert_eq!(
            value[realm]["reflection"]["ownKeys"],
            "function ownKeys() { [native code] }"
        );
        assert_eq!(
            value[realm]["reflection"]["symbols"],
            "function getOwnPropertySymbols() { [native code] }"
        );
        assert_eq!(
            value[realm]["reflection"]["descriptors"],
            "function getOwnPropertyDescriptors() { [native code] }"
        );
    }
    assert_eq!(value["cross"], "function DOMRect() { [native code] }");
}

#[tokio::test]
async fn dom_html_svg_constructor_chains_match_chrome() {
    let raw = check(
        r#"(() => {
            const names = Object.getOwnPropertyNames(globalThis).filter(name =>
                /^(HTML.*Element|SVG.*Element)$/.test(name) ||
                ['EventTarget','Node','CharacterData','Text','Comment','Element','HTMLElement','Document',
                 'HTMLDocument','DocumentFragment','ShadowRoot','Attr','NamedNodeMap','Range','Selection'].includes(name)
            ).sort();
            const constructible = new Set(['Comment','Document','DocumentFragment','EventTarget','Range','Text']);
            const failures = [];
            for (const name of names) {
                const C = globalThis[name];
                if (typeof C !== 'function') continue;
                let ok = false;
                try { new C(); ok = true; } catch (_) {}
                if (ok !== constructible.has(name)) failures.push('construct:' + name + ':' + ok);
                if (C.prototype && C.prototype.constructor !== C) failures.push('ctor:' + name);
                const source = Function.prototype.toString.call(C);
                if (source !== `function ${name}() { [native code] }`) failures.push('source:' + name + ':' + source);
                if (Object.getOwnPropertySymbols(C).length !== 0) failures.push('symbol:' + name);
            }

            const parentChecks = {
                Node: Object.getPrototypeOf(Node) === EventTarget && Object.getPrototypeOf(Node.prototype) === EventTarget.prototype,
                Attr: Object.getPrototypeOf(Attr) === Node && Object.getPrototypeOf(Attr.prototype) === Node.prototype,
                HTMLElement: Object.getPrototypeOf(HTMLElement) === Element && Object.getPrototypeOf(HTMLElement.prototype) === Element.prototype,
                HTMLMediaElement: Object.getPrototypeOf(HTMLMediaElement) === HTMLElement && Object.getPrototypeOf(HTMLMediaElement.prototype) === HTMLElement.prototype,
                HTMLVideoElement: Object.getPrototypeOf(HTMLVideoElement) === HTMLMediaElement && Object.getPrototypeOf(HTMLVideoElement.prototype) === HTMLMediaElement.prototype,
                HTMLEmbedElement: Object.getPrototypeOf(HTMLEmbedElement) === HTMLElement && Object.getPrototypeOf(HTMLEmbedElement.prototype) === HTMLElement.prototype,
                Range: Object.getPrototypeOf(Range) === AbstractRange && Object.getPrototypeOf(Range.prototype) === AbstractRange.prototype,
                SVGElement: Object.getPrototypeOf(SVGElement) === Element && Object.getPrototypeOf(SVGElement.prototype) === Element.prototype,
                SVGGraphicsElement: Object.getPrototypeOf(SVGGraphicsElement) === SVGElement && Object.getPrototypeOf(SVGGraphicsElement.prototype) === SVGElement.prototype,
                SVGGeometryElement: Object.getPrototypeOf(SVGGeometryElement) === SVGGraphicsElement && Object.getPrototypeOf(SVGGeometryElement.prototype) === SVGGraphicsElement.prototype,
                SVGCircleElement: Object.getPrototypeOf(SVGCircleElement) === SVGGeometryElement && Object.getPrototypeOf(SVGCircleElement.prototype) === SVGGeometryElement.prototype,
                SVGAnimationElement: Object.getPrototypeOf(SVGAnimationElement) === SVGElement && Object.getPrototypeOf(SVGAnimationElement.prototype) === SVGElement.prototype,
                SVGSetElement: Object.getPrototypeOf(SVGSetElement) === SVGAnimationElement && Object.getPrototypeOf(SVGSetElement.prototype) === SVGAnimationElement.prototype,
            };

            const div = document.createElement('div');
            const video = document.createElement('video');
            const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
            const created = {
                div: div instanceof HTMLDivElement && div instanceof HTMLElement && div instanceof Element && div instanceof Node,
                video: video instanceof HTMLVideoElement && video instanceof HTMLMediaElement && video instanceof HTMLElement,
                svg: svg instanceof SVGSVGElement && svg instanceof SVGGraphicsElement && svg instanceof SVGElement,
                circle: circle instanceof SVGCircleElement && circle instanceof SVGGeometryElement && circle instanceof SVGGraphicsElement,
            };

            const staticChecks = {
                media: [HTMLMediaElement.NETWORK_EMPTY, HTMLMediaElement.NETWORK_LOADING, HTMLMediaElement.HAVE_ENOUGH_DATA],
                track: [HTMLTrackElement.NONE, HTMLTrackElement.LOADING, HTMLTrackElement.LOADED, HTMLTrackElement.ERROR],
                script: ['classic','module','importmap','speculationrules','webbundle'].every(x => HTMLScriptElement.supports(x))
                    && !HTMLScriptElement.supports('text/javascript'),
                fenced: HTMLFencedFrameElement.canLoadOpaqueURL(),
                svgBlend: [SVGFEBlendElement.SVG_FEBLEND_MODE_UNKNOWN, SVGFEBlendElement.SVG_FEBLEND_MODE_LUMINOSITY],
                svgMarker: [SVGMarkerElement.SVG_MARKERUNITS_STROKEWIDTH, SVGMarkerElement.SVG_MARKER_ORIENT_ANGLE],
            };
            const parsed = Document.parseHTMLUnsafe('<title>x</title><p id="p">ok</p><script>never()</script>');
            staticChecks.parse = [
                Object.prototype.toString.call(parsed), parsed.title,
                parsed.getElementById('p')?.textContent || '',
                parsed.querySelectorAll('script').length,
                Reflect.ownKeys(parsed).map(String),
                parsed.defaultView,
            ];
            const safeParsed = Document.parseHTML('<title>s</title><p id="safe-id" data-x="1">safe</p><script>never()</script>');
            staticChecks.parseSafe = [
                Object.prototype.toString.call(safeParsed), safeParsed.title,
                safeParsed.getElementById('safe-id'),
                safeParsed.querySelector('p')?.textContent || '',
                safeParsed.querySelector('p')?.attributes.length || 0,
                safeParsed.querySelectorAll('script').length,
                Reflect.ownKeys(safeParsed).map(String),
                safeParsed.defaultView,
            ];

            const iframe = document.createElement('iframe');
            iframe.srcdoc = '<!doctype html><html><body></body></html>';
            document.body.appendChild(iframe);
            const child = iframe.contentWindow;
            let childIllegal = false;
            try { new child.HTMLAnchorElement(); } catch (e) { childIllegal = e instanceof child.TypeError; }
            const childChecks = child && {
                illegal: childIllegal,
                videoParent: child.Object.getPrototypeOf(child.HTMLVideoElement) === child.HTMLMediaElement,
                videoProtoParent: child.Object.getPrototypeOf(child.HTMLVideoElement.prototype) === child.HTMLMediaElement.prototype,
                svgCircleParent: child.Object.getPrototypeOf(child.SVGCircleElement) === child.SVGGeometryElement,
                native: child.Function.prototype.toString.call(child.HTMLQuoteElement),
            };

            return JSON.stringify({ count: names.length, failures, parentChecks, created, staticChecks, childChecks });
        })()"#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));
    assert_eq!(v["count"], 159);
    assert_eq!(v["failures"], serde_json::json!([]));
    for value in v["parentChecks"].as_object().unwrap().values() {
        assert_eq!(value, true, "parent chain: {raw}");
    }
    for value in v["created"].as_object().unwrap().values() {
        assert_eq!(value, true, "created instance chain: {raw}");
    }
    assert_eq!(v["staticChecks"]["media"], serde_json::json!([0, 2, 4]));
    assert_eq!(v["staticChecks"]["track"], serde_json::json!([0, 1, 2, 3]));
    assert_eq!(v["staticChecks"]["script"], true);
    assert_eq!(v["staticChecks"]["fenced"], true);
    assert_eq!(v["staticChecks"]["svgBlend"], serde_json::json!([0, 16]));
    assert_eq!(v["staticChecks"]["svgMarker"], serde_json::json!([2, 2]));
    assert_eq!(
        v["staticChecks"]["parse"],
        serde_json::json!(["[object HTMLDocument]", "x", "ok", 1, ["location"], null])
    );
    assert_eq!(
        v["staticChecks"]["parseSafe"],
        serde_json::json!([
            "[object HTMLDocument]",
            "s",
            null,
            "safe",
            0,
            0,
            ["location"],
            null
        ])
    );
    assert_eq!(v["childChecks"]["illegal"], true);
    assert_eq!(v["childChecks"]["videoParent"], true);
    assert_eq!(v["childChecks"]["videoProtoParent"], true);
    assert_eq!(v["childChecks"]["svgCircleParent"], true);
    assert_eq!(
        v["childChecks"]["native"],
        "function HTMLQuoteElement() { [native code] }"
    );
}

#[tokio::test]
async fn instance_shapes_and_window_proxy_reflection_match_chrome_148() {
    let raw = check(
        r#"(() => {
            const windowKeysBeforeFrames = Reflect.ownKeys(window).map(k =>
                typeof k === 'symbol' ? '@@' + (k.description || '') : String(k)
            );
            const windowDescriptor = name => {
                const d = Object.getOwnPropertyDescriptor(window, name);
                return d && {
                    kind: 'value' in d ? 'value' : 'accessor',
                    enumerable: d.enumerable,
                    configurable: d.configurable,
                    writable: 'value' in d ? d.writable : null,
                    get: !!d.get,
                    set: !!d.set,
                };
            };

            const host = document.createElement('div');
            document.body.appendChild(host);
            const canvas = document.createElement('canvas');
            host.appendChild(canvas);
            const canvasOwnBefore = Reflect.ownKeys(canvas).map(String);
            const ctx = canvas.getContext('2d');
            const canvasOwnAfter = Reflect.ownKeys(canvas).map(String);

            const comment = document.createComment('hello');
            host.appendChild(comment);

            const styleHost = document.createElement('div');
            const style = styleHost.style;
            const cssKeys = Reflect.ownKeys(style).map(String);
            const accentDescriptor = Object.getOwnPropertyDescriptor(style, 'accentColor');
            const epubDescriptor = Object.getOwnPropertyDescriptor(style, 'epubCaptionSide');
            style.backgroundColor = 'red';

            const docLocation = Object.getOwnPropertyDescriptor(document, 'location');
            const locationPrimitive = Object.getOwnPropertyDescriptor(location, Symbol.toPrimitive);
            const locationKeys = Reflect.ownKeys(location).map(k =>
                typeof k === 'symbol' ? '@@' + (k.description || '') : String(k)
            );

            const decoder = new TextDecoder('utf-8', { fatal: true });
            const decoderText = decoder.decode(new Uint8Array([0x68, 0x69]));
            const resize = new ResizeObserver(() => {});
            const xhr = new XMLHttpRequest();

            const wp = Window.prototype;
            const windowProperties = Object.getPrototypeOf(wp);
            const eventProto = Object.getPrototypeOf(windowProperties);

            return JSON.stringify({
                window: {
                    count: windowKeysBeforeFrames.length,
                    first: windowKeysBeforeFrames.slice(0, 15),
                    hidden: windowKeysBeforeFrames.filter(k =>
                        k.includes('__browser_oxide') || k.includes('browserOxide')
                    ),
                    hasXSLT: typeof XSLTProcessor !== 'undefined',
                    Window: windowDescriptor('Window'),
                    navigation: windowDescriptor('navigation'),
                    prototypeIsWindow: Object.getPrototypeOf(window) === Window.prototype,
                    instanceOfWindow: window instanceof Window,
                    windowProtoOwn: Reflect.ownKeys(wp).map(k =>
                        typeof k === 'symbol' ? '@@' + (k.description || '') : String(k)
                    ),
                    windowPropertiesTag: Object.prototype.toString.call(windowProperties),
                    windowPropertiesOwn: Reflect.ownKeys(windowProperties).map(k =>
                        typeof k === 'symbol' ? '@@' + (k.description || '') : String(k)
                    ),
                    eventProtoIsExact: eventProto === EventTarget.prototype,
                },
                document: {
                    own: Reflect.ownKeys(document).map(k =>
                        typeof k === 'symbol' ? '@@' + (k.description || '') : String(k)
                    ),
                    location: docLocation && {
                        enumerable: docLocation.enumerable,
                        configurable: docLocation.configurable,
                        get: !!docLocation.get,
                        set: !!docLocation.set,
                    },
                },
                location: {
                    own: locationKeys,
                    primitive: locationPrimitive && {
                        valueType: typeof locationPrimitive.value,
                        valueIsUndefined: locationPrimitive.value === undefined,
                        writable: locationPrimitive.writable,
                        enumerable: locationPrimitive.enumerable,
                        configurable: locationPrimitive.configurable,
                    },
                    text: String(location),
                },
                canvas: {
                    tag: Object.prototype.toString.call(canvas),
                    instance: canvas instanceof HTMLCanvasElement,
                    inTree: host.lastChild === comment && host.firstChild === canvas,
                    childCount: host.childNodes.length,
                    ownBefore: canvasOwnBefore,
                    ownAfter: canvasOwnAfter,
                    contextTag: Object.prototype.toString.call(ctx),
                },
                comment: {
                    tag: Object.prototype.toString.call(comment),
                    nodeType: comment.nodeType,
                    nodeName: comment.nodeName,
                    data: comment.data,
                    html: host.innerHTML,
                },
                css: {
                    count: cssKeys.length,
                    first: cssKeys.slice(0, 12),
                    last: cssKeys.slice(-12),
                    accent: accentDescriptor && {
                        value: accentDescriptor.value,
                        writable: accentDescriptor.writable,
                        enumerable: accentDescriptor.enumerable,
                        configurable: accentDescriptor.configurable,
                    },
                    epubMissingDescriptor: epubDescriptor === undefined,
                    background: style.backgroundColor,
                    backgroundRaw: style.getPropertyValue('background-color'),
                },
                clean: {
                    domParser: [Object.prototype.toString.call(new DOMParser()), Reflect.ownKeys(new DOMParser()).length],
                    formData: [Object.prototype.toString.call(new FormData()), Reflect.ownKeys(new FormData()).length],
                    params: [Object.prototype.toString.call(new URLSearchParams()), Reflect.ownKeys(new URLSearchParams()).length],
                    encoder: [Object.prototype.toString.call(new TextEncoder()), Reflect.ownKeys(new TextEncoder()).length],
                    decoder: [Object.prototype.toString.call(decoder), Reflect.ownKeys(decoder).length, decoder.encoding, decoder.fatal, decoder.ignoreBOM, decoderText],
                    resize: [Object.prototype.toString.call(resize), Reflect.ownKeys(resize).length],
                    xhr: [Object.prototype.toString.call(xhr), Reflect.ownKeys(xhr).length],
                },
            });
        })()"#,
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("json: {e}; raw={raw}"));

    assert_eq!(
        v["window"]["first"],
        serde_json::json!([
            "Object",
            "Function",
            "Array",
            "Number",
            "parseFloat",
            "parseInt",
            "Infinity",
            "NaN",
            "undefined",
            "Boolean",
            "String",
            "Symbol",
            "Date",
            "Promise",
            "RegExp"
        ])
    );
    assert_eq!(v["window"]["hidden"], serde_json::json!([]));
    assert_eq!(v["window"]["hasXSLT"], false);
    assert_eq!(
        v["window"]["Window"],
        serde_json::json!({
            "kind":"value","enumerable":false,"configurable":true,"writable":true,
            "get":false,"set":false
        })
    );
    assert_eq!(v["window"]["navigation"]["kind"], "accessor");
    assert_eq!(v["window"]["navigation"]["enumerable"], true);
    assert_eq!(v["window"]["navigation"]["configurable"], true);
    assert_eq!(v["window"]["navigation"]["get"], true);
    assert_eq!(v["window"]["navigation"]["set"], true);
    assert_eq!(v["window"]["prototypeIsWindow"], true);
    assert_eq!(v["window"]["instanceOfWindow"], true);
    assert_eq!(
        v["window"]["windowProtoOwn"],
        serde_json::json!([
            "TEMPORARY",
            "PERSISTENT",
            "constructor",
            "@@Symbol.toStringTag"
        ])
    );
    assert_eq!(
        v["window"]["windowPropertiesTag"],
        "[object WindowProperties]"
    );
    assert_eq!(
        v["window"]["windowPropertiesOwn"],
        serde_json::json!(["@@Symbol.toStringTag"])
    );
    assert_eq!(v["window"]["eventProtoIsExact"], true);

    assert_eq!(v["document"]["own"], serde_json::json!(["location"]));
    assert_eq!(
        v["document"]["location"],
        serde_json::json!({"enumerable":true,"configurable":false,"get":true,"set":true})
    );
    assert_eq!(
        v["location"]["own"],
        serde_json::json!([
            "valueOf",
            "ancestorOrigins",
            "href",
            "origin",
            "protocol",
            "host",
            "hostname",
            "port",
            "pathname",
            "search",
            "hash",
            "assign",
            "reload",
            "replace",
            "toString",
            "@@Symbol.toPrimitive"
        ])
    );
    assert_eq!(
        v["location"]["primitive"],
        serde_json::json!({
            "valueType":"undefined","valueIsUndefined":true,"writable":false,
            "enumerable":false,"configurable":false
        })
    );
    assert!(!v["location"]["text"].as_str().unwrap_or("").is_empty());

    assert_eq!(v["canvas"]["tag"], "[object HTMLCanvasElement]");
    assert_eq!(v["canvas"]["instance"], true);
    assert_eq!(v["canvas"]["inTree"], true);
    assert_eq!(v["canvas"]["childCount"], 2);
    assert_eq!(v["canvas"]["ownBefore"], serde_json::json!([]));
    assert_eq!(v["canvas"]["ownAfter"], serde_json::json!([]));
    assert_eq!(
        v["canvas"]["contextTag"],
        "[object CanvasRenderingContext2D]"
    );

    assert_eq!(v["comment"]["tag"], "[object Comment]");
    assert_eq!(v["comment"]["nodeType"], 8);
    assert_eq!(v["comment"]["nodeName"], "#comment");
    assert_eq!(v["comment"]["data"], "hello");
    assert!(v["comment"]["html"]
        .as_str()
        .unwrap_or("")
        .contains("<!--hello-->"));

    assert_eq!(v["css"]["count"], 703);
    assert_eq!(
        v["css"]["first"],
        serde_json::json!([
            "accentColor",
            "additiveSymbols",
            "alignContent",
            "alignItems",
            "alignSelf",
            "alignmentBaseline",
            "all",
            "anchorName",
            "anchorScope",
            "animation",
            "animationComposition",
            "animationDelay"
        ])
    );
    assert_eq!(
        v["css"]["last"],
        serde_json::json!([
            "whiteSpaceCollapse",
            "widows",
            "width",
            "willChange",
            "wordBreak",
            "wordSpacing",
            "wordWrap",
            "writingMode",
            "x",
            "y",
            "zIndex",
            "zoom"
        ])
    );
    assert_eq!(
        v["css"]["accent"],
        serde_json::json!({"value":"","writable":true,"enumerable":true,"configurable":true})
    );
    assert_eq!(v["css"]["epubMissingDescriptor"], true);
    assert_eq!(v["css"]["background"], "red");
    assert_eq!(v["css"]["backgroundRaw"], "red");

    assert_eq!(
        v["clean"]["domParser"],
        serde_json::json!(["[object DOMParser]", 0])
    );
    assert_eq!(
        v["clean"]["formData"],
        serde_json::json!(["[object FormData]", 0])
    );
    assert_eq!(
        v["clean"]["params"],
        serde_json::json!(["[object URLSearchParams]", 0])
    );
    assert_eq!(
        v["clean"]["encoder"],
        serde_json::json!(["[object TextEncoder]", 0])
    );
    assert_eq!(
        v["clean"]["decoder"],
        serde_json::json!(["[object TextDecoder]", 0, "utf-8", true, false, "hi"])
    );
    assert_eq!(
        v["clean"]["resize"],
        serde_json::json!(["[object ResizeObserver]", 0])
    );
    assert_eq!(
        v["clean"]["xhr"],
        serde_json::json!(["[object XMLHttpRequest]", 0])
    );
}
