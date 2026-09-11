//! WebGL parity vs captured Chrome 147 on macOS arm64.
//! Real Chrome values from tests/fixtures/chrome147/captured_macos_arm64.json.

use browser_oxide::Page;

async fn evaluate(js: &str) -> String {
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><body><canvas id='c' width='100' height='100'></canvas></body></html>",
        None::<browser_oxide::stealth::StealthProfile>,
    )
    .await
    .unwrap();
    page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"))
}

/// Same, but with the chrome_148_macos stealth profile active so WebGL reads
/// the real apple_m3 surfaces (incl. the FIX-D2 WebGL 1 surface).
async fn evaluate_macos(js: &str) -> String {
    let mut page = Page::from_html(
        "<!DOCTYPE html><html><body><canvas id='c' width='100' height='100'></canvas></body></html>",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    page.evaluate(js).unwrap_or_else(|e| format!("ERROR: {e}"))
}

/// FIX-D2: `getContext("webgl")` must NOT return the WebGL 2 surface. Pre-fix,
/// a WebGL 1 context reported the WebGL 2 version string and advertised
/// WebGL-2-only extensions (e.g. EXT_color_buffer_float) — a deterministic
/// cross-API bot tell. Guards version strings, distinct classes, and the
/// extension-set delta on both the no-profile fallback and the macOS profile.
#[tokio::test]
async fn webgl1_webgl2_surfaces_are_distinct_fix_d2() {
    let probe = "
        const gl1 = document.createElement('canvas').getContext('webgl');
        const gl2 = document.createElement('canvas').getContext('webgl2');
        const e1 = gl1.getSupportedExtensions();
        JSON.stringify({
            v1: gl1.getParameter(gl1.VERSION),
            v2: gl2.getParameter(gl2.VERSION),
            classes_distinct: WebGLRenderingContext !== WebGL2RenderingContext,
            ctor1: gl1.constructor.name,
            ctor2: gl2.constructor.name,
            gl1_has_webgl2_only: e1.includes('EXT_color_buffer_float') || e1.includes('OES_draw_buffers_indexed'),
            gl1_has_webgl1_only: e1.includes('OES_texture_float') && e1.includes('ANGLE_instanced_arrays'),
            tag2: Object.prototype.toString.call(gl2),
        })
    ";
    for (label, r) in [
        ("no-profile", evaluate(probe).await),
        ("macos", evaluate_macos(probe).await),
    ] {
        let v: serde_json::Value = serde_json::from_str(&r)
            .unwrap_or_else(|_| panic!("{label}: probe returned non-JSON: {r}"));
        assert_eq!(
            v["v1"], "WebGL 1.0 (OpenGL ES 2.0 Chromium)",
            "{label}: webgl1 VERSION must be WebGL 1.0"
        );
        assert_eq!(
            v["v2"], "WebGL 2.0 (OpenGL ES 3.0 Chromium)",
            "{label}: webgl2 VERSION must be WebGL 2.0"
        );
        assert_eq!(
            v["classes_distinct"], true,
            "{label}: WebGLRenderingContext must != WebGL2RenderingContext"
        );
        assert_eq!(v["ctor1"], "WebGLRenderingContext", "{label}: webgl1 ctor");
        assert_eq!(v["ctor2"], "WebGL2RenderingContext", "{label}: webgl2 ctor");
        assert_eq!(
            v["gl1_has_webgl2_only"], false,
            "{label}: webgl1 must NOT advertise WebGL-2-only extensions (the bot tell)"
        );
        assert_eq!(
            v["gl1_has_webgl1_only"], true,
            "{label}: webgl1 must advertise WebGL-1 core-promoted extensions"
        );
        assert_eq!(
            v["tag2"], "[object WebGL2RenderingContext]",
            "{label}: webgl2 Symbol.toStringTag"
        );
    }
}

const GL_SETUP: &str = "
const c = document.getElementById('c');
const gl = c.getContext('webgl2') || c.getContext('webgl');
";

#[tokio::test]
async fn webgl_unmasked_renderer_is_angle_format() {
    let r = evaluate(&format!(
        "{GL_SETUP}
        const ext = gl.getExtension('WEBGL_debug_renderer_info');
        gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)"
    ))
    .await;
    assert!(
        r.contains("ANGLE"),
        "UNMASKED_RENDERER_WEBGL must be ANGLE-format, got: {r}"
    );
}

#[tokio::test]
async fn webgl_unmasked_vendor_is_google() {
    let r = evaluate(&format!(
        "{GL_SETUP}
        const ext = gl.getExtension('WEBGL_debug_renderer_info');
        gl.getParameter(ext.UNMASKED_VENDOR_WEBGL)"
    ))
    .await;
    assert!(
        r.starts_with("Google Inc."),
        "UNMASKED_VENDOR_WEBGL must start with 'Google Inc.', got: {r}"
    );
}

#[tokio::test]
async fn webgl_max_texture_size_chrome_value() {
    let r = evaluate(&format!("{GL_SETUP}gl.getParameter(gl.MAX_TEXTURE_SIZE)")).await;
    assert_eq!(
        r, "16384",
        "MAX_TEXTURE_SIZE must match Chrome 147 captured value"
    );
}

#[tokio::test]
async fn webgl_max_renderbuffer_size_chrome_value() {
    let r = evaluate(&format!(
        "{GL_SETUP}gl.getParameter(gl.MAX_RENDERBUFFER_SIZE)"
    ))
    .await;
    assert_eq!(r, "16384");
}

#[tokio::test]
async fn webgl_max_vertex_attribs_chrome_value() {
    let r = evaluate(&format!("{GL_SETUP}gl.getParameter(gl.MAX_VERTEX_ATTRIBS)")).await;
    assert_eq!(r, "16");
}

#[tokio::test]
async fn webgl_aliased_line_width_chrome_value() {
    // Chrome ANGLE on every OS: [1, 1]
    let r = evaluate(&format!(
        "{GL_SETUP}Array.from(gl.getParameter(gl.ALIASED_LINE_WIDTH_RANGE)).join(',')"
    ))
    .await;
    assert_eq!(r, "1,1");
}

#[tokio::test]
async fn webgl_max_viewport_dims_chrome_value() {
    let r = evaluate(&format!(
        "{GL_SETUP}Array.from(gl.getParameter(gl.MAX_VIEWPORT_DIMS)).join(',')"
    ))
    .await;
    assert_eq!(r, "16384,16384");
}

#[tokio::test]
async fn webgl_supported_extensions_count_chrome147() {
    let r = evaluate(&format!("{GL_SETUP}gl.getSupportedExtensions().length")).await;
    let n: usize = r.parse().unwrap_or(0);
    assert!(
        n >= 30,
        "supportedExtensions count should be ≥30 (Chrome 147 captured: 36); got {n}"
    );
}

#[tokio::test]
async fn webgl_supported_extensions_includes_chrome147_set() {
    let r = evaluate(&format!(
        "{GL_SETUP}
        const exts = gl.getSupportedExtensions();
        const required = ['WEBGL_debug_renderer_info','EXT_texture_filter_anisotropic',
            'WEBGL_compressed_texture_s3tc','WEBGL_lose_context','OES_texture_float_linear',
            'KHR_parallel_shader_compile'];
        required.every(e => exts.includes(e))"
    ))
    .await;
    assert_eq!(
        r, "true",
        "WebGL must support Chrome 147 baseline extensions"
    );
}

#[tokio::test]
async fn webgl_shader_precision_high_float_chrome_values() {
    // Chrome ANGLE: {rangeMin:127, rangeMax:127, precision:23}
    let r = evaluate(&format!(
        "{GL_SETUP}
        const f = gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER, gl.HIGH_FLOAT);
        f.rangeMin + ',' + f.rangeMax + ',' + f.precision"
    ))
    .await;
    assert_eq!(r, "127,127,23");
}

#[tokio::test]
async fn webgl_debug_renderer_info_extension_present() {
    let r = evaluate(&format!(
        "{GL_SETUP}
        const ext = gl.getExtension('WEBGL_debug_renderer_info');
        ext !== null && typeof ext.UNMASKED_VENDOR_WEBGL === 'number'"
    ))
    .await;
    assert_eq!(r, "true");
}

#[tokio::test]
async fn webgl2_internalformat_and_lose_context_match_chrome() {
    let r = evaluate(&format!(
        "{GL_SETUP}
        const samples = gl.getInternalformatParameter(0x8D41, 0x8D8E, 0x80A9);
        const lose = gl.getExtension('WEBGL_lose_context');
        JSON.stringify({{
            sampleTag: Object.prototype.toString.call(samples),
            samples: Array.from(samples),
            loseTag: Object.prototype.toString.call(lose),
            loseType: typeof lose.loseContext,
            restoreType: typeof lose.restoreContext,
            same: lose === gl.getExtension('WEBGL_lose_context')
        }})"
    ))
    .await;
    assert_eq!(
        r,
        r#"{"sampleTag":"[object Int32Array]","samples":[],"loseTag":"[object WebGLLoseContext]","loseType":"function","restoreType":"function","same":true}"#
    );
}

#[tokio::test]
async fn webgl2_extension_objects_match_chrome_surface() {
    // Captured on Chrome 148/macOS and re-verified on Chromium 154/macOS:
    // the extension set and extension-object prototype surfaces are stable
    // across both versions. WebGL extension objects have no global constructor
    // and no own properties; constants/methods live on a branded prototype.
    let r = evaluate_macos(
        r#"
        (() => {
            const gl = document.getElementById('c').getContext('webgl2');
            const expected = {
                "EXT_clip_control": ["[object EXTClipControl]", "LOWER_LEFT_EXT,UPPER_LEFT_EXT,NEGATIVE_ONE_TO_ONE_EXT,ZERO_TO_ONE_EXT,CLIP_ORIGIN_EXT,CLIP_DEPTH_MODE_EXT,clipControlEXT"],
                "EXT_color_buffer_float": ["[object EXTColorBufferFloat]", ""],
                "EXT_color_buffer_half_float": ["[object EXTColorBufferHalfFloat]", "RGBA16F_EXT,RGB16F_EXT,FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT,UNSIGNED_NORMALIZED_EXT"],
                "EXT_conservative_depth": ["[object EXTConservativeDepth]", ""],
                "EXT_depth_clamp": ["[object EXTDepthClamp]", "DEPTH_CLAMP_EXT"],
                "EXT_disjoint_timer_query_webgl2": ["[object EXTDisjointTimerQueryWebGL2]", "QUERY_COUNTER_BITS_EXT,TIME_ELAPSED_EXT,TIMESTAMP_EXT,GPU_DISJOINT_EXT,queryCounterEXT"],
                "EXT_float_blend": ["[object EXTFloatBlend]", ""],
                "EXT_polygon_offset_clamp": ["[object EXTPolygonOffsetClamp]", "POLYGON_OFFSET_CLAMP_EXT,polygonOffsetClampEXT"],
                "EXT_render_snorm": ["[object EXTRenderSnorm]", ""],
                "EXT_texture_compression_bptc": ["[object EXTTextureCompressionBPTC]", "COMPRESSED_RGBA_BPTC_UNORM_EXT,COMPRESSED_SRGB_ALPHA_BPTC_UNORM_EXT,COMPRESSED_RGB_BPTC_SIGNED_FLOAT_EXT,COMPRESSED_RGB_BPTC_UNSIGNED_FLOAT_EXT"],
                "EXT_texture_compression_rgtc": ["[object EXTTextureCompressionRGTC]", "COMPRESSED_RED_RGTC1_EXT,COMPRESSED_SIGNED_RED_RGTC1_EXT,COMPRESSED_RED_GREEN_RGTC2_EXT,COMPRESSED_SIGNED_RED_GREEN_RGTC2_EXT"],
                "EXT_texture_filter_anisotropic": ["[object EXTTextureFilterAnisotropic]", "TEXTURE_MAX_ANISOTROPY_EXT,MAX_TEXTURE_MAX_ANISOTROPY_EXT"],
                "EXT_texture_mirror_clamp_to_edge": ["[object EXTTextureMirrorClampToEdge]", "MIRROR_CLAMP_TO_EDGE_EXT"],
                "EXT_texture_norm16": ["[object EXTTextureNorm16]", "R16_EXT,RG16_EXT,RGB16_EXT,RGBA16_EXT,R16_SNORM_EXT,RG16_SNORM_EXT,RGB16_SNORM_EXT,RGBA16_SNORM_EXT"],
                "KHR_parallel_shader_compile": ["[object KHRParallelShaderCompile]", "COMPLETION_STATUS_KHR"],
                "NV_shader_noperspective_interpolation": ["[object NVShaderNoperspectiveInterpolation]", ""],
                "OES_draw_buffers_indexed": ["[object OESDrawBuffersIndexed]", "blendEquationSeparateiOES,blendEquationiOES,blendFuncSeparateiOES,blendFunciOES,colorMaskiOES,disableiOES,enableiOES"],
                "OES_sample_variables": ["[object OESSampleVariables]", ""],
                "OES_shader_multisample_interpolation": ["[object OESShaderMultisampleInterpolation]", "MIN_FRAGMENT_INTERPOLATION_OFFSET_OES,MAX_FRAGMENT_INTERPOLATION_OFFSET_OES,FRAGMENT_INTERPOLATION_OFFSET_BITS_OES"],
                "OES_texture_float_linear": ["[object OESTextureFloatLinear]", ""],
                "WEBGL_blend_func_extended": ["[object WebGLBlendFuncExtended]", "SRC1_COLOR_WEBGL,SRC1_ALPHA_WEBGL,ONE_MINUS_SRC1_COLOR_WEBGL,ONE_MINUS_SRC1_ALPHA_WEBGL,MAX_DUAL_SOURCE_DRAW_BUFFERS_WEBGL"],
                "WEBGL_clip_cull_distance": ["[object WebGLClipCullDistance]", "MAX_CLIP_DISTANCES_WEBGL,MAX_CULL_DISTANCES_WEBGL,MAX_COMBINED_CLIP_AND_CULL_DISTANCES_WEBGL,CLIP_DISTANCE0_WEBGL,CLIP_DISTANCE1_WEBGL,CLIP_DISTANCE2_WEBGL,CLIP_DISTANCE3_WEBGL,CLIP_DISTANCE4_WEBGL,CLIP_DISTANCE5_WEBGL,CLIP_DISTANCE6_WEBGL,CLIP_DISTANCE7_WEBGL"],
                "WEBGL_compressed_texture_astc": ["[object WebGLCompressedTextureASTC]", "COMPRESSED_RGBA_ASTC_4x4_KHR,COMPRESSED_RGBA_ASTC_5x4_KHR,COMPRESSED_RGBA_ASTC_5x5_KHR,COMPRESSED_RGBA_ASTC_6x5_KHR,COMPRESSED_RGBA_ASTC_6x6_KHR,COMPRESSED_RGBA_ASTC_8x5_KHR,COMPRESSED_RGBA_ASTC_8x6_KHR,COMPRESSED_RGBA_ASTC_8x8_KHR,COMPRESSED_RGBA_ASTC_10x5_KHR,COMPRESSED_RGBA_ASTC_10x6_KHR,COMPRESSED_RGBA_ASTC_10x8_KHR,COMPRESSED_RGBA_ASTC_10x10_KHR,COMPRESSED_RGBA_ASTC_12x10_KHR,COMPRESSED_RGBA_ASTC_12x12_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_4x4_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_5x4_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_5x5_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_6x5_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_6x6_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_8x5_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_8x6_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_8x8_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_10x5_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_10x6_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_10x8_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_10x10_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_12x10_KHR,COMPRESSED_SRGB8_ALPHA8_ASTC_12x12_KHR,getSupportedProfiles"],
                "WEBGL_compressed_texture_etc": ["[object WebGLCompressedTextureETC]", "COMPRESSED_R11_EAC,COMPRESSED_SIGNED_R11_EAC,COMPRESSED_RG11_EAC,COMPRESSED_SIGNED_RG11_EAC,COMPRESSED_RGB8_ETC2,COMPRESSED_SRGB8_ETC2,COMPRESSED_RGB8_PUNCHTHROUGH_ALPHA1_ETC2,COMPRESSED_SRGB8_PUNCHTHROUGH_ALPHA1_ETC2,COMPRESSED_RGBA8_ETC2_EAC,COMPRESSED_SRGB8_ALPHA8_ETC2_EAC"],
                "WEBGL_compressed_texture_etc1": ["[object WebGLCompressedTextureETC1]", "COMPRESSED_RGB_ETC1_WEBGL"],
                "WEBGL_compressed_texture_pvrtc": ["[object WebGLCompressedTexturePVRTC]", "COMPRESSED_RGB_PVRTC_4BPPV1_IMG,COMPRESSED_RGB_PVRTC_2BPPV1_IMG,COMPRESSED_RGBA_PVRTC_4BPPV1_IMG,COMPRESSED_RGBA_PVRTC_2BPPV1_IMG"],
                "WEBGL_compressed_texture_s3tc": ["[object WebGLCompressedTextureS3TC]", "COMPRESSED_RGB_S3TC_DXT1_EXT,COMPRESSED_RGBA_S3TC_DXT1_EXT,COMPRESSED_RGBA_S3TC_DXT3_EXT,COMPRESSED_RGBA_S3TC_DXT5_EXT"],
                "WEBGL_compressed_texture_s3tc_srgb": ["[object WebGLCompressedTextureS3TCsRGB]", "COMPRESSED_SRGB_S3TC_DXT1_EXT,COMPRESSED_SRGB_ALPHA_S3TC_DXT1_EXT,COMPRESSED_SRGB_ALPHA_S3TC_DXT3_EXT,COMPRESSED_SRGB_ALPHA_S3TC_DXT5_EXT"],
                "WEBGL_debug_renderer_info": ["[object WebGLDebugRendererInfo]", "UNMASKED_VENDOR_WEBGL,UNMASKED_RENDERER_WEBGL"],
                "WEBGL_debug_shaders": ["[object WebGLDebugShaders]", "getTranslatedShaderSource"],
                "WEBGL_lose_context": ["[object WebGLLoseContext]", "loseContext,restoreContext"],
                "WEBGL_multi_draw": ["[object WebGLMultiDraw]", "multiDrawArraysInstancedWEBGL,multiDrawArraysWEBGL,multiDrawElementsInstancedWEBGL,multiDrawElementsWEBGL"],
                "WEBGL_polygon_mode": ["[object WebGLPolygonMode]", "POLYGON_MODE_WEBGL,POLYGON_OFFSET_LINE_WEBGL,LINE_WEBGL,FILL_WEBGL,polygonModeWEBGL"],
                "WEBGL_provoking_vertex": ["[object WebGLProvokingVertex]", "FIRST_VERTEX_CONVENTION_WEBGL,LAST_VERTEX_CONVENTION_WEBGL,PROVOKING_VERTEX_WEBGL,provokingVertexWEBGL"],
                "WEBGL_render_shared_exponent": ["[object WebGLRenderSharedExponent]", ""],
                "WEBGL_stencil_texturing": ["[object WebGLStencilTexturing]", "DEPTH_STENCIL_TEXTURE_MODE_WEBGL,STENCIL_INDEX_WEBGL"],
            };

            const errors = [];
            const supported = gl.getSupportedExtensions();
            if (supported.length !== Object.keys(expected).length) errors.push(`count:${supported.length}`);
            const beforeContextKeys = Reflect.ownKeys(gl).map(String).join('|');
            for (const name of supported) {
                const want = expected[name];
                if (!want) { errors.push(`unexpected:${name}`); continue; }
                const ext = gl.getExtension(name);
                const proto = Object.getPrototypeOf(ext);
                if (Object.prototype.toString.call(ext) !== want[0]) errors.push(`tag:${name}`);
                if (Reflect.ownKeys(ext).length !== 0) errors.push(`own:${name}`);
                if (Object.getPrototypeOf(proto) !== Object.prototype) errors.push(`parent:${name}`);
                if (Object.getOwnPropertyNames(proto).join(',') !== want[1]) errors.push(`proto:${name}`);
                if (ext !== gl.getExtension(name.toLowerCase())) errors.push(`identity:${name}`);
                const tagDesc = Object.getOwnPropertyDescriptor(proto, Symbol.toStringTag);
                if (!tagDesc || tagDesc.enumerable || !tagDesc.configurable || tagDesc.writable) errors.push(`tagdesc:${name}`);
                for (const key of Object.getOwnPropertyNames(proto)) {
                    const desc = Object.getOwnPropertyDescriptor(proto, key);
                    if (typeof desc.value === 'function') {
                        if (!desc.enumerable || !desc.configurable || !desc.writable) errors.push(`mdesc:${name}.${key}`);
                        if (!String(desc.value).includes('[native code]')) errors.push(`native:${name}.${key}`);
                    } else if (!desc.enumerable || desc.configurable || desc.writable || typeof desc.value !== 'number') {
                        errors.push(`cdesc:${name}.${key}`);
                    }
                }
            }
            if (beforeContextKeys !== Reflect.ownKeys(gl).map(String).join('|')) errors.push('context-own-leak');

            const anisotropic = gl.getExtension('EXT_texture_filter_anisotropic');
            if (anisotropic.TEXTURE_MAX_ANISOTROPY_EXT !== 34046 || anisotropic.MAX_TEXTURE_MAX_ANISOTROPY_EXT !== 34047) errors.push('anisotropic-values');
            if (gl.getExtension('KHR_parallel_shader_compile').COMPLETION_STATUS_KHR !== 37297) errors.push('khr-value');
            const debug = gl.getExtension('WEBGL_debug_renderer_info');
            if (debug.UNMASKED_VENDOR_WEBGL !== 37445 || debug.UNMASKED_RENDERER_WEBGL !== 37446) errors.push('debug-values');
            const multi = gl.getExtension('WEBGL_multi_draw');
            if (multi.multiDrawArraysInstancedWEBGL.length !== 8 || multi.multiDrawArraysWEBGL.length !== 6 || multi.multiDrawElementsInstancedWEBGL.length !== 9 || multi.multiDrawElementsWEBGL.length !== 7) errors.push('multi-arity');
            if (JSON.stringify(gl.getExtension('WEBGL_compressed_texture_astc').getSupportedProfiles()) !== '["ldr","hdr"]') errors.push('astc-profiles');
            return JSON.stringify(errors);
        })()
        "#,
    )
    .await;
    assert_eq!(r, "[]", "WebGL2 extension-object parity mismatches: {r}");
}

#[tokio::test]
async fn webgl1_only_extension_objects_match_chrome_surface() {
    // WebGL1-only extension objects captured from hardware Chromium on macOS.
    // Extensions shared with WebGL2 are covered exhaustively by the test above.
    let r = evaluate_macos(
        r#"
        (() => {
            const gl = document.getElementById('c').getContext('webgl');
            const expected = {
                "ANGLE_instanced_arrays": ["[object ANGLEInstancedArrays]", "VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE,drawArraysInstancedANGLE,drawElementsInstancedANGLE,vertexAttribDivisorANGLE"],
                "EXT_blend_minmax": ["[object EXTBlendMinMax]", "MIN_EXT,MAX_EXT"],
                "EXT_disjoint_timer_query": ["[object EXTDisjointTimerQuery]", "QUERY_COUNTER_BITS_EXT,CURRENT_QUERY_EXT,QUERY_RESULT_EXT,QUERY_RESULT_AVAILABLE_EXT,TIME_ELAPSED_EXT,TIMESTAMP_EXT,GPU_DISJOINT_EXT,beginQueryEXT,createQueryEXT,deleteQueryEXT,endQueryEXT,getQueryEXT,getQueryObjectEXT,isQueryEXT,queryCounterEXT"],
                "EXT_frag_depth": ["[object EXTFragDepth]", ""],
                "EXT_sRGB": ["[object EXTsRGB]", "SRGB_EXT,SRGB_ALPHA_EXT,SRGB8_ALPHA8_EXT,FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT"],
                "EXT_shader_texture_lod": ["[object EXTShaderTextureLOD]", ""],
                "OES_element_index_uint": ["[object OESElementIndexUint]", ""],
                "OES_fbo_render_mipmap": ["[object OESFboRenderMipmap]", ""],
                "OES_standard_derivatives": ["[object OESStandardDerivatives]", "FRAGMENT_SHADER_DERIVATIVE_HINT_OES"],
                "OES_texture_float": ["[object OESTextureFloat]", ""],
                "OES_texture_half_float": ["[object OESTextureHalfFloat]", "HALF_FLOAT_OES"],
                "OES_texture_half_float_linear": ["[object OESTextureHalfFloatLinear]", ""],
                "OES_vertex_array_object": ["[object OESVertexArrayObject]", "VERTEX_ARRAY_BINDING_OES,bindVertexArrayOES,createVertexArrayOES,deleteVertexArrayOES,isVertexArrayOES"],
                "WEBGL_color_buffer_float": ["[object WebGLColorBufferFloat]", "RGBA32F_EXT,FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT,UNSIGNED_NORMALIZED_EXT"],
                "WEBGL_depth_texture": ["[object WebGLDepthTexture]", "UNSIGNED_INT_24_8_WEBGL"],
                "WEBGL_draw_buffers": ["[object WebGLDrawBuffers]", "COLOR_ATTACHMENT0_WEBGL,COLOR_ATTACHMENT1_WEBGL,COLOR_ATTACHMENT2_WEBGL,COLOR_ATTACHMENT3_WEBGL,COLOR_ATTACHMENT4_WEBGL,COLOR_ATTACHMENT5_WEBGL,COLOR_ATTACHMENT6_WEBGL,COLOR_ATTACHMENT7_WEBGL,COLOR_ATTACHMENT8_WEBGL,COLOR_ATTACHMENT9_WEBGL,COLOR_ATTACHMENT10_WEBGL,COLOR_ATTACHMENT11_WEBGL,COLOR_ATTACHMENT12_WEBGL,COLOR_ATTACHMENT13_WEBGL,COLOR_ATTACHMENT14_WEBGL,COLOR_ATTACHMENT15_WEBGL,DRAW_BUFFER0_WEBGL,DRAW_BUFFER1_WEBGL,DRAW_BUFFER2_WEBGL,DRAW_BUFFER3_WEBGL,DRAW_BUFFER4_WEBGL,DRAW_BUFFER5_WEBGL,DRAW_BUFFER6_WEBGL,DRAW_BUFFER7_WEBGL,DRAW_BUFFER8_WEBGL,DRAW_BUFFER9_WEBGL,DRAW_BUFFER10_WEBGL,DRAW_BUFFER11_WEBGL,DRAW_BUFFER12_WEBGL,DRAW_BUFFER13_WEBGL,DRAW_BUFFER14_WEBGL,DRAW_BUFFER15_WEBGL,MAX_COLOR_ATTACHMENTS_WEBGL,MAX_DRAW_BUFFERS_WEBGL,drawBuffersWEBGL"],
            };
            const errors = [];
            if (gl.getSupportedExtensions().length !== 39) errors.push(`count:${gl.getSupportedExtensions().length}`);
            for (const [name, want] of Object.entries(expected)) {
                const ext = gl.getExtension(name);
                const proto = Object.getPrototypeOf(ext);
                if (Object.prototype.toString.call(ext) !== want[0]) errors.push(`tag:${name}`);
                if (Reflect.ownKeys(ext).length !== 0) errors.push(`own:${name}`);
                if (Object.getOwnPropertyNames(proto).join(',') !== want[1]) errors.push(`proto:${name}`);
                if (Object.getPrototypeOf(proto) !== Object.prototype) errors.push(`parent:${name}`);
                if (ext !== gl.getExtension(name.toLowerCase())) errors.push(`identity:${name}`);
            }

            const timer = gl.getExtension('EXT_disjoint_timer_query');
            const query = timer.createQueryEXT();
            if (Object.prototype.toString.call(query) !== '[object WebGLTimerQueryEXT]' || Reflect.ownKeys(query).length) errors.push('timer-query-shape');
            if (timer.isQueryEXT(query) !== false || timer.getQueryObjectEXT(query, timer.QUERY_RESULT_AVAILABLE_EXT) !== false || timer.getQueryObjectEXT(query, timer.QUERY_RESULT_EXT) !== 0 || timer.getQueryEXT(timer.TIME_ELAPSED_EXT, timer.CURRENT_QUERY_EXT) !== null) errors.push('timer-query-semantics');

            const vaoExt = gl.getExtension('OES_vertex_array_object');
            const vao = vaoExt.createVertexArrayOES();
            if (Object.prototype.toString.call(vao) !== '[object WebGLVertexArrayObjectOES]' || Reflect.ownKeys(vao).length || vaoExt.isVertexArrayOES(vao) !== false) errors.push('vao-shape');
            if (gl.getExtension('ANGLE_instanced_arrays').VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE !== 35070) errors.push('angle-value');
            if (gl.getExtension('WEBGL_draw_buffers').MAX_DRAW_BUFFERS_WEBGL !== 34852) errors.push('draw-buffers-value');
            return JSON.stringify(errors);
        })()
        "#,
    )
    .await;
    assert_eq!(r, "[]", "WebGL1 extension-object parity mismatches: {r}");
}
