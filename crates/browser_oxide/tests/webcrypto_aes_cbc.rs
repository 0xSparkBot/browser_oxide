use browser_oxide::js_runtime::{runtime::BrowserRuntimeOptions, BrowserJsRuntime};
use browser_oxide::Page;
use std::time::Duration;

#[tokio::test]
async fn window_aes_cbc_matches_chrome_semantics() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const enc = new TextEncoder();
        const dec = new TextDecoder();
        const hex = (buffer) => Array.from(new Uint8Array(buffer))
          .map((b) => b.toString(16).padStart(2, '0')).join('');
        const capture = async (name, run) => {
          try { await run(); out[name] = 'NO_THROW'; }
          catch (error) { out[name] = `${error.name}:${error.message}`; }
        };

        const raw = new Uint8Array(16);
        const iv = new Uint8Array(16);
        const key = await crypto.subtle.importKey(
          'raw', raw, 'AES-CBC', true, ['encrypt', 'decrypt', 'wrapKey', 'unwrapKey']
        );
        out.key = {
          tag: Object.prototype.toString.call(key),
          own: Reflect.ownKeys(key).length,
          type: key.type,
          extractable: key.extractable,
          algorithm: key.algorithm,
          usages: key.usages,
        };
        const cipher = await crypto.subtle.encrypt(
          { name: 'AES-CBC', iv }, key, enc.encode('hello')
        );
        out.cipher = { length: cipher.byteLength, hex: hex(cipher) };
        out.plain = dec.decode(await crypto.subtle.decrypt({ name: 'AES-CBC', iv }, key, cipher));

        await capture('badIv', () => crypto.subtle.encrypt(
          { name: 'AES-CBC', iv: new Uint8Array(15) }, key, enc.encode('hello')
        ));
        await capture('missingIv', () => crypto.subtle.encrypt(
          { name: 'AES-CBC' }, key, enc.encode('hello')
        ));
        const tampered = new Uint8Array(cipher.slice(0));
        tampered[tampered.length - 1] ^= 1;
        await capture('badPadding', () => crypto.subtle.decrypt(
          { name: 'AES-CBC', iv }, key, tampered
        ));
        await capture('shortCipher', () => crypto.subtle.decrypt(
          { name: 'AES-CBC', iv }, key, new Uint8Array(15)
        ));
        await capture('badKeyLength', () => crypto.subtle.importKey(
          'raw', new Uint8Array(15), 'AES-CBC', true, ['encrypt']
        ));
        await capture('aes192Import', () => crypto.subtle.importKey(
          'raw', new Uint8Array(24), 'AES-CBC', true, ['encrypt']
        ));
        await capture('aes192Generate', () => crypto.subtle.generateKey(
          { name: 'AES-CBC', length: 192 }, true, ['encrypt']
        ));
        await capture('emptyUsages', () => crypto.subtle.importKey(
          'raw', raw, 'AES-CBC', true, []
        ));
        await capture('badUsage', () => crypto.subtle.importKey(
          'raw', raw, 'AES-CBC', true, ['sign']
        ));
        const encryptOnly = await crypto.subtle.importKey(
          'raw', raw, 'AES-CBC', true, ['encrypt']
        );
        await capture('wrongUsage', () => crypto.subtle.decrypt(
          { name: 'AES-CBC', iv }, encryptOnly, cipher
        ));

        const generated = await crypto.subtle.generateKey(
          { name: 'AES-CBC', length: 256 }, true, ['encrypt', 'decrypt']
        );
        out.generated = {
          algorithm: generated.algorithm,
          usages: generated.usages,
          rawLength: (await crypto.subtle.exportKey('raw', generated)).byteLength,
        };

        const base = await crypto.subtle.importKey(
          'raw', enc.encode('pw'), 'PBKDF2', false, ['deriveKey']
        );
        const derived = await crypto.subtle.deriveKey(
          { name: 'PBKDF2', salt: enc.encode('salt'), iterations: 2, hash: 'SHA-256' },
          base, { name: 'AES-CBC', length: 128 }, true, ['encrypt']
        );
        out.derived = {
          algorithm: derived.algorithm,
          usages: derived.usages,
          raw: hex(await crypto.subtle.exportKey('raw', derived)),
        };

        const wrappingKey = await crypto.subtle.importKey(
          'raw', new Uint8Array(16), 'AES-CBC', true, ['wrapKey', 'unwrapKey']
        );
        const target = await crypto.subtle.importKey(
          'raw', new Uint8Array([1,2,3,4]),
          { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
        );
        const wrapped = await crypto.subtle.wrapKey(
          'raw', target, wrappingKey, { name: 'AES-CBC', iv }
        );
        const unwrapped = await crypto.subtle.unwrapKey(
          'raw', wrapped, wrappingKey, { name: 'AES-CBC', iv },
          { name: 'HMAC', hash: 'SHA-256' }, true, ['sign']
        );
        out.wrap = {
          length: wrapped.byteLength,
          raw: hex(await crypto.subtle.exportKey('raw', unwrapped)),
        };
        globalThis.__aesCbcResult = out;
      })();
    </script>"#;
    let mut page = Page::from_html_with_url(
        html,
        "http://127.0.0.1:9233/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__aesCbcResult)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["key"]["tag"], "[object CryptoKey]");
    assert_eq!(value["key"]["own"], 0);
    assert_eq!(value["key"]["type"], "secret");
    assert_eq!(value["key"]["extractable"], true);
    assert_eq!(
        value["key"]["algorithm"],
        serde_json::json!({"name":"AES-CBC","length":128})
    );
    assert_eq!(
        value["key"]["usages"],
        serde_json::json!(["encrypt", "decrypt", "wrapKey", "unwrapKey"])
    );
    assert_eq!(value["cipher"]["length"], 16);
    assert_eq!(value["cipher"]["hex"], "9834ed518cbc8fbe9af3c6ecb75eb8c0");
    assert_eq!(value["plain"], "hello");
    assert_eq!(
        value["badIv"],
        "OperationError:The \"iv\" has an unexpected length -- must be 16 bytes"
    );
    assert_eq!(value["missingIv"], "TypeError:Failed to execute 'encrypt' on 'SubtleCrypto': AesCbcParams: iv: Missing required property");
    assert_eq!(value["badPadding"], "OperationError:");
    assert_eq!(value["shortCipher"], "OperationError:");
    assert_eq!(value["badKeyLength"], "DataError:Invalid key length");
    assert_eq!(
        value["aes192Import"],
        "OperationError:192-bit AES keys are not supported"
    );
    assert_eq!(
        value["aes192Generate"],
        "OperationError:192-bit AES keys are not supported"
    );
    assert_eq!(
        value["emptyUsages"],
        "SyntaxError:Usages cannot be empty when creating a key."
    );
    assert_eq!(
        value["badUsage"],
        "SyntaxError:Cannot create a key using the specified key usages."
    );
    assert_eq!(
        value["wrongUsage"],
        "InvalidAccessError:key.usages does not permit this operation"
    );
    assert_eq!(
        value["generated"]["algorithm"],
        serde_json::json!({"name":"AES-CBC","length":256})
    );
    assert_eq!(
        value["generated"]["usages"],
        serde_json::json!(["encrypt", "decrypt"])
    );
    assert_eq!(value["generated"]["rawLength"], 32);
    assert_eq!(
        value["derived"]["algorithm"],
        serde_json::json!({"name":"AES-CBC","length":128})
    );
    assert_eq!(value["derived"]["usages"], serde_json::json!(["encrypt"]));
    assert_eq!(value["derived"]["raw"], "061df616343cb988307172e2c3074be6");
    assert_eq!(value["wrap"]["length"], 16);
    assert_eq!(value["wrap"]["raw"], "01020304");
}

#[test]
fn worker_aes_cbc_uses_shared_webcrypto_backend() {
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
                const key = await crypto.subtle.importKey(
                  'raw', new Uint8Array(16), 'AES-CBC', true, ['encrypt','decrypt']
                );
                const iv = new Uint8Array(16);
                const cipher = await crypto.subtle.encrypt(
                  {name:'AES-CBC',iv}, key, new TextEncoder().encode('hello')
                );
                const plain = await crypto.subtle.decrypt({name:'AES-CBC',iv}, key, cipher);
                const hex = Array.from(new Uint8Array(cipher))
                  .map((b) => b.toString(16).padStart(2,'0')).join('');
                self.postMessage(JSON.stringify({
                  hex,
                  plain:new TextDecoder().decode(plain),
                  algorithm:key.algorithm,
                  tag:Object.prototype.toString.call(key),
                  own:Reflect.ownKeys(key).length
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
    assert_eq!(value["hex"], "9834ed518cbc8fbe9af3c6ecb75eb8c0");
    assert_eq!(value["plain"], "hello");
    assert_eq!(
        value["algorithm"],
        serde_json::json!({"name":"AES-CBC","length":128})
    );
    assert_eq!(value["tag"], "[object CryptoKey]");
    assert_eq!(value["own"], 0);
    assert!(value.get("error").is_none());
}
