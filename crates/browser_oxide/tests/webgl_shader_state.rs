use browser_oxide::Page;

#[tokio::test(flavor = "current_thread")]
async fn webgl_shader_and_program_lifecycle_matches_chrome() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><canvas id='c'></canvas>",
        "https://example.test/webgl-shader-state",
        Some(profile),
    )
    .await
    .expect("page");

    let result = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl');
                const out = {};
                out.context = {
                    own: Reflect.ownKeys(gl).map(String),
                    canvas: gl.canvas === document.getElementById('c'),
                    drawingBuffer: [gl.drawingBufferWidth, gl.drawingBufferHeight],
                };
                gl.viewport(2, 3, 7, 9);
                out.viewport = {
                    value: Array.from(gl.getParameter(0x0BA2)),
                    drawingBuffer: [gl.drawingBufferWidth, gl.drawingBufferHeight],
                };
                const shader = gl.createShader(gl.VERTEX_SHADER);
                out.shaderFresh = {
                    tag: Object.prototype.toString.call(shader),
                    own: Reflect.ownKeys(shader).map(String),
                    is: gl.isShader(shader),
                    compile: gl.getShaderParameter(shader, gl.COMPILE_STATUS),
                    deleted: gl.getShaderParameter(shader, gl.DELETE_STATUS),
                    type: gl.getShaderParameter(shader, gl.SHADER_TYPE),
                    source: gl.getShaderSource(shader),
                };
                gl.shaderSource(shader, 'attribute vec4 pos; void main(){ gl_Position=pos; }');
                gl.compileShader(shader);
                out.shaderCompiled = gl.getShaderParameter(shader, gl.COMPILE_STATUS);

                const bad = gl.createShader(gl.FRAGMENT_SHADER);
                gl.shaderSource(bad, 'this is not glsl');
                gl.compileShader(bad);
                out.bad = {
                    compile: gl.getShaderParameter(bad, gl.COMPILE_STATUS),
                    hasLog: gl.getShaderInfoLog(bad).length > 0,
                };

                const fragment = gl.createShader(gl.FRAGMENT_SHADER);
                gl.shaderSource(fragment, 'precision mediump float; uniform float gain; void main(){ gl_FragColor=vec4(gain); }');
                gl.compileShader(fragment);

                const program = gl.createProgram();
                out.programFresh = {
                    tag: Object.prototype.toString.call(program),
                    own: Reflect.ownKeys(program).map(String),
                    is: gl.isProgram(program),
                    link: gl.getProgramParameter(program, gl.LINK_STATUS),
                    attached: gl.getProgramParameter(program, gl.ATTACHED_SHADERS),
                };
                out.preAttrib = gl.getAttribLocation(program, 'pos');
                out.preAttribError = gl.getError();
                gl.attachShader(program, shader);
                gl.attachShader(program, fragment);
                gl.bindAttribLocation(program, 3, 'pos');
                gl.linkProgram(program);
                out.linked = {
                    link: gl.getProgramParameter(program, gl.LINK_STATUS),
                    attached: gl.getProgramParameter(program, gl.ATTACHED_SHADERS),
                    attributes: gl.getProgramParameter(program, gl.ACTIVE_ATTRIBUTES),
                    uniforms: gl.getProgramParameter(program, gl.ACTIVE_UNIFORMS),
                    attrib: gl.getAttribLocation(program, 'pos'),
                    missingAttrib: gl.getAttribLocation(program, 'missing'),
                    uniformTag: Object.prototype.toString.call(gl.getUniformLocation(program, 'gain')),
                    missingUniform: gl.getUniformLocation(program, 'missing'),
                };
                gl.useProgram(program);
                out.currentProgram = gl.getParameter(gl.CURRENT_PROGRAM) === program;

                gl.deleteShader(shader);
                out.deletedAttached = {
                    is: gl.isShader(shader),
                    deleted: gl.getShaderParameter(shader, gl.DELETE_STATUS),
                };
                gl.detachShader(program, shader);
                out.deletedDetached = gl.isShader(shader);
                gl.deleteProgram(program);
                out.deletedBoundProgram = {
                    is: gl.isProgram(program),
                    current: gl.getParameter(gl.CURRENT_PROGRAM) === program,
                    deleted: gl.getProgramParameter(program, gl.DELETE_STATUS),
                };
                gl.useProgram(null);
                out.deletedProgram = {
                    is: gl.isProgram(program),
                    current: gl.getParameter(gl.CURRENT_PROGRAM) === null,
                    link: gl.getProgramParameter(program, gl.LINK_STATUS),
                    log: gl.getProgramInfoLog(program),
                };
                return out;
            })())"#,
        )
        .expect("result");

    let value: serde_json::Value = serde_json::from_str(&result).expect("json");
    assert_eq!(value["context"]["own"], serde_json::json!([]));
    assert_eq!(value["context"]["canvas"], true);
    assert_eq!(
        value["context"]["drawingBuffer"],
        serde_json::json!([300, 150])
    );
    assert_eq!(value["viewport"]["value"], serde_json::json!([2, 3, 7, 9]));
    assert_eq!(
        value["viewport"]["drawingBuffer"],
        serde_json::json!([300, 150])
    );
    assert_eq!(value["shaderFresh"]["tag"], "[object WebGLShader]");
    assert_eq!(value["shaderFresh"]["own"], serde_json::json!([]));
    assert_eq!(value["shaderFresh"]["is"], true);
    assert_eq!(value["shaderFresh"]["compile"], false);
    assert_eq!(value["shaderFresh"]["deleted"], false);
    assert_eq!(value["shaderFresh"]["type"], 35633);
    assert_eq!(value["shaderFresh"]["source"], "");
    assert_eq!(value["shaderCompiled"], true);
    assert_eq!(value["bad"]["compile"], false);
    assert_eq!(value["bad"]["hasLog"], true);
    assert_eq!(value["programFresh"]["tag"], "[object WebGLProgram]");
    assert_eq!(value["programFresh"]["own"], serde_json::json!([]));
    assert_eq!(value["programFresh"]["is"], true);
    assert_eq!(value["programFresh"]["link"], false);
    assert_eq!(value["programFresh"]["attached"], 0);
    assert_eq!(value["preAttrib"], -1);
    assert_eq!(value["preAttribError"], 1282);
    assert_eq!(value["linked"]["link"], true);
    assert_eq!(value["linked"]["attached"], 2);
    assert_eq!(value["linked"]["attributes"], 1);
    assert_eq!(value["linked"]["uniforms"], 1);
    assert_eq!(value["linked"]["attrib"], 3);
    assert_eq!(value["linked"]["missingAttrib"], -1);
    assert_eq!(
        value["linked"]["uniformTag"],
        "[object WebGLUniformLocation]"
    );
    assert_eq!(value["linked"]["missingUniform"], serde_json::Value::Null);
    assert_eq!(value["currentProgram"], true);
    assert_eq!(value["deletedAttached"]["is"], true);
    assert_eq!(value["deletedAttached"]["deleted"], true);
    assert_eq!(value["deletedDetached"], false);
    assert_eq!(value["deletedBoundProgram"]["is"], true);
    assert_eq!(value["deletedBoundProgram"]["current"], true);
    assert_eq!(value["deletedBoundProgram"]["deleted"], true);
    assert_eq!(value["deletedProgram"]["is"], false);
    assert_eq!(value["deletedProgram"]["current"], true);
    assert_eq!(value["deletedProgram"]["link"], serde_json::Value::Null);
    assert_eq!(value["deletedProgram"]["log"], serde_json::Value::Null);
}
