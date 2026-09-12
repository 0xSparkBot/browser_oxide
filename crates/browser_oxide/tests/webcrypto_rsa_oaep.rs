use browser_oxide::Page;

#[tokio::test]
async fn window_rsa_oaep_matches_chrome_round_trip_and_key_shape() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const enc = new TextEncoder();
        const dec = new TextDecoder();
        const algorithm = {
          name: 'RSA-OAEP',
          modulusLength: 1024,
          publicExponent: new Uint8Array([1, 0, 1]),
          hash: 'SHA-256',
        };
        const pair = await crypto.subtle.generateKey(
          algorithm, true, ['encrypt', 'decrypt']
        );
        const message = enc.encode('browser oxide rsa oaep');
        const label = enc.encode('label');
        const cipher = await crypto.subtle.encrypt(
          { name: 'RSA-OAEP', label }, pair.publicKey, message
        );
        const plain = await crypto.subtle.decrypt(
          { name: 'RSA-OAEP', label }, pair.privateKey, cipher
        );
        const spki = await crypto.subtle.exportKey('spki', pair.publicKey);
        const pkcs8 = await crypto.subtle.exportKey('pkcs8', pair.privateKey);
        const importedPublic = await crypto.subtle.importKey(
          'spki', spki, { name: 'RSA-OAEP', hash: 'SHA-256' }, true, ['encrypt']
        );
        const importedPrivate = await crypto.subtle.importKey(
          'pkcs8', pkcs8, { name: 'RSA-OAEP', hash: 'SHA-256' }, true, ['decrypt']
        );
        const cipher2 = await crypto.subtle.encrypt('RSA-OAEP', importedPublic, message);
        const plain2 = await crypto.subtle.decrypt('RSA-OAEP', importedPrivate, cipher2);
        out.text = dec.decode(plain);
        out.text2 = dec.decode(plain2);
        out.cipherLength = cipher.byteLength;
        out.spkiLength = spki.byteLength;
        out.pkcs8Length = pkcs8.byteLength;
        out.publicKey = {
          tag: Object.prototype.toString.call(pair.publicKey),
          own: Reflect.ownKeys(pair.publicKey).length,
          type: pair.publicKey.type,
          extractable: pair.publicKey.extractable,
          algorithm: pair.publicKey.algorithm,
          usages: pair.publicKey.usages,
        };
        out.privateKey = {
          tag: Object.prototype.toString.call(pair.privateKey),
          own: Reflect.ownKeys(pair.privateKey).length,
          type: pair.privateKey.type,
          extractable: pair.privateKey.extractable,
          algorithm: pair.privateKey.algorithm,
          usages: pair.privateKey.usages,
        };
        globalThis.__rsaOaep = out;
      })().catch((error) => {
        globalThis.__rsaOaep = { error: `${error.name}:${error.message}` };
      });
    </script>"#;

    let mut page = Page::from_html_with_url(
        html,
        "http://127.0.0.1:9341/",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();
    let raw = page
        .evaluate("JSON.stringify(globalThis.__rsaOaep)")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(value.get("error").is_none(), "RSA-OAEP failed: {value}");
    assert_eq!(value["text"], "browser oxide rsa oaep");
    assert_eq!(value["text2"], "browser oxide rsa oaep");
    assert_eq!(value["cipherLength"], 128);
    assert_eq!(value["spkiLength"], 162);
    assert!(value["pkcs8Length"].as_u64().unwrap() >= 630);

    for (name, key_type, usages) in [
        ("publicKey", "public", serde_json::json!(["encrypt"])),
        ("privateKey", "private", serde_json::json!(["decrypt"])),
    ] {
        let key = &value[name];
        assert_eq!(key["tag"], "[object CryptoKey]");
        assert_eq!(key["own"], 0);
        assert_eq!(key["type"], key_type);
        assert_eq!(key["extractable"], true);
        assert_eq!(key["usages"], usages);
        assert_eq!(key["algorithm"]["name"], "RSA-OAEP");
        assert_eq!(key["algorithm"]["modulusLength"], 1024);
        assert_eq!(key["algorithm"]["hash"]["name"], "SHA-256");
        assert_eq!(
            key["algorithm"]["publicExponent"],
            serde_json::json!({"0":1,"1":0,"2":1})
        );
    }
}
