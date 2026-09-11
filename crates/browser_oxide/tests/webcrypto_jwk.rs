use browser_oxide::js_runtime::{runtime::BrowserRuntimeOptions, BrowserJsRuntime};
use browser_oxide::Page;
use std::time::Duration;

#[tokio::test]
async fn secret_key_jwk_import_export_matches_chrome() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const zero128 = new Uint8Array(16);
        const zero128Jwk = 'AAAAAAAAAAAAAAAAAAAAAA';
        const capture = async (name, run) => {
          try { out[name] = await run(); }
          catch (error) { out[name] = `${error.name}:${error.message}`; }
        };
        const summarize = (jwk) => ({
          keys: Object.keys(jwk),
          alg: jwk.alg,
          ext: jwk.ext,
          k: jwk.k,
          key_ops: jwk.key_ops,
          kty: jwk.kty,
        });
        const hex = (buffer) => Array.from(new Uint8Array(buffer))
          .map((b) => b.toString(16).padStart(2, '0')).join('');

        for (const [name, algorithm, usages] of [
          ['gcm', 'AES-GCM', ['encrypt', 'decrypt']],
          ['cbc', 'AES-CBC', ['encrypt', 'decrypt']],
          ['kw', 'AES-KW', ['wrapKey', 'unwrapKey']],
        ]) {
          const key = await crypto.subtle.importKey('raw', zero128, algorithm, true, usages);
          out[name] = summarize(await crypto.subtle.exportKey('jwk', key));
        }
        const hmac = await crypto.subtle.importKey(
          'raw', zero128, {name:'HMAC', hash:'SHA-256'}, true, ['sign', 'verify']
        );
        out.hmac = summarize(await crypto.subtle.exportKey('jwk', hmac));

        await capture('gcmImport', async () => {
          const key = await crypto.subtle.importKey('jwk', {
            kty:'oct', k:zero128Jwk, alg:'A128GCM', key_ops:['encrypt'], ext:true
          }, {name:'AES-GCM'}, true, ['encrypt']);
          return {
            algorithm:key.algorithm, type:key.type, extractable:key.extractable,
            usages:key.usages, raw:hex(await crypto.subtle.exportKey('raw', key))
          };
        });
        await capture('cbcImport', async () => {
          const key = await crypto.subtle.importKey('jwk', {
            kty:'oct', k:zero128Jwk, alg:'A128CBC', key_ops:['decrypt'], ext:true
          }, {name:'AES-CBC'}, true, ['decrypt']);
          return {algorithm:key.algorithm, usages:key.usages};
        });
        await capture('kwImport', async () => {
          const key = await crypto.subtle.importKey('jwk', {
            kty:'oct', k:zero128Jwk, alg:'A128KW', key_ops:['unwrapKey'], ext:true
          }, {name:'AES-KW'}, true, ['unwrapKey']);
          return {algorithm:key.algorithm, usages:key.usages};
        });
        await capture('hmacImport', async () => {
          const key = await crypto.subtle.importKey('jwk', {
            kty:'oct', k:zero128Jwk, alg:'HS256', key_ops:['sign'], ext:true
          }, {name:'HMAC', hash:'SHA-256'}, true, ['sign']);
          return {algorithm:key.algorithm, usages:key.usages};
        });
        await capture('optionalMembers', async () => {
          const key = await crypto.subtle.importKey(
            'jwk', {kty:'oct', k:zero128Jwk}, {name:'AES-GCM'}, true, ['encrypt']
          );
          return key.algorithm;
        });
        await capture('algMismatch', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:zero128Jwk, alg:'A128CBC', key_ops:['encrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('keyOpsMismatch', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:zero128Jwk, alg:'A128GCM', key_ops:['decrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('extMismatch', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:zero128Jwk, alg:'A128GCM', key_ops:['encrypt'], ext:false
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('useMismatch', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:zero128Jwk, alg:'A128GCM', key_ops:['encrypt'], ext:true, use:'sig'
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('badKty', () => crypto.subtle.importKey('jwk', {
          kty:'RSA', k:zero128Jwk, alg:'A128GCM', key_ops:['encrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('missingKty', () => crypto.subtle.importKey('jwk', {
          k:zero128Jwk, alg:'A128GCM', key_ops:['encrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('badK', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:'!', alg:'A128GCM', key_ops:['encrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('missingK', () => crypto.subtle.importKey('jwk', {
          kty:'oct', alg:'A128GCM', key_ops:['encrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('wrongLengthForAlg', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
          alg:'A128GCM', key_ops:['encrypt'], ext:true
        }, {name:'AES-GCM'}, true, ['encrypt']));
        await capture('hmacAlgMismatch', () => crypto.subtle.importKey('jwk', {
          kty:'oct', k:zero128Jwk, alg:'HS384', key_ops:['sign'], ext:true
        }, {name:'HMAC', hash:'SHA-256'}, true, ['sign']));
        await capture('nonObjectJwk', () => crypto.subtle.importKey(
          'jwk', 'nope', {name:'AES-GCM'}, true, ['encrypt']
        ));
        globalThis.__jwkResult = out;
      })();
    </script>"#;
    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/webcrypto-jwk",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__jwkResult)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    for (name, alg, usages) in [
        ("gcm", "A128GCM", serde_json::json!(["encrypt", "decrypt"])),
        ("cbc", "A128CBC", serde_json::json!(["encrypt", "decrypt"])),
        ("kw", "A128KW", serde_json::json!(["wrapKey", "unwrapKey"])),
        ("hmac", "HS256", serde_json::json!(["sign", "verify"])),
    ] {
        assert_eq!(
            value[name]["keys"],
            serde_json::json!(["alg", "ext", "k", "key_ops", "kty"])
        );
        assert_eq!(value[name]["alg"], alg);
        assert_eq!(value[name]["ext"], true);
        assert_eq!(value[name]["k"], "AAAAAAAAAAAAAAAAAAAAAA");
        assert_eq!(value[name]["key_ops"], usages);
        assert_eq!(value[name]["kty"], "oct");
    }
    assert_eq!(
        value["gcmImport"]["algorithm"],
        serde_json::json!({"name":"AES-GCM","length":128})
    );
    assert_eq!(value["gcmImport"]["type"], "secret");
    assert_eq!(value["gcmImport"]["extractable"], true);
    assert_eq!(value["gcmImport"]["usages"], serde_json::json!(["encrypt"]));
    assert_eq!(
        value["gcmImport"]["raw"],
        "00000000000000000000000000000000"
    );
    assert_eq!(
        value["cbcImport"]["algorithm"],
        serde_json::json!({"name":"AES-CBC","length":128})
    );
    assert_eq!(
        value["kwImport"]["algorithm"],
        serde_json::json!({"name":"AES-KW","length":128})
    );
    assert_eq!(
        value["hmacImport"]["algorithm"],
        serde_json::json!({"name":"HMAC","hash":{"name":"SHA-256"},"length":128})
    );
    assert_eq!(
        value["optionalMembers"],
        serde_json::json!({"name":"AES-GCM","length":128})
    );
    assert_eq!(value["algMismatch"], "DataError:The JWK \"alg\" member was inconsistent with that specified by the Web Crypto call");
    assert_eq!(value["keyOpsMismatch"], "DataError:The JWK \"key_ops\" member was inconsistent with that specified by the Web Crypto call. The JWK usage must be a superset of those requested");
    assert_eq!(value["extMismatch"], "DataError:The \"ext\" member of the JWK dictionary is inconsistent what that specified by the Web Crypto call");
    assert_eq!(value["useMismatch"], "DataError:The JWK \"use\" member was inconsistent with that specified by the Web Crypto call. The JWK usage must be a superset of those requested");
    assert_eq!(
        value["badKty"],
        "DataError:The JWK \"kty\" member was not \"oct\""
    );
    assert_eq!(
        value["missingKty"],
        "DataError:The required JWK member \"kty\" was missing"
    );
    assert_eq!(
        value["badK"],
        "DataError:The JWK member \"k\" could not be base64url decoded or contained padding"
    );
    assert_eq!(
        value["missingK"],
        "DataError:The required JWK member \"k\" was missing"
    );
    assert_eq!(value["wrongLengthForAlg"], "DataError:The JWK \"k\" member did not include the right length of key data for the given algorithm.");
    assert_eq!(value["hmacAlgMismatch"], "DataError:The JWK \"alg\" member was inconsistent with that specified by the Web Crypto call");
    assert_eq!(value["nonObjectJwk"], "TypeError:Failed to execute 'importKey' on 'SubtleCrypto': The provided value is not of type '(ArrayBuffer or ArrayBufferView or JsonWebKey)'.");
}

#[test]
fn worker_secret_key_jwk_uses_shared_webcrypto_backend() {
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
            BrowserRuntimeOptions { is_secure_context: true, ..Default::default() },
        );
        runtime.execute_script(r#"
          const workerSource = `
            (async () => {
              try {
                const jwk = {kty:'oct', k:'AAAAAAAAAAAAAAAAAAAAAA', alg:'A128GCM', key_ops:['encrypt'], ext:true};
                const key = await crypto.subtle.importKey('jwk', jwk, {name:'AES-GCM'}, true, ['encrypt']);
                const exported = await crypto.subtle.exportKey('jwk', key);
                self.postMessage(JSON.stringify({
                  algorithm:key.algorithm,
                  usages:key.usages,
                  keys:Object.keys(exported),
                  alg:exported.alg,
                  k:exported.k,
                  key_ops:exported.key_ops,
                  ext:exported.ext,
                  kty:exported.kty
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
            let value = runtime.execute_script("document.querySelector('#out').textContent || ''", None).unwrap_or_default();
            if !value.is_empty() || std::time::Instant::now() >= deadline { break value; }
        }
    });
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        value["algorithm"],
        serde_json::json!({"name":"AES-GCM","length":128})
    );
    assert_eq!(value["usages"], serde_json::json!(["encrypt"]));
    assert_eq!(
        value["keys"],
        serde_json::json!(["alg", "ext", "k", "key_ops", "kty"])
    );
    assert_eq!(value["alg"], "A128GCM");
    assert_eq!(value["k"], "AAAAAAAAAAAAAAAAAAAAAA");
    assert_eq!(value["key_ops"], serde_json::json!(["encrypt"]));
    assert_eq!(value["ext"], true);
    assert_eq!(value["kty"], "oct");
    assert!(value.get("error").is_none());
}

#[tokio::test]
async fn jwk_wrap_unwrap_serializes_json_like_chrome() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const zero16 = new Uint8Array(16);
        const targetRaw = new Uint8Array([1,2,3,4,5,6,7,8]);
        const target = await crypto.subtle.importKey(
          'raw', targetRaw, {name:'HMAC', hash:'SHA-256'}, true, ['sign']
        );
        const capture = async (name, run) => {
          try { out[name] = await run(); }
          catch (error) { out[name] = `${error.name}:${error.message}`; }
        };
        const roundTrip = async (keyAlgorithm, usages, wrapAlgorithm) => {
          const wrappingKey = await crypto.subtle.importKey('raw', zero16, keyAlgorithm, true, usages);
          const wrapped = await crypto.subtle.wrapKey('jwk', target, wrappingKey, wrapAlgorithm);
          const unwrapped = await crypto.subtle.unwrapKey(
            'jwk', wrapped, wrappingKey, wrapAlgorithm,
            {name:'HMAC', hash:'SHA-256'}, true, ['sign']
          );
          return {
            wrappedLen: wrapped.byteLength,
            raw: Array.from(new Uint8Array(await crypto.subtle.exportKey('raw', unwrapped))),
            algorithm: unwrapped.algorithm,
            usages: unwrapped.usages,
          };
        };
        await capture('gcm', () => roundTrip(
          {name:'AES-GCM'}, ['wrapKey','unwrapKey'], {name:'AES-GCM', iv:new Uint8Array(12)}
        ));
        await capture('cbc', () => roundTrip(
          {name:'AES-CBC'}, ['wrapKey','unwrapKey'], {name:'AES-CBC', iv:new Uint8Array(16)}
        ));
        await capture('kw', () => roundTrip(
          {name:'AES-KW'}, ['wrapKey','unwrapKey'], {name:'AES-KW'}
        ));
        globalThis.__jwkWrapResult = out;
      })();
    </script>"#;
    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/webcrypto-jwk-wrap",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__jwkWrapResult)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    for (name, wrapped_len) in [("gcm", 91), ("cbc", 80)] {
        assert_eq!(value[name]["wrappedLen"], wrapped_len);
        assert_eq!(
            value[name]["raw"],
            serde_json::json!([1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(
            value[name]["algorithm"],
            serde_json::json!({"name":"HMAC","hash":{"name":"SHA-256"},"length":64})
        );
        assert_eq!(value[name]["usages"], serde_json::json!(["sign"]));
    }
    assert_eq!(
        value["kw"],
        "DataError:The AES-KW input data length is invalid: not a multiple of 8 bytes"
    );
}
