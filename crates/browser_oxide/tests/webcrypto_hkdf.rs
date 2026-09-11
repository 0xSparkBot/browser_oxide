use browser_oxide::js_runtime::{runtime::BrowserRuntimeOptions, BrowserJsRuntime};
use browser_oxide::Page;
use std::time::Duration;

#[tokio::test]
async fn window_hkdf_matches_chrome_and_rfc5869() {
    let html = r#"<!doctype html><script>
      (async () => {
        const hex = (buffer) => Array.from(new Uint8Array(buffer))
          .map((b) => b.toString(16).padStart(2, '0')).join('');
        const out = {};
        const ikm = new Uint8Array(22).fill(0x0b);
        const base = await crypto.subtle.importKey(
          'raw', ikm, 'HKDF', false, ['deriveBits', 'deriveKey']
        );
        out.key = {
          type: base.type,
          extractable: base.extractable,
          algorithm: base.algorithm,
          usages: base.usages,
          tag: Object.prototype.toString.call(base),
          own: Reflect.ownKeys(base).length,
        };
        out.rfc = hex(await crypto.subtle.deriveBits({
          name: 'HKDF', hash: 'SHA-256',
          salt: Uint8Array.from([0,1,2,3,4,5,6,7,8,9,10,11,12]),
          info: Uint8Array.from([240,241,242,243,244,245,246,247,248,249]),
        }, base, 336));
        const aes = await crypto.subtle.deriveKey({
          name: 'HKDF', hash: 'SHA-256',
          salt: new Uint8Array([1, 2]), info: new Uint8Array([3]),
        }, base, { name: 'AES-GCM', length: 128 }, true, ['encrypt']);
        out.aes = {
          algorithm: aes.algorithm,
          extractable: aes.extractable,
          usages: aes.usages,
          raw: hex(await crypto.subtle.exportKey('raw', aes)),
        };
        for (const [name, run] of Object.entries({
          extractableTrue: () => crypto.subtle.importKey('raw', new Uint8Array([1]), 'HKDF', true, ['deriveBits']),
          emptyUsages: () => crypto.subtle.importKey('raw', new Uint8Array([1]), 'HKDF', false, []),
          badUsage: () => crypto.subtle.importKey('raw', new Uint8Array([1]), 'HKDF', false, ['sign']),
          sevenBits: () => crypto.subtle.deriveBits({name:'HKDF',hash:'SHA-256',salt:new Uint8Array(0),info:new Uint8Array(0)}, base, 7),
          tooLong: () => crypto.subtle.deriveBits({name:'HKDF',hash:'SHA-256',salt:new Uint8Array(0),info:new Uint8Array(0)}, base, 65288),
        })) {
          try { await run(); out[name] = 'NO_THROW'; }
          catch (error) { out[name] = error.name; }
        }
        out.zeroLength = (await crypto.subtle.deriveBits({
          name:'HKDF', hash:'SHA-256', salt:new Uint8Array(0), info:new Uint8Array(0)
        }, base, 0)).byteLength;
        globalThis.__hkdfResult = out;
      })();
    </script>"#;
    let mut page = Page::from_html_with_url(
        html,
        "http://127.0.0.1:9232/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__hkdfResult)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["key"]["type"], "secret");
    assert_eq!(value["key"]["extractable"], false);
    assert_eq!(
        value["key"]["algorithm"],
        serde_json::json!({"name":"HKDF"})
    );
    assert_eq!(
        value["key"]["usages"],
        serde_json::json!(["deriveKey", "deriveBits"])
    );
    assert_eq!(value["key"]["tag"], "[object CryptoKey]");
    assert_eq!(value["key"]["own"], 0);
    assert_eq!(
        value["rfc"],
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    assert_eq!(
        value["aes"],
        serde_json::json!({
            "algorithm":{"name":"AES-GCM","length":128},
            "extractable":true,
            "usages":["encrypt"],
            "raw":"9fc8358ab33b65c15f61192941b0d89e"
        })
    );
    assert_eq!(value["extractableTrue"], "SyntaxError");
    assert_eq!(value["emptyUsages"], "SyntaxError");
    assert_eq!(value["badUsage"], "SyntaxError");
    assert_eq!(value["sevenBits"], "OperationError");
    assert_eq!(value["tooLong"], "OperationError");
    assert_eq!(value["zeroLength"], 0);
}

#[test]
fn worker_hkdf_uses_the_shared_webcrypto_backend() {
    let dom =
        browser_oxide::html_parser::parse_html("<html><body><div id=\"out\"></div></body></html>");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    let out = local.block_on(&rt, async move {
        let mut runtime = BrowserJsRuntime::with_options(
            dom,
            BrowserRuntimeOptions {
                is_secure_context: true,
                ..Default::default()
            },
        );
        runtime
            .execute_script(
                r#"
                const workerSource = `
                  (async () => {
                    try {
                      const ikm = new Uint8Array(22).fill(0x0b);
                      const key = await crypto.subtle.importKey('raw', ikm, 'HKDF', false, ['deriveBits']);
                      const bits = await crypto.subtle.deriveBits({
                        name:'HKDF', hash:'SHA-256',
                        salt:Uint8Array.from([0,1,2,3,4,5,6,7,8,9,10,11,12]),
                        info:Uint8Array.from([240,241,242,243,244,245,246,247,248,249])
                      }, key, 336);
                      const hex = Array.from(new Uint8Array(bits))
                        .map((b) => b.toString(16).padStart(2, '0')).join('');
                      self.postMessage(JSON.stringify({hex, algorithm:key.algorithm, usages:key.usages}));
                    } catch (error) {
                      self.postMessage(JSON.stringify({error:error.name + ':' + error.message}));
                    }
                  })();
                `;
                const worker = new Worker(URL.createObjectURL(new Blob([workerSource], {type:'text/javascript'})));
                worker.onmessage = (event) => {
                  document.querySelector('#out').textContent = event.data;
                  worker.terminate();
                };
                "#,
                None,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let _ = tokio::time::timeout(Duration::from_millis(50), runtime.run_event_loop()).await;
            let value = runtime
                .execute_script("document.querySelector('#out').textContent || ''", None)
                .unwrap_or_default();
            if !value.is_empty() || std::time::Instant::now() >= deadline {
                break value;
            }
        }
    });
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        value["hex"],
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    assert_eq!(value["algorithm"], serde_json::json!({"name":"HKDF"}));
    assert_eq!(value["usages"], serde_json::json!(["deriveBits"]));
    assert!(value.get("error").is_none());
}
