use browser_oxide::js_runtime::{runtime::BrowserRuntimeOptions, BrowserJsRuntime};
use browser_oxide::Page;
use std::time::Duration;

#[tokio::test]
async fn window_aes_kw_matches_rfc3394_and_webcrypto_semantics() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const enc = new TextEncoder();
        const hex = (buffer) => Array.from(new Uint8Array(buffer))
          .map((b) => b.toString(16).padStart(2, '0')).join('');
        const capture = async (name, run) => {
          try { await run(); out[name] = 'NO_THROW'; }
          catch (error) { out[name] = `${error.name}:${error.message}`; }
        };

        const kek = Uint8Array.from([
          0x00,0x01,0x02,0x03,0x04,0x05,0x06,0x07,
          0x08,0x09,0x0a,0x0b,0x0c,0x0d,0x0e,0x0f
        ]);
        const targetBytes = Uint8Array.from([
          0x00,0x11,0x22,0x33,0x44,0x55,0x66,0x77,
          0x88,0x99,0xaa,0xbb,0xcc,0xdd,0xee,0xff
        ]);
        const wrappingKey = await crypto.subtle.importKey(
          'raw', kek, 'AES-KW', true, ['wrapKey', 'unwrapKey']
        );
        const target = await crypto.subtle.importKey(
          'raw', targetBytes, { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
        );
        out.key = {
          tag: Object.prototype.toString.call(wrappingKey),
          own: Reflect.ownKeys(wrappingKey).length,
          type: wrappingKey.type,
          extractable: wrappingKey.extractable,
          algorithm: wrappingKey.algorithm,
          usages: wrappingKey.usages,
        };

        const wrapped = await crypto.subtle.wrapKey('raw', target, wrappingKey, 'AES-KW');
        out.wrapped = hex(wrapped);
        const unwrapped = await crypto.subtle.unwrapKey(
          'raw', wrapped, wrappingKey, { name: 'AES-KW' },
          { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
        );
        out.unwrapped = hex(await crypto.subtle.exportKey('raw', unwrapped));

        const generated = await crypto.subtle.generateKey(
          { name: 'AES-KW', length: 256 }, true, ['wrapKey']
        );
        out.generated = {
          algorithm: generated.algorithm,
          usages: generated.usages,
          rawLength: (await crypto.subtle.exportKey('raw', generated)).byteLength,
        };
        await capture('aes192Import', () => crypto.subtle.importKey(
          'raw', new Uint8Array(24), 'AES-KW', true, ['wrapKey']
        ));
        await capture('aes192Generate', () => crypto.subtle.generateKey(
          { name: 'AES-KW', length: 192 }, true, ['wrapKey']
        ));

        const pbkdf2 = await crypto.subtle.importKey(
          'raw', enc.encode('pw'), 'PBKDF2', false, ['deriveKey']
        );
        const derived = await crypto.subtle.deriveKey(
          { name: 'PBKDF2', salt: enc.encode('salt'), iterations: 2, hash: 'SHA-256' },
          pbkdf2, { name: 'AES-KW', length: 128 }, true, ['wrapKey']
        );
        out.derived = {
          algorithm: derived.algorithm,
          usages: derived.usages,
          raw: hex(await crypto.subtle.exportKey('raw', derived)),
        };
        await capture('aes192Derive', () => crypto.subtle.deriveKey(
          { name: 'PBKDF2', salt: enc.encode('salt'), iterations: 2, hash: 'SHA-256' },
          pbkdf2, { name: 'AES-KW', length: 192 }, true, ['wrapKey']
        ));

        const hkdf = await crypto.subtle.importKey(
          'raw', new Uint8Array(16).fill(7), 'HKDF', false, ['deriveKey']
        );
        const hkdfDerived = await crypto.subtle.deriveKey(
          { name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array([1]), info: new Uint8Array([2]) },
          hkdf, { name: 'AES-KW', length: 128 }, true, ['unwrapKey']
        );
        out.hkdfDerived = {
          algorithm: hkdfDerived.algorithm,
          usages: hkdfDerived.usages,
          rawLength: (await crypto.subtle.exportKey('raw', hkdfDerived)).byteLength,
        };

        await capture('badUsage', () => crypto.subtle.importKey(
          'raw', kek, 'AES-KW', true, ['encrypt']
        ));
        await capture('emptyUsages', () => crypto.subtle.importKey(
          'raw', kek, 'AES-KW', true, []
        ));
        await capture('badKeyLength', () => crypto.subtle.importKey(
          'raw', new Uint8Array(15), 'AES-KW', true, ['wrapKey']
        ));
        for (const [name, size] of [['tooSmallWrap', 8], ['unalignedWrap', 17]]) {
          const shortTarget = await crypto.subtle.importKey(
            'raw', new Uint8Array(size), { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
          );
          await capture(name, () => crypto.subtle.wrapKey('raw', shortTarget, wrappingKey, 'AES-KW'));
        }
        await capture('directEncrypt', () => crypto.subtle.encrypt(
          'AES-KW', wrappingKey, targetBytes
        ));
        const wrapOnly = await crypto.subtle.importKey(
          'raw', kek, 'AES-KW', true, ['wrapKey']
        );
        await capture('wrongUsage', () => crypto.subtle.unwrapKey(
          'raw', wrapped, wrapOnly, 'AES-KW',
          { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
        ));
        const tampered = new Uint8Array(wrapped.slice(0));
        tampered[0] ^= 1;
        await capture('tampered', () => crypto.subtle.unwrapKey(
          'raw', tampered, wrappingKey, 'AES-KW',
          { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
        ));
        globalThis.__aesKwResult = out;
      })();
    </script>"#;
    let mut page = Page::from_html_with_url(
        html,
        "http://127.0.0.1:9234/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__aesKwResult)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["key"]["tag"], "[object CryptoKey]");
    assert_eq!(value["key"]["own"], 0);
    assert_eq!(value["key"]["type"], "secret");
    assert_eq!(value["key"]["extractable"], true);
    assert_eq!(
        value["key"]["algorithm"],
        serde_json::json!({"name":"AES-KW","length":128})
    );
    assert_eq!(
        value["key"]["usages"],
        serde_json::json!(["wrapKey", "unwrapKey"])
    );
    assert_eq!(
        value["wrapped"],
        "1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5"
    );
    assert_eq!(value["unwrapped"], "00112233445566778899aabbccddeeff");
    assert_eq!(
        value["generated"]["algorithm"],
        serde_json::json!({"name":"AES-KW","length":256})
    );
    assert_eq!(value["generated"]["usages"], serde_json::json!(["wrapKey"]));
    assert_eq!(value["generated"]["rawLength"], 32);
    assert_eq!(
        value["aes192Import"],
        "OperationError:192-bit AES keys are not supported"
    );
    assert_eq!(
        value["aes192Generate"],
        "OperationError:192-bit AES keys are not supported"
    );
    assert_eq!(
        value["aes192Derive"],
        "OperationError:192-bit AES keys are not supported"
    );
    assert_eq!(
        value["derived"]["algorithm"],
        serde_json::json!({"name":"AES-KW","length":128})
    );
    assert_eq!(value["derived"]["usages"], serde_json::json!(["wrapKey"]));
    assert_eq!(value["derived"]["raw"], "061df616343cb988307172e2c3074be6");
    assert_eq!(
        value["hkdfDerived"]["algorithm"],
        serde_json::json!({"name":"AES-KW","length":128})
    );
    assert_eq!(
        value["hkdfDerived"]["usages"],
        serde_json::json!(["unwrapKey"])
    );
    assert_eq!(value["hkdfDerived"]["rawLength"], 16);
    assert!(value["badUsage"]
        .as_str()
        .unwrap()
        .starts_with("SyntaxError:"));
    assert!(value["emptyUsages"]
        .as_str()
        .unwrap()
        .starts_with("SyntaxError:"));
    assert!(value["badKeyLength"]
        .as_str()
        .unwrap()
        .starts_with("DataError:"));
    assert_eq!(
        value["tooSmallWrap"],
        "OperationError:The provided data is too small"
    );
    assert_eq!(
        value["unalignedWrap"],
        "DataError:The AES-KW input data length is invalid: not a multiple of 8 bytes"
    );
    assert!(value["directEncrypt"]
        .as_str()
        .unwrap()
        .starts_with("NotSupportedError:"));
    assert!(value["wrongUsage"]
        .as_str()
        .unwrap()
        .starts_with("InvalidAccessError:"));
    assert!(value["tampered"]
        .as_str()
        .unwrap()
        .starts_with("OperationError:"));
}

#[test]
fn worker_aes_kw_uses_shared_webcrypto_backend() {
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
        runtime.execute_script(r#"
          const workerSource = `
            (async () => {
              try {
                const kek = Uint8Array.from([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
                const raw = Uint8Array.from([0,17,34,51,68,85,102,119,136,153,170,187,204,221,238,255]);
                const wk = await crypto.subtle.importKey('raw', kek, 'AES-KW', true, ['wrapKey','unwrapKey']);
                const target = await crypto.subtle.importKey('raw', raw, {name:'HMAC',hash:'SHA-256'}, true, ['sign']);
                const wrapped = await crypto.subtle.wrapKey('raw', target, wk, 'AES-KW');
                const unwrapped = await crypto.subtle.unwrapKey(
                  'raw', wrapped, wk, 'AES-KW', {name:'HMAC',hash:'SHA-256'}, true, ['sign']
                );
                let aes192Error = null;
                try {
                  await crypto.subtle.importKey('raw', new Uint8Array(24), 'AES-KW', true, ['wrapKey']);
                } catch (error) {
                  aes192Error = error.name + ':' + error.message;
                }
                const hex = (buffer) => Array.from(new Uint8Array(buffer))
                  .map((b) => b.toString(16).padStart(2,'0')).join('');
                self.postMessage(JSON.stringify({
                  wrapped:hex(wrapped),
                  raw:hex(await crypto.subtle.exportKey('raw', unwrapped)),
                  algorithm:wk.algorithm,
                  tag:Object.prototype.toString.call(wk),
                  own:Reflect.ownKeys(wk).length,
                  aes192Error
                }));
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
        "#, None).unwrap();
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
        value["wrapped"],
        "1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5"
    );
    assert_eq!(value["raw"], "00112233445566778899aabbccddeeff");
    assert_eq!(
        value["algorithm"],
        serde_json::json!({"name":"AES-KW","length":128})
    );
    assert_eq!(value["tag"], "[object CryptoKey]");
    assert_eq!(value["own"], 0);
    assert_eq!(
        value["aes192Error"],
        "OperationError:192-bit AES keys are not supported"
    );
    assert!(value.get("error").is_none());
}
