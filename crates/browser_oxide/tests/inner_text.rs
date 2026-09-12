use browser_oxide::stealth::presets::chrome_148_macos;
use browser_oxide::Page;

#[tokio::test]
async fn inner_text_matches_rendered_text_semantics() {
    let html = r#"<!doctype html><html><body>
      <style>.css-hidden { display:none } .css-invisible { visibility:hidden }</style>
      <div id="host">A<span style="display:none">HIDDEN_DISPLAY</span><span style="visibility:hidden">HIDDEN_VIS</span><span class="css-hidden">CSS_HIDDEN</span><span class="css-invisible">CSS_INVISIBLE</span><span> B </span><br>C<div>D<span>E</span></div><script>var x='SCRIPT_TEXT'</script><style>.x{color:red} STYLE_TEXT</style><p> P1 </p><p>P2</p></div>
      <pre id="pre">  AA
 BB  </pre>
      <div id="hidden" style="display:none">Q<b>R</b></div>
      <div id="vis-hidden" style="visibility:hidden">VIS</div>
    </body></html>"#;
    let mut page = Page::from_html(html, Some(chrome_148_macos()))
        .await
        .unwrap();

    assert_eq!(
        page.evaluate("document.getElementById('host').innerText")
            .unwrap(),
        "A B\nC\nDE\n\nP1\n\nP2"
    );
    assert_eq!(
        page.evaluate("document.getElementById('hidden').innerText")
            .unwrap(),
        "QR"
    );
    assert_eq!(
        page.evaluate("document.getElementById('vis-hidden').innerText")
            .unwrap(),
        ""
    );
    assert_eq!(
        page.evaluate("document.getElementById('pre').innerText")
            .unwrap(),
        "  AA\n BB  "
    );
    assert_eq!(
        page.evaluate("(() => { const e=document.createElement('div'); e.innerHTML='X<span style=\"display:none\">Y</span>Z'; return e.innerText; })()").unwrap(),
        "XYZ"
    );
}

#[tokio::test]
async fn inner_and_outer_text_setters_create_line_break_nodes() {
    let mut page = Page::from_html(
        r#"<!doctype html><html><body><div id="a"></div><div id="wrap"><span id="x">old</span><b>tail</b></div></body></html>"#,
        Some(chrome_148_macos()),
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate("(() => { const a=document.getElementById('a'); a.innerText='A\\nB\\r\\nC'; return JSON.stringify([a.innerHTML,a.textContent,a.innerText]); })()").unwrap(),
        r#"["A<br>B<br>C","ABC","A\nB\nC"]"#
    );
    assert_eq!(
        page.evaluate("(() => { const x=document.getElementById('x'); x.outerText='P\\nQ'; const w=document.getElementById('wrap'); return JSON.stringify([w.innerHTML,w.textContent]); })()").unwrap(),
        r#"["P<br>Q<b>tail</b>","PQtail"]"#
    );
}
