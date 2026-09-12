//! Custom Elements browser-semantics regressions.
//!
//! Expected values are captured from Chrome for Testing 148.0.7778.167.

use browser_oxide::Page;

async fn page(html: &str) -> Page {
    Page::from_html_with_url(
        html,
        "https://example.test/custom-elements",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .expect("custom-elements page")
}

#[tokio::test]
async fn custom_element_upgrade_and_lifecycle_match_chrome_148() {
    let html = r#"<!doctype html><html><body>
        <x-pre id="pre" data-v="one"></x-pre><div id="host"></div>
        <script>
        globalThis.__ceResult = null;
        (async () => {
            const log = [];
            const err = fn => {
                try { fn(); return 'ok'; }
                catch (e) { return e.name + ':' + e.message; }
            };
            let whenCtor = '';
            const pending = customElements.whenDefined('x-pre').then(c => {
                whenCtor = c.name;
                log.push('when:' + c.name);
            });
            class XPre extends HTMLElement {
                static observedAttributes = ['data-v'];
                constructor() {
                    super();
                    log.push('pre:ctor');
                    this.ctorCount = (this.ctorCount || 0) + 1;
                }
                connectedCallback() { log.push('pre:connected'); }
                disconnectedCallback() { log.push('pre:disconnected'); }
                attributeChangedCallback(name, oldValue, newValue) {
                    log.push(`pre:attr:${name}:${oldValue}->${newValue}`);
                }
            }
            const pre = document.getElementById('pre');
            customElements.define('x-pre', XPre);
            await pending;
            await Promise.resolve();
            const preAfter = {
                instance: pre instanceof XPre,
                ctorCount: pre.ctorCount || 0,
                value: pre.getAttribute('data-v'),
            };
            pre.setAttribute('data-v', 'two');
            pre.remove();

            class XNew extends HTMLElement {
                constructor() { super(); log.push('new:ctor'); this.ready = 17; }
                connectedCallback() { log.push('new:connected'); }
                disconnectedCallback() { log.push('new:disconnected'); }
            }
            customElements.define('x-new', XNew);
            const created = document.createElement('x-new');
            const newBefore = {
                instance: created instanceof XNew,
                ready: created.ready || 0,
                connected: created.isConnected,
            };
            document.getElementById('host').appendChild(created);
            created.remove();

            const detached = document.createElement('div');
            detached.innerHTML = '<x-late></x-late>';
            const late = detached.firstElementChild;
            class XLate extends HTMLElement {
                constructor() { super(); log.push('late:ctor'); this.late = 23; }
                connectedCallback() { log.push('late:connected'); }
            }
            customElements.define('x-late', XLate);
            const lateBefore = { instance: late instanceof XLate, late: late.late || 0 };
            customElements.upgrade(detached);
            const lateAfter = {
                instance: late instanceof XLate,
                late: late.late || 0,
                connected: late.isConnected,
            };

            const dupName = err(() => customElements.define('x-pre', class extends HTMLElement {}));
            const dupCtor = err(() => customElements.define('x-other', XPre));
            const invalid = err(() => customElements.define('x', class extends HTMLElement {}));
            let invalidWhen = 'pending';
            try { await customElements.whenDefined('x'); invalidWhen = 'resolved'; }
            catch (e) { invalidWhen = e.name + ':' + e.message; }

            globalThis.__ceResult = {
                log, whenCtor, preAfter, newBefore, lateBefore, lateAfter,
                dupName, dupCtor, invalid, invalidWhen,
                getSame: customElements.get('x-pre') === XPre,
            };
        })();
        </script>
    </body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__ceResult)")
        .expect("custom element result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("custom element json");

    assert_eq!(
        value["log"],
        serde_json::json!([
            "pre:ctor",
            "pre:attr:data-v:null->one",
            "pre:connected",
            "when:XPre",
            "pre:attr:data-v:one->two",
            "pre:disconnected",
            "new:ctor",
            "new:connected",
            "new:disconnected",
            "late:ctor"
        ])
    );
    assert_eq!(value["whenCtor"], "XPre");
    assert_eq!(
        value["preAfter"],
        serde_json::json!({"instance":true,"ctorCount":1,"value":"one"})
    );
    assert_eq!(
        value["newBefore"],
        serde_json::json!({"instance":true,"ready":17,"connected":false})
    );
    assert_eq!(
        value["lateBefore"],
        serde_json::json!({"instance":false,"late":0})
    );
    assert_eq!(
        value["lateAfter"],
        serde_json::json!({"instance":true,"late":23,"connected":false})
    );
    assert_eq!(value["getSame"], true);
    assert_eq!(value["dupName"], "NotSupportedError:Failed to execute 'define' on 'CustomElementRegistry': the name \"x-pre\" has already been used with this registry");
    assert_eq!(value["dupCtor"], "NotSupportedError:Failed to execute 'define' on 'CustomElementRegistry': this constructor has already been used with this registry");
    assert_eq!(value["invalid"], "SyntaxError:Failed to execute 'define' on 'CustomElementRegistry': \"x\" is not a valid custom element name");
    assert_eq!(value["invalidWhen"], "SyntaxError:Failed to execute 'whenDefined' on 'CustomElementRegistry': \"x\" is not a valid custom element name");
}

#[tokio::test]
async fn custom_element_direct_construction_and_registry_shape_match_chrome_148() {
    let html = r#"<!doctype html><html><body><script>
        globalThis.__ceCtorResult = null;
        (async () => {
            const capture = fn => {
                try {
                    const value = fn();
                    return { ok: true, tag: value.tagName, is: value.getAttribute?.('is') || '', instance: value instanceof HTMLElement };
                } catch (e) { return { ok: false, error: e.name + ':' + e.message }; }
            };
            class XA extends HTMLElement { constructor() { super(); this.a = 1; } }
            const autoBefore = capture(() => new XA());
            customElements.define('x-a', XA);
            const autoAfter = capture(() => new XA());

            class XB extends HTMLButtonElement { constructor() { super(); this.b = 1; } }
            const builtBefore = capture(() => new XB());
            customElements.define('x-b', XB, { extends: 'button' });
            const builtAfter = capture(() => new XB());
            const createdBuilt = document.createElement('button', { is: 'x-b' });

            const scoped = new CustomElementRegistry();
            class XScoped extends HTMLElement {}
            const globalBefore = customElements.get('x-scoped');
            scoped.define('x-scoped', XScoped);
            const whenScoped = await scoped.whenDefined('x-scoped');
            let directCall;
            try { CustomElementRegistry(); directCall = 'ok'; }
            catch (e) { directCall = e.name + ':' + e.message; }

            globalThis.__ceCtorResult = {
                autoBefore, autoAfter, builtBefore, builtAfter,
                createdBuilt: {
                    tag: createdBuilt.tagName,
                    is: createdBuilt.getAttribute('is'),
                    instance: createdBuilt instanceof XB,
                    b: createdBuilt.b || 0,
                },
                registry: {
                    proto: Reflect.ownKeys(CustomElementRegistry.prototype).map(String),
                    tag: Object.prototype.toString.call(scoped),
                    own: Reflect.ownKeys(scoped).map(String),
                    globalUndefined: globalBefore === undefined,
                    scopedSame: scoped.get('x-scoped') === XScoped,
                    getName: scoped.getName(XScoped),
                    whenSame: whenScoped === XScoped,
                    directCall,
                    lengths: {
                        define: scoped.define.length,
                        get: scoped.get.length,
                        getName: scoped.getName.length,
                        upgrade: scoped.upgrade.length,
                        whenDefined: scoped.whenDefined.length,
                        initialize: scoped.initialize.length,
                    },
                },
            };
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__ceCtorResult)")
        .expect("constructor result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("constructor json");

    assert_eq!(value["autoBefore"]["ok"], false);
    assert_eq!(
        value["autoBefore"]["error"],
        "TypeError:Failed to construct 'HTMLElement': Illegal constructor"
    );
    assert_eq!(
        value["autoAfter"],
        serde_json::json!({"ok":true,"tag":"X-A","is":"","instance":true})
    );
    assert_eq!(value["builtBefore"]["ok"], false);
    assert_eq!(
        value["builtBefore"]["error"],
        "TypeError:Failed to construct 'HTMLButtonElement': Illegal constructor"
    );
    assert_eq!(
        value["builtAfter"],
        serde_json::json!({"ok":true,"tag":"BUTTON","is":"","instance":true})
    );
    assert_eq!(
        value["createdBuilt"],
        serde_json::json!({"tag":"BUTTON","is":null,"instance":true,"b":1})
    );
    assert_eq!(
        value["registry"]["proto"],
        serde_json::json!([
            "define",
            "get",
            "getName",
            "upgrade",
            "whenDefined",
            "initialize",
            "constructor",
            "Symbol(Symbol.toStringTag)"
        ])
    );
    assert_eq!(value["registry"]["tag"], "[object CustomElementRegistry]");
    assert_eq!(value["registry"]["own"], serde_json::json!([]));
    assert_eq!(value["registry"]["globalUndefined"], true);
    assert_eq!(value["registry"]["scopedSame"], true);
    assert_eq!(value["registry"]["getName"], "x-scoped");
    assert_eq!(value["registry"]["whenSame"], true);
    assert_eq!(value["registry"]["directCall"], "TypeError:Failed to construct 'CustomElementRegistry': Please use the 'new' operator, this DOM object constructor cannot be called as a function.");
    assert_eq!(
        value["registry"]["lengths"],
        serde_json::json!({"define":2,"get":1,"getName":1,"upgrade":1,"whenDefined":1,"initialize":1})
    );
}

#[tokio::test]
async fn upgraded_detached_subtree_connects_and_disconnects_in_tree_order() {
    let html = r#"<!doctype html><html><body><div id="host"></div><script>
        globalThis.__ceTree = null;
        (() => {
            const log = [];
            const root = document.createElement('section');
            root.innerHTML = '<x-parent><x-child></x-child></x-parent>';
            class XParent extends HTMLElement {
                constructor() { super(); log.push('p:ctor'); }
                connectedCallback() { log.push('p:connected'); }
                disconnectedCallback() { log.push('p:disconnected'); }
            }
            class XChild extends HTMLElement {
                constructor() { super(); log.push('c:ctor'); }
                connectedCallback() { log.push('c:connected'); }
                disconnectedCallback() { log.push('c:disconnected'); }
            }
            customElements.define('x-parent', XParent);
            customElements.define('x-child', XChild);
            customElements.upgrade(root);
            log.push('upgraded');
            document.getElementById('host').appendChild(root);
            log.push('appended');
            root.remove();
            log.push('removed');
            globalThis.__ceTree = log;
        })();
    </script></body></html>"#;

    let mut page = page(html).await;
    let result = page
        .evaluate("JSON.stringify(globalThis.__ceTree)")
        .expect("subtree lifecycle result");
    let value: serde_json::Value = serde_json::from_str(&result).expect("subtree lifecycle json");
    assert_eq!(
        value,
        serde_json::json!([
            "p:ctor",
            "c:ctor",
            "upgraded",
            "p:connected",
            "c:connected",
            "appended",
            "p:disconnected",
            "c:disconnected",
            "removed"
        ])
    );
}
