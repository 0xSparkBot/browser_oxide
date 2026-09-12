use browser_oxide::Page;

#[tokio::test]
async fn webgl2_core_lifecycle_matches_chrome() {
    let mut page = Page::from_html(
        "<!doctype html><html><body><canvas id='c'></canvas></body></html>",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    let result = page
        .evaluate(
            r#"
            (() => {
                const gl = document.getElementById('c').getContext('webgl2');
                const tag = value => Object.prototype.toString.call(value);
                const own = value => Reflect.ownKeys(value).map(String);
                const call = fn => { try { return fn(); } catch (e) { return `${e.name}:${e.message}`; } };
                const methodNames = [
                    'createVertexArray','bindVertexArray','deleteVertexArray','isVertexArray',
                    'createQuery','beginQuery','endQuery','getQuery','getQueryParameter','deleteQuery','isQuery',
                    'createSampler','bindSampler','samplerParameteri','samplerParameterf','getSamplerParameter','deleteSampler','isSampler',
                    'createTransformFeedback','bindTransformFeedback','beginTransformFeedback','endTransformFeedback',
                    'pauseTransformFeedback','resumeTransformFeedback','deleteTransformFeedback','isTransformFeedback',
                    'fenceSync','clientWaitSync','waitSync','getSyncParameter','deleteSync','isSync'
                ];
                const arities = Object.fromEntries(methodNames.map(name => [name, gl[name].length]));
                const descriptors = Object.fromEntries(methodNames.map(name => {
                    const d = Object.getOwnPropertyDescriptor(WebGL2RenderingContext.prototype, name);
                    return [name, [d.enumerable, d.configurable, d.writable, String(d.value).includes('[native code]')]];
                }));

                const vao = gl.createVertexArray();
                const vaoState = { tag: tag(vao), own: own(vao), initial: gl.isVertexArray(vao), binding0: gl.getParameter(gl.VERTEX_ARRAY_BINDING) };
                gl.bindVertexArray(vao);
                vaoState.bound = gl.isVertexArray(vao);
                vaoState.binding = gl.getParameter(gl.VERTEX_ARRAY_BINDING) === vao;
                gl.deleteVertexArray(vao);
                vaoState.deleted = gl.isVertexArray(vao);
                vaoState.bindingAfter = gl.getParameter(gl.VERTEX_ARRAY_BINDING);

                const query = gl.createQuery();
                const queryState = { tag: tag(query), own: own(query), initial: gl.isQuery(query) };
                gl.beginQuery(gl.ANY_SAMPLES_PASSED, query);
                queryState.started = gl.isQuery(query);
                queryState.current = gl.getQuery(gl.ANY_SAMPLES_PASSED, gl.CURRENT_QUERY) === query;
                gl.endQuery(gl.ANY_SAMPLES_PASSED);
                queryState.currentAfter = gl.getQuery(gl.ANY_SAMPLES_PASSED, gl.CURRENT_QUERY);
                queryState.available = gl.getQueryParameter(query, gl.QUERY_RESULT_AVAILABLE);
                queryState.result = gl.getQueryParameter(query, gl.QUERY_RESULT);
                gl.deleteQuery(query);
                queryState.deleted = gl.isQuery(query);

                const sampler = gl.createSampler();
                const samplerState = {
                    tag: tag(sampler), own: own(sampler), initial: gl.isSampler(sampler),
                    defaults: [
                        gl.getSamplerParameter(sampler, gl.TEXTURE_MIN_FILTER),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_MAG_FILTER),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_WRAP_S),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_WRAP_T),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_WRAP_R),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_MIN_LOD),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_MAX_LOD),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_COMPARE_MODE),
                        gl.getSamplerParameter(sampler, gl.TEXTURE_COMPARE_FUNC),
                    ]
                };
                gl.activeTexture(gl.TEXTURE3);
                gl.bindSampler(3, sampler);
                samplerState.binding = gl.getParameter(gl.SAMPLER_BINDING) === sampler;
                gl.samplerParameteri(sampler, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
                samplerState.parameter = gl.getSamplerParameter(sampler, gl.TEXTURE_MIN_FILTER);
                gl.deleteSampler(sampler);
                samplerState.deleted = gl.isSampler(sampler);
                samplerState.bindingAfter = gl.getParameter(gl.SAMPLER_BINDING);

                const tf = gl.createTransformFeedback();
                const tfState = {
                    tag: tag(tf), own: own(tf), initial: gl.isTransformFeedback(tf),
                    binding0: gl.getParameter(gl.TRANSFORM_FEEDBACK_BINDING)
                };
                gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, tf);
                tfState.bound = gl.isTransformFeedback(tf);
                tfState.binding = gl.getParameter(gl.TRANSFORM_FEEDBACK_BINDING) === tf;
                gl.beginTransformFeedback(gl.POINTS);
                tfState.noProgramError = gl.getError();
                tfState.activeWithoutProgram = gl.getParameter(gl.TRANSFORM_FEEDBACK_ACTIVE);
                gl.deleteTransformFeedback(tf);
                tfState.deleted = gl.isTransformFeedback(tf);
                tfState.bindingAfter = gl.getParameter(gl.TRANSFORM_FEEDBACK_BINDING);

                const sync = gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE, 0);
                const syncState = {
                    tag: tag(sync), own: own(sync), initial: gl.isSync(sync),
                    parameters: [
                        gl.getSyncParameter(sync, gl.OBJECT_TYPE),
                        gl.getSyncParameter(sync, gl.SYNC_CONDITION),
                        gl.getSyncParameter(sync, gl.SYNC_STATUS),
                        gl.getSyncParameter(sync, gl.SYNC_FLAGS),
                    ],
                    wait0: gl.clientWaitSync(sync, 0, 0),
                };
                gl.deleteSync(sync);
                syncState.deleted = gl.isSync(sync);

                const errors = {
                    vao: call(() => gl.isVertexArray({})),
                    query: call(() => gl.isQuery({})),
                    sampler: call(() => gl.isSampler({})),
                    tf: call(() => gl.isTransformFeedback({})),
                    sync: call(() => gl.isSync({})),
                };

                return JSON.stringify({
                    separateInterfaces:
                        WebGLRenderingContext !== WebGL2RenderingContext &&
                        !(gl instanceof WebGLRenderingContext) &&
                        gl instanceof WebGL2RenderingContext &&
                        Object.getPrototypeOf(WebGL2RenderingContext.prototype) === Object.prototype &&
                        Object.getPrototypeOf(WebGL2RenderingContext) === Function.prototype,
                    arities, descriptors, vaoState, queryState, samplerState, tfState, syncState, errors,
                });
            })()
            "#,
        )
        .unwrap();

    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(value["separateInterfaces"], true);

    let expected_arities = serde_json::json!({
        "createVertexArray":0,"bindVertexArray":1,"deleteVertexArray":1,"isVertexArray":1,
        "createQuery":0,"beginQuery":2,"endQuery":1,"getQuery":2,"getQueryParameter":2,"deleteQuery":1,"isQuery":1,
        "createSampler":0,"bindSampler":2,"samplerParameteri":3,"samplerParameterf":3,"getSamplerParameter":2,"deleteSampler":1,"isSampler":1,
        "createTransformFeedback":0,"bindTransformFeedback":2,"beginTransformFeedback":1,"endTransformFeedback":0,
        "pauseTransformFeedback":0,"resumeTransformFeedback":0,"deleteTransformFeedback":1,"isTransformFeedback":1,
        "fenceSync":2,"clientWaitSync":3,"waitSync":3,"getSyncParameter":2,"deleteSync":1,"isSync":1
    });
    assert_eq!(value["arities"], expected_arities);
    for descriptor in value["descriptors"].as_object().unwrap().values() {
        assert_eq!(descriptor, &serde_json::json!([true, true, true, true]));
    }

    assert_eq!(
        value["vaoState"],
        serde_json::json!({
            "tag":"[object WebGLVertexArrayObject]","own":[],"initial":false,"binding0":null,
            "bound":true,"binding":true,"deleted":false,"bindingAfter":null
        })
    );
    assert_eq!(
        value["queryState"],
        serde_json::json!({
            "tag":"[object WebGLQuery]","own":[],"initial":false,"started":true,"current":true,
            "currentAfter":null,"available":false,"result":0,"deleted":false
        })
    );
    assert_eq!(
        value["samplerState"],
        serde_json::json!({
            "tag":"[object WebGLSampler]","own":[],"initial":true,
            "defaults":[9986,9729,10497,10497,10497,-1000,1000,0,515],
            "binding":true,"parameter":9728,"deleted":false,"bindingAfter":null
        })
    );
    assert_eq!(
        value["tfState"],
        serde_json::json!({
            "tag":"[object WebGLTransformFeedback]","own":[],"initial":false,"binding0":null,
            "bound":true,"binding":true,"noProgramError":1282,"activeWithoutProgram":false,
            "deleted":false,"bindingAfter":null
        })
    );
    assert_eq!(
        value["syncState"],
        serde_json::json!({
            "tag":"[object WebGLSync]","own":[],"initial":true,
            "parameters":[37142,37143,37144,0],"wait0":37147,"deleted":false
        })
    );

    assert_eq!(
        value["errors"]["vao"],
        "TypeError:Failed to execute 'isVertexArray' on 'WebGL2RenderingContext': parameter 1 is not of type 'WebGLVertexArrayObject'."
    );
    assert_eq!(
        value["errors"]["query"],
        "TypeError:Failed to execute 'isQuery' on 'WebGL2RenderingContext': parameter 1 is not of type 'WebGLQuery'."
    );
    assert_eq!(
        value["errors"]["sampler"],
        "TypeError:Failed to execute 'isSampler' on 'WebGL2RenderingContext': parameter 1 is not of type 'WebGLSampler'."
    );
    assert_eq!(
        value["errors"]["tf"],
        "TypeError:Failed to execute 'isTransformFeedback' on 'WebGL2RenderingContext': parameter 1 is not of type 'WebGLTransformFeedback'."
    );
    assert_eq!(
        value["errors"]["sync"],
        "TypeError:Failed to execute 'isSync' on 'WebGL2RenderingContext': parameter 1 is not of type 'WebGLSync'."
    );
}
