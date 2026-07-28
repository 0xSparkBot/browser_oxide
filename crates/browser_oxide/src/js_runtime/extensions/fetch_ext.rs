use crate::js_runtime::state::DomState;
use deno_core::op2;
use deno_core::OpState;
use serde::Serialize;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use url::Url;

/// Per-page sync-fetch chain ceiling. Without this, sites like
/// delta.com and taobao.com cascade nested document.write(<script src>)
/// + setTimeout-driven JSONP polls indefinitely, holding the V8 worker
/// thread for minutes and starving tokio of yield points.
///
/// 30 is comfortable: typical challenge/handshake script chains fit in
/// <10 sync fetches, leaving headroom for legitimate inline scripts.
const MAX_SYNC_FETCH_PER_PAGE: usize = 30;
thread_local! {
    static SYNC_FETCH_COUNT: Cell<usize> = const { Cell::new(0) };
}

/// Reset the per-page sync-fetch counter. Called by Page::navigate_with_init
/// at the start of each navigation iteration.
pub fn reset_sync_fetch_count() {
    SYNC_FETCH_COUNT.with(|c| c.set(0));
}

pub fn record_resource_timing(state: &mut OpState, timings: crate::net::TimingStats) {
    if let Some(dom_state) = state.try_borrow_mut::<DomState>() {
        dom_state.resource_timings.push(timings);
    }
}

/// HTTP client state stored in OpState.
pub struct FetchState {
    pub client: Option<crate::net::HttpClient>,
}

impl FetchState {
    pub fn new(client: Option<crate::net::HttpClient>) -> Self {
        Self { client }
    }

    pub fn with_profile(profile: &crate::stealth::StealthProfile) -> Self {
        Self {
            client: crate::net::HttpClient::new(profile).ok(),
        }
    }
}

// Per-thread fetch client, initialized from the stealth profile when a
// Page is constructed. Thread-local (not process-global) so concurrent
// `ParallelPager` workers don't clobber each other's HttpClient + cookie
// jar — each worker owns its own V8 isolate on a dedicated OS thread,
// and that's the natural scope for the fetch state too. Before this was
// a `OnceLock<HttpClient>`, two parallel workers caused the SECOND
// worker's JS `fetch()` to go through the FIRST worker's HttpClient
// (with the first site's cookies), which silently corrupted XHR-driven
// SPA hydration on yandex / reddit / amazon / zara / yandex-ru / etc.
thread_local! {
    static FETCH_CLIENT: RefCell<Option<crate::net::HttpClient>> = const { RefCell::new(None) };
}

// Per-thread active CSP policy + origin. Same thread-local rationale as
// FETCH_CLIENT: concurrent parallel workers were overwriting each other,
// so worker B's fetches were enforced against worker A's policy.
thread_local! {
    static ACTIVE_CSP: RefCell<Option<ActiveCsp>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct ActiveCsp {
    policy: std::sync::Arc<crate::net::csp::PolicySet>,
    origin: Url,
    enforce: bool,
}

/// Install a CSP policy + origin for the current navigation. `enforce`
/// is wired from `profile.enforce_csp` and the `BROWSER_OXIDE_CSP_BYPASS=1`
/// escape hatch. Called by `Page::navigate_with_init` after parsing
/// the response headers + meta tags.
///
/// Drains the violation queue at install time — violations from the
/// previous document are no longer relevant once a new navigation
/// installs its own policy. Real Chrome resets the violation list per
/// top-level navigation; this matches that behaviour.
pub fn set_csp_policy(
    policy: std::sync::Arc<crate::net::csp::PolicySet>,
    origin: Url,
    enforce: bool,
) {
    CSP_VIOLATIONS.with(|q| q.borrow_mut().clear());
    ACTIVE_CSP.with(|c| {
        *c.borrow_mut() = Some(ActiveCsp {
            policy,
            origin,
            enforce,
        });
    });
}

/// Clear any active CSP. Called between top-level navigations so a
/// strict policy from site A doesn't leak into site B. Also drains
/// any queued violations — they belong to the previous document.
pub fn clear_csp_policy() {
    CSP_VIOLATIONS.with(|q| q.borrow_mut().clear());
    ACTIVE_CSP.with(|c| *c.borrow_mut() = None);
}

/// Returns `Err(blocked_directive)` when the active policy denies the
/// fetch; `Ok(())` when allowed (no policy installed, or matched).
/// On block, pushes a record onto the per-runtime violation queue so
/// JS can later dispatch `securitypolicyviolation` events for each.
pub fn check_csp(
    directive: crate::net::csp::Directive,
    url: &Url,
    nonce: Option<&str>,
    parser_inserted: bool,
) -> Result<(), &'static str> {
    let decision = ACTIVE_CSP.with(|c| {
        let guard = c.borrow();
        let active = guard.as_ref()?;
        if !active.enforce {
            return None;
        }
        let ctx = crate::net::csp::CheckCtx {
            directive,
            url,
            page_origin: &active.origin,
            nonce,
            parser_inserted,
        };
        Some(active.policy.allows(&ctx))
    });
    let Some(decision) = decision else {
        return Ok(());
    };
    if decision.allowed {
        Ok(())
    } else {
        let dir_name = decision.matched_directive.as_str();
        push_csp_violation(CspViolation {
            blocked_uri: url.as_str().to_string(),
            effective_directive: dir_name.to_string(),
            violated_directive: dir_name.to_string(),
            disposition: "enforce".to_string(),
        });
        Err(dir_name)
    }
}

// ---------------------------------------------------------------------
// Violation queue — the gates push, JS drains via `op_drain_csp_violations`
// and dispatches `securitypolicyviolation` events. We keep the queue
// process-global next to ACTIVE_CSP because the gates run from
// non-op call sites (page.rs build_page_with_scripts) where there's no
// OpState handle.
// ---------------------------------------------------------------------

#[derive(Clone, serde::Serialize)]
pub struct CspViolation {
    #[serde(rename = "blockedURI")]
    pub blocked_uri: String,
    #[serde(rename = "effectiveDirective")]
    pub effective_directive: String,
    #[serde(rename = "violatedDirective")]
    pub violated_directive: String,
    pub disposition: String,
}

thread_local! {
    static CSP_VIOLATIONS: RefCell<Vec<CspViolation>> = const { RefCell::new(Vec::new()) };
}

fn push_csp_violation(v: CspViolation) {
    CSP_VIOLATIONS.with(|q| {
        let mut q = q.borrow_mut();
        // Cap queue at 256 to avoid unbounded growth on pathological
        // scripts that retry blocked fetches in a loop.
        if q.len() < 256 {
            q.push(v);
        }
    });
}

/// JS-callable drain. Returns the queue contents and clears it.
/// Caller iterates and dispatches one `securitypolicyviolation` event
/// per item on `document` and `window`.
#[op2]
#[serde]
pub fn op_drain_csp_violations() -> Vec<CspViolation> {
    CSP_VIOLATIONS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

// ---------------------------------------------------------------------
// Mixed-content gate — the fetch path's half of `net::mixed_content`.
// The module decides; this decides what the two call sites do about it,
// and is a named function rather than inline code at each site so both
// stay reachable from a test (an `#[op2]` body is generated inside a
// `const fn` and cannot be called directly).
// ---------------------------------------------------------------------

/// What a call site should do once the mixed-content check has spoken.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MixedContentGate {
    /// Go ahead, using this URL — which may have been upgraded from `http:`
    /// to `https:` and is therefore not always the URL that was asked for.
    Proceed(String),
    /// Do not fetch. Each call site returns whatever "nothing loaded" looks
    /// like in its own shape; the two differ, so the gate does not decide it.
    Refuse,
}

/// The page URL to judge a `window.fetch()` against.
///
/// `fetch_bootstrap.js` stamps `x-browser-oxide-origin` from
/// `location.origin` so the net layer can compute `sec-fetch-site`; an origin
/// is all `mixed_content::check` inspects, so it doubles as the document URL
/// here. It is absent only where there is no `location` to read — the worker
/// bootstrap, and `about:blank` — and there `check` sees an empty page URL
/// and allows.
///
/// That residual gap is real. Nothing else in `op_fetch` can supply the
/// document URL: an `op2(async)` future cannot borrow `OpState` to reach
/// `DomState::base_url` (the same limitation that leaves
/// `record_resource_timing` uncalled there), and `ACTIVE_CSP.origin` is
/// cleared on every page that ships no policy, so leaning on it would
/// silently stop protecting most of the web while looking like it worked.
fn document_origin_hint(headers: &HashMap<String, String>) -> &str {
    headers
        .get("x-browser-oxide-origin")
        .map(|s| s.as_str())
        .unwrap_or("")
}

/// Apply `net::mixed_content` to one request. `label` names the call site in
/// the console line, because "which of the two fetch paths refused this" is
/// the first thing anyone debugging a missing subresource wants to know.
fn mixed_content_gate(
    page_url: &str,
    url: &str,
    request_type: &str,
    label: &str,
) -> MixedContentGate {
    match crate::net::mixed_content::check(page_url, url, request_type) {
        crate::net::mixed_content::Verdict::Allow => MixedContentGate::Proceed(url.to_string()),
        crate::net::mixed_content::Verdict::Upgrade => {
            MixedContentGate::Proceed(crate::net::mixed_content::upgrade(url))
        }
        crate::net::mixed_content::Verdict::Block => {
            eprintln!(
                "[mixed-content] Blocked loading mixed active content '{url}' ({label}) on secure page '{page_url}'."
            );
            MixedContentGate::Refuse
        }
    }
}

/// Initialize the shared fetch client from a profile.
/// Call this once during runtime setup.
pub fn init_fetch_client(profile: &crate::stealth::StealthProfile) {
    if let Ok(client) = crate::net::HttpClient::new(profile) {
        FETCH_CLIENT.with(|c| *c.borrow_mut() = Some(client));
    }
}

/// Set the shared fetch client to an existing HttpClient.
/// Used by Page::navigate_with_init to share cookies between the
/// navigation client and the JS fetch() calls. Thread-local so each
/// ParallelPager worker thread has its own slot.
pub fn set_fetch_client(client: crate::net::HttpClient) {
    FETCH_CLIENT.with(|c| *c.borrow_mut() = Some(client));
}

/// Clone of the shared fetch client, if one has been installed.
/// Used by the worker `importScripts` synchronous fetch path in
/// `worker_ext::op_worker_sync_fetch`.
pub fn fetch_client() -> Option<crate::net::HttpClient> {
    FETCH_CLIENT.with(|c| c.borrow().clone())
}

#[derive(Serialize)]
pub struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String,
    pub ok: bool,
}

/// Async fetch op. Uses the profile-configured client (with proxy, TLS
/// emulation, cookies) when available, falling back to a default Chrome 130.
///
/// The JS side sends the body as a base64 string in the `body` parameter to
/// preserve binary data (some challenge scripts POST
/// `application/octet-stream` with a raw byte payload). The first character
/// of `body` is a marker: 's' for plain UTF-8 string bodies, 'b' for
/// base64-encoded binary bodies. This keeps the op signature stable as a
/// `#[string]` while supporting binary POSTs.
// Use `async(deferred)`, not `async(lazy)`: `lazy` never eager-polls the
// future, so the JS promise only settles a full event-loop turn later. That
// extra turn shifts the observable timing of `fetch()` relative to real
// Chrome — pages that self-schedule work and measure `performance.now()`
// deltas see a different async-settle cadence. `deferred` eager-polls the
// future while staying off the synchronous fast path, matching Chrome's
// settle timing.
#[op2(async(deferred))]
#[serde]
pub async fn op_fetch(
    #[string] url: String,
    #[string] method: String,
    #[serde] headers: HashMap<String, String>,
    #[string] body: String,
) -> Result<FetchResponse, deno_error::JsErrorBox> {
    // CSP `connect-src` enforcement — `window.fetch()` and XHR both
    // route through this op. Real Chrome blocks fetches that violate
    // the active policy by returning a 0-status, opaque, network-error
    // response. We mirror that shape so JS code's
    // `try { await fetch(...) } catch (e) { ... }` path fires the same
    // way it would in Chrome.
    if let Ok(parsed) = Url::parse(&url) {
        if let Err(violated) =
            check_csp(crate::net::csp::Directive::ConnectSrc, &parsed, None, false)
        {
            eprintln!(
                "[csp] Refused to connect to '{}' because it violates the following Content Security Policy directive: \"{}\".",
                url, violated
            );
            return Ok(FetchResponse {
                status: 0,
                status_text: "".to_string(),
                headers: HashMap::new(),
                body: String::new(),
                url: url.clone(),
                ok: false,
            });
        }
    }

    // Resource blocker — short-circuit ad/tracker requests before TLS+JS.
    // Empty source_url is OK; the JS layer doesn't currently pass the page
    // origin here, but adblock's first-party rules degrade gracefully.
    let request_type = crate::net::blocker::classify_request_type(
        &url,
        headers
            .get("x-browser-oxide-request-type")
            .map(|s| s.as_str()),
    );
    if crate::net::blocker::should_block(&url, "", request_type) {
        return Ok(FetchResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body: String::new(),
            url: url.clone(),
            ok: true,
        });
    }

    // Mixed content — decided before any TLS work, because the whole point is
    // that the plaintext hop must never happen.
    let url = match mixed_content_gate(document_origin_hint(&headers), &url, request_type, "fetch")
    {
        MixedContentGate::Proceed(u) => u,
        MixedContentGate::Refuse => {
            // Same empty-but-OK shape the blocker returns above, not the
            // 0-status CSP shape: what a page can observe of a refused
            // subresource should not be richer than what Chrome's opaque
            // network-error response tells it.
            return Ok(FetchResponse {
                status: 200,
                status_text: "OK".to_string(),
                headers: HashMap::new(),
                body: String::new(),
                url,
                ok: true,
            });
        }
    };

    // Clone the thread-local client out so we don't hold the RefCell borrow
    // across awaits below. Each ParallelPager worker has its own slot.
    let installed_client = FETCH_CLIENT.with(|c| c.borrow().clone());
    let default_client;
    let client = match installed_client.as_ref() {
        Some(c) => c,
        None => {
            let profile = crate::stealth::chrome_148_linux();
            default_client = crate::net::HttpClient::new(&profile)
                .map_err(|e| deno_error::JsErrorBox::generic(e.to_string()))?;
            &default_client
        }
    };

    // Pull JS-provided headers. JS may pass "x-browser-oxide-origin" as a pseudo
    // header carrying the page's origin; strip it here and forward as the
    // origin context so the net layer can compute sec-fetch-site correctly.
    let mut extra_headers: Vec<(String, String)> = Vec::with_capacity(headers.len());
    let mut origin: Option<String> = None;
    for (k, v) in headers.into_iter() {
        let lk = k.to_ascii_lowercase();
        if lk == "x-browser-oxide-origin" {
            origin = Some(v);
            continue;
        }
        extra_headers.push((lk, v));
    }

    // Decode the body marker. Legacy callers that don't set a marker send
    // plain UTF-8 strings; we treat those as 's' by default.
    let body_bytes: Vec<u8> = if let Some(rest) = body.strip_prefix("b:") {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(rest.as_bytes())
            .unwrap_or_default()
    } else if let Some(rest) = body.strip_prefix("s:") {
        rest.as_bytes().to_vec()
    } else {
        body.as_bytes().to_vec()
    };

    // Use fetch-API-style headers (accept: */*, sec-fetch-dest: empty, no
    // upgrade-insecure-requests) — this is a JS fetch() call, not a navigation.
    // The nav-vs-fetch header distinction is observable, so a real fetch()
    // must send fetch-style headers (not navigation headers) here.
    let method_upper = method.to_uppercase();

    // Apply a 30-second hard timeout so hanging connections (e.g. a server
    // that black-holes requests it dislikes) don't hold the V8 event loop
    // open indefinitely.
    let fetch_timeout = std::time::Duration::from_secs(30);
    let resp_result = tokio::time::timeout(fetch_timeout, async {
        match method_upper.as_str() {
            "POST" | "PUT" | "PATCH" => {
                client
                    .fetch_post_bytes(&url, &body_bytes, &extra_headers, origin.as_deref())
                    .await
            }
            _ => {
                client
                    .fetch_get(&url, &extra_headers, origin.as_deref())
                    .await
            }
        }
    })
    .await;
    let resp = match resp_result {
        Ok(r) => r,
        Err(_) => {
            return Err(deno_error::JsErrorBox::generic(format!(
                "fetch timeout after {}s: {}",
                fetch_timeout.as_secs(),
                url
            )));
        }
    };

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return Err(deno_error::JsErrorBox::generic(e.to_string())),
    };

    let ok = resp.ok();
    let body_text = resp.text();

    let final_resp = FetchResponse {
        status: resp.status,
        status_text: resp.status_text.clone(),
        headers: resp.headers.clone(),
        body: body_text,
        url: resp.url.clone(),
        ok,
    };

    // record_resource_timing is sync (uses try_borrow_mut), so it's safe to call here.
    // However, op_fetch is an async op; we need access to OpState.
    // In deno_core 0.311, op2(async(lazy)) can't easily borrow &mut OpState from its future.
    // Instead, we use the process-global DomState if accessible, or we'll just return it.
    // For now, let's keep it simple: we need to find where the OpState is for this isolate.

    Ok(final_resp)
}

/// Get the cookie string for a URL from the shared HTTP client's cookie jar.
/// Returns "name=value; name2=value2" — the format document.cookie expects.
#[op2(async(lazy), fast)]
#[string]
pub async fn op_cookie_get(#[string] url: String) -> String {
    let Some(client) = FETCH_CLIENT.with(|c| c.borrow().clone()) else {
        return String::new();
    };
    let Ok(parsed) = Url::parse(&url) else {
        return String::new();
    };
    client.cookies_for_url(&parsed).await.unwrap_or_default()
}

/// Set a cookie via a raw "name=value; path=/; ..." string, scoped to the URL's origin.
#[op2(async(lazy), fast)]
pub async fn op_cookie_set(#[string] url: String, #[string] cookie: String) {
    let Some(client) = FETCH_CLIENT.with(|c| c.borrow().clone()) else {
        return;
    };
    let Ok(parsed) = Url::parse(&url) else { return };
    client.set_cookie_str(&parsed, &cookie).await;
}

/// Synchronous `document.cookie` write.
/// The async `op_cookie_set` was called fire-and-forget by the
/// `document.cookie` setter, so a write issued in the last microtasks before
/// `location.reload()` (e.g. a challenge token) was torn down before
/// its future ran — the jar stayed empty and the reload re-fetched the stub.
/// This sync op persists immediately via `try_lock` (the jar is never held
/// across an await during synchronous JS execution); under genuine contention
/// it falls back to spawning the async write so nothing is lost.
#[op2(fast)]
pub fn op_cookie_set_sync(#[string] url: String, #[string] cookie: String) {
    let Some(client) = FETCH_CLIENT.with(|c| c.borrow().clone()) else {
        tracing::debug!("op_cookie_set_sync: no FETCH_CLIENT");
        return;
    };
    let Ok(parsed) = Url::parse(&url) else {
        tracing::debug!(url = %url, "op_cookie_set_sync: bad url");
        return;
    };
    let synced = client.set_cookie_str_sync(&parsed, &cookie);
    // Also write to the process-wide shared-session jar: the nav reload/refetch
    // reads cookies from a shared() client, which can diverge from this
    // per-thread FETCH_CLIENT — without this a token a challenge deposits via
    // document.cookie never reached the reload.
    let shared_synced = crate::net::set_shared_cookie_sync(&parsed, &cookie);
    if std::env::var("BROWSER_OXIDE_COOKIE_TRACE").is_ok() {
        let for_url = crate::net::shared_session()
            .cookies
            .try_lock()
            .ok()
            .and_then(|j| j.cookies_for(&parsed))
            .unwrap_or_default();
        eprintln!(
            "[cookie-set-sync] url={url} synced={synced} shared={shared_synced} shared_for_url='{}' cookie={}",
            for_url.chars().take(60).collect::<String>(),
            cookie.chars().take(50).collect::<String>()
        );
    }
    if !synced {
        // Rare contention path: don't lose the write — defer to the async op.
        tokio::task::spawn(async move {
            client.set_cookie_str(&parsed, &cookie).await;
        });
    }
}

/// Synchronous fetch op. Blocks the V8 thread until the request completes.
/// Used by document.write and appendChild(script) when synchronous execution
/// is required.
#[op2]
#[string]
pub fn op_net_fetch_sync(#[string] url: String, #[string] referer: String) -> String {
    // CSP `script-src-elem` enforcement. Sync-fetch is the path
    // `document.write('<script src=...>')` and dynamic
    // `appendChild(script)` use. Real Chrome enforces CSP on these
    // identically to parser-injected scripts. Without a nonce on the
    // dynamically-inserted script (we don't track them today), under
    // strict-dynamic this fetch will block.
    if let Ok(parsed) = Url::parse(&url) {
        if let Err(violated) = check_csp(
            crate::net::csp::Directive::ScriptSrcElem,
            &parsed,
            None,
            false,
        ) {
            eprintln!(
                "[csp] Refused to load the script '{}' (sync-fetch) — violates: \"{}\".",
                url, violated
            );
            return String::new();
        }
    }

    // Resource blocker — return empty body for ad/tracker URLs without
    // doing any HTTP work. Tracker JS that loads via <script src=…>
    // (gtm.js, gpt.js, doubleclick) is the dominant time sink on
    // news/store sites; blocking these saves 1-3 s per site on average.
    let request_type = crate::net::blocker::classify_request_type(&url, Some("script"));
    if crate::net::blocker::should_block(&url, &referer, request_type) {
        return String::new();
    }

    // Mixed content — decided before the chain counter and before any client
    // is built, so a refused script costs nothing and does not burn one of the
    // page's MAX_SYNC_FETCH_PER_PAGE slots.
    //
    // `referer` is the document that ran the `document.write('<script src>')`
    // or `appendChild(script)`, so unlike the async op the page URL is
    // unambiguous here. This path is also the sharpest instance of the
    // problem: everything arriving through it is script, an attacker who
    // rewrites it in flight owns the document, and it executes synchronously
    // the moment it lands.
    let url = match mixed_content_gate(&referer, &url, request_type, "sync-fetch") {
        MixedContentGate::Proceed(u) => u,
        // Empty body, exactly as the blocker returns above — the caller
        // evaluates the result as script, so "" is a script that does nothing
        // rather than a parse error.
        MixedContentGate::Refuse => return String::new(),
    };

    // Per-page chain ceiling — see MAX_SYNC_FETCH_PER_PAGE.
    let n = SYNC_FETCH_COUNT.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    if n >= MAX_SYNC_FETCH_PER_PAGE {
        eprintln!(
            "[op_net_fetch_sync] CHAIN LIMIT ({}) exceeded — returning empty for {}",
            MAX_SYNC_FETCH_PER_PAGE, url
        );
        return String::new();
    }

    tracing::debug!("[op_net_fetch_sync] fetching {}", url);

    // 1. Get a client instance.
    //
    // NOTE: we deliberately build a FRESH client here rather than reuse
    // FETCH_CLIENT. Reason: the V8 op runs on the main tokio runtime's
    // thread (synchronous from JS's perspective). It then std::thread::spawn
    // a new tokio runtime to do the await. If we used the shared
    // FETCH_CLIENT, its pooled HTTP/2 connections — whose reader/writer
    // tasks live on the MAIN runtime — would deadlock because the main
    // runtime is blocked waiting for this op to return. A fresh client
    // with its own connection pool fully owned by the spawned runtime
    // sidesteps the deadlock. We DO read the profile from FETCH_CLIENT
    // so cookies + stealth settings are consistent.
    let main_client = FETCH_CLIENT.with(|c| c.borrow().clone());
    let (_profile, client_res) = match main_client.as_ref() {
        Some(main) => (
            main.profile().clone(),
            crate::net::HttpClient::new_with_shared_state(
                main.profile(),
                main.cookies(),
                main.accept_ch_origins(),
                main.dns_cache(),
                main.alt_svc_cache(),
            ),
        ),
        None => {
            let p = crate::stealth::presets::chrome_148_ru();
            (p.clone(), crate::net::HttpClient::new(&p))
        }
    };
    let client = match client_res {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // 2. Build browser-native headers for a script fetch
    let mut extra_headers = vec![
        ("referer".to_string(), referer.clone()),
        ("sec-fetch-dest".to_string(), "script".to_string()),
        ("sec-fetch-mode".to_string(), "no-cors".to_string()),
        ("sec-fetch-site".to_string(), "same-origin".to_string()),
    ];
    if let Ok(parsed) = Url::parse(&referer) {
        if let Some(origin) = parsed.origin().ascii_serialization().into() {
            extra_headers.push(("origin".to_string(), origin));
        }
    }

    let url_clone = url.clone();
    let result = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("[op_net_fetch_sync] runtime build error: {e}");
                return String::new();
            }
        };
        rt.block_on(async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                client.get_with_headers(&url_clone, &extra_headers),
            )
            .await
            {
                Ok(Ok(resp)) => {
                    let body = resp.text();
                    if body.is_empty() {
                        eprintln!(
                            "[op_net_fetch_sync] empty body for {} (status={})",
                            url_clone, resp.status
                        );
                    } else if url_clone.ends_with(".js") && body.len() > 10000 {
                        let filename = format!("/tmp/fetched_script_{}.js", body.len());
                        let _ = std::fs::write(&filename, &body);
                        eprintln!("[op_net_fetch_sync] saved script to {}", filename);
                    }
                    body
                }
                Ok(Err(e)) => {
                    eprintln!("[op_net_fetch_sync] FAILED fetch {}: {}", url_clone, e);
                    String::new()
                }
                Err(_) => {
                    eprintln!("[op_net_fetch_sync] TIMEOUT fetching {}", url_clone);
                    String::new()
                }
            }
        })
    })
    .join()
    .unwrap_or_default();

    eprintln!(
        "[op_net_fetch_sync] fetched {} bytes from {}",
        result.len(),
        url
    );
    result
}

/// Synchronous XHR op: makes a network request (GET or POST) synchronously,
/// returning a JSON string `{status, headers, body, url}`.
///
/// Used by the XHR polyfill so that synchronous challenge POSTs
/// complete even when V8 is busy with a proof-of-work computation
/// loop that starves the async event loop. Cookies set by the response are
/// written back to the shared FETCH_CLIENT cookie jar.
///
/// Body is marker-prefixed: "s:<utf8>" or "b:<base64>". Empty string = no body.
#[op2]
#[string]
pub fn op_net_xhr_sync(
    #[string] url: String,
    #[string] method: String,
    #[string] headers_json: String,
    #[string] body: String,
    #[string] origin: String,
) -> String {
    // Parse extra headers provided by JS.
    let extra_headers: Vec<(String, String)> =
        serde_json::from_str(&headers_json).unwrap_or_default();

    // Decode the body.
    let body_bytes: Vec<u8> = if let Some(rest) = body.strip_prefix("b:") {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(rest.as_bytes())
            .unwrap_or_default()
    } else if let Some(rest) = body.strip_prefix("s:") {
        rest.as_bytes().to_vec()
    } else if body.is_empty() {
        Vec::new()
    } else {
        body.as_bytes().to_vec()
    };

    // Resolve relative/empty URLs against the document origin. Some
    // challenge scripts issue sync POSTs to a relative path (and to ''
    // = the document URL); if such a url reaches here unresolved (the JS-side
    // XHR open() / fetch resolution can be skipped for empty strings), the
    // network layer's Url::parse rejects it ("relative URL without a base")
    // and the POST silently fails. Resolve here as a robust,
    // path-independent fallback so every such POST lands.
    let url = if url::Url::parse(&url).is_ok() {
        url
    } else if let Ok(base) = url::Url::parse(&origin) {
        base.join(&url).map(|u| u.to_string()).unwrap_or(url)
    } else {
        url
    };

    let url_clone = url.clone();
    let method_upper = method.to_uppercase();
    let origin_str = if origin.is_empty() {
        None
    } else {
        Some(origin)
    };

    // Clone the thread-local client BEFORE spawning a new thread — TLS is
    // per-thread, so the spawned thread sees an empty slot. We pass the
    // main client in by ownership and share its state into a fresh client
    // owned by the spawned runtime.
    let main_client = FETCH_CLIENT.with(|c| c.borrow().clone());
    let result = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => return "{}".to_string(),
        };
        rt.block_on(async move {
            // Fresh client for sync execution (avoids H2 deadlock on FETCH_CLIENT).
            // Shares all state (cookies, tokens, cache) with the main client.
            let client = match main_client.as_ref() {
                Some(main) => {
                    crate::net::HttpClient::new_with_shared_state(
                        main.profile(),
                        main.cookies(),
                        main.accept_ch_origins(),
                        main.dns_cache(),
                        main.alt_svc_cache(),
                    ).unwrap_or_else(|_| crate::net::HttpClient::new(main.profile()).unwrap())
                }
                None => {
                    let p = crate::stealth::presets::chrome_148_ru();
                    crate::net::HttpClient::new(&p).unwrap()
                }
            };

            let resp_result = match method_upper.as_str() {
                "GET" | "HEAD" => {
                    client.get_with_headers(&url_clone, &extra_headers).await
                }
                _ => {
                    let hdrs = crate::net::headers::chrome_headers_fetch(
                        client.profile(),
                        &url_clone,
                        origin_str.as_deref(),
                    );
                    let mut merged = hdrs;
                    for h in &extra_headers { merged.push(h.clone()); }
                    client.post_bytes_with_exact_headers(&url_clone, &body_bytes, &merged).await
                }
            };

            match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                async { resp_result },
            ).await {
                Ok(Ok(resp)) => {
                    // Write response cookies back to the main client (whose
                    // jar is shared with the cookies-Arc above, so the write
                    // is observable from the V8 thread).
                    if let Some(main) = main_client.as_ref() {
                        if let Ok(parsed) = url::Url::parse(&url_clone) {
                            for ck in &resp.set_cookies {
                                main.set_cookie_str(&parsed, ck).await;
                            }
                        }
                    }
                    let status = resp.status;
                    let resp_url = resp.url.clone();
                    let body_text = resp.text();
                    // Serialize headers as [[k,v],...] for JS.
                    let headers_arr: Vec<[String; 2]> = resp.headers
                        .into_iter()
                        .map(|(k, v)| [k, v])
                        .collect();
                    serde_json::json!({
                        "status": status,
                        "url": resp_url,
                        "headers": headers_arr,
                        "body": body_text,
                    }).to_string()
                }
                Ok(Err(e)) => {
                    eprintln!("[op_net_xhr_sync] error {}: {e}", url_clone);
                    serde_json::json!({"status": 0, "url": url_clone, "headers": [], "body": "", "error": e.to_string()}).to_string()
                }
                Err(_) => {
                    eprintln!("[op_net_xhr_sync] timeout {}", url_clone);
                    serde_json::json!({"status": 0, "url": url_clone, "headers": [], "body": "", "error": "timeout"}).to_string()
                }
            }
        })
    })
    .join()
    .unwrap_or_else(|_| "{}".to_string());

    result
}

deno_core::extension!(
    fetch_extension,
    ops = [
        op_fetch,
        op_cookie_get,
        op_cookie_set,
        op_cookie_set_sync,
        op_net_fetch_sync,
        op_net_xhr_sync,
        op_drain_csp_violations
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    /// Reproduce `op_fetch`'s own request-type derivation rather than naming a
    /// type by hand. The point of these tests is the wiring, so a test that
    /// hard-coded `"script"` would keep passing if the call site started
    /// classifying differently.
    fn async_fetch_gate(page_origin: Option<&str>, url: &str) -> MixedContentGate {
        let headers: HashMap<String, String> = page_origin
            .map(|o| {
                // Exactly the pseudo header fetch_bootstrap.js sends.
                HashMap::from([("x-browser-oxide-origin".to_string(), o.to_string())])
            })
            .unwrap_or_default();
        let request_type = crate::net::blocker::classify_request_type(
            url,
            headers
                .get("x-browser-oxide-request-type")
                .map(|s| s.as_str()),
        );
        mixed_content_gate(document_origin_hint(&headers), url, request_type, "fetch")
    }

    /// The `document.write('<script src=…>')` / `appendChild(script)` path,
    /// which always classifies its load as script.
    fn sync_fetch_gate(referer: &str, url: &str) -> MixedContentGate {
        let request_type = crate::net::blocker::classify_request_type(url, Some("script"));
        mixed_content_gate(referer, url, request_type, "sync-fetch")
    }

    #[test]
    fn a_plaintext_script_written_by_a_secure_page_never_loads() {
        // The headline case. Without this, a user on https://bank.example
        // executes whatever the café wifi chose to return, padlock intact.
        assert_eq!(
            sync_fetch_gate(
                "https://bank.example/account",
                "http://cdn.example/widget.js"
            ),
            MixedContentGate::Refuse
        );
        // Not upgraded, refused: silently retrying over HTTPS would run a
        // *different* script than the page asked for, which is its own bug.
        assert_eq!(
            sync_fetch_gate(
                "https://bank.example/account",
                "https://cdn.example/widget.js"
            ),
            MixedContentGate::Proceed("https://cdn.example/widget.js".to_string())
        );
    }

    #[test]
    fn fetch_from_a_secure_document_refuses_a_plaintext_url() {
        // `window.fetch('http://…')` on an HTTPS page. The URL has no
        // extension, so the call site classifies it `xmlhttprequest` — active,
        // and refused. A user notices this as a request that returns nothing
        // instead of one an attacker on the path could read and rewrite.
        assert_eq!(
            async_fetch_gate(Some("https://bank.example"), "http://api.example/balance"),
            MixedContentGate::Refuse
        );
        // A page served over plaintext has nothing left to protect, so the
        // same request there is left alone rather than broken.
        assert_eq!(
            async_fetch_gate(Some("http://old.example"), "http://api.example/balance"),
            MixedContentGate::Proceed("http://api.example/balance".to_string())
        );
        // No `location` to read (worker bootstrap, about:blank) — the gate
        // does not invent an origin. This is the known gap, asserted so it is
        // a decision on the record rather than an accident.
        assert_eq!(
            async_fetch_gate(None, "http://api.example/balance"),
            MixedContentGate::Proceed("http://api.example/balance".to_string())
        );
    }

    #[test]
    fn a_secure_page_gets_its_images_upgraded_and_its_dev_server_untouched() {
        // Passive content is rewritten, not refused: the user sees the image,
        // over HTTPS, instead of a hole in the page. The call site classifies
        // by extension, which is what makes `.png` passive here.
        assert_eq!(
            async_fetch_gate(Some("https://shop.example"), "http://cdn.example/logo.png"),
            MixedContentGate::Proceed("https://cdn.example/logo.png".to_string())
        );
        // And the carve-out that decides whether this is shippable at all: a
        // developer's plaintext loopback server keeps working, unrewritten,
        // even when the page claims an HTTPS origin.
        for dev in [
            "http://localhost:3000/bundle.js",
            "http://127.0.0.1:8080/bundle.js",
        ] {
            assert_eq!(
                sync_fetch_gate("https://app.example/", dev),
                MixedContentGate::Proceed(dev.to_string()),
                "{dev} was treated as mixed content; the local dev server is broken"
            );
        }
    }
}
