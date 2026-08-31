//! Genuine-native host functions built from raw `v8::FunctionTemplate`
//! (the same primitive deno_core uses for every `#[op2]`, see
//! deno_core's `runtime/bindings.rs`).
//!
//! WHY: V8 builds the `class X extends <fn> {}` TypeError message,
//! `Object::NoSideEffectsToString`, error stacks and `eval`-stringify
//! from a function's internal `[[SourceText]]` via `JSFunction::ToString`,
//! which emits `function NAME() { [native code] }` ONLY when
//! `!SharedFunctionInfo::IsUserJavaScript()` — true exactly for API
//! functions (FunctionTemplate, `script()==undefined`). A JS-level
//! `Function.prototype.toString` patch (stealth_bootstrap.js) CANNOT
//! intercept these internal stringifiers, so our JS shims would leak
//! their source (e.g. `class K extends Function.prototype.toString{}`
//! leaks `_patchedFnToStr` source) — differs from real Chrome, where
//! these are native functions.
//!
//! This installs `Function.prototype.toString` as a genuine API
//! function. Behaviour preserved: masked host fns (carrying the
//! `Symbol.for('__browser_oxide_native__')` tag set by stealth_bootstrap.js)
//! stringify as `function <tag>() { [native code] }`; everything else
//! delegates to the GENUINE original `Function.prototype.toString`
//! (captured BEFORE any bootstrap ran) so real JS user functions still
//! return their source and real natives return `[native code]` — exactly
//! as V8 would. Because the installed function is itself an API
//! function it is non-constructable, `.prototype`-less, and source-less:
//! the class-extends / NoSideEffectsToString leak is structurally gone.

use deno_core::v8;
use std::collections::HashMap;

/// Realm-local helpers that perform Window/global reflection using JavaScript
/// semantics inside the owning V8 context. Calling these functions from Rust is
/// more reliable than using cross-context GlobalProxy C++ property APIs, whose
/// behavior intentionally differs from HTML's WindowProxy exotic object.
pub struct IframePropertyBridge {
    pub get: v8::Global<v8::Function>,
    pub set: v8::Global<v8::Function>,
    pub has: v8::Global<v8::Function>,
    pub delete: v8::Global<v8::Function>,
    pub own_keys: v8::Global<v8::Function>,
    pub descriptor: v8::Global<v8::Function>,
    pub define: v8::Global<v8::Function>,
}

const NATIVE_TAG: &str = "__browser_oxide_native__";

/// Per-runtime storage for child iframe realms (genuine v8::Context instances).
///
/// Each entry keeps the child `v8::Context` alive (without a Global it would
/// be GC'd) and caches the child global object to avoid re-creating on
/// successive `contentWindow` reads from JS. Keyed by a monotonically-
/// increasing realm ID assigned by `_nextRealmId` in dom_bootstrap.js.
///
/// `orig_fp_tostring` is the builtin `Function.prototype.toString` captured
/// pre-bootstrap; it is installed into every child realm so cross-realm
/// `toString` calls (`cw.Function.prototype.toString.call(parent.fetch)`)
/// produce `[native code]` — same as the main window.
///
/// `native_tag_sym` is the JS-global-registry symbol `Symbol.for('__browser_oxide_native__')`
/// captured after bootstrap runs. It is the SAME symbol object that stealth_bootstrap.js
/// uses to tag masked host functions. The V8 API registry symbol from
/// `v8::Symbol::for_api` is a DIFFERENT registry and will NOT find these tags.
pub struct IframeRealmStore {
    pub contexts: HashMap<u32, v8::Global<v8::Context>>,
    /// Raw V8 GlobalProxy returned to the realm/FrameContext path.
    pub globals: HashMap<u32, v8::Global<v8::Object>>,
    /// Ordinary inner global behind each V8 GlobalProxy. Public HTML
    /// WindowProxy wrappers forward data/reflection operations here.
    pub inner_globals: HashMap<u32, v8::Global<v8::Object>>,
    /// Host DOM node -> realm id. This is the canonical bridge between the
    /// browser-visible `iframe.contentWindow` realm and Rust's frame API.
    pub node_to_realm: HashMap<u32, u32>,
    /// Reverse mapping used for deterministic teardown on iframe navigation or
    /// removal. Keeping both directions avoids scanning the store on hot paths.
    pub realm_to_node: HashMap<u32, u32>,
    /// Stable V8 global proxy per iframe host. Creating a new Context with this
    /// object resets its inner global while preserving `contentWindow` identity,
    /// matching browser navigation semantics.
    pub window_proxies: HashMap<u32, v8::Global<v8::Object>>,
    /// Browser-visible WindowProxy wrapper for each live same-isolate realm.
    /// This is distinct from the V8 GlobalProxy: the wrapper implements HTML's
    /// WindowProxy reflection semantics while the raw GlobalProxy remains the
    /// realm's unique execution global.
    pub public_windows: HashMap<u32, v8::Global<v8::Object>>,
    /// Cached reflection helpers created in each child realm.
    pub property_bridges: HashMap<u32, IframePropertyBridge>,
    /// Stack of same-isolate iframe realms whose JavaScript is currently being
    /// executed by the embedder. postMessage uses the top entry as the sender
    /// browsing context; nested eval/message delivery therefore composes
    /// naturally instead of relying on identity-changing JS routing proxies.
    pub execution_stack: Vec<u32>,
    pub orig_fp_tostring: Option<v8::Global<v8::Function>>,
    pub native_tag_sym: Option<v8::Global<v8::Symbol>>,
}

impl Default for IframeRealmStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IframeRealmStore {
    fn drop(&mut self) {
        // Realm-local functions and public wrappers retain creation-context
        // references. Release those handles before Context globals so V8's
        // weak Context callbacks never observe an already-cleared persistent.
        self.property_bridges.clear();
        self.public_windows.clear();
        self.inner_globals.clear();
        self.globals.clear();
        self.contexts.clear();
        self.execution_stack.clear();
    }
}

impl IframeRealmStore {
    pub fn new() -> Self {
        Self {
            contexts: HashMap::new(),
            globals: HashMap::new(),
            inner_globals: HashMap::new(),
            node_to_realm: HashMap::new(),
            realm_to_node: HashMap::new(),
            window_proxies: HashMap::new(),
            public_windows: HashMap::new(),
            property_bridges: HashMap::new(),
            execution_stack: Vec::new(),
            orig_fp_tostring: None,
            native_tag_sym: None,
        }
    }
}

/// Create a GENUINE child realm — a second `v8::Context` in the same
/// isolate, exactly what Chrome does per iframe (the primitive
/// deno_core itself uses, `jsruntime.rs:2048`). The child
/// gets its OWN full set of native builtins for free
/// (`Object`/`Function`/`Array`/`Reflect`/`Symbol`/`Map`/`TypeError`/…),
/// each genuinely `[native code]` and realm-distinct from the parent —
/// i.e. NOT a `Proxy` and NOT parent-aliased, matching real Chrome's
/// per-iframe realm semantics.
///
/// This is the PRIMITIVE only (additive, not yet wired into
/// `_getIframeWindow`) — proves the core mechanism works in our
/// single-isolate deno_core engine before the larger wiring pass.
/// Returns the child context's global object as a Global so callers
/// can hold it as `iframe.contentWindow` and keep the context alive.
pub fn create_child_realm(
    scope: &mut v8::PinScope,
) -> Option<(v8::Global<v8::Context>, v8::Global<v8::Object>)> {
    // A fresh context. Default options: the child gets the full native
    // intrinsic set + its own global. (Real Chrome exposes native
    // builtins/prototype-chains/ctor-names per realm — all native here;
    // DOM host shims are a later delegation step.)
    let ctx = v8::Context::new(scope, v8::ContextOptions::default());
    let global = {
        let cscope = &mut v8::ContextScope::new(scope, ctx);
        let g = ctx.global(cscope);
        v8::Global::new(cscope, g)
    };
    Some((v8::Global::new(scope, ctx), global))
}

/// Capture the genuine builtin `Function.prototype.toString` BEFORE any
/// bootstrap replaces it. Returned Global is passed as the API
/// function's `data` so untagged functions delegate to real V8
/// semantics. Call right after `JsRuntime::new`, before bootstrap.
pub fn capture_original_fp_tostring(scope: &mut v8::PinScope) -> Option<v8::Global<v8::Function>> {
    let ctx = scope.get_current_context();
    let global = ctx.global(scope);
    let fkey = v8::String::new(scope, "Function")?;
    let fctor = global.get(scope, fkey.into())?;
    let fctor = v8::Local::<v8::Object>::try_from(fctor).ok()?;
    let pkey = v8::String::new(scope, "prototype")?;
    let fproto = fctor.get(scope, pkey.into())?;
    let fproto = v8::Local::<v8::Object>::try_from(fproto).ok()?;
    let tskey = v8::String::new(scope, "toString")?;
    let ts = fproto.get(scope, tskey.into())?;
    let ts = v8::Local::<v8::Function>::try_from(ts).ok()?;
    Some(v8::Global::new(scope, ts))
}

/// The genuine-native `Function.prototype.toString` callback.
///
/// `args.data()` is an Array `[orig, sym]` where:
///   - index 0: the captured genuine `Function.prototype.toString` (v8::Function)
///   - index 1: the JS-global-registry `Symbol.for('__browser_oxide_native__')` (v8::Symbol)
///
/// Using Array data is necessary because V8 callback data can only hold a
/// single v8::Value. The symbol MUST come from the JS global registry
/// (`Symbol::For` / `v8::Symbol::for_key`), not V8's API registry
/// (`Symbol::ForApi` / `v8::Symbol::for_api`) — those are different tables.
/// Stealth_bootstrap.js tags host functions via `Symbol.for('__browser_oxide_native__')`
/// which writes to the JS registry; looking up via `v8::Symbol::for_api`
/// silently misses all tags.
// `v8::Symbol::for_api` is the API-registry lookup we use here only as a
// documented fallback (see the doc comment above); the primary path is the
// JS-registry symbol passed in via the data Array, which we try first.
fn fp_to_string_cb<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this: v8::Local<v8::Value> = args.this().into();
    let data = args.data();

    // Extract [orig, sym] from Array data, falling back to direct-function
    // data for contexts where no symbol was available at install time.
    let orig_fn: Option<v8::Local<v8::Function>>;
    let tag_sym: Option<v8::Local<v8::Symbol>>;
    if let Ok(arr) = v8::Local::<v8::Array>::try_from(data) {
        orig_fn = arr
            .get_index(scope, 0)
            .and_then(|v| v8::Local::<v8::Function>::try_from(v).ok());
        tag_sym = arr
            .get_index(scope, 1)
            .and_then(|v| v8::Local::<v8::Symbol>::try_from(v).ok());
    } else {
        orig_fn = v8::Local::<v8::Function>::try_from(data).ok();
        tag_sym = None;
    }

    // Masked host fn? Check via the JS-global-registry Symbol before the
    // is_function() guard, so Proxy-wrapped tagged objects also stringify.
    // stealth_bootstrap.js sets `fn[Symbol.for('__browser_oxide_native__')] = name`.
    if let Ok(this_obj) = v8::Local::<v8::Object>::try_from(this) {
        // Resolve which symbol to use for the tag lookup.
        // Primary path: the JS-registry symbol from Array data.
        // Fallback: v8::Symbol::for_global (V8 API registry, won't find
        // JS-tagged symbols but avoids hard failure in no-sym contexts).
        let maybe_tag: Option<String> = if let Some(sym) = tag_sym {
            if let Some(tagv) = this_obj.get(scope, sym.into()) {
                if tagv.is_string() {
                    Some(tagv.to_rust_string_lossy(scope))
                } else {
                    None
                }
            } else {
                None
            }
        } else if let Some(key) = v8::String::new(scope, NATIVE_TAG) {
            let sym = v8::Symbol::for_api(scope, key);
            if let Some(tagv) = this_obj.get(scope, sym.into()) {
                if tagv.is_string() {
                    Some(tagv.to_rust_string_lossy(scope))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(tag) = maybe_tag {
            let s = format!("function {tag}() {{ [native code] }}");
            if let Some(out) = v8::String::new(scope, &s) {
                rv.set(out.into());
                return;
            }
        }
    }

    // Some scripts wrap DOM functions in Proxies and call
    // Function.prototype.toString on them. V8's original FP.toString throws
    // "requires that 'this' be a Function" for Proxy objects even when the
    // Proxy wraps a function (V8 checks the JSReceiver type directly, not
    // [[Call]]). Real Chrome returns a native string here; pre-detect
    // callable Proxies and do the same.
    if this.is_proxy() {
        if let Ok(proxy) = v8::Local::<v8::Proxy>::try_from(this) {
            let target = proxy.get_target(scope);
            if target.is_function() {
                let name = v8::Local::<v8::Object>::try_from(target)
                    .ok()
                    .and_then(|to| {
                        v8::String::new(scope, "name")
                            .and_then(|k| to.get(scope, k.into()))
                            .map(|nv| nv.to_rust_string_lossy(scope))
                    })
                    .unwrap_or_default();
                let s = format!("function {name}() {{ [native code] }}");
                if let Some(out) = v8::String::new(scope, &s) {
                    rv.set(out.into());
                }
                return;
            }
        }
    }

    // Delegate to the GENUINE original Function.prototype.toString
    // (captured pre-bootstrap, stored at index 0 of the data Array).
    if let Some(orig) = orig_fn {
        if let Some(res) = orig.call(scope, this, &[]) {
            rv.set(res);
        }
        // If orig.call returns None the exception is already set on the
        // scope (e.g. non-callable `this` → TypeError). Just return.
        return;
    }
    // Last-resort fallback (should be unreachable): anonymous native.
    if let Some(out) = v8::String::new(scope, "function () { [native code] }") {
        rv.set(out.into());
    }
}

/// Install the genuine-native `Function.prototype.toString`, replacing
/// the JS-level patch. `original` must be the builtin captured pre-
/// bootstrap (see `capture_original_fp_tostring`). `native_tag_sym` must
/// be `Symbol.for('__browser_oxide_native__')` captured from the JS environment
/// AFTER bootstrap runs — it is the JS-global-registry symbol that
/// stealth_bootstrap.js uses to tag host functions. Pass `None` only when
/// no bootstrap has run (e.g. child realms before symbol is captured).
/// Run AFTER bootstrap + cleanup, before site/init scripts.
pub fn install_native_fp_tostring(
    scope: &mut v8::PinScope,
    original: &v8::Global<v8::Function>,
    native_tag_sym: Option<&v8::Global<v8::Symbol>>,
) -> bool {
    let orig_local = v8::Local::new(scope, original);

    // Pack [orig, sym] into an Array so the callback can access both.
    // A single FunctionTemplate data slot can hold only one v8::Value,
    // so we use an Array to carry the pair.
    let data_val: v8::Local<v8::Value> = if let Some(sym_g) = native_tag_sym {
        let sym_local = v8::Local::new(scope, sym_g);
        let arr = v8::Array::new(scope, 2);
        // v8::Object::set with integer key — guaranteed stable in all rusty_v8 versions.
        let i0 = v8::Integer::new(scope, 0);
        let i1 = v8::Integer::new(scope, 1);
        arr.set(scope, i0.into(), orig_local.into());
        arr.set(scope, i1.into(), sym_local.into());
        arr.into()
    } else {
        // No symbol yet (child realm created before post-bootstrap capture).
        // Fall back to direct function data; callback will use for_api.
        orig_local.into()
    };

    let tmpl = v8::FunctionTemplate::builder(fp_to_string_cb)
        .length(0)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .side_effect_type(v8::SideEffectType::HasNoSideEffect)
        .data(data_val)
        .build(scope);
    if let Some(name) = v8::String::new(scope, "toString") {
        tmpl.set_class_name(name);
    }
    let func = match tmpl.get_function(scope) {
        Some(f) => f,
        None => return false,
    };
    if let Some(name) = v8::String::new(scope, "toString") {
        func.set_name(name);
    }

    // Install on Function.prototype with Chrome's attribute shape:
    // { value, writable:true, enumerable:false, configurable:true }.
    let ctx = scope.get_current_context();
    let global = ctx.global(scope);
    let Some(fkey) = v8::String::new(scope, "Function") else {
        return false;
    };
    let Some(fctor) = global.get(scope, fkey.into()) else {
        return false;
    };
    let Ok(fctor) = v8::Local::<v8::Object>::try_from(fctor) else {
        return false;
    };
    let Some(pkey) = v8::String::new(scope, "prototype") else {
        return false;
    };
    let Some(fproto) = fctor.get(scope, pkey.into()) else {
        return false;
    };
    let Ok(fproto) = v8::Local::<v8::Object>::try_from(fproto) else {
        return false;
    };
    let Some(tskey) = v8::String::new(scope, "toString") else {
        return false;
    };
    // define_own_property with DONT_ENUM (writable+configurable default).
    fproto.define_own_property(
        scope,
        tskey.into(),
        func.into(),
        v8::PropertyAttribute::DONT_ENUM,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use deno_core::{JsRuntime, RuntimeOptions};

    /// Verifies that `proxy.get_prototype()` returns the inner global (not
    /// Window.prototype), and that `create_data_property` on the inner global
    /// creates properties visible from INSIDE the child realm via script eval.
    /// Also tests that calling `set_prototype()` on the proxy (as done by
    /// `op_create_child_realm`) doesn't change what `get_prototype()` returns.
    #[test]
    fn verify_inner_global_property_visibility() {
        let mut rt = JsRuntime::new(RuntimeOptions::default());
        let main_ctx = rt.main_context();
        v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);

        let child_ctx = v8::Context::new(scope, v8::ContextOptions::default());
        {
            let cs = &mut v8::ContextScope::new(scope, child_ctx);

            // Simulate what op_create_child_realm does: set a Window prototype
            let window_proto_src = v8::String::new(cs, "(function Window(){}).prototype").unwrap();
            let window_proto_script = v8::Script::compile(cs, window_proto_src, None).unwrap();
            let window_proto_val = window_proto_script.run(cs).unwrap();

            let proxy = child_ctx.global(cs);
            proxy.set_prototype(cs, window_proto_val); // mimic op_create_child_realm

            // Now get inner global via get_prototype (must still be inner global, not window_proto)
            let proto_after = proxy
                .get_prototype(cs)
                .expect("proxy must have a prototype");
            let inner = v8::Local::<v8::Object>::try_from(proto_after)
                .expect("prototype after set_prototype must still be an Object (inner global)");

            // The inner global must NOT be the window_proto we set
            // (set_prototype sets inner_global's [[Prototype]], not the proxy's)
            let inner_hash = inner.get_identity_hash();
            let proxy_hash = proxy.get_identity_hash();
            assert_ne!(
                inner_hash, proxy_hash,
                "inner global must differ from proxy"
            );

            let key = v8::String::new(cs, "__testProp__").unwrap();
            let val = v8::Integer::new(cs, 42);
            inner.create_data_property(cs, key.into(), val.into());

            // Property must be readable from inside the realm via eval
            let src = v8::String::new(cs, "typeof __testProp__ + ':' + __testProp__").unwrap();
            let script = v8::Script::compile(cs, src, None).unwrap();
            let res = script.run(cs).unwrap().to_rust_string_lossy(cs);
            assert_eq!(res, "number:42",
                "inner-global property must be visible inside realm after set_prototype; got: {res}");
        }
    }

    /// Proves the child-realm PRIMITIVE: a raw `v8::Context::new` child
    /// in the same isolate has its OWN genuine native intrinsics
    /// (`[native code]`, correct ctor names) and a global object
    /// distinct from the parent — i.e. exactly what real Chrome exposes
    /// for `iframe.contentWindow` (real realm, NOT a Proxy, NOT
    /// parent-aliased). Foundation for the _getIframeWindow wiring.
    #[test]
    fn child_realm_has_genuine_native_intrinsics() {
        let mut rt = JsRuntime::new(RuntimeOptions::default());
        let main_ctx = rt.main_context();
        v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);

        // Parent realm Object identity (for distinctness check).
        let parent_obj_hash = {
            let g = scope.get_current_context().global(scope);
            let k = v8::String::new(scope, "Object").unwrap();
            let o = g.get(scope, k.into()).unwrap();
            v8::Local::<v8::Object>::try_from(o)
                .unwrap()
                .get_identity_hash()
        };

        let (ctx_g, _glob_g) = create_child_realm(scope).expect("child realm created");

        let ctx = v8::Local::new(scope, &ctx_g);
        let cs = &mut v8::ContextScope::new(scope, ctx);

        // Run a probe IN the child realm.
        let src = v8::String::new(
            cs,
            "JSON.stringify({\
               objTS: Function.prototype.toString.call(Object),\
               fnName: Function.name,\
               objName: Object.name,\
               arrTS: Array.prototype.slice.toString(),\
               typeofWin: typeof globalThis,\
               isProxyish: (function(){try{return String(globalThis).indexOf('Proxy')>=0}catch(e){return 'err'}})()\
             })",
        )
        .unwrap();
        let script = v8::Script::compile(cs, src, None).unwrap();
        let res = script.run(cs).unwrap();
        let json = res.to_rust_string_lossy(cs);

        // Child intrinsics must be GENUINE natives.
        assert!(
            json.contains("function Object() { [native code] }"),
            "child Object must be a real native, got: {json}"
        );
        assert!(
            json.contains("function slice() { [native code] }"),
            "child Array.prototype.slice must be native, got: {json}"
        );
        assert!(
            json.contains("\"objName\":\"Object\""),
            "child Object.name must be 'Object', got: {json}"
        );

        // Child global Object must be a DISTINCT object from parent's.
        let child_obj_hash = {
            let g = ctx.global(cs);
            let k = v8::String::new(cs, "Object").unwrap();
            let o = g.get(cs, k.into()).unwrap();
            v8::Local::<v8::Object>::try_from(o)
                .unwrap()
                .get_identity_hash()
        };
        assert_ne!(
            parent_obj_hash, child_obj_hash,
            "child realm Object must be realm-distinct from parent \
             (real per-frame realm, not parent-aliased)"
        );
    }

    /// Verifies Fix A (Symbol registry): `install_native_fp_tostring` with the
    /// JS-global-registry Symbol returns `function <tag>() { [native code] }`
    /// for tagged functions, instead of falling through to the wrong registry.
    ///
    /// This is the regression test for the v8::Symbol::for_global vs
    /// Symbol::For bug: `for_global` uses the API registry (Symbol::ForApi),
    /// not the JS global registry (Symbol::For). Tags set via Symbol.for() in
    /// JS are INVISIBLE to for_global lookups.
    #[test]
    fn native_fp_tostring_uses_js_symbol_registry() {
        use deno_core::JsRuntime;

        let mut rt = JsRuntime::new(RuntimeOptions::default());

        // Capture original FP.toString (pre-bootstrap).
        let orig_g = {
            let main_ctx = rt.main_context();
            v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);
            capture_original_fp_tostring(scope).expect("capture original")
        };

        // Simulate bootstrap: set Symbol.for('__browser_oxide_native__') tag on a function.
        rt.execute_script(
            "<test>",
            r#"
                const _nativeTag = Symbol.for('__browser_oxide_native__');
                function myTaggedFn() {}
                Object.defineProperty(myTaggedFn, _nativeTag, { value: 'myTaggedFn', configurable: true });
                globalThis.__testFn = myTaggedFn;
            "#,
        )
        .expect("setup script");

        // Capture Symbol.for('__browser_oxide_native__') from JS environment.
        let native_tag_sym_g: Option<v8::Global<v8::Symbol>> = {
            let main_ctx = rt.main_context();
            v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);
            let src = v8::String::new(scope, "Symbol.for('__browser_oxide_native__')").unwrap();
            let script = v8::Script::compile(scope, src, None).unwrap();
            let val = script.run(scope).unwrap();
            let sym = v8::Local::<v8::Symbol>::try_from(val).ok().unwrap();
            Some(v8::Global::new(scope, sym))
        };

        // Install native FP.toString with the correct symbol.
        {
            let main_ctx = rt.main_context();
            v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);
            install_native_fp_tostring(scope, &orig_g, native_tag_sym_g.as_ref());
        }

        // Test: FP.toString.call(myTaggedFn) should return tagged string.
        let s = rt
            .execute_script(
                "<test>",
                "Function.prototype.toString.call(globalThis.__testFn)",
            )
            .map(|v| {
                let main_ctx = rt.main_context();
                v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);
                let local = v8::Local::new(scope, &v);
                local.to_rust_string_lossy(scope)
            })
            .expect("eval");
        assert_eq!(
            s, "function myTaggedFn() { [native code] }",
            "tagged function should return native-code string; got: {s}"
        );

        // Test: FP.toString.call(Array.prototype.slice) should also work
        // (real native - should return native string via orig delegation).
        let s2 = rt
            .execute_script(
                "<test>",
                "Function.prototype.toString.call(Array.prototype.slice)",
            )
            .map(|v| {
                let main_ctx = rt.main_context();
                v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);
                let local = v8::Local::new(scope, &v);
                local.to_rust_string_lossy(scope)
            })
            .expect("eval2");
        assert!(
            s2.contains("[native code]"),
            "real native should return [native code]; got: {s2}"
        );
    }

    /// V8's embedder-supported WindowProxy handoff: reuse the prior global
    /// object when creating a fresh Context. V8 resets its state while retaining
    /// object identity, matching browser navigation semantics.
    #[tokio::test(flavor = "current_thread")]
    async fn reused_global_proxy_can_back_a_new_context() {
        let mut rt = JsRuntime::new(RuntimeOptions::default());
        let main_ctx = rt.main_context();
        v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);

        let first = v8::Context::new(scope, v8::ContextOptions::default());
        let proxy = first.global(scope);
        let proxy_g = v8::Global::new(scope, proxy);
        {
            let cs = &mut v8::ContextScope::new(scope, first);
            let source = v8::String::new(cs, "globalThis.beforeNavigation = 1").unwrap();
            v8::Script::compile(cs, source, None)
                .unwrap()
                .run(cs)
                .unwrap();
        }
        let proxy = v8::Local::new(scope, &proxy_g);
        let second = v8::Context::new(
            scope,
            v8::ContextOptions {
                global_object: Some(proxy.into()),
                ..Default::default()
            },
        );
        let second_proxy = second.global(scope);
        assert!(second_proxy.strict_equals(proxy.into()));

        let cs = &mut v8::ContextScope::new(scope, second);
        let source = v8::String::new(
            cs,
            "JSON.stringify({old:typeof beforeNavigation,fresh:Array !== undefined})",
        )
        .unwrap();
        let value = v8::Script::compile(cs, source, None)
            .unwrap()
            .run(cs)
            .unwrap()
            .to_rust_string_lossy(cs);
        assert_eq!(value, r#"{"old":"undefined","fresh":true}"#);
    }

    /// Validate whether V8 accepts a real JS Proxy as the context global
    /// object without inserting a distinct identity in front of it. If this
    /// holds, BrowserOxide can model HTML's WindowProxy exotic methods on the
    /// V8 absorbs a supplied JS Proxy into its native GlobalProxy machinery.
    /// The identity is reused, but the original JS Proxy traps do not remain
    /// the realm-global internal methods. This contract is why BrowserOxide
    /// keeps HTML WindowProxy reflection in a public wrapper instead of trying
    /// to make that wrapper the child Context's actual global object.
    #[tokio::test(flavor = "current_thread")]
    async fn js_proxy_global_object_is_absorbed_by_v8() {
        let mut rt = JsRuntime::new(RuntimeOptions::default());
        let main_ctx = rt.main_context();
        v8::scope_with_context!(let scope, rt.v8_isolate(), &main_ctx);

        let proxy = {
            let source = v8::String::new(
                scope,
                r#"new Proxy({}, {
                    isExtensible(t) { return Reflect.isExtensible(t); },
                    preventExtensions() { return false; },
                    defineProperty(t, p, d) { return Reflect.defineProperty(t, p, d); }
                })"#,
            )
            .unwrap();
            let value = v8::Script::compile(scope, source, None)
                .unwrap()
                .run(scope)
                .unwrap();
            v8::Local::<v8::Object>::try_from(value).unwrap()
        };
        let proxy_g = v8::Global::new(scope, proxy);
        let proxy = v8::Local::new(scope, &proxy_g);
        let child = v8::Context::new(
            scope,
            v8::ContextOptions {
                global_object: Some(proxy.into()),
                ..Default::default()
            },
        );
        assert!(child.global(scope).strict_equals(proxy.into()));

        let cs = &mut v8::ContextScope::new(scope, child);
        let source = v8::String::new(
            cs,
            r#"(() => {
                let preventError='';
                try { Object.preventExtensions(globalThis); } catch(e) { preventError=e.name; }
                const soft=Reflect.defineProperty(globalThis,'soft',{value:7,configurable:true});
                const hard=Reflect.defineProperty(globalThis,'hard',{value:9,configurable:false});
                return JSON.stringify({
                    thisIdentity:this===globalThis,
                    extensible:Object.isExtensible(globalThis),
                    preventError,
                    soft,
                    softValue:globalThis.soft,
                    hard,
                    hardValue:globalThis.hard,
                });
            })()"#,
        )
        .unwrap();
        let result = v8::Script::compile(cs, source, None)
            .unwrap()
            .run(cs)
            .unwrap()
            .to_rust_string_lossy(cs);
        assert_eq!(
            result,
            r#"{"thisIdentity":true,"extensible":false,"preventError":"","soft":false,"hard":false}"#
        );
    }
}
