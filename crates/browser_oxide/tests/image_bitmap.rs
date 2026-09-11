//! ImageBitmap crop/resize/snapshot behavior aligned with Chromium.

use browser_oxide::Page;
use std::time::Duration;

async fn run_async(js: &str) -> serde_json::Value {
    let mut page = Page::from_html(
        "<!doctype html><html><body></body></html>",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    page.evaluate(&format!(
        r#"globalThis.__bitmapResult = 'pending';
        (async () => {{ {js} }})().then(
            value => globalThis.__bitmapResult = JSON.stringify(value),
            error => globalThis.__bitmapResult = 'ERROR:' + error.name + ':' + error.message
        );"#
    ))
    .unwrap();

    for _ in 0..80 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(50))
            .await;
        let raw = page.evaluate("globalThis.__bitmapResult").unwrap();
        if raw != "pending" {
            return serde_json::from_str(&raw).unwrap_or_else(|error| panic!("{error}: {raw}"));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("ImageBitmap promise did not settle");
}

#[tokio::test(flavor = "current_thread")]
async fn image_bitmap_crop_resize_and_close_match_chromium() {
    let result = run_async(
        r#"
        const px = new Uint8ClampedArray([
            255,0,0,255,   0,255,0,255,   0,0,255,255,
            255,255,0,255, 255,0,255,255, 0,255,255,255,
        ]);
        const src = new ImageData(px, 3, 2);
        async function snap(args) {
            const bitmap = await createImageBitmap(...args);
            const canvas = document.createElement('canvas');
            canvas.width = bitmap.width;
            canvas.height = bitmap.height;
            const context = canvas.getContext('2d');
            context.drawImage(bitmap, 0, 0);
            const out = {
                width: bitmap.width,
                height: bitmap.height,
                own: Reflect.ownKeys(bitmap).map(String),
                tag: Object.prototype.toString.call(bitmap),
                pixels: Array.from(context.getImageData(0, 0, bitmap.width, bitmap.height).data),
            };
            bitmap.close();
            out.afterClose = [bitmap.width, bitmap.height];
            return out;
        }
        const result = {};
        result.fnLength = createImageBitmap.length;
        result.crop = await snap([src, 1, 0, 2, 2]);
        result.negWidth = await snap([src, 2, 0, -2, 2]);
        result.outOfBounds = await snap([src, 2, 0, 3, 2]);
        result.resizeWidth = await snap([src, { resizeWidth: 6 }]);
        result.pixelated = await snap([src, {
            resizeWidth: 2,
            resizeHeight: 4,
            resizeQuality: 'pixelated',
        }]);
        try { await createImageBitmap(src, 0, 0, 0, 2); }
        catch (error) { result.zeroCrop = error.name; }
        try { await createImageBitmap(src, { resizeWidth: 0 }); }
        catch (error) { result.zeroResize = error.name; }
        for (const [name, options] of Object.entries({
            negativeResize: { resizeWidth: -1 },
            nanResize: { resizeWidth: NaN },
            infiniteResize: { resizeWidth: Infinity },
            invalidQuality: { resizeQuality: 'bogus' },
        })) {
            try { await createImageBitmap(src, options); }
            catch (error) { result[name] = error.name; }
        }
        const optionReads = [];
        await createImageBitmap(src, new Proxy({}, {
            get(_target, property) {
                optionReads.push(String(property));
                return undefined;
            }
        }));
        result.optionReads = optionReads;
        try { await createImageBitmap(src, 3); }
        catch (error) { result.nonDictionary = error.name; }
        return result;
        "#,
    )
    .await;

    assert_eq!(result["fnLength"], 1);
    assert_eq!(result["crop"]["width"], 2);
    assert_eq!(result["crop"]["height"], 2);
    assert_eq!(result["crop"]["own"], serde_json::json!([]));
    assert_eq!(result["crop"]["tag"], "[object ImageBitmap]");
    assert_eq!(result["crop"]["afterClose"], serde_json::json!([0, 0]));
    assert_eq!(
        result["crop"]["pixels"],
        serde_json::json!([0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255, 0, 255, 255, 255])
    );
    assert_eq!(
        result["negWidth"]["pixels"],
        serde_json::json!([255, 0, 0, 255, 0, 255, 0, 255, 255, 255, 0, 255, 255, 0, 255, 255])
    );
    assert_eq!(
        result["outOfBounds"]["pixels"],
        serde_json::json!([
            0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0
        ])
    );
    assert_eq!(result["resizeWidth"]["width"], 6);
    assert_eq!(result["resizeWidth"]["height"], 4);
    assert_eq!(result["pixelated"]["width"], 2);
    assert_eq!(result["pixelated"]["height"], 4);
    assert_eq!(
        result["pixelated"]["pixels"],
        serde_json::json!([
            255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255, 0,
            255, 255, 255, 255, 255, 0, 255, 0, 255, 255, 255
        ])
    );
    assert_eq!(result["zeroCrop"], "RangeError");
    assert_eq!(result["zeroResize"], "InvalidStateError");
    assert_eq!(result["negativeResize"], "TypeError");
    assert_eq!(result["nanResize"], "TypeError");
    assert_eq!(result["infiniteResize"], "TypeError");
    assert_eq!(result["invalidQuality"], "TypeError");
    assert_eq!(result["nonDictionary"], "TypeError");
    assert_eq!(
        result["optionReads"],
        serde_json::json!([
            "colorSpaceConversion",
            "imageOrientation",
            "premultiplyAlpha",
            "resizeHeight",
            "resizeQuality",
            "resizeWidth"
        ])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn image_bitmap_from_canvas_is_a_snapshot() {
    let result = run_async(
        r#"
        const source = document.createElement('canvas');
        source.width = source.height = 1;
        const sourceContext = source.getContext('2d');
        sourceContext.fillStyle = '#ff0000';
        sourceContext.fillRect(0, 0, 1, 1);
        const bitmap = await createImageBitmap(source);
        sourceContext.fillStyle = '#0000ff';
        sourceContext.fillRect(0, 0, 1, 1);
        const output = document.createElement('canvas');
        output.width = output.height = 1;
        const outputContext = output.getContext('2d');
        outputContext.drawImage(bitmap, 0, 0);
        return Array.from(outputContext.getImageData(0, 0, 1, 1).data);
        "#,
    )
    .await;

    assert_eq!(result, serde_json::json!([255, 0, 0, 255]));
}
