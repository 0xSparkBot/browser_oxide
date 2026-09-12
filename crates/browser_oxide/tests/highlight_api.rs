use browser_oxide::Page;
use serde_json::Value;

async fn probe(js: &str) -> Value {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/highlight",
        Some(profile),
    )
    .await
    .unwrap();
    serde_json::from_str(&page.evaluate(js).unwrap()).unwrap()
}

#[tokio::test]
async fn highlight_and_registry_match_chrome_collection_semantics() {
    let value = probe(r#"
        (() => {
          const text = document.createTextNode('abcdef'); document.body.append(text);
          const r1 = new Range(); r1.setStart(text, 0); r1.setEnd(text, 2);
          const r2 = new Range(); r2.setStart(text, 2); r2.setEnd(text, 4);
          const h = new Highlight(r1);
          const initial = [Object.prototype.toString.call(h), Reflect.ownKeys(h).length,
            h.size, h.priority, h.type, h.has(r1), h.has(r2)];
          const same = h.add(r2) === h;
          const iter = [...h].map(x => x === r1 ? 'r1' : 'r2');
          const entries = [...h.entries()].map(([a,b]) => [a === b, a === r1 ? 'r1' : 'r2']);
          let each = []; h.forEach((v,k,self) => each.push([v === k, self === h, v === r1 ? 'r1' : 'r2']));
          h.priority = 1.5; const p1 = h.priority;
          h.priority = NaN; const p2 = h.priority;
          h.type = 'grammar-error'; h.type = 'invalid-value'; const typeAfterInvalid = h.type;

          const reg = CSS.highlights; const h2 = new Highlight(r2);
          const setSame = reg.set('one', h) === reg; reg.set('two', h2);
          let regEach = []; reg.forEach((v,k,self) => regEach.push([k, v === h ? 'h' : 'h2', self === reg]));
          let illegal, badSet;
          try { new HighlightRegistry(); illegal = 'no-throw'; } catch(e) { illegal = e.name + ':' + e.message; }
          try { reg.set('bad', {}); badSet = 'no-throw'; } catch(e) { badSet = e.name + ':' + e.message; }
          return JSON.stringify({
            initial, same, iter, entries, each, p1, p2, typeAfterInvalid,
            registry: [Object.prototype.toString.call(reg), Reflect.ownKeys(reg).length, reg.size,
              setSame, reg.get('one') === h, reg.has('two'), [...reg.keys()],
              [...reg.values()].map(v => v === h ? 'h' : 'h2'),
              [...reg].map(([k,v]) => [k, v === h ? 'h' : 'h2']), regEach,
              reg.highlightsFromPoint(1, 1).length],
            identities: [Highlight.prototype.keys === Highlight.prototype.values,
              Highlight.prototype[Symbol.iterator] === Highlight.prototype.values,
              HighlightRegistry.prototype[Symbol.iterator] === HighlightRegistry.prototype.entries,
              CSS.highlights === CSS.highlights], illegal, badSet
          });
        })()
    "#).await;

    assert_eq!(
        value["initial"],
        serde_json::json!(["[object Highlight]", 0, 1, 0, "highlight", true, false])
    );
    assert_eq!(value["same"], true);
    assert_eq!(value["iter"], serde_json::json!(["r1", "r2"]));
    assert_eq!(
        value["entries"],
        serde_json::json!([[true, "r1"], [true, "r2"]])
    );
    assert_eq!(
        value["each"],
        serde_json::json!([[true, true, "r1"], [true, true, "r2"]])
    );
    assert_eq!(value["p1"], 1);
    assert_eq!(value["p2"], 0);
    assert_eq!(value["typeAfterInvalid"], "grammar-error");
    assert_eq!(
        value["registry"],
        serde_json::json!([
            "[object HighlightRegistry]",
            0,
            2,
            true,
            true,
            true,
            ["one", "two"],
            ["h", "h2"],
            [["one", "h"], ["two", "h2"]],
            [["one", "h", true], ["two", "h2", true]],
            0
        ])
    );
    assert_eq!(
        value["identities"],
        serde_json::json!([false, true, true, true])
    );
    assert_eq!(
        value["illegal"],
        "TypeError:Failed to construct 'HighlightRegistry': Illegal constructor"
    );
    assert_eq!(value["badSet"], "TypeError:Failed to execute 'set' on 'HighlightRegistry': parameter 2 is not of type 'Highlight'.");
}

#[tokio::test]
async fn highlight_webidl_surface_matches_chrome_148() {
    let value = probe(r#"
        (() => {
          const shape = proto => Reflect.ownKeys(proto).map(k => {
            const d = Object.getOwnPropertyDescriptor(proto, k);
            return [typeof k === 'symbol' ? String(k) : k, d.enumerable, d.configurable,
              'writable' in d ? d.writable : null, typeof d.get, typeof d.set,
              typeof d.value, typeof d.value === 'function' ? d.value.length : null,
              typeof d.value === 'function' ? d.value.name : null];
          });
          const css = Object.getOwnPropertyDescriptor(CSS, 'highlights');
          return JSON.stringify({ h: shape(Highlight.prototype), r: shape(HighlightRegistry.prototype),
            lengths: [Highlight.length, HighlightRegistry.length],
            css: [typeof css.get, typeof css.set, css.enumerable, css.configurable] });
        })()
    "#).await;

    let h_names: Vec<&str> = value["h"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v[0].as_str().unwrap())
        .collect();
    let r_names: Vec<&str> = value["r"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v[0].as_str().unwrap())
        .collect();
    assert_eq!(
        h_names,
        vec![
            "priority",
            "type",
            "size",
            "add",
            "clear",
            "delete",
            "entries",
            "forEach",
            "has",
            "keys",
            "values",
            "constructor",
            "Symbol(Symbol.toStringTag)",
            "Symbol(Symbol.iterator)"
        ]
    );
    assert_eq!(
        r_names,
        vec![
            "size",
            "clear",
            "delete",
            "entries",
            "forEach",
            "get",
            "has",
            "keys",
            "set",
            "values",
            "highlightsFromPoint",
            "constructor",
            "Symbol(Symbol.toStringTag)",
            "Symbol(Symbol.iterator)"
        ]
    );
    assert_eq!(value["lengths"], serde_json::json!([0, 0]));
    assert_eq!(
        value["css"],
        serde_json::json!(["function", "undefined", true, true])
    );
}
