use browser_oxide::js_runtime::{runtime::BrowserRuntimeOptions, BrowserJsRuntime};
use browser_oxide::Page;
use std::time::Duration;

#[tokio::test]
async fn window_ecdsa_matches_chrome_semantics() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const msg = new TextEncoder().encode('browser-oxide-ecdsa');
        const hashFor = { 'P-256': 'SHA-256', 'P-384': 'SHA-384', 'P-521': 'SHA-512' };
        for (const curve of ['P-256', 'P-384', 'P-521']) {
          const kp = await crypto.subtle.generateKey(
            { name: 'ECDSA', namedCurve: curve }, true, ['sign', 'verify']
          );
          const algorithm = { name: 'ECDSA', hash: hashFor[curve] };
          const sig = await crypto.subtle.sign(algorithm, kp.privateKey, msg);
          const raw = await crypto.subtle.exportKey('raw', kp.publicKey);
          const spki = await crypto.subtle.exportKey('spki', kp.publicKey);
          const pkcs8 = await crypto.subtle.exportKey('pkcs8', kp.privateKey);
          const jwkPub = await crypto.subtle.exportKey('jwk', kp.publicKey);
          const jwkPriv = await crypto.subtle.exportKey('jwk', kp.privateKey);
          const imported = {
            raw: await crypto.subtle.importKey('raw', raw, {name:'ECDSA', namedCurve:curve}, true, ['verify']),
            spki: await crypto.subtle.importKey('spki', spki, {name:'ECDSA', namedCurve:curve}, true, ['verify']),
            pkcs8: await crypto.subtle.importKey('pkcs8', pkcs8, {name:'ECDSA', namedCurve:curve}, true, ['sign']),
            jwkPub: await crypto.subtle.importKey('jwk', jwkPub, {name:'ECDSA', namedCurve:curve}, true, ['verify']),
            jwkPriv: await crypto.subtle.importKey('jwk', jwkPriv, {name:'ECDSA', namedCurve:curve}, true, ['sign']),
          };
          const sig2 = await crypto.subtle.sign(algorithm, imported.pkcs8, msg);
          const sig3 = await crypto.subtle.sign(algorithm, imported.jwkPriv, msg);
          out[curve] = {
            pub: {
              tag: Object.prototype.toString.call(kp.publicKey), own: Reflect.ownKeys(kp.publicKey).length,
              type: kp.publicKey.type, extractable: kp.publicKey.extractable,
              algorithm: kp.publicKey.algorithm, usages: kp.publicKey.usages,
            },
            priv: {
              tag: Object.prototype.toString.call(kp.privateKey), own: Reflect.ownKeys(kp.privateKey).length,
              type: kp.privateKey.type, extractable: kp.privateKey.extractable,
              algorithm: kp.privateKey.algorithm, usages: kp.privateKey.usages,
            },
            sigLen: sig.byteLength,
            verify: await crypto.subtle.verify(algorithm, kp.publicKey, sig, msg),
            rawLen: raw.byteLength,
            rawFirst: new Uint8Array(raw)[0],
            spkiLen: spki.byteLength,
            pkcs8Len: pkcs8.byteLength,
            jwkPub: { keys:Object.keys(jwkPub), kty:jwkPub.kty, crv:jwkPub.crv, ext:jwkPub.ext, key_ops:jwkPub.key_ops, xLen:jwkPub.x.length, yLen:jwkPub.y.length },
            jwkPriv: { keys:Object.keys(jwkPriv), kty:jwkPriv.kty, crv:jwkPriv.crv, ext:jwkPriv.ext, key_ops:jwkPriv.key_ops, xLen:jwkPriv.x.length, yLen:jwkPriv.y.length, dLen:jwkPriv.d.length },
            imported: {
              raw: await crypto.subtle.verify(algorithm, imported.raw, sig, msg),
              spki: await crypto.subtle.verify(algorithm, imported.spki, sig, msg),
              pkcs8: await crypto.subtle.verify(algorithm, kp.publicKey, sig2, msg),
              jwkPub: await crypto.subtle.verify(algorithm, imported.jwkPub, sig3, msg),
            },
          };
        }
        const capture = async (name, f) => {
          try { await f(); out[name] = 'NO_THROW'; }
          catch (e) { out[name] = `${e.name}:${e.message}`; }
        };
        await capture('badCurve', () => crypto.subtle.generateKey(
          {name:'ECDSA',namedCurve:'P-192'}, true, ['sign','verify']
        ));
        await capture('badUsage', () => crypto.subtle.generateKey(
          {name:'ECDSA',namedCurve:'P-256'}, true, ['encrypt']
        ));
        await capture('verifyOnly', () => crypto.subtle.generateKey(
          {name:'ECDSA',namedCurve:'P-256'}, true, ['verify']
        ));
        const signOnly = await crypto.subtle.generateKey(
          {name:'ECDSA',namedCurve:'P-256'}, false, ['sign']
        );
        out.signOnly = {
          pubExtractable: signOnly.publicKey.extractable,
          pubUsages: signOnly.publicKey.usages,
          privExtractable: signOnly.privateKey.extractable,
          privUsages: signOnly.privateKey.usages,
        };
        await capture('wrongHash', () => crypto.subtle.sign(
          {name:'ECDSA',hash:'SHA-999'}, signOnly.privateKey, msg
        ));
        await capture('wrongKeyUsage', () => crypto.subtle.sign(
          {name:'ECDSA',hash:'SHA-256'}, signOnly.publicKey, msg
        ));
        await capture('badRaw', () => crypto.subtle.importKey(
          'raw', new Uint8Array(10), {name:'ECDSA',namedCurve:'P-256'}, true, ['verify']
        ));
        globalThis.__ecdsa = out;
      })();
    </script>"#;

    let mut page = Page::from_html_with_url(
        html,
        "http://127.0.0.1:9326/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page.evaluate("JSON.stringify(globalThis.__ecdsa)").unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    for (curve, sig_len, raw_len, spki_len, pkcs8_len, coord_b64_len) in [
        ("P-256", 64, 65, 91, 138, 43),
        ("P-384", 96, 97, 120, 185, 64),
        ("P-521", 132, 133, 158, 241, 88),
    ] {
        let v = &value[curve];
        assert_eq!(v["pub"]["tag"], "[object CryptoKey]");
        assert_eq!(v["pub"]["own"], 0);
        assert_eq!(v["pub"]["type"], "public");
        assert_eq!(v["pub"]["extractable"], true);
        assert_eq!(
            v["pub"]["algorithm"],
            serde_json::json!({"name":"ECDSA","namedCurve":curve})
        );
        assert_eq!(v["pub"]["usages"], serde_json::json!(["verify"]));
        assert_eq!(v["priv"]["tag"], "[object CryptoKey]");
        assert_eq!(v["priv"]["own"], 0);
        assert_eq!(v["priv"]["type"], "private");
        assert_eq!(v["priv"]["extractable"], true);
        assert_eq!(
            v["priv"]["algorithm"],
            serde_json::json!({"name":"ECDSA","namedCurve":curve})
        );
        assert_eq!(v["priv"]["usages"], serde_json::json!(["sign"]));
        assert_eq!(v["sigLen"], sig_len);
        assert_eq!(v["verify"], true);
        assert_eq!(v["rawLen"], raw_len);
        assert_eq!(v["rawFirst"], 4);
        assert_eq!(v["spkiLen"], spki_len);
        assert_eq!(v["pkcs8Len"], pkcs8_len);
        assert_eq!(
            v["jwkPub"]["keys"],
            serde_json::json!(["crv", "ext", "key_ops", "kty", "x", "y"])
        );
        assert_eq!(
            v["jwkPriv"]["keys"],
            serde_json::json!(["crv", "d", "ext", "key_ops", "kty", "x", "y"])
        );
        assert_eq!(v["jwkPub"]["kty"], "EC");
        assert_eq!(v["jwkPub"]["crv"], curve);
        assert_eq!(v["jwkPub"]["xLen"], coord_b64_len);
        assert_eq!(v["jwkPub"]["yLen"], coord_b64_len);
        assert_eq!(v["jwkPriv"]["dLen"], coord_b64_len);
        assert_eq!(
            v["imported"],
            serde_json::json!({"raw":true,"spki":true,"pkcs8":true,"jwkPub":true})
        );
    }
    assert_eq!(value["badCurve"], "NotSupportedError:Failed to execute 'generateKey' on 'SubtleCrypto': EcKeyGenParams: Unrecognized namedCurve");
    assert_eq!(
        value["badUsage"],
        "SyntaxError:Cannot create a key using the specified key usages."
    );
    assert_eq!(
        value["verifyOnly"],
        "SyntaxError:Usages cannot be empty when creating a key."
    );
    assert_eq!(
        value["signOnly"],
        serde_json::json!({"pubExtractable":true,"pubUsages":[],"privExtractable":false,"privUsages":["sign"]})
    );
    assert_eq!(value["wrongHash"], "NotSupportedError:Failed to execute 'sign' on 'SubtleCrypto': EcdsaParams: hash: Algorithm: Unrecognized name");
    assert_eq!(
        value["wrongKeyUsage"],
        "InvalidAccessError:key.usages does not permit this operation"
    );
    assert_eq!(value["badRaw"], "DataError:");
}

#[test]
fn worker_ecdsa_uses_shared_webcrypto_backend() {
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
                const msg = new TextEncoder().encode('worker-ecdsa');
                const pair = await crypto.subtle.generateKey(
                  {name:'ECDSA', namedCurve:'P-256'}, true, ['sign','verify']
                );
                const alg = {name:'ECDSA', hash:'SHA-256'};
                const sig = await crypto.subtle.sign(alg, pair.privateKey, msg);
                const raw = await crypto.subtle.exportKey('raw', pair.publicKey);
                const imported = await crypto.subtle.importKey(
                  'raw', raw, {name:'ECDSA',namedCurve:'P-256'}, true, ['verify']
                );
                self.postMessage(JSON.stringify({
                  sigLen:sig.byteLength,
                  verify:await crypto.subtle.verify(alg, imported, sig, msg),
                  publicAlgorithm:pair.publicKey.algorithm,
                  privateAlgorithm:pair.privateKey.algorithm,
                  publicUsages:pair.publicKey.usages,
                  privateUsages:pair.privateKey.usages,
                  publicOwn:Reflect.ownKeys(pair.publicKey).length,
                  privateOwn:Reflect.ownKeys(pair.privateKey).length
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
    assert!(value.get("error").is_none(), "worker ECDSA failed: {value}");
    assert_eq!(value["sigLen"], 64);
    assert_eq!(value["verify"], true);
    assert_eq!(
        value["publicAlgorithm"],
        serde_json::json!({"name":"ECDSA","namedCurve":"P-256"})
    );
    assert_eq!(
        value["privateAlgorithm"],
        serde_json::json!({"name":"ECDSA","namedCurve":"P-256"})
    );
    assert_eq!(value["publicUsages"], serde_json::json!(["verify"]));
    assert_eq!(value["privateUsages"], serde_json::json!(["sign"]));
    assert_eq!(value["publicOwn"], 0);
    assert_eq!(value["privateOwn"], 0);
}
