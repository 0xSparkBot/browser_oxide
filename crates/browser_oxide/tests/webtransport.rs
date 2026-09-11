use browser_oxide::Page;

fn profile() -> browser_oxide::stealth::StealthProfile {
    browser_oxide::stealth::presets::chrome_148_macos()
}

#[tokio::test]
async fn webtransport_matches_chrome_fail_closed_semantics() {
    let html = r#"<!doctype html><script>
      (async () => {
        const out = {};
        const capture = async (promise) => {
          try { return { ok: true, value: await promise }; }
          catch (error) {
            return {
              ok: false,
              error: `${error.name}:${error.message}`,
              tag: Object.prototype.toString.call(error),
              source: error.source,
              code: error.streamErrorCode,
            };
          }
        };
        const sync = (run) => {
          try { return { ok: true, value: run() }; }
          catch (error) { return { ok: false, error: `${error.name}:${error.message}` }; }
        };

        out.proto = {
          wt: Reflect.ownKeys(WebTransport.prototype).map(String),
          bidi: Reflect.ownKeys(WebTransportBidirectionalStream.prototype).map(String),
          datagram: Reflect.ownKeys(WebTransportDatagramDuplexStream.prototype).map(String),
          error: Reflect.ownKeys(WebTransportError.prototype).map(String),
          wtLength: WebTransport.length,
          errorParent: Object.getPrototypeOf(WebTransportError) === DOMException,
        };
        out.invalid = {
          missing: sync(() => new WebTransport()),
          empty: sync(() => new WebTransport('')),
          relative: sync(() => new WebTransport('/x')),
          http: sync(() => new WebTransport('http://example.test/')),
          hash: sync(() => new WebTransport('https://example.test/x#y')),
          bidi: sync(() => new WebTransportBidirectionalStream()),
          datagram: sync(() => new WebTransportDatagramDuplexStream()),
        };

        const publicError = new WebTransportError({ message: 'hello', streamErrorCode: 7 });
        out.publicError = {
          tag: Object.prototype.toString.call(publicError),
          own: Reflect.ownKeys(publicError).map(String),
          name: publicError.name,
          message: publicError.message,
          source: publicError.source,
          code: publicError.streamErrorCode,
          domException: publicError instanceof DOMException,
        };
        out.badErrorInit = sync(() => new WebTransportError('hello'));

        const transport = new WebTransport('https://127.0.0.1:1/');
        transport.ready.catch(() => {});
        transport.closed.catch(() => {});
        const datagrams = transport.datagrams;
        out.initial = {
          tag: Object.prototype.toString.call(transport),
          own: Reflect.ownKeys(transport).map(String),
          protocol: transport.protocol,
          readyTag: Object.prototype.toString.call(transport.ready),
          closedTag: Object.prototype.toString.call(transport.closed),
          datagramTag: Object.prototype.toString.call(datagrams),
          incomingBi: Object.prototype.toString.call(transport.incomingBidirectionalStreams),
          incomingUni: Object.prototype.toString.call(transport.incomingUnidirectionalStreams),
          dgReadable: Object.prototype.toString.call(datagrams.readable),
          dgWritable: Object.prototype.toString.call(datagrams.writable),
          maxDatagramSize: datagrams.maxDatagramSize,
          incomingMaxAge: datagrams.incomingMaxAge,
          outgoingMaxAge: datagrams.outgoingMaxAge,
          incomingHighWaterMark: datagrams.incomingHighWaterMark,
          outgoingHighWaterMark: datagrams.outgoingHighWaterMark,
        };

        out.createBi = await capture(transport.createBidirectionalStream());
        out.createUni = await capture(transport.createUnidirectionalStream());
        out.ready = await capture(transport.ready);
        out.closed = await capture(transport.closed);
        out.close = sync(() => transport.close({ closeCode: 42, reason: 'bye' }));

        datagrams.incomingHighWaterMark = -1;
        out.hwmNegative = datagrams.incomingHighWaterMark;
        datagrams.incomingHighWaterMark = 2.5;
        out.hwmFraction = datagrams.incomingHighWaterMark;
        datagrams.incomingMaxAge = -1;
        out.ageNegative = datagrams.incomingMaxAge;
        datagrams.incomingMaxAge = 2.5;
        out.ageFraction = datagrams.incomingMaxAge;
        out.ageInfinite = sync(() => { datagrams.outgoingMaxAge = Infinity; });

        globalThis.__webTransportResult = out;
      })();
    </script>"#;

    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile()))
        .await
        .expect("page");
    let raw = page
        .evaluate("JSON.stringify(globalThis.__webTransportResult)")
        .expect("result");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("json result");

    assert_eq!(value["proto"]["wtLength"], 1);
    assert_eq!(value["proto"]["errorParent"], true);
    assert_eq!(
        value["proto"]["wt"],
        serde_json::json!([
            "incomingUnidirectionalStreams",
            "incomingBidirectionalStreams",
            "datagrams",
            "ready",
            "closed",
            "close",
            "createBidirectionalStream",
            "createUnidirectionalStream",
            "protocol",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["proto"]["bidi"],
        serde_json::json!([
            "readable",
            "writable",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["proto"]["datagram"],
        serde_json::json!([
            "readable",
            "writable",
            "maxDatagramSize",
            "incomingMaxAge",
            "outgoingMaxAge",
            "incomingHighWaterMark",
            "outgoingHighWaterMark",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(
        value["proto"]["error"],
        serde_json::json!([
            "streamErrorCode",
            "source",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );

    assert_eq!(
        value["invalid"]["missing"]["error"],
        "TypeError:Failed to construct 'WebTransport': 1 argument required, but only 0 present."
    );
    assert_eq!(
        value["invalid"]["empty"]["error"],
        "SyntaxError:Failed to construct 'WebTransport': The URL '' is invalid."
    );
    assert_eq!(
        value["invalid"]["relative"]["error"],
        "SyntaxError:Failed to construct 'WebTransport': The URL '/x' is invalid."
    );
    assert_eq!(value["invalid"]["http"]["error"], "SyntaxError:Failed to construct 'WebTransport': The URL's scheme must be 'https'. 'http' is not allowed.");
    assert_eq!(value["invalid"]["hash"]["error"], "SyntaxError:Failed to construct 'WebTransport': The URL contains a fragment identifier ('#y'). Fragment identifiers are not allowed in WebTransport URLs.");
    assert_eq!(
        value["invalid"]["bidi"]["error"],
        "TypeError:Failed to construct 'WebTransportBidirectionalStream': Illegal constructor"
    );
    assert_eq!(
        value["invalid"]["datagram"]["error"],
        "TypeError:Failed to construct 'WebTransportDatagramDuplexStream': Illegal constructor"
    );

    assert_eq!(value["publicError"]["tag"], "[object WebTransportError]");
    assert_eq!(value["publicError"]["own"], serde_json::json!([]));
    assert_eq!(value["publicError"]["name"], "WebTransportError");
    assert_eq!(value["publicError"]["message"], "hello");
    assert_eq!(value["publicError"]["source"], "stream");
    assert_eq!(value["publicError"]["code"], 7);
    assert_eq!(value["publicError"]["domException"], true);
    assert_eq!(value["badErrorInit"]["error"], "TypeError:Failed to construct 'WebTransportError': The provided value is not of type 'WebTransportErrorInit'.");

    assert_eq!(value["initial"]["tag"], "[object WebTransport]");
    assert_eq!(value["initial"]["own"], serde_json::json!([]));
    assert_eq!(value["initial"]["protocol"], "");
    assert_eq!(value["initial"]["readyTag"], "[object Promise]");
    assert_eq!(value["initial"]["closedTag"], "[object Promise]");
    assert_eq!(
        value["initial"]["datagramTag"],
        "[object WebTransportDatagramDuplexStream]"
    );
    assert_eq!(value["initial"]["incomingBi"], "[object ReadableStream]");
    assert_eq!(value["initial"]["incomingUni"], "[object ReadableStream]");
    assert_eq!(value["initial"]["dgReadable"], "[object ReadableStream]");
    assert_eq!(value["initial"]["dgWritable"], "[object WritableStream]");
    assert_eq!(value["initial"]["maxDatagramSize"], 1024);
    assert_eq!(value["initial"]["incomingMaxAge"], serde_json::Value::Null);
    assert_eq!(value["initial"]["outgoingMaxAge"], serde_json::Value::Null);
    assert_eq!(value["initial"]["incomingHighWaterMark"], 1);
    assert_eq!(value["initial"]["outgoingHighWaterMark"], 1);

    assert_eq!(value["createBi"]["error"], "NetworkError:Failed to execute 'createBidirectionalStream' on 'WebTransport': No connection.");
    assert_eq!(value["createUni"]["error"], "NetworkError:Failed to execute 'createUnidirectionalStream' on 'WebTransport': No connection.");
    assert_eq!(
        value["ready"]["error"],
        "WebTransportError:Opening handshake failed."
    );
    assert_eq!(value["ready"]["tag"], "[object WebTransportError]");
    assert_eq!(value["ready"]["source"], "session");
    assert_eq!(value["ready"]["code"], serde_json::Value::Null);
    assert_eq!(value["closed"], value["ready"]);
    assert_eq!(value["close"]["ok"], true);

    assert_eq!(value["hwmNegative"], 0);
    assert_eq!(value["hwmFraction"], 2);
    assert_eq!(value["ageNegative"], serde_json::Value::Null);
    assert_eq!(value["ageFraction"], 2.5);
    assert_eq!(value["ageInfinite"]["error"], "TypeError:Failed to set the 'outgoingMaxAge' property on 'WebTransportDatagramDuplexStream': The provided double value is non-finite.");
}

#[tokio::test]
async fn webtransport_interfaces_are_hidden_in_insecure_contexts() {
    let mut page =
        Page::from_html_with_url("<!doctype html>", "http://example.test/", Some(profile()))
            .await
            .expect("page");
    assert_eq!(
        page.evaluate(
            "JSON.stringify([typeof WebTransport, typeof WebTransportBidirectionalStream, typeof WebTransportDatagramDuplexStream, typeof WebTransportError])"
        )
        .unwrap(),
        r#"["undefined","undefined","undefined","undefined"]"#
    );
}

#[tokio::test]
async fn webtransport_is_exposed_in_secure_dedicated_workers() {
    let html = r#"<!doctype html><script>
      globalThis.__wtWorker = null;
      const source = `postMessage({
        wt: typeof WebTransport,
        bidi: typeof WebTransportBidirectionalStream,
        datagram: typeof WebTransportDatagramDuplexStream,
        error: typeof WebTransportError,
        length: typeof WebTransport === 'function' ? WebTransport.length : null,
        secure: isSecureContext,
      });`;
      const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
      const worker = new Worker(url);
      worker.onmessage = (event) => {
        globalThis.__wtWorker = event.data;
        worker.terminate();
        URL.revokeObjectURL(url);
      };
    </script>"#;
    let mut page = Page::from_html_with_url(html, "https://example.test/", Some(profile()))
        .await
        .expect("page");
    for _ in 0..20 {
        if page.evaluate("globalThis.__wtWorker !== null").unwrap() == "true" {
            break;
        }
        let _ = page
            .event_loop()
            .run_until_settled(std::time::Duration::from_millis(100))
            .await;
    }
    let value = page
        .evaluate("JSON.stringify(globalThis.__wtWorker)")
        .unwrap();
    assert_eq!(
        value,
        r#"{"wt":"function","bidi":"function","datagram":"function","error":"function","length":1,"secure":true}"#
    );
}
