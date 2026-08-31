//! V8 JavaScript runtime with DOM bindings for browser_oxide.
//!
//! MIT/Apache-2.0 licensed. Part of the browser_oxide project.

pub mod extensions;
pub mod frame_waker;
pub mod inspector;
pub mod module_loader;
pub mod native_fns;
pub mod readiness;
pub mod runtime;
pub mod snapshot;
pub mod state;
pub mod tokio_fallback;
pub mod utils;

use crate::dom::Dom;
use crate::stealth::StealthProfile;
use deno_core::v8;
use deno_core::JsRuntime;
use extensions::nav_ext::NavSignal;
use runtime::{create_runtime_with_signals, BrowserRuntimeOptions, RuntimeInternalFns};
use state::{ConsoleMessage, DomState};

/// Native stack for a thread that builds or drives a V8 isolate. The default
/// ~2 MB overflows while V8 parses deno_core's primordials.
pub const V8_THREAD_STACK: usize = 64 * 1024 * 1024;

/// Like `std::thread::spawn` but with a stack big enough to build or drive a V8 isolate.
pub fn spawn_v8_thread<F>(name: impl Into<String>, f: F) -> std::thread::JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    let name = name.into();
    std::thread::Builder::new()
        .name(name.clone())
        .stack_size(V8_THREAD_STACK)
        .spawn(f)
        .unwrap_or_else(|e| panic!("failed to spawn V8 thread {name}: {e}"))
}

/// A V8 JavaScript runtime with browser DOM bindings.
pub struct BrowserJsRuntime {
    inner: JsRuntime,
    /// Cached `__pumpFrameMessages` function so frame messages deliver via a
    /// compiled call. Cleared and re-captured on `replace_dom`.
    frame_deliver_fn: Option<v8::Global<v8::Function>>,
    /// Privileged bootstrap closures retained after cleanup removes the
    /// temporary page-visible bridge.
    set_current_script_fn: Option<v8::Global<v8::Function>>,
    complete_lifecycle_fn: Option<v8::Global<v8::Function>>,
    /// Per-runtime navigation-pending signal. JS sets it via
    /// `op_set_pending_nav` (called from window_bootstrap.js whenever
    /// `__pendingNavigation` is assigned). The event loop polls it to
    /// short-circuit `run_until_idle` for fast nav handoff (some sites
    /// expect a navigation to begin within a few seconds).
    nav_signal: NavSignal,
}

/// RAII guard that enters a V8 isolate on creation and exits it on drop,
/// restoring whatever isolate was thread-current before. Required because
/// browser_oxide keeps several `OwnedIsolate`s alive at once (page + per-iframe
/// runtimes) and, under v8-149/deno_core-0.403, the isolate is only
/// auto-entered at construction — so the "current" isolate is just the
/// last-constructed one unless we explicitly re-enter the one we're about to
/// drive. `Isolate::enter`/`exit` nest correctly (V8 saves/restores the
/// previous isolate), so this is safe to use on every entry point even when
/// the isolate already happens to be current.
struct IsolateEnterGuard {
    isolate: *mut v8::Isolate,
}

impl IsolateEnterGuard {
    fn enter(isolate: &mut v8::OwnedIsolate) -> Self {
        let isolate: *mut v8::Isolate = &mut **isolate;
        // SAFETY: `isolate` is a live, valid V8 isolate (owned by `self.inner`,
        // which outlives this guard — the guard is dropped at the end of the
        // calling method, well before the runtime). `enter`/`exit` are balanced
        // by the guard's `Drop`, and V8 restores the previously-entered isolate
        // on exit, so the thread-current isolate is left unchanged on return.
        // We hold a raw pointer (not a borrow) so the caller can still take a
        // fresh `&mut` to build its scope.
        unsafe { (*isolate).enter() };
        Self { isolate }
    }
}

impl Drop for IsolateEnterGuard {
    fn drop(&mut self) {
        // SAFETY: paired with the `enter()` in `IsolateEnterGuard::enter`;
        // `self.isolate` is still alive (the owning runtime outlives the guard).
        unsafe { (*self.isolate).exit() };
    }
}

impl BrowserJsRuntime {
    /// Create a new runtime with the given DOM (no stealth profile).
    pub fn new(dom: Dom) -> Self {
        let (inner, nav_signal, internal_fns) =
            create_runtime_with_signals(dom, BrowserRuntimeOptions::default());
        Self::from_parts(inner, nav_signal, internal_fns)
    }

    /// Create with a stealth profile.
    pub fn with_profile(dom: Dom, profile: StealthProfile) -> Self {
        let (inner, nav_signal, internal_fns) = create_runtime_with_signals(
            dom,
            BrowserRuntimeOptions {
                stealth_profile: Some(profile),
                ..Default::default()
            },
        );
        Self::from_parts(inner, nav_signal, internal_fns)
    }

    /// Create with full options.
    pub fn with_options(dom: Dom, mut options: BrowserRuntimeOptions) -> Self {
        // Disabled: building it crashes V8's serializer on an op-returned typed
        // array's empty backing store, and it would not beat a cold bootstrap anyway.
        if std::env::var_os("BROWSER_OXIDE_USE_SNAPSHOT").is_some()
            && options.startup_snapshot.is_none()
        {
            options.startup_snapshot = Some(snapshot::get_snapshot());
        }
        let (inner, nav_signal, internal_fns) = create_runtime_with_signals(dom, options);
        Self::from_parts(inner, nav_signal, internal_fns)
    }

    fn from_parts(
        inner: JsRuntime,
        nav_signal: NavSignal,
        internal_fns: RuntimeInternalFns,
    ) -> Self {
        Self {
            inner,
            nav_signal,
            frame_deliver_fn: None,
            set_current_script_fn: internal_fns.set_current_script,
            complete_lifecycle_fn: internal_fns.complete_document_lifecycle,
        }
    }

    /// Returns true iff JS has set a pending navigation since the last
    /// reset. Cheap (atomic load); safe to poll from the event loop.
    pub fn nav_pending(&self) -> bool {
        self.nav_signal.pending()
    }

    /// Reset the pending-navigation flag. Called by the event loop after
    /// it has acted on the signal (e.g., before starting a fresh iteration).
    pub fn reset_nav_pending(&self) {
        self.nav_signal.reset();
    }

    /// `Notify` that fires the instant JS raises a pending navigation, waking the
    /// event-loop driver immediately.
    pub fn nav_notify(&self) -> std::sync::Arc<tokio::sync::Notify> {
        self.nav_signal.notify()
    }

    /// Set `document.currentScript` through a private bootstrap closure that is
    /// retained after the page-visible bridge has been deleted.
    pub fn set_current_script(&mut self, node_id: Option<u32>) {
        let Some(function) = self.set_current_script_fn.clone() else {
            return;
        };
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let __ctx = self.inner.main_context();
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        v8::scope_with_context!(scope, self.inner.v8_isolate(), __ctx);
        let function = v8::Local::new(scope, &function);
        let receiver = v8::undefined(scope).into();
        let argument: v8::Local<v8::Value> = match node_id {
            Some(id) => v8::Integer::new_from_unsigned(scope, id).into(),
            None => v8::null(scope).into(),
        };
        let _ = function.call(scope, receiver, &[argument]);
    }

    /// Advance the document through the trusted browser lifecycle sequence.
    pub fn complete_document_lifecycle(&mut self) {
        let Some(function) = self.complete_lifecycle_fn.clone() else {
            return;
        };
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let __ctx = self.inner.main_context();
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        v8::scope_with_context!(scope, self.inner.v8_isolate(), __ctx);
        let function = v8::Local::new(scope, &function);
        let receiver = v8::undefined(scope).into();
        let _ = function.call(scope, receiver, &[]);
    }

    /// Block until a CDP frontend connects, then pause at the next statement
    /// (`--inspect-brk`). No-op if the inspector wasn't started; blocks the thread.
    pub fn inspector_break_on_next_statement(&mut self) {
        let has_inspector = self
            .inner
            .op_state()
            .borrow()
            .try_borrow::<crate::js_runtime::inspector::InspectorServer>()
            .is_some();
        if !has_inspector {
            return;
        }
        let _guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        self.inner
            .inspector()
            .wait_for_session_and_break_on_next_statement();
    }

    /// Get a thread-safe handle to the V8 isolate. Used to call
    /// `terminate_execution()` from a watcher thread when a wall-clock
    /// deadline expires — preempts CPU-bound JS spin loops that
    /// `tokio::time::timeout` cannot interrupt because they never yield
    /// to the tokio scheduler. The returned handle is `Send + Sync`.
    pub fn isolate_handle(&mut self) -> deno_core::v8::IsolateHandle {
        self.inner.v8_isolate().thread_safe_handle()
    }

    /// Run the V8 microtask queue to completion, matching the boundary between
    /// two `<script>`s: promise `.then`s resolve but `setTimeout`/`fetch` do not.
    pub fn drain_microtasks(&mut self) {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let _guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        self.inner.v8_isolate().perform_microtask_checkpoint();
    }

    /// Cancel a previously-issued `terminate_execution()`. Required if
    /// you want the runtime to be usable for further script execution
    /// after a deadline fired. Without this, the next `execute_script`
    /// returns "Uncaught Error: execution terminated".
    pub fn cancel_terminate_execution(&mut self) {
        self.inner.v8_isolate().cancel_terminate_execution();
    }

    /// V8's `used_heap_size` for this isolate, in bytes.
    ///
    /// Intended for monitoring warm reuse: pair with [`Self::collect_garbage`]
    /// and sample after each navigation. On a healthy pool the value is flat
    /// across navigations; a monotonic climb means something is retaining the
    /// previous page (see `Page::reset_for_reuse`).
    ///
    /// Note this is V8 heap only — it excludes external/`ArrayBuffer` backing
    /// stores and everything Rust-side, so it is not process RSS.
    pub fn v8_heap_used_bytes(&mut self) -> usize {
        self.inner
            .v8_isolate()
            .get_heap_statistics()
            .used_heap_size()
    }

    /// Ask V8 to perform a full garbage collection.
    ///
    /// Only meaningful for measurement: call it before
    /// [`Self::v8_heap_used_bytes`] so the reading reflects *live* (reachable)
    /// objects rather than not-yet-collected garbage. Without it, heap-growth
    /// numbers are dominated by GC scheduling noise. Not a correctness tool —
    /// never call it on a hot path.
    pub fn collect_garbage(&mut self) {
        let _guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        self.inner.v8_isolate().low_memory_notification();
    }

    /// Execute a JavaScript script and return the string representation of the result.
    ///
    /// Uses V8 directly in a single HandleScope — avoids the overhead of
    /// deno_core's `execute_script` (which allocates a Global handle) and
    /// a second `handle_scope()` call for stringification.
    pub fn execute_script(
        &mut self,
        code: &str,
        name: Option<&str>,
    ) -> Result<String, deno_core::error::AnyError> {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let __ctx = self.inner.main_context();
        // v8-149 + deno_core 0.403: a V8 isolate is entered (made the
        // thread-current isolate) when its `OwnedIsolate` is constructed and
        // only exited when dropped — the per-call scope macros no longer
        // enter/exit the isolate. browser_oxide runs MULTIPLE live isolates on
        // one thread (the page plus a separate isolate per child iframe; see
        // `crates/browser/src/iframe.rs`). Whichever isolate was constructed
        // most recently is the thread-current one, so calling `execute_script`
        // on a *different* runtime would make `scope_with_context!`'s
        // `ContextScope::new` panic ("… do not belong to the same Isolate").
        // Re-enter this runtime's own isolate for the duration of the call so
        // the scope/context we build always match the thread-current isolate.
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        deno_core::v8::scope_with_context!(scope, self.inner.v8_isolate(), __ctx);
        let source = deno_core::v8::String::new(scope, code)
            .ok_or_else(|| deno_core::error::AnyError::msg("failed to create V8 string"))?;

        let mut script_origin = None;
        if let Some(n) = name {
            let n_v8 = deno_core::v8::String::new(scope, n).unwrap();
            let resource_name = n_v8.into();
            script_origin = Some(deno_core::v8::ScriptOrigin::new(
                scope,
                resource_name,
                0,
                0,
                false,
                0,
                None,
                false,
                false,
                false,
                None,
            ));
        }

        deno_core::v8::tc_scope!(let tc_scope, scope);
        let script = deno_core::v8::Script::compile(tc_scope, source, script_origin.as_ref())
            .ok_or_else(|| {
                let exception = match tc_scope.exception() {
                    Some(exc) => exc,
                    None => return deno_core::error::AnyError::msg("script compilation failed"),
                };
                let msg = exception
                    .to_string(tc_scope)
                    .map(|s| s.to_rust_string_lossy(tc_scope))
                    .unwrap_or_default();
                deno_core::error::AnyError::msg(msg)
            })?;
        match script.run(tc_scope) {
            Some(value) => Ok(value
                .to_string(tc_scope)
                .map(|s| s.to_rust_string_lossy(tc_scope))
                .unwrap_or_default()),
            None => {
                let exception = match tc_scope.exception() {
                    Some(exc) => exc,
                    None => return Err(deno_core::error::AnyError::msg("script execution failed")),
                };
                let msg = exception
                    .to_string(tc_scope)
                    .map(|s| s.to_rust_string_lossy(tc_scope))
                    .unwrap_or_default();
                Err(deno_core::error::AnyError::msg(msg))
            }
        }
    }

    /// Return the browser-visible child realm associated with an iframe host
    /// node. This mapping is populated by `op_create_child_realm` when
    /// `iframe.contentWindow` is first materialized.
    pub fn child_realm_id_for_node(&self, node_id: u32) -> Option<u32> {
        let state = self.inner.op_state();
        let state = state.borrow();
        state
            .try_borrow::<native_fns::IframeRealmStore>()
            .and_then(|store| store.node_to_realm.get(&node_id).copied())
    }

    /// Execute JavaScript in the exact V8 context backing an iframe's
    /// `contentWindow` and stringify the result.
    pub fn execute_child_realm_script(
        &mut self,
        realm_id: u32,
        code: &str,
        name: Option<&str>,
    ) -> Result<String, deno_core::error::AnyError> {
        // Clone the OpState handle before borrowing the V8 isolate mutably.
        // Using `self.inner.op_state()` after `scope_with_context!` would
        // overlap immutable and mutable borrows of the runtime.
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let op_state_rc = self.inner.op_state();
        let child_ctx = {
            let state = op_state_rc.borrow();
            let store = state
                .try_borrow::<native_fns::IframeRealmStore>()
                .ok_or_else(|| deno_core::error::AnyError::msg("iframe realm store missing"))?;
            store.contexts.get(&realm_id).cloned().ok_or_else(|| {
                deno_core::error::AnyError::msg(format!(
                    "iframe realm {realm_id} is no longer alive"
                ))
            })?
        };

        let __ctx = child_ctx;
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        v8::scope_with_context!(scope, self.inner.v8_isolate(), __ctx);
        let source = v8::String::new(scope, code)
            .ok_or_else(|| deno_core::error::AnyError::msg("failed to create V8 string"))?;

        let mut script_origin = None;
        if let Some(resource_name) = name {
            let resource_name = v8::String::new(scope, resource_name)
                .ok_or_else(|| deno_core::error::AnyError::msg("invalid script name"))?;
            script_origin = Some(v8::ScriptOrigin::new(
                scope,
                resource_name.into(),
                0,
                0,
                false,
                0,
                None,
                false,
                false,
                false,
                None,
            ));
        }

        v8::tc_scope!(let tc_scope, scope);
        let script =
            v8::Script::compile(tc_scope, source, script_origin.as_ref()).ok_or_else(|| {
                let message = tc_scope
                    .exception()
                    .and_then(|exception| exception.to_string(tc_scope))
                    .map(|value| value.to_rust_string_lossy(tc_scope))
                    .unwrap_or_else(|| "child-realm script compilation failed".to_string());
                deno_core::error::AnyError::msg(message)
            })?;
        {
            let mut state = op_state_rc.borrow_mut();
            if let Some(store) = state.try_borrow_mut::<native_fns::IframeRealmStore>() {
                store.execution_stack.push(realm_id);
            }
        }
        let run_value = script.run(tc_scope);
        // HTML performs a microtask checkpoint at the end of each script task.
        // Run it while the child realm is still on `execution_stack` so Promise
        // callbacks observe the correct incumbent sender for postMessage and
        // other realm-sensitive APIs.
        let current_ctx = tc_scope.get_current_context();
        let queue = current_ctx.get_microtask_queue();
        queue.perform_checkpoint(tc_scope);
        {
            let mut state = op_state_rc.borrow_mut();
            if let Some(store) = state.try_borrow_mut::<native_fns::IframeRealmStore>() {
                let popped = store.execution_stack.pop();
                debug_assert_eq!(popped, Some(realm_id));
            }
        }
        let value = run_value.ok_or_else(|| {
            let message = tc_scope
                .exception()
                .and_then(|exception| exception.to_string(tc_scope))
                .map(|value| value.to_rust_string_lossy(tc_scope))
                .unwrap_or_else(|| "child-realm script execution failed".to_string());
            deno_core::error::AnyError::msg(message)
        })?;
        Ok(value
            .to_string(tc_scope)
            .map(|value| value.to_rust_string_lossy(tc_scope))
            .unwrap_or_default())
    }

    /// Remove every same-isolate iframe realm. Called before replacing the DOM
    /// so a warm navigation cannot retain old WindowProxy/Document graphs.
    pub fn clear_child_realms(&mut self) {
        let state = self.inner.op_state();
        let mut state = state.borrow_mut();
        if let Some(store) = state.try_borrow_mut::<native_fns::IframeRealmStore>() {
            store.contexts.clear();
            store.globals.clear();
            store.inner_globals.clear();
            store.node_to_realm.clear();
            store.realm_to_node.clear();
            store.window_proxies.clear();
            store.public_windows.clear();
            store.property_bridges.clear();
            store.execution_stack.clear();
        }
    }

    /// Run the V8 event loop until all pending work is done.
    pub async fn run_event_loop(&mut self) -> Result<(), deno_core::error::AnyError> {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        // v8-149: re-enter this runtime's own isolate so driving the event
        // loop (which runs JS, microtasks, and ops that build scopes) targets
        // the correct thread-current isolate even when a child-iframe runtime
        // was constructed more recently and made *its* isolate current. See
        // the long note in `execute_script`. Without this, sites that spawn
        // iframes/workers crash with the scope.rs "not the same Isolate" panic.
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        self.inner
            .run_event_loop(deno_core::PollEventLoopOptions::default())
            .await
            .map_err(|e| deno_core::error::AnyError::msg(e.to_string()))
    }

    /// Poll this runtime's event loop once with the driver's context, so the
    /// waker in `cx` becomes the wake target for this isolate's timers and ops.
    pub fn poll_once(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), deno_core::error::AnyError>> {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        self.inner
            .poll_event_loop(cx, deno_core::PollEventLoopOptions::default())
            .map_err(|e| deno_core::error::AnyError::msg(e.to_string()))
    }

    /// Cache `globalThis.__pumpFrameMessages` as a compiled function for direct
    /// calls. No-op if the symbol is absent.
    fn capture_frame_deliver_fn(&mut self) {
        let __ctx = self.inner.main_context();
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        deno_core::v8::scope_with_context!(scope, self.inner.v8_isolate(), __ctx);
        let ctx = scope.get_current_context();
        let global = ctx.global(scope);
        let Some(key) = deno_core::v8::String::new(scope, "__pumpFrameMessages") else {
            return;
        };
        let Some(val) = global.get(scope, key.into()) else {
            return;
        };
        if let Ok(func) = deno_core::v8::Local::<deno_core::v8::Function>::try_from(val) {
            self.frame_deliver_fn = Some(deno_core::v8::Global::new(scope, func));
        }
    }

    /// Deliver any queued cross-frame messages into this runtime by calling the
    /// cached `__pumpFrameMessages` (compiled once). Lazily captures it.
    pub fn deliver_frame_messages(&mut self) {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        if self.frame_deliver_fn.is_none() {
            self.capture_frame_deliver_fn();
        }
        let Some(func) = self.frame_deliver_fn.clone() else {
            return;
        };
        let __ctx = self.inner.main_context();
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        deno_core::v8::scope_with_context!(scope, self.inner.v8_isolate(), __ctx);
        let f = deno_core::v8::Local::new(scope, &func);
        deno_core::v8::tc_scope!(let tc_scope, scope);
        let recv = deno_core::v8::undefined(tc_scope).into();
        let _ = f.call(tc_scope, recv, &[]);
    }

    /// P2 — load + evaluate an EXTERNAL ES module (`<script type="module" src>`).
    /// The configured `BrowserModuleLoader` fetches the import graph on demand;
    /// we drive the event loop so those async fetches + top-level async work
    /// resolve. Returns Err for the caller to log — a throwing/failing module
    /// must NOT blank the page (matches classic-script handling).
    pub async fn load_eval_module_url(
        &mut self,
        url: &str,
    ) -> Result<(), deno_core::error::AnyError> {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let spec = deno_core::ModuleSpecifier::parse(url)
            .map_err(|e| deno_core::error::AnyError::msg(format!("module url {url}: {e}")))?;
        // v8-149: see `run_event_loop` — module loading drives V8 and must
        // target this runtime's isolate, not a more-recently-entered child's.
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        let mod_id = self.inner.load_main_es_module(&spec).await?;
        self.eval_module(mod_id).await
    }

    /// P2 — load + evaluate an INLINE ES module. `specifier` must be a unique
    /// URL whose path is the document URL (e.g. `https://site/p#oxide-mod-3`) so
    /// relative `import`s resolve against the document while staying distinct
    /// from other inline modules on the page.
    pub async fn load_eval_module_code(
        &mut self,
        specifier: &str,
        code: String,
    ) -> Result<(), deno_core::error::AnyError> {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        let spec = deno_core::ModuleSpecifier::parse(specifier).map_err(|e| {
            deno_core::error::AnyError::msg(format!("inline module spec {specifier}: {e}"))
        })?;
        // v8-149: see `run_event_loop` — module loading drives V8 and must
        // target this runtime's isolate, not a more-recently-entered child's.
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        let mod_id = self
            .inner
            .load_main_es_module_from_code(&spec, code)
            .await?;
        self.eval_module(mod_id).await
    }

    async fn eval_module(
        &mut self,
        mod_id: deno_core::ModuleId,
    ) -> Result<(), deno_core::error::AnyError> {
        let _tokio_guard = tokio_fallback::ensure_tokio_context();
        // v8-149: see `run_event_loop` — mod_evaluate + the loop drive run on
        // this runtime's isolate; re-enter it in case a child is current.
        let _isolate_guard = IsolateEnterGuard::enter(self.inner.v8_isolate());
        let eval = self.inner.mod_evaluate(mod_id);
        // Drive the loop so the loader's async fetches + any top-level await
        // resolve, THEN await the module's evaluation result.
        self.inner
            .run_event_loop(deno_core::PollEventLoopOptions::default())
            .await
            .map_err(|e| deno_core::error::AnyError::msg(e.to_string()))?;
        eval.await
            .map_err(|e| deno_core::error::AnyError::msg(e.to_string()))
    }

    /// Get console output captured so far.
    pub fn console_output(&mut self) -> Vec<ConsoleMessage> {
        let state = self.inner.op_state();
        let state = state.borrow();
        state.borrow::<DomState>().console_output.clone()
    }

    /// Replace the DOM in this runtime with a new one.
    /// Used for CDP Page.navigate to avoid recreating the V8 isolate.
    pub fn replace_dom(&mut self, dom: Dom, stylesheets: Vec<String>) {
        // The bootstrap (and thus __pumpFrameMessages) is re-installed here;
        // drop the cached handle so it is re-captured against the fresh function.
        self.frame_deliver_fn = None;
        self.clear_child_realms();
        let state = self.inner.op_state();
        let mut state = state.borrow_mut();
        // Replace DomState — ops will pick up the new DOM on next call
        let mut dom_state = DomState::new(dom);
        dom_state.stylesheets = stylesheets;
        dom_state.update_cached_rules();
        state.put(dom_state);
        // Reset timer state (clear pending timers from old page)
        state.put(extensions::timer_ext::TimerState::new());
    }

    /// Take the DOM out of the runtime (consumes self).
    pub fn take_dom(self) -> Dom {
        let state = self.inner.op_state();
        let mut state = state.borrow_mut();
        state.take::<DomState>().dom
    }

    /// Snapshot the current localStorage and sessionStorage contents.
    /// Used by the navigation loop to carry storage across same-origin reloads.
    pub fn get_storage(
        &mut self,
    ) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
        let state = self.inner.op_state();
        let state = state.borrow();
        state.borrow::<DomState>().storage.clone()
    }

    /// Get the inner deno_core JsRuntime.
    pub fn inner(&mut self) -> &mut JsRuntime {
        &mut self.inner
    }

    /// Get the OpState (shared state).
    pub fn op_state(&self) -> std::rc::Rc<std::cell::RefCell<deno_core::OpState>> {
        self.inner.op_state()
    }

    /// Clone this runtime's frame-tree signal, the queue of `<iframe src>` inserts
    /// the JS requested. The frame-tree driver drains it to materialize children.
    pub fn iframe_signal(&mut self) -> crate::js_runtime::extensions::frame_ext::IframeSignal {
        self.inner
            .op_state()
            .borrow()
            .borrow::<crate::js_runtime::extensions::frame_ext::IframeSignal>()
            .clone()
    }

    pub fn record_resource_timing(&mut self, timings: crate::net::TimingStats) {
        let op_state = self.inner.op_state();
        let mut state = op_state.borrow_mut();
        extensions::fetch_ext::record_resource_timing(&mut state, timings);
    }
}
