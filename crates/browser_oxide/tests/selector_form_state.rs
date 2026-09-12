use browser_oxide::stealth::presets::chrome_148_macos;
use browser_oxide::Page;

const FORM_HTML: &str = r#"<!doctype html><html><body>
<input id="text" placeholder="x">
<input id="hidden" type="hidden" required>
<input id="check" type="checkbox" required>
<input id="ro" readonly>
<textarea id="ta" placeholder="x"></textarea>
<textarea id="tar" readonly></textarea>
<button id="btn"></button>
<select id="sel" required><option id="opt">one</option></select>
<select id="seld" disabled><option id="optd">one</option></select>
<select><optgroup id="og" disabled><option id="ogopt">one</option></optgroup></select>
<fieldset id="fs" disabled>
  <legend id="leg">Legend <input id="legin"></legend>
  <input id="fsin">
</fieldset>
<div id="ce" contenteditable>hi</div>
<div id="cef" contenteditable="false">hi</div>
<div id="plain">hi</div>
<output id="outel"></output>
<form id="form"></form>
</body></html>"#;

#[tokio::test(flavor = "current_thread")]
async fn form_state_pseudo_classes_match_chrome148() {
    let mut page = Page::from_html(FORM_HTML, Some(chrome_148_macos()))
        .await
        .expect("page");

    let actual = page
        .evaluate(
            r#"(() => {
                const ids = ['text','hidden','check','ro','ta','tar','btn','sel','opt','seld','optd','og','ogopt','fs','leg','legin','fsin','ce','cef','plain','outel','form'];
                const pseudos = ['enabled','disabled','required','optional','read-write','read-only','placeholder-shown'];
                return JSON.stringify(ids.map(id => {
                    const el = document.getElementById(id);
                    return [id, pseudos.filter(p => el.matches(':' + p))];
                }));
            })()"#,
        )
        .expect("selector results");

    assert_eq!(
        actual,
        r#"[["text",["enabled","optional","read-write","placeholder-shown"]],["hidden",["enabled","optional","read-only"]],["check",["enabled","required","read-only"]],["ro",["enabled","optional","read-only"]],["ta",["enabled","optional","read-write","placeholder-shown"]],["tar",["enabled","optional","read-only"]],["btn",["enabled","optional","read-only"]],["sel",["enabled","required","read-only"]],["opt",["enabled","read-only"]],["seld",["disabled","optional","read-only"]],["optd",["enabled","read-only"]],["og",["disabled","read-only"]],["ogopt",["disabled","read-only"]],["fs",["disabled","read-only"]],["leg",["read-only"]],["legin",["enabled","optional","read-write"]],["fsin",["disabled","optional","read-only"]],["ce",["read-write"]],["cef",["read-only"]],["plain",["read-only"]],["outel",["read-only"]],["form",["read-only"]]]"#
    );

    let query_sets = page
        .evaluate(
            r#"JSON.stringify({
                disabled: Array.from(document.querySelectorAll(':disabled'), e => e.id),
                required: Array.from(document.querySelectorAll(':required'), e => e.id),
                placeholder: Array.from(document.querySelectorAll(':placeholder-shown'), e => e.id),
            })"#,
        )
        .expect("querySelectorAll state");
    assert_eq!(
        query_sets,
        r#"{"disabled":["seld","og","ogopt","fs","fsin"],"required":["check","sel"],"placeholder":["text","ta"]}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn placeholder_shown_tracks_live_form_value_without_reflecting_attribute() {
    let mut page = Page::from_html(
        r#"<!doctype html><html><body>
            <input id="i" placeholder="x">
            <textarea id="t" placeholder="x"></textarea>
        </body></html>"#,
        Some(chrome_148_macos()),
    )
    .await
    .expect("page");

    let actual = page
        .evaluate(
            r#"(() => {
                const i = document.getElementById('i');
                const t = document.getElementById('t');
                const snap = el => ({
                    ph: el.matches(':placeholder-shown'),
                    rw: el.matches(':read-write'),
                    dis: el.matches(':disabled'),
                    req: el.matches(':required'),
                });
                const r = { initial: snap(i) };
                i.disabled = true; r.disabled = snap(i);
                i.disabled = false; i.readOnly = true; r.readonly = snap(i);
                i.readOnly = false; i.required = true; r.required = snap(i);
                i.value = 'x'; r.value = snap(i);
                r.attributeAfterValue = i.getAttribute('value');
                r.defaultAfterValue = i.defaultValue;
                r.ta0 = snap(t);
                t.value = 'x'; r.taValue = snap(t);
                r.taDefaultAfterValue = t.defaultValue;
                return JSON.stringify(r);
            })()"#,
        )
        .expect("live selector state");

    assert_eq!(
        actual,
        r#"{"initial":{"ph":true,"rw":true,"dis":false,"req":false},"disabled":{"ph":true,"rw":false,"dis":true,"req":false},"readonly":{"ph":true,"rw":false,"dis":false,"req":false},"required":{"ph":true,"rw":true,"dis":false,"req":true},"value":{"ph":false,"rw":true,"dis":false,"req":true},"attributeAfterValue":null,"defaultAfterValue":"","ta0":{"ph":true,"rw":true,"dis":false,"req":false},"taValue":{"ph":false,"rw":true,"dis":false,"req":false},"taDefaultAfterValue":""}"#
    );
}
