use browser_oxide::Page;

fn chrome_profile() -> browser_oxide::stealth::StealthProfile {
    browser_oxide::stealth::presets::chrome_148_macos()
}

#[tokio::test]
async fn webgl_uniform_state_matches_chrome_148() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body><canvas id='c'></canvas></body></html>",
        "https://example.test/webgl-uniforms",
        Some(chrome_profile()),
    )
    .await
    .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl');
                const out = {};
                function shader(type, source) {
                    const shader = gl.createShader(type);
                    gl.shaderSource(shader, source);
                    gl.compileShader(shader);
                    return shader;
                }
                const vertex = shader(
                    gl.VERTEX_SHADER,
                    'attribute vec4 a; uniform float uf; uniform vec2 uv2; uniform mat4 um; void main(){ gl_Position = um * (a + vec4(uv2, uf, 0.0)); }'
                );
                const fragment = shader(
                    gl.FRAGMENT_SHADER,
                    'precision mediump float; uniform int ui; uniform vec4 uv4; uniform bool ub; void main(){ gl_FragColor = uv4 + vec4(float(ui)) + (ub ? vec4(1.0) : vec4(0.0)); }'
                );
                const program = gl.createProgram();
                gl.attachShader(program, vertex);
                gl.attachShader(program, fragment);
                gl.linkProgram(program);
                out.link = gl.getProgramParameter(program, gl.LINK_STATUS);
                out.active = gl.getProgramParameter(program, gl.ACTIVE_UNIFORMS);
                const names = ['uf', 'uv2', 'um', 'ui', 'uv4', 'ub'];
                const locations = {};
                for (const name of names) locations[name] = gl.getUniformLocation(program, name);
                const value = (v) => ArrayBuffer.isView(v)
                    ? { tag: Object.prototype.toString.call(v), v: Array.from(v) }
                    : v;
                out.loc = Object.fromEntries(names.map(name => [name, {
                    tag: Object.prototype.toString.call(locations[name]),
                    own: Reflect.ownKeys(locations[name]).map(String),
                }]));
                out.defaults = Object.fromEntries(names.map(name => [name, value(gl.getUniform(program, locations[name]))]));
                while (gl.getError());
                gl.uniform1f(locations.uf, 9);
                out.noProgram = { e: gl.getError(), v: value(gl.getUniform(program, locations.uf)) };
                gl.useProgram(program);
                gl.uniform1f(locations.uf, 2.5);
                gl.uniform2f(locations.uv2, 1.25, -2);
                gl.uniformMatrix4fv(locations.um, false, new Float32Array([1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]));
                gl.uniform1i(locations.ui, 3);
                gl.uniform4f(locations.uv4, 4, 5, 6, 7);
                gl.uniform1i(locations.ub, 1);
                out.set = Object.fromEntries(names.map(name => [name, value(gl.getUniform(program, locations[name]))]));
                out.setErr = gl.getError();
                gl.uniform1f(null, 8);
                out.nullErr = gl.getError();
                gl.uniformMatrix4fv(locations.um, true, new Float32Array(16));
                out.transposeErr = gl.getError();
                const other = gl.createProgram();
                gl.attachShader(other, vertex);
                gl.attachShader(other, fragment);
                gl.linkProgram(other);
                const foreignLocation = gl.getUniformLocation(other, 'uf');
                while (gl.getError());
                gl.uniform1f(foreignLocation, 8);
                out.foreignSetErr = gl.getError();
                while (gl.getError());
                let foreignValue;
                try { foreignValue = gl.getUniform(program, foreignLocation); }
                catch (e) { foreignValue = `${e.name}:${e.message}`; }
                out.foreignGet = { v: value(foreignValue), e: gl.getError() };
                out.methods = {
                    getUniform: typeof gl.getUniform,
                    u1f: gl.uniform1f.length,
                    u1i: gl.uniform1i.length,
                    u2f: gl.uniform2f.length,
                    u4f: gl.uniform4f.length,
                    um4: gl.uniformMatrix4fv.length,
                };
                return out;
            })())"#,
        )
        .expect("uniform probe");

    let expected = r#"{"link":true,"active":6,"loc":{"uf":{"tag":"[object WebGLUniformLocation]","own":[]},"uv2":{"tag":"[object WebGLUniformLocation]","own":[]},"um":{"tag":"[object WebGLUniformLocation]","own":[]},"ui":{"tag":"[object WebGLUniformLocation]","own":[]},"uv4":{"tag":"[object WebGLUniformLocation]","own":[]},"ub":{"tag":"[object WebGLUniformLocation]","own":[]}},"defaults":{"uf":0,"uv2":{"tag":"[object Float32Array]","v":[0,0]},"um":{"tag":"[object Float32Array]","v":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"ui":0,"uv4":{"tag":"[object Float32Array]","v":[0,0,0,0]},"ub":false},"noProgram":{"e":1282,"v":0},"set":{"uf":2.5,"uv2":{"tag":"[object Float32Array]","v":[1.25,-2]},"um":{"tag":"[object Float32Array]","v":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]},"ui":3,"uv4":{"tag":"[object Float32Array]","v":[4,5,6,7]},"ub":true},"setErr":0,"nullErr":0,"transposeErr":1281,"foreignSetErr":1282,"foreignGet":{"v":null,"e":1282},"methods":{"getUniform":"function","u1f":2,"u1i":2,"u2f":3,"u4f":5,"um4":3}}"#;
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn webgl_uniform_location_is_invalidated_by_relink() {
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body><canvas id='c'></canvas></body></html>",
        "https://example.test/webgl-uniform-relink",
        Some(chrome_profile()),
    )
    .await
    .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl');
                const shader = (type, source) => {
                    const object = gl.createShader(type);
                    gl.shaderSource(object, source);
                    gl.compileShader(object);
                    return object;
                };
                const vertex = shader(gl.VERTEX_SHADER, 'attribute vec4 a; uniform float u; void main(){gl_Position=a*max(u,0.001);}');
                const fragment = shader(gl.FRAGMENT_SHADER, 'precision mediump float; void main(){gl_FragColor=vec4(1.0);}');
                const program = gl.createProgram();
                gl.attachShader(program, vertex);
                gl.attachShader(program, fragment);
                gl.linkProgram(program);
                gl.useProgram(program);
                const oldLocation = gl.getUniformLocation(program, 'u');
                gl.uniform1f(oldLocation, 3);
                const out = { before: gl.getUniform(program, oldLocation) };
                gl.linkProgram(program);
                out.link = gl.getProgramParameter(program, gl.LINK_STATUS);
                while (gl.getError());
                gl.uniform1f(oldLocation, 4);
                out.oldSetErr = gl.getError();
                while (gl.getError());
                let oldValue;
                try { oldValue = gl.getUniform(program, oldLocation); }
                catch (e) { oldValue = `THROW:${e.name}:${e.message}`; }
                out.oldGet = { v: oldValue, e: gl.getError() };
                const fresh = gl.getUniformLocation(program, 'u');
                out.freshDefault = gl.getUniform(program, fresh);
                while (gl.getError());
                gl.uniform1f(fresh, 5);
                out.fresh = { v: gl.getUniform(program, fresh), e: gl.getError() };
                return out;
            })())"#,
        )
        .expect("relink probe");

    assert_eq!(
        actual,
        r#"{"before":3,"link":true,"oldSetErr":1282,"oldGet":{"v":null,"e":1282},"freshDefault":0,"fresh":{"v":5,"e":0}}"#
    );
}
