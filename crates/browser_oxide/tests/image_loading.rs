//! Regression coverage for browser-managed HTML image loading.

use browser_oxide::Page;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve_png() -> (String, tokio::sync::oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (request_tx, request_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = socket.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let _ = request_tx.send(String::from_utf8_lossy(&request).into_owned());

        let pixels =
            image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 128, 255, 64]).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            png.len()
        );
        socket.write_all(headers.as_bytes()).await.unwrap();
        socket.write_all(&png).await.unwrap();
        let _ = socket.shutdown().await;
    });
    (format!("http://{addr}"), request_rx)
}

#[tokio::test(flavor = "current_thread")]
async fn image_load_is_trusted_and_decode_waits_for_intrinsic_dimensions() {
    let (origin, request_rx) = serve_png().await;
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        &format!("{origin}/page"),
        Some(profile),
    )
    .await
    .unwrap();

    let script = format!(
        r#"(() => {{
            const image = new Image();
            let loadEvent = null;
            image.addEventListener('load', event => {{
                loadEvent = {{
                    trusted: event.isTrusted,
                    isEvent: event instanceof Event,
                    target: event.target === image,
                }};
            }});
            image.src = {image_url:?};
            const completeWhilePending = image.complete;
            globalThis.__imageResult = 'pending';
            image.decode().then(() => {{
                return createImageBitmap(image).then(bitmap => {{
                const canvas = document.createElement('canvas');
                canvas.width = 2;
                canvas.height = 1;
                const context = canvas.getContext('2d');
                context.drawImage(bitmap, 0, 0);
                globalThis.__imageResult = JSON.stringify({{
                    completeWhilePending,
                    complete: image.complete,
                    width: image.width,
                    height: image.height,
                    naturalWidth: image.naturalWidth,
                    naturalHeight: image.naturalHeight,
                    loadEvent,
                    bitmapWidth: bitmap.width,
                    bitmapHeight: bitmap.height,
                    bitmapOwnNames: Object.getOwnPropertyNames(bitmap),
                    bitmapPrototypeNames: Object.getOwnPropertyNames(ImageBitmap.prototype),
                    bitmapTag: Object.prototype.toString.call(bitmap),
                    pixels: Array.from(context.getImageData(0, 0, 2, 1).data),
                }});
                }});
            }}, error => {{
                globalThis.__imageResult = 'ERROR:' + error;
            }});
        }})()"#,
        image_url = format!("{origin}/challenge.png")
    );
    page.evaluate(&script).unwrap();
    for _ in 0..40 {
        let _ = page
            .event_loop()
            .run_until_settled(Duration::from_millis(100))
            .await;
        if !matches!(
            page.evaluate("globalThis.__imageResult").as_deref(),
            Ok("pending")
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let raw = page.evaluate("globalThis.__imageResult").unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|error| panic!("{error}: {raw}"));
    assert_eq!(value["completeWhilePending"], false, "{raw}");
    assert_eq!(value["complete"], true, "{raw}");
    assert_eq!(value["width"], 2, "{raw}");
    assert_eq!(value["height"], 1, "{raw}");
    assert_eq!(value["naturalWidth"], 2, "{raw}");
    assert_eq!(value["naturalHeight"], 1, "{raw}");
    assert_eq!(value["bitmapWidth"], 2, "{raw}");
    assert_eq!(value["bitmapHeight"], 1, "{raw}");
    assert_eq!(value["bitmapOwnNames"], serde_json::json!([]), "{raw}");
    assert_eq!(
        value["bitmapPrototypeNames"],
        serde_json::json!(["width", "height", "close", "constructor"]),
        "{raw}"
    );
    assert_eq!(value["bitmapTag"], "[object ImageBitmap]", "{raw}");
    assert_eq!(
        value["pixels"],
        serde_json::json!([255, 0, 0, 255, 0, 128, 255, 64]),
        "{raw}"
    );
    assert_eq!(value["loadEvent"]["trusted"], true, "{raw}");
    assert_eq!(value["loadEvent"]["isEvent"], true, "{raw}");
    assert_eq!(value["loadEvent"]["target"], true, "{raw}");

    let request = request_rx.await.unwrap().to_ascii_lowercase();
    assert!(request.starts_with("get /challenge.png http/1.1\r\n"));
    assert!(
        request.contains("\r\nsec-fetch-dest: image\r\n"),
        "{request}"
    );
    assert!(
        request.contains("\r\nsec-fetch-mode: no-cors\r\n"),
        "{request}"
    );
    assert!(request.contains("\r\nreferer: "), "{request}");
}
