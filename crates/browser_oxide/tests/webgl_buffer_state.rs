use browser_oxide::Page;

#[tokio::test(flavor = "current_thread")]
async fn webgl_buffer_lifecycle_matches_chrome() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><canvas id='c'></canvas>",
        "https://example.test/webgl-buffer-state",
        Some(profile),
    )
    .await
    .expect("page");

    let result = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl');
                const out = {};
                const buffer = gl.createBuffer();
                out.fresh = {
                    tag: Object.prototype.toString.call(buffer),
                    own: Reflect.ownKeys(buffer).map(String),
                    is: gl.isBuffer(buffer),
                    array: gl.getParameter(gl.ARRAY_BUFFER_BINDING),
                    element: gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING),
                    error: gl.getError(),
                };
                gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
                out.bound = {
                    is: gl.isBuffer(buffer),
                    binding: gl.getParameter(gl.ARRAY_BUFFER_BINDING) === buffer,
                    error: gl.getError(),
                };
                gl.bufferData(gl.ARRAY_BUFFER, new Uint8Array([1,2,3,4]), gl.STATIC_DRAW);
                out.data = {
                    size: gl.getBufferParameter(gl.ARRAY_BUFFER, gl.BUFFER_SIZE),
                    usage: gl.getBufferParameter(gl.ARRAY_BUFFER, gl.BUFFER_USAGE),
                    error: gl.getError(),
                };
                gl.bindBuffer(gl.ARRAY_BUFFER, null);
                out.unbound = gl.getParameter(gl.ARRAY_BUFFER_BINDING) === null;
                gl.bufferData(gl.ARRAY_BUFFER, 4, gl.STATIC_DRAW);
                out.noBindingError = gl.getError();
                gl.bindBuffer(123, buffer);
                out.invalidTargetError = gl.getError();
                try { gl.isBuffer({}); out.isPlain = 'no throw'; }
                catch (error) { out.isPlain = error.name + ':' + error.message; }
                gl.deleteBuffer(buffer);
                out.deleted = gl.isBuffer(buffer);
                gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
                out.bindDeletedError = gl.getError();
                return out;
            })())"#,
        )
        .expect("result");

    let value: serde_json::Value = serde_json::from_str(&result).expect("json");
    assert_eq!(value["fresh"]["tag"], "[object WebGLBuffer]");
    assert_eq!(value["fresh"]["own"], serde_json::json!([]));
    assert_eq!(value["fresh"]["is"], false);
    assert_eq!(value["fresh"]["array"], serde_json::Value::Null);
    assert_eq!(value["fresh"]["element"], serde_json::Value::Null);
    assert_eq!(value["fresh"]["error"], 0);
    assert_eq!(value["bound"]["is"], true);
    assert_eq!(value["bound"]["binding"], true);
    assert_eq!(value["bound"]["error"], 0);
    assert_eq!(value["data"]["size"], 4);
    assert_eq!(value["data"]["usage"], 35044);
    assert_eq!(value["data"]["error"], 0);
    assert_eq!(value["unbound"], true);
    assert_eq!(value["noBindingError"], 1282);
    assert_eq!(value["invalidTargetError"], 1280);
    assert_eq!(value["isPlain"], "TypeError:Failed to execute 'isBuffer' on 'WebGLRenderingContext': parameter 1 is not of type 'WebGLBuffer'.");
    assert_eq!(value["deleted"], false);
    assert_eq!(value["bindDeletedError"], 1282);
}
