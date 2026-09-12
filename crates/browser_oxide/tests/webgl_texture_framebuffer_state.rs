use browser_oxide::Page;

#[tokio::test(flavor = "current_thread")]
async fn webgl_texture_framebuffer_renderbuffer_lifecycle_matches_chrome() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><canvas id='c'></canvas>",
        "https://example.test/webgl-object-state",
        Some(profile),
    )
    .await
    .expect("page");

    let result = page
        .evaluate(
            r#"JSON.stringify((() => {
                const gl = document.getElementById('c').getContext('webgl');
                const tag = value => Object.prototype.toString.call(value);
                const own = value => Reflect.ownKeys(value).map(String);
                const call = fn => { try { return fn(); } catch (error) { return error.name + ':' + error.message; } };
                const takeError = () => gl.getError();
                const out = {};

                const texture = gl.createTexture();
                out.textureFresh = {
                    tag: tag(texture), own: own(texture), is: gl.isTexture(texture),
                    binding: gl.getParameter(gl.TEXTURE_BINDING_2D) === null, error: takeError(),
                };
                gl.bindTexture(gl.TEXTURE_2D, texture);
                out.textureBound = {
                    is: gl.isTexture(texture),
                    binding: gl.getParameter(gl.TEXTURE_BINDING_2D) === texture,
                    error: takeError(),
                };
                gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
                out.textureParam = {
                    min: gl.getTexParameter(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER),
                    error: takeError(),
                };
                out.texturePlain = call(() => gl.isTexture({}));
                gl.deleteTexture(texture);
                out.textureDeleted = {
                    is: gl.isTexture(texture),
                    binding: gl.getParameter(gl.TEXTURE_BINDING_2D) === null,
                    error: takeError(),
                };

                const framebuffer = gl.createFramebuffer();
                out.frameFresh = {
                    tag: tag(framebuffer), own: own(framebuffer), is: gl.isFramebuffer(framebuffer),
                    binding: gl.getParameter(gl.FRAMEBUFFER_BINDING) === null, error: takeError(),
                };
                gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
                out.frameBound = {
                    is: gl.isFramebuffer(framebuffer),
                    binding: gl.getParameter(gl.FRAMEBUFFER_BINDING) === framebuffer,
                    status: gl.checkFramebufferStatus(gl.FRAMEBUFFER),
                    error: takeError(),
                };
                out.framePlain = call(() => gl.isFramebuffer({}));

                const renderbuffer = gl.createRenderbuffer();
                out.renderFresh = {
                    tag: tag(renderbuffer), own: own(renderbuffer), is: gl.isRenderbuffer(renderbuffer),
                    binding: gl.getParameter(gl.RENDERBUFFER_BINDING) === null, error: takeError(),
                };
                gl.bindRenderbuffer(gl.RENDERBUFFER, renderbuffer);
                out.renderBound = {
                    is: gl.isRenderbuffer(renderbuffer),
                    binding: gl.getParameter(gl.RENDERBUFFER_BINDING) === renderbuffer,
                    error: takeError(),
                };
                gl.renderbufferStorage(gl.RENDERBUFFER, gl.RGBA4, 4, 5);
                out.renderStorage = {
                    width: gl.getRenderbufferParameter(gl.RENDERBUFFER, gl.RENDERBUFFER_WIDTH),
                    height: gl.getRenderbufferParameter(gl.RENDERBUFFER, gl.RENDERBUFFER_HEIGHT),
                    format: gl.getRenderbufferParameter(gl.RENDERBUFFER, gl.RENDERBUFFER_INTERNAL_FORMAT),
                    error: takeError(),
                };
                gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, renderbuffer);
                out.frameAttached = { status: gl.checkFramebufferStatus(gl.FRAMEBUFFER), error: takeError() };
                out.renderPlain = call(() => gl.isRenderbuffer({}));
                gl.deleteRenderbuffer(renderbuffer);
                out.renderDeleted = {
                    is: gl.isRenderbuffer(renderbuffer),
                    binding: gl.getParameter(gl.RENDERBUFFER_BINDING) === null,
                    status: gl.checkFramebufferStatus(gl.FRAMEBUFFER),
                    error: takeError(),
                };
                gl.deleteFramebuffer(framebuffer);
                out.frameDeleted = {
                    is: gl.isFramebuffer(framebuffer),
                    binding: gl.getParameter(gl.FRAMEBUFFER_BINDING) === null,
                    error: takeError(),
                };
                return out;
            })())"#,
        )
        .expect("result");

    let value: serde_json::Value = serde_json::from_str(&result).expect("json");
    assert_eq!(
        value["textureFresh"],
        serde_json::json!({
            "tag":"[object WebGLTexture]", "own":[], "is":false, "binding":true, "error":0
        })
    );
    assert_eq!(
        value["textureBound"],
        serde_json::json!({"is":true,"binding":true,"error":0})
    );
    assert_eq!(
        value["textureParam"],
        serde_json::json!({"min":9728,"error":0})
    );
    assert_eq!(value["texturePlain"], "TypeError:Failed to execute 'isTexture' on 'WebGLRenderingContext': parameter 1 is not of type 'WebGLTexture'.");
    assert_eq!(
        value["textureDeleted"],
        serde_json::json!({"is":false,"binding":true,"error":0})
    );

    assert_eq!(
        value["frameFresh"],
        serde_json::json!({
            "tag":"[object WebGLFramebuffer]", "own":[], "is":false, "binding":true, "error":0
        })
    );
    assert_eq!(
        value["frameBound"],
        serde_json::json!({"is":true,"binding":true,"status":36055,"error":0})
    );
    assert_eq!(value["framePlain"], "TypeError:Failed to execute 'isFramebuffer' on 'WebGLRenderingContext': parameter 1 is not of type 'WebGLFramebuffer'.");

    assert_eq!(
        value["renderFresh"],
        serde_json::json!({
            "tag":"[object WebGLRenderbuffer]", "own":[], "is":false, "binding":true, "error":0
        })
    );
    assert_eq!(
        value["renderBound"],
        serde_json::json!({"is":true,"binding":true,"error":0})
    );
    assert_eq!(
        value["renderStorage"],
        serde_json::json!({"width":4,"height":5,"format":32854,"error":0})
    );
    assert_eq!(
        value["frameAttached"],
        serde_json::json!({"status":36053,"error":0})
    );
    assert_eq!(value["renderPlain"], "TypeError:Failed to execute 'isRenderbuffer' on 'WebGLRenderingContext': parameter 1 is not of type 'WebGLRenderbuffer'.");
    assert_eq!(
        value["renderDeleted"],
        serde_json::json!({"is":false,"binding":true,"status":36055,"error":0})
    );
    assert_eq!(
        value["frameDeleted"],
        serde_json::json!({"is":false,"binding":true,"error":0})
    );
}
