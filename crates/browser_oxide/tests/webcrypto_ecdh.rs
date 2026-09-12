use browser_oxide::js_runtime::{runtime::BrowserRuntimeOptions, BrowserJsRuntime};
use browser_oxide::Page;
use std::time::Duration;

#[tokio::test]
async fn ecdh_matches_chrome_148_semantics() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const hex = (buffer) => Array.from(new Uint8Array(buffer))
          .map((b) => b.toString(16).padStart(2, '0')).join('');
        const capture = async (name, run) => {
          try { await run(); out[name] = 'NO_THROW'; }
          catch (error) { out[name] = `${error.name}:${error.message}`; }
        };
        const maskPrefix = (buffer, bits) => {
          const full = new Uint8Array(buffer);
          const bytes = Math.ceil(bits / 8);
          const result = full.slice(0, bytes);
          const remainder = bits % 8;
          if (remainder && result.length) result[result.length - 1] &= (0xff << (8 - remainder)) & 0xff;
          return hex(result);
        };

        for (const curve of ['P-256', 'P-384', 'P-521']) {
          const a = await crypto.subtle.generateKey(
            { name: 'ECDH', namedCurve: curve }, true, ['deriveBits', 'deriveKey']
          );
          const b = await crypto.subtle.generateKey(
            { name: 'ECDH', namedCurve: curve }, false, ['deriveBits']
          );
          const fullBits = { 'P-256': 256, 'P-384': 384, 'P-521': 528 }[curve];
          const ab = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey, fullBits);
          const ba = await crypto.subtle.deriveBits({name:'ECDH', public:a.publicKey}, b.privateKey, fullBits);
          const raw = await crypto.subtle.exportKey('raw', a.publicKey);
          const spki = await crypto.subtle.exportKey('spki', a.publicKey);
          const pkcs8 = await crypto.subtle.exportKey('pkcs8', a.privateKey);
          const jwkPub = await crypto.subtle.exportKey('jwk', a.publicKey);
          const jwkPriv = await crypto.subtle.exportKey('jwk', a.privateKey);
          const importedRaw = await crypto.subtle.importKey(
            'raw', raw, {name:'ECDH', namedCurve:curve}, true, []
          );
          const importedSpki = await crypto.subtle.importKey(
            'spki', spki, {name:'ECDH', namedCurve:curve}, true, []
          );
          const importedPkcs8 = await crypto.subtle.importKey(
            'pkcs8', pkcs8, {name:'ECDH', namedCurve:curve}, true, ['deriveBits']
          );
          const importedJwkPub = await crypto.subtle.importKey(
            'jwk', jwkPub, {name:'ECDH', namedCurve:curve}, true, []
          );
          const importedJwkPriv = await crypto.subtle.importKey(
            'jwk', jwkPriv, {name:'ECDH', namedCurve:curve}, true, ['deriveBits']
          );
          const importedRawNonExtractable = await crypto.subtle.importKey(
            'raw', raw, {name:'ECDH', namedCurve:curve}, false, []
          );
          const importedPrivateBits = await crypto.subtle.deriveBits(
            {name:'ECDH', public:b.publicKey}, importedPkcs8, fullBits
          );
          const importedRawBits = await crypto.subtle.deriveBits(
            {name:'ECDH', public:importedRaw}, b.privateKey, fullBits
          );
          const importedSpkiBits = await crypto.subtle.deriveBits(
            {name:'ECDH', public:importedSpki}, b.privateKey, fullBits
          );
          const importedJwkPubBits = await crypto.subtle.deriveBits(
            {name:'ECDH', public:importedJwkPub}, b.privateKey, fullBits
          );
          const importedJwkPrivBits = await crypto.subtle.deriveBits(
            {name:'ECDH', public:b.publicKey}, importedJwkPriv, fullBits
          );
          out[curve] = {
            pub: {
              tag:Object.prototype.toString.call(a.publicKey), own:Reflect.ownKeys(a.publicKey).length,
              type:a.publicKey.type, extractable:a.publicKey.extractable,
              algorithm:a.publicKey.algorithm, usages:a.publicKey.usages,
            },
            priv: {
              tag:Object.prototype.toString.call(a.privateKey), own:Reflect.ownKeys(a.privateKey).length,
              type:a.privateKey.type, extractable:a.privateKey.extractable,
              algorithm:a.privateKey.algorithm, usages:a.privateKey.usages,
            },
            nonExtractable: { pub:b.publicKey.extractable, priv:b.privateKey.extractable },
            rawLen:raw.byteLength, spkiLen:spki.byteLength, pkcs8Len:pkcs8.byteLength,
            jwkPub:{keys:Object.keys(jwkPub), crv:jwkPub.crv, ext:jwkPub.ext, key_ops:jwkPub.key_ops, kty:jwkPub.kty},
            jwkPriv:{keys:Object.keys(jwkPriv), crv:jwkPriv.crv, ext:jwkPriv.ext, key_ops:jwkPriv.key_ops, kty:jwkPriv.kty},
            symmetric:hex(ab) === hex(ba),
            importedSymmetric:[importedPrivateBits, importedRawBits, importedSpkiBits, importedJwkPubBits, importedJwkPrivBits]
              .every((bits) => hex(bits) === hex(ab)),
            imported:{
              raw:importedRaw.algorithm, spki:importedSpki.algorithm,
              jwkPub:importedJwkPub.algorithm, jwkPriv:importedJwkPriv.algorithm,
            },
            importedPublicExtractable: importedRawNonExtractable.extractable,
          };

          if (curve === 'P-256') {
            const omitted = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey);
            const nullLength = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey, null);
            const one = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey, 1);
            const nine = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey, 9);
            const oneTwentySeven = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey, 127);
            const zero = await crypto.subtle.deriveBits({name:'ECDH', public:b.publicKey}, a.privateKey, 0);
            const derived = await crypto.subtle.deriveKey(
              {name:'ECDH', public:b.publicKey}, a.privateKey,
              {name:'AES-GCM', length:128}, true, ['encrypt']
            );
            out.bits = {
              full:hex(ab), omitted:hex(omitted), nullLength:hex(nullLength), zeroLen:zero.byteLength,
              one:hex(one), oneExpected:maskPrefix(ab, 1),
              nine:hex(nine), nineExpected:maskPrefix(ab, 9),
              oneTwentySeven:hex(oneTwentySeven), oneTwentySevenExpected:maskPrefix(ab, 127),
              derivedAlgorithm:derived.algorithm, derivedUsages:derived.usages,
              derivedRawLen:(await crypto.subtle.exportKey('raw', derived)).byteLength,
            };
            await capture('tooLong', () => crypto.subtle.deriveBits(
              {name:'ECDH', public:b.publicKey}, a.privateKey, 257
            ));
            const p384 = await crypto.subtle.generateKey(
              {name:'ECDH', namedCurve:'P-384'}, false, ['deriveBits']
            );
            await capture('badCurve', () => crypto.subtle.deriveBits(
              {name:'ECDH', public:p384.publicKey}, a.privateKey, 256
            ));
            await capture('publicAsBase', () => crypto.subtle.deriveBits(
              {name:'ECDH', public:b.publicKey}, a.publicKey, 256
            ));
            await capture('emptyUsage', () => crypto.subtle.generateKey(
              {name:'ECDH', namedCurve:'P-256'}, false, []
            ));
            await capture('badUsage', () => crypto.subtle.generateKey(
              {name:'ECDH', namedCurve:'P-256'}, false, ['sign']
            ));
          }
        }
        globalThis.__ecdh = out;
      })().catch((error) => {
        globalThis.__ecdhError = `${error.name}:${error.message}`;
      });
    </script>"#;

    let mut page = Page::from_html_with_url(
        html,
        "http://127.0.0.1:9328/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify({value:globalThis.__ecdh,error:globalThis.__ecdhError||null})")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(payload["error"], serde_json::Value::Null, "{raw}");
    let value = &payload["value"];

    for (curve, raw_len, spki_len, pkcs8_len) in [
        ("P-256", 65, 91, 138),
        ("P-384", 97, 120, 185),
        ("P-521", 133, 158, 241),
    ] {
        let v = &value[curve];
        assert_eq!(v["pub"]["tag"], "[object CryptoKey]");
        assert_eq!(v["pub"]["own"], 0);
        assert_eq!(v["pub"]["type"], "public");
        assert_eq!(v["pub"]["extractable"], true);
        assert_eq!(
            v["pub"]["algorithm"],
            serde_json::json!({"name":"ECDH","namedCurve":curve})
        );
        assert_eq!(v["pub"]["usages"], serde_json::json!([]));
        assert_eq!(v["priv"]["tag"], "[object CryptoKey]");
        assert_eq!(v["priv"]["own"], 0);
        assert_eq!(v["priv"]["type"], "private");
        assert_eq!(v["priv"]["extractable"], true);
        assert_eq!(
            v["priv"]["algorithm"],
            serde_json::json!({"name":"ECDH","namedCurve":curve})
        );
        assert_eq!(
            v["priv"]["usages"],
            serde_json::json!(["deriveKey", "deriveBits"])
        );
        assert_eq!(
            v["nonExtractable"],
            serde_json::json!({"pub":true,"priv":false})
        );
        assert_eq!(v["rawLen"], raw_len);
        assert_eq!(v["spkiLen"], spki_len);
        assert_eq!(v["pkcs8Len"], pkcs8_len);
        assert_eq!(
            v["jwkPub"]["keys"],
            serde_json::json!(["crv", "ext", "key_ops", "kty", "x", "y"])
        );
        assert_eq!(v["jwkPub"]["crv"], curve);
        assert_eq!(v["jwkPub"]["ext"], true);
        assert_eq!(v["jwkPub"]["key_ops"], serde_json::json!([]));
        assert_eq!(v["jwkPub"]["kty"], "EC");
        assert_eq!(
            v["jwkPriv"]["keys"],
            serde_json::json!(["crv", "d", "ext", "key_ops", "kty", "x", "y"])
        );
        assert_eq!(
            v["jwkPriv"]["key_ops"],
            serde_json::json!(["deriveKey", "deriveBits"])
        );
        assert_eq!(v["symmetric"], true);
        assert_eq!(v["importedSymmetric"], true);
        assert_eq!(v["importedPublicExtractable"], false);
        for imported in ["raw", "spki", "jwkPub", "jwkPriv"] {
            assert_eq!(
                v["imported"][imported],
                serde_json::json!({"name":"ECDH","namedCurve":curve})
            );
        }
    }

    assert_eq!(value["bits"]["omitted"], value["bits"]["full"]);
    assert_eq!(value["bits"]["nullLength"], value["bits"]["full"]);
    assert_eq!(value["bits"]["zeroLen"], 0);
    assert_eq!(value["bits"]["one"], value["bits"]["oneExpected"]);
    assert_eq!(value["bits"]["nine"], value["bits"]["nineExpected"]);
    assert_eq!(
        value["bits"]["oneTwentySeven"],
        value["bits"]["oneTwentySevenExpected"]
    );
    assert_eq!(
        value["bits"]["derivedAlgorithm"],
        serde_json::json!({"name":"AES-GCM","length":128})
    );
    assert_eq!(
        value["bits"]["derivedUsages"],
        serde_json::json!(["encrypt"])
    );
    assert_eq!(value["bits"]["derivedRawLen"], 16);
    assert!(value["tooLong"]
        .as_str()
        .unwrap()
        .starts_with("OperationError:"));
    assert!(value["badCurve"]
        .as_str()
        .unwrap()
        .starts_with("InvalidAccessError:"));
    assert!(value["publicAsBase"]
        .as_str()
        .unwrap()
        .starts_with("InvalidAccessError:"));
    assert!(value["emptyUsage"]
        .as_str()
        .unwrap()
        .starts_with("SyntaxError:"));
    assert!(value["badUsage"]
        .as_str()
        .unwrap()
        .starts_with("SyntaxError:"));
}

#[test]
fn worker_ecdh_uses_shared_webcrypto_backend() {
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
                      const a = await crypto.subtle.generateKey(
                        {name:'ECDH', namedCurve:'P-256'}, false, ['deriveBits']
                      );
                      const b = await crypto.subtle.generateKey(
                        {name:'ECDH', namedCurve:'P-256'}, false, ['deriveBits']
                      );
                      const ab = new Uint8Array(await crypto.subtle.deriveBits(
                        {name:'ECDH', public:b.publicKey}, a.privateKey, 256
                      ));
                      const ba = new Uint8Array(await crypto.subtle.deriveBits(
                        {name:'ECDH', public:a.publicKey}, b.privateKey, 256
                      ));
                      self.postMessage(JSON.stringify({
                        len:ab.byteLength,
                        symmetric:ab.every((value, index) => value === ba[index]),
                        publicAlgorithm:a.publicKey.algorithm,
                        privateAlgorithm:a.privateKey.algorithm,
                        publicUsages:a.publicKey.usages,
                        privateUsages:a.privateKey.usages,
                        publicExtractable:a.publicKey.extractable,
                        privateExtractable:a.privateKey.extractable
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
    assert!(value.get("error").is_none(), "worker ECDH failed: {value}");
    assert_eq!(value["len"], 32);
    assert_eq!(value["symmetric"], true);
    assert_eq!(
        value["publicAlgorithm"],
        serde_json::json!({"name":"ECDH","namedCurve":"P-256"})
    );
    assert_eq!(
        value["privateAlgorithm"],
        serde_json::json!({"name":"ECDH","namedCurve":"P-256"})
    );
    assert_eq!(value["publicUsages"], serde_json::json!([]));
    assert_eq!(value["privateUsages"], serde_json::json!(["deriveBits"]));
    assert_eq!(value["publicExtractable"], true);
    assert_eq!(value["privateExtractable"], false);
}
