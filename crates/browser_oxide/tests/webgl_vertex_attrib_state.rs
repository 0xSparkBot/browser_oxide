use browser_oxide::Page;

#[tokio::test]
async fn webgl2_vertex_attribute_state_matches_chrome_148() {
    let html =
        r#"<!doctype html><html><body><canvas id="c" width="8" height="8"></canvas></body></html>"#;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile))
        .await
        .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl2');
                const error = () => gl.getError();
                const snapshot = (index) => ({
                    enabled: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_ENABLED),
                    size: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_SIZE),
                    stride: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_STRIDE),
                    type: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_TYPE),
                    normalized: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_NORMALIZED),
                    buffer: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING) === null
                        ? null : Object.prototype.toString.call(gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING)),
                    integer: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_INTEGER),
                    divisor: gl.getVertexAttrib(index, gl.VERTEX_ATTRIB_ARRAY_DIVISOR),
                    pointer: gl.getVertexAttribOffset(index, gl.VERTEX_ATTRIB_ARRAY_POINTER),
                    current: Array.from(gl.getVertexAttrib(index, gl.CURRENT_VERTEX_ATTRIB)),
                });
                const out = { max: gl.getParameter(gl.MAX_VERTEX_ATTRIBS) };
                out.initial = snapshot(1);
                gl.enableVertexAttribArray(1);
                out.afterEnable = snapshot(1);
                out.errEnable = error();
                gl.disableVertexAttribArray(1);
                out.afterDisable = snapshot(1);
                out.errDisable = error();
                gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 16, 4);
                out.noBufferErr = error();
                const buffer = gl.createBuffer();
                gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
                gl.bufferData(gl.ARRAY_BUFFER, 64, gl.STATIC_DRAW);
                gl.vertexAttribPointer(1, 3, gl.FLOAT, true, 16, 4);
                out.pointer = snapshot(1);
                out.pointerErr = error();
                gl.vertexAttribIPointer(2, 2, gl.INT, 8, 0);
                out.ipointer = snapshot(2);
                out.ipointerErr = error();
                gl.vertexAttribDivisor(1, 7);
                out.divisor = snapshot(1);
                out.divisorErr = error();
                gl.vertexAttrib4f(3, 1, 2, 3, 4);
                out.current3 = snapshot(3);
                out.current3Err = error();
                gl.enableVertexAttribArray(out.max);
                out.badIndexErr = error();
                gl.vertexAttribPointer(0, 5, gl.FLOAT, false, 0, 0);
                out.badSizeErr = error();
                gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 300, 0);
                out.badStrideErr = error();
                gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 0, -4);
                out.badOffsetErr = error();
                return out;
            })())"#,
        )
        .expect("probe");

    assert_eq!(
        actual,
        r#"{"max":16,"initial":{"enabled":false,"size":4,"stride":0,"type":5126,"normalized":false,"buffer":null,"integer":false,"divisor":0,"pointer":0,"current":[0,0,0,1]},"afterEnable":{"enabled":true,"size":4,"stride":0,"type":5126,"normalized":false,"buffer":null,"integer":false,"divisor":0,"pointer":0,"current":[0,0,0,1]},"errEnable":0,"afterDisable":{"enabled":false,"size":4,"stride":0,"type":5126,"normalized":false,"buffer":null,"integer":false,"divisor":0,"pointer":0,"current":[0,0,0,1]},"errDisable":0,"noBufferErr":1282,"pointer":{"enabled":false,"size":3,"stride":16,"type":5126,"normalized":true,"buffer":"[object WebGLBuffer]","integer":false,"divisor":0,"pointer":4,"current":[0,0,0,1]},"pointerErr":0,"ipointer":{"enabled":false,"size":2,"stride":8,"type":5124,"normalized":false,"buffer":"[object WebGLBuffer]","integer":true,"divisor":0,"pointer":0,"current":[0,0,0,1]},"ipointerErr":0,"divisor":{"enabled":false,"size":3,"stride":16,"type":5126,"normalized":true,"buffer":"[object WebGLBuffer]","integer":false,"divisor":7,"pointer":4,"current":[0,0,0,1]},"divisorErr":0,"current3":{"enabled":false,"size":4,"stride":0,"type":5126,"normalized":false,"buffer":null,"integer":false,"divisor":0,"pointer":0,"current":[1,2,3,4]},"current3Err":0,"badIndexErr":1281,"badSizeErr":1281,"badStrideErr":1281,"badOffsetErr":1281}"#
    );
}

#[tokio::test]
async fn webgl2_vertex_attributes_are_vertex_array_state() {
    let html = r#"<!doctype html><html><body><canvas id="c"></canvas></body></html>"#;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile))
        .await
        .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl2');
                const buffer = gl.createBuffer();
                gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
                gl.bufferData(gl.ARRAY_BUFFER, 32, gl.STATIC_DRAW);
                const vao = gl.createVertexArray();
                gl.bindVertexArray(vao);
                gl.enableVertexAttribArray(2);
                gl.vertexAttribPointer(2, 2, gl.FLOAT, false, 8, 4);
                const inVao = [
                    gl.getVertexAttrib(2, gl.VERTEX_ATTRIB_ARRAY_ENABLED),
                    gl.getVertexAttribOffset(2, gl.VERTEX_ATTRIB_ARRAY_POINTER),
                ];
                gl.bindVertexArray(null);
                const inDefault = [
                    gl.getVertexAttrib(2, gl.VERTEX_ATTRIB_ARRAY_ENABLED),
                    gl.getVertexAttribOffset(2, gl.VERTEX_ATTRIB_ARRAY_POINTER),
                ];
                gl.bindVertexArray(vao);
                const restored = [
                    gl.getVertexAttrib(2, gl.VERTEX_ATTRIB_ARRAY_ENABLED),
                    gl.getVertexAttribOffset(2, gl.VERTEX_ATTRIB_ARRAY_POINTER),
                ];
                return { inVao, inDefault, restored };
            })())"#,
        )
        .expect("probe");

    assert_eq!(
        actual,
        r#"{"inVao":[true,4],"inDefault":[false,0],"restored":[true,4]}"#
    );
}

#[tokio::test]
async fn webgl2_element_array_buffer_binding_is_vertex_array_state() {
    let html = r#"<!doctype html><html><body><canvas id="c"></canvas></body></html>"#;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile))
        .await
        .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl2');
                const defaultBuffer = gl.createBuffer();
                const vaoBuffer = gl.createBuffer();
                const vao = gl.createVertexArray();
                gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, defaultBuffer);
                const defaultBound = gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING) === defaultBuffer;
                gl.bindVertexArray(vao);
                const vaoStartsEmpty = gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING) === null;
                gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, vaoBuffer);
                const vaoBound = gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING) === vaoBuffer;
                gl.bindVertexArray(null);
                const defaultRestored = gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING) === defaultBuffer;
                gl.bindVertexArray(vao);
                const vaoRestored = gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING) === vaoBuffer;
                return { defaultBound, vaoStartsEmpty, vaoBound, defaultRestored, vaoRestored };
            })())"#,
        )
        .expect("probe");

    assert_eq!(
        actual,
        r#"{"defaultBound":true,"vaoStartsEmpty":true,"vaoBound":true,"defaultRestored":true,"vaoRestored":true}"#
    );
}
