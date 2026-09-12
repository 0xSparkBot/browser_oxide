use browser_oxide::js_runtime::BrowserJsRuntime;

fn runtime(html: &str) -> BrowserJsRuntime {
    let dom = browser_oxide::html_parser::parse_html(html);
    BrowserJsRuntime::new(dom)
}

#[tokio::test]
async fn dialog_webidl_surface_and_modal_lifecycle_match_chromium() {
    let mut rt = runtime(
        r#"<!doctype html><html><body><div id="a"></div><div id="b"></div><dialog id="d"></dialog></body></html>"#,
    );
    let result = rt
        .execute_script(
            r#"
            (() => {
                const d = document.getElementById('d');
                const proto = HTMLDialogElement.prototype;
                const desc = name => {
                    const x = Object.getOwnPropertyDescriptor(proto, name);
                    return {
                        e: x.enumerable,
                        c: x.configurable,
                        w: 'writable' in x ? x.writable : null,
                        g: !!x.get,
                        s: !!x.set,
                        len: typeof x.value === 'function' ? x.value.length : null,
                    };
                };
                const call = fn => {
                    try { fn(); return 'ok'; }
                    catch (e) { return e.name + ': ' + e.message; }
                };
                const result = {
                    protoKeys: Reflect.ownKeys(proto).map(k => typeof k === 'symbol' ? String(k) : k),
                    desc: {
                        open: desc('open'), returnValue: desc('returnValue'), closedBy: desc('closedBy'),
                        close: desc('close'), requestClose: desc('requestClose'),
                        show: desc('show'), showModal: desc('showModal'),
                    },
                    initial: {
                        tag: Object.prototype.toString.call(d),
                        own: Reflect.ownKeys(d).map(String),
                        open: d.open,
                        returnValue: d.returnValue,
                        closedBy: d.closedBy,
                        modal: d.matches(':modal'),
                    },
                };

                d.show();
                result.afterShow = { open: d.open, modal: d.matches(':modal'), closedBy: d.closedBy };
                result.modalWhileShown = call(() => d.showModal());
                d.close('rv1');
                result.afterClose = { open: d.open, modal: d.matches(':modal'), returnValue: d.returnValue };

                d.showModal();
                result.afterModal = {
                    open: d.open,
                    modal: d.matches(':modal'),
                    closedBy: d.closedBy,
                    q: document.querySelector(':modal') === d,
                    qa: document.querySelectorAll(':modal').length,
                };
                let cancelCount = 0;
                const block = e => { cancelCount++; e.preventDefault(); };
                d.addEventListener('cancel', block);
                d.requestClose('blocked');
                result.afterPrevented = { open: d.open, modal: d.matches(':modal'), returnValue: d.returnValue, cancelCount };
                d.removeEventListener('cancel', block);
                d.requestClose('rv2');
                result.afterRequest = { open: d.open, modal: d.matches(':modal'), returnValue: d.returnValue, cancelCount };

                d.showModal();
                document.getElementById('b').appendChild(d);
                result.afterMove = { open: d.open, modal: d.matches(':modal'), closedBy: d.closedBy };

                d.closedBy = 'AnY';
                const cbAny = [d.closedBy, d.getAttribute('closedby')];
                d.closedBy = 'bogus';
                const cbInvalid = [d.closedBy, d.getAttribute('closedby')];
                result.closedBy = { cbAny, cbInvalid };
                result.native = ['close','requestClose','show','showModal'].every(
                    name => Function.prototype.toString.call(proto[name]).includes('[native code]')
                );
                return JSON.stringify(result);
            })()
            "#,
            None,
        )
        .unwrap();

    assert_eq!(
        result,
        r#"{"protoKeys":["open","returnValue","closedBy","close","requestClose","show","showModal","constructor","Symbol(Symbol.toStringTag)"],"desc":{"open":{"e":true,"c":true,"w":null,"g":true,"s":true,"len":null},"returnValue":{"e":true,"c":true,"w":null,"g":true,"s":true,"len":null},"closedBy":{"e":true,"c":true,"w":null,"g":true,"s":true,"len":null},"close":{"e":true,"c":true,"w":true,"g":false,"s":false,"len":0},"requestClose":{"e":true,"c":true,"w":true,"g":false,"s":false,"len":0},"show":{"e":true,"c":true,"w":true,"g":false,"s":false,"len":0},"showModal":{"e":true,"c":true,"w":true,"g":false,"s":false,"len":0}},"initial":{"tag":"[object HTMLDialogElement]","own":[],"open":false,"returnValue":"","closedBy":"none","modal":false},"afterShow":{"open":true,"modal":false,"closedBy":"none"},"modalWhileShown":"InvalidStateError: Failed to execute 'showModal' on 'HTMLDialogElement': The dialog is already open as a non-modal dialog, and therefore cannot be opened as a modal dialog.","afterClose":{"open":false,"modal":false,"returnValue":"rv1"},"afterModal":{"open":true,"modal":true,"closedBy":"closerequest","q":true,"qa":1},"afterPrevented":{"open":true,"modal":true,"returnValue":"rv1","cancelCount":1},"afterRequest":{"open":false,"modal":false,"returnValue":"rv2","cancelCount":1},"afterMove":{"open":true,"modal":false,"closedBy":"none"},"closedBy":{"cbAny":["any","AnY"],"cbInvalid":["none","bogus"]},"native":true}"#
    );
}

#[tokio::test]
async fn dialog_detached_and_manual_open_edge_cases_match_chromium() {
    let mut rt = runtime("<!doctype html><html><body></body></html>");
    let result = rt
        .execute_script(
            r#"
            (() => {
                const call = fn => { try { fn(); return 'ok'; } catch (e) { return e.name + ': ' + e.message; } };
                const detached = document.createElement('dialog');
                const modalDetached = call(() => detached.showModal());
                const showDetached = call(() => detached.show());
                const detachedState = { open: detached.open, modal: detached.matches(':modal') };

                const d = document.createElement('dialog');
                document.body.appendChild(d);
                d.showModal();
                d.open = false;
                const manual = { open: d.open, modal: d.matches(':modal'), closedBy: d.closedBy };
                d.close('ignored');
                const closeNoop = { open: d.open, modal: d.matches(':modal'), returnValue: d.returnValue };
                d.show();
                const restoredByShow = { open: d.open, modal: d.matches(':modal') };
                d.open = false;
                d.showModal();
                const restoredByModal = { open: d.open, modal: d.matches(':modal') };
                return JSON.stringify({ modalDetached, showDetached, detachedState, manual, closeNoop, restoredByShow, restoredByModal });
            })()
            "#,
            None,
        )
        .unwrap();
    assert_eq!(
        result,
        r#"{"modalDetached":"InvalidStateError: Failed to execute 'showModal' on 'HTMLDialogElement': The element is not in a Document.","showDetached":"ok","detachedState":{"open":true,"modal":false},"manual":{"open":false,"modal":true,"closedBy":"closerequest"},"closeNoop":{"open":false,"modal":true,"returnValue":""},"restoredByShow":{"open":true,"modal":true},"restoredByModal":{"open":true,"modal":true}}"#
    );
}
