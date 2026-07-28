//! Favicons: finding a page's icon, fetching it once per origin, decoding it.
//!
//! A tab strip without favicons is a column of truncated titles, and telling
//! two of them apart costs the user a read rather than a glance. That is the
//! whole benefit. The cost is a network request, and it is a request with an
//! unusually bad shape, so the rules below are not decoration.
//!
//! # What a favicon fetch discloses, and when it happens
//!
//! A favicon fetch is an HTTP GET to the page's own origin, carrying that
//! origin's cookies, from the same client and connection pool as the document.
//! To the origin it therefore says: *this visitor's browser rendered the page*
//! — it does not distinguish a human from a crawler, and it reveals nothing the
//! document request did not already reveal, because the document request came
//! first and came from the same identity.
//!
//! Two things would change that, and both are prevented here rather than
//! documented as caveats:
//!
//! * **Refetching.** An icon re-requested on every tab switch turns a static
//!   image into a liveness beacon: the origin learns not just *that* you
//!   visited but *how long you kept the tab open and when you looked at it*,
//!   which the document request cannot tell it. So the answer is cached per
//!   origin for the life of the session, and **negative answers are cached
//!   too** — an origin with no icon is exactly the one that would otherwise be
//!   asked forever.
//! * **Third parties.** `<link rel=icon href="https://tracker.example/i.png">`
//!   is a tracking pixel wearing an icon's clothes. It is fetched through
//!   [`crate::net::blocker::should_block`] and
//!   [`crate::net::mixed_content::check`] exactly like any other subresource,
//!   with request type `image` — so a blocked host stays blocked here, and a
//!   plaintext icon on a secure page is upgraded rather than fetched in the
//!   clear. A page whose *own* URL the blocklist refuses gets no favicon fetch
//!   at all: having declined to let the document load, going on to request its
//!   icon would hand the blocked origin the visit anyway.
//!
//! A `data:` icon is decoded inline and never reaches the network. Pages that
//! inline their icon did so to save a request; falling back to `/favicon.ico`
//! because we could not read it would spend the request they avoided.
//!
//! # Shape of the API
//!
//! Discovery is synchronous and takes a `&Dom`; fetching is asynchronous and
//! takes bytes. They are separate because the DOM lives inside the JS runtime
//! and is only reachable through a closure ([`crate::Page::with_dom`]), so a
//! `&Dom` cannot be held across an `await`. [`FaviconCache::plan`] decides
//! what, if anything, to fetch; [`FaviconCache::fulfil`] performs it. The split
//! is also what makes the cache testable without a network.

use std::collections::HashMap;

use crate::dom::node::{NodeData, NodeId};
use crate::dom::Dom;

/// Which `rel` a candidate icon came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconRel {
    /// `rel="icon"` or `rel="shortcut icon"` — what the page nominates.
    Icon,
    /// `rel="apple-touch-icon"`. Usable, but a home-screen tile rather than a
    /// tab-strip icon, so it only wins when there is nothing else.
    AppleTouch,
}

/// An icon candidate as the document declared it: `href` unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconLink {
    pub href: String,
    pub rel: IconRel,
    /// The `sizes` attribute verbatim, if present.
    pub sizes: Option<String>,
}

/// Find every `<link>` that offers an icon, in document order.
///
/// Deliberately shaped like [`crate::stylesheet_collector::find_stylesheets`]:
/// the same recursive walk over the same arena, matching `rel` case- and
/// whitespace-insensitively because `rel="Shortcut Icon"` is real and common.
pub fn find_icon_links(dom: &Dom) -> Vec<IconLink> {
    let mut out = Vec::new();
    collect(dom, NodeId::DOCUMENT, &mut out);
    out
}

fn collect(dom: &Dom, node_id: NodeId, out: &mut Vec<IconLink>) {
    for child_id in dom.children(node_id) {
        if let Some(node) = dom.get(child_id) {
            if let NodeData::Element(elem) = &node.data {
                if elem.name.local.eq_ignore_ascii_case("link") {
                    if let Some(link) = icon_link_of(elem) {
                        out.push(link);
                    }
                }
            }
            collect(dom, child_id, out);
        }
    }
}

fn icon_link_of(elem: &crate::dom::node::ElementData) -> Option<IconLink> {
    let attr = |name: &str| {
        elem.attrs
            .iter()
            .find(|a| a.name.local.eq_ignore_ascii_case(name))
            .map(|a| a.value.as_str())
    };

    // `rel` is a space-separated token list, so `contains("icon")` would also
    // match `rel="icon-license"`. Tokenise instead.
    let rel_attr = attr("rel")?;
    let mut rel = None;
    for token in rel_attr.split_ascii_whitespace() {
        if token.eq_ignore_ascii_case("icon") {
            rel = Some(IconRel::Icon);
            break;
        }
        if token.eq_ignore_ascii_case("apple-touch-icon")
            || token.eq_ignore_ascii_case("apple-touch-icon-precomposed")
        {
            rel = Some(IconRel::AppleTouch);
        }
    }
    // `rel="shortcut icon"` tokenises to `shortcut` + `icon`, so the loop above
    // already caught it; a bare `rel="shortcut"` is not an icon and does not.
    let rel = rel?;

    let href = attr("href")?.trim();
    if href.is_empty() {
        return None;
    }

    Some(IconLink {
        href: href.to_string(),
        rel,
        sizes: attr("sizes").map(str::to_string),
    })
}

/// The `<base href>` in effect, if the document sets one.
///
/// Only the first `<base>` counts, which is what the HTML specification says
/// and what every engine does. Without this a page that relocates its assets
/// with `<base>` would have its icon resolved against the document URL and
/// 404 — or, worse, resolve onto a path that exists and is not the icon.
pub fn base_href(dom: &Dom) -> Option<String> {
    fn walk(dom: &Dom, node_id: NodeId) -> Option<String> {
        for child_id in dom.children(node_id) {
            if let Some(node) = dom.get(child_id) {
                if let NodeData::Element(elem) = &node.data {
                    if elem.name.local.eq_ignore_ascii_case("base") {
                        if let Some(href) = elem
                            .attrs
                            .iter()
                            .find(|a| a.name.local.eq_ignore_ascii_case("href"))
                        {
                            let href = href.value.trim();
                            if !href.is_empty() {
                                return Some(href.to_string());
                            }
                        }
                    }
                }
                if let Some(found) = walk(dom, child_id) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(dom, NodeId::DOCUMENT)
}

/// Score a candidate: higher wins, document order breaks ties.
///
/// A nominated `rel=icon` always beats an `apple-touch-icon`, whatever the
/// declared sizes — the latter is a 180px home-screen tile, usually with a
/// different design and no transparency, and using it when the page offered a
/// real favicon shows the user the wrong mark.
fn score(link: &IconLink) -> u32 {
    let rel_weight = match link.rel {
        IconRel::Icon => 1000,
        IconRel::AppleTouch => 0,
    };
    // Bigger is better only up to a point: everything above 64 is downscaled to
    // the same 16 or 32 pixels, and preferring a 512px PNG costs bandwidth for
    // no visible gain.
    let declared = link
        .sizes
        .as_deref()
        .map(largest_declared)
        // No `sizes` is the overwhelmingly common case and is nearly always a
        // 16 or 32px icon. Rank it as 32 so it beats a link that explicitly
        // declares 16x16 and loses to one declaring 48x48.
        .unwrap_or(32);
    rel_weight + declared.min(64)
}

/// Largest edge in a `sizes` list such as `"16x16 32x32"`. `any` (an SVG) wins
/// outright — it rescales without artefacts.
fn largest_declared(sizes: &str) -> u32 {
    let mut best = 0;
    for token in sizes.split_ascii_whitespace() {
        if token.eq_ignore_ascii_case("any") {
            return 64;
        }
        let edge = token
            .split(['x', 'X'])
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        best = best.max(edge);
    }
    best
}

/// Pick the icon a page most likely wants shown, if it declares one.
pub fn choose(links: &[IconLink]) -> Option<&IconLink> {
    links
        .iter()
        .enumerate()
        .max_by_key(|(i, link)| (score(link), std::cmp::Reverse(*i)))
        .map(|(_, link)| link)
}

/// `scheme://host[:port]` for a URL, or `None` for anything without a network
/// origin (`file:`, `about:`, `data:`).
///
/// This is the cache key. Per *origin*, not per page: a site's icon is a
/// property of the site, and keying per URL would refetch on every article.
pub fn origin_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    match parsed.port() {
        Some(port) => Some(format!("{}://{host}:{port}", parsed.scheme())),
        None => Some(format!("{}://{host}", parsed.scheme())),
    }
}

/// Resolve a declared `href` against the document, or `None` if it is not
/// something we can fetch.
fn absolutise(base: &str, href: &str) -> Option<String> {
    let joined = url::Url::parse(base).ok()?.join(href).ok()?;
    match joined.scheme() {
        "http" | "https" => Some(joined.to_string()),
        _ => None,
    }
}

/// Decoded icon pixels at their natural size, **straight** (non-premultiplied)
/// RGBA8, top-down — which is what the decoder produces. Premultiplication
/// happens in [`Favicon::copy_scaled`], at the ABI boundary that requires it.
#[derive(Debug, Clone)]
pub struct Favicon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Where it came from. Worth keeping: a shell that wants to show the user
    /// what a page caused it to request can, and it is what the tests assert.
    pub source_url: String,
}

impl Favicon {
    /// Decode image bytes into an icon.
    ///
    /// Reuses the engine's one decoder rather than adding a second. PNG, JPEG
    /// and ICO, all pure Rust; a `.ico` that is really a PNG (which is most of
    /// them now) works either way because the format is sniffed from the bytes
    /// and not from the extension.
    pub fn decode(bytes: &[u8], source_url: &str) -> Option<Self> {
        let (rgba, width, height) = crate::canvas::Canvas2D::decode_image(bytes)?;
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            width,
            height,
            rgba,
            source_url: source_url.to_string(),
        })
    }

    /// Write this icon into `out` as `size * size` **premultiplied** RGBA8,
    /// top-down — the same pixel contract as `bo_browser_render`.
    ///
    /// Returns `false` if `out` is too small, rather than filling part of it:
    /// a half-written buffer that a native app blits is a visible corruption,
    /// and it would be blamed on the icon rather than on the caller.
    pub fn copy_scaled(&self, size: u32, out: &mut [u8]) -> bool {
        let needed = (size as usize) * (size as usize) * 4;
        if size == 0 || out.len() < needed {
            return false;
        }
        let Some(src) = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
        else {
            return false;
        };
        // Lanczos3 rather than nearest: a 256px icon reduced to 16px by point
        // sampling keeps one pixel in sixteen, which for the thin strokes most
        // logos are made of means the mark frequently vanishes entirely. The
        // cost is irrelevant at this size.
        let scaled = if self.width == size && self.height == size {
            src
        } else {
            image::imageops::resize(&src, size, size, image::imageops::FilterType::Lanczos3)
        };
        for (dst, px) in out[..needed].chunks_exact_mut(4).zip(scaled.pixels()) {
            let [r, g, b, a] = px.0;
            let m = |c: u8| ((c as u16 * a as u16) / 255) as u8;
            dst.copy_from_slice(&[m(r), m(g), m(b), a]);
        }
        true
    }
}

/// What [`FaviconCache::plan`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// This origin has already been answered — positively or negatively. Do
    /// not touch the network.
    Cached,
    /// Nothing to do, and why. Not an error: a `file:` page has no origin and
    /// a blocked page must not be asked for its icon.
    Refused(&'static str),
    /// The icon is inline in the document. Decode it; no request is made.
    Inline { origin: String, bytes: Vec<u8> },
    /// Fetch this absolute URL and hand the bytes to [`FaviconCache::store`].
    Fetch { origin: String, url: String },
}

/// One decoded icon per origin, for the life of the session.
///
/// Negative entries are kept as well as positive ones. Without them an origin
/// that has no icon is re-asked on every navigation, which is precisely the
/// repeated-request pattern the cache exists to stop.
#[derive(Debug, Default)]
pub struct FaviconCache {
    entries: HashMap<String, Option<Favicon>>,
}

impl FaviconCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The icon for an origin, or `None` if there is none or none yet.
    pub fn get(&self, origin: &str) -> Option<&Favicon> {
        self.entries.get(origin).and_then(Option::as_ref)
    }

    /// Has this origin been answered, either way?
    pub fn is_answered(&self, origin: &str) -> bool {
        self.entries.contains_key(origin)
    }

    /// The icon for a page URL, by its origin.
    pub fn for_url(&self, url: &str) -> Option<&Favicon> {
        self.get(&origin_of(url)?)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget everything. Part of clearing browsing data: the set of origins
    /// whose icons are held is a list of where the user has been.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Decide what fetching this page's icon would take, without doing it.
    ///
    /// Every refusal is here rather than spread across the caller: the cache,
    /// the blocklist, mixed content and the `file:`/`about:` cases all produce
    /// a `Plan`, so there is one place to read to know what can reach the
    /// network.
    pub fn plan(&self, page_url: &str, dom: &Dom) -> Plan {
        let Some(origin) = origin_of(page_url) else {
            return Plan::Refused("the page has no network origin");
        };
        if self.is_answered(&origin) {
            return Plan::Cached;
        }
        // A page the blocklist refused must not have its icon fetched either.
        // Otherwise blocking the document and then requesting its icon hands
        // the origin the visit we just declined to give it.
        if crate::net::blocker::should_block(page_url, "", "document") {
            return Plan::Refused("the page itself is blocked");
        }

        let base = base_href(dom)
            .and_then(|href| absolutise(page_url, &href))
            .unwrap_or_else(|| page_url.to_string());

        let links = find_icon_links(dom);
        if let Some(link) = choose(&links) {
            if let Some(bytes) = decode_data_url(&link.href) {
                return Plan::Inline { origin, bytes };
            }
            if let Some(url) = absolutise(&base, &link.href) {
                return self.gate(origin, page_url, url);
            }
        }

        // No usable `<link rel=icon>`. `/favicon.ico` at the origin is the
        // convention every browser still honours and most sites still serve,
        // and asking for it is one request that the negative cache makes
        // at most once per origin.
        let fallback = format!("{origin}/favicon.ico");
        self.gate(origin, page_url, fallback)
    }

    /// Apply the same subresource rules any other image would face.
    fn gate(&self, origin: String, page_url: &str, url: String) -> Plan {
        use crate::net::mixed_content::{check, upgrade, Verdict};

        // `image`, not `other`: a favicon *is* an image, and classifying it
        // otherwise would both miss image-scoped filter rules and make mixed
        // content treat it as active content.
        let request_type = crate::net::blocker::classify_request_type(&url, Some("image"));
        if crate::net::blocker::should_block(&url, page_url, request_type) {
            return Plan::Refused("the icon URL is blocked");
        }
        let url = match check(page_url, &url, request_type) {
            Verdict::Allow => url,
            Verdict::Upgrade => upgrade(&url),
            Verdict::Block => return Plan::Refused("the icon is mixed content"),
        };
        Plan::Fetch { origin, url }
    }

    /// Record an answer for an origin. `bytes` of `None`, or bytes that do not
    /// decode, record the *negative* — which is the entry that stops the
    /// refetch loop, so it must not be skipped on failure.
    pub fn store(&mut self, origin: &str, url: &str, bytes: Option<&[u8]>) -> bool {
        let icon = bytes.and_then(|b| Favicon::decode(b, url));
        let ok = icon.is_some();
        self.entries.insert(origin.to_string(), icon);
        ok
    }

    /// Carry out a plan, calling `get` at most once and only for
    /// [`Plan::Fetch`]. Returns whether an icon is now available.
    ///
    /// `get` is a parameter rather than an `HttpClient` so that the caching
    /// behaviour can be tested by counting calls, with no network involved —
    /// "the second navigation makes no request" is the property that matters
    /// and the only way to assert it is to watch the request being not made.
    pub async fn fulfil<F, Fut>(&mut self, plan: Plan, get: F) -> bool
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Option<Vec<u8>>>,
    {
        match plan {
            Plan::Cached | Plan::Refused(_) => false,
            Plan::Inline { origin, bytes } => {
                // The source URL a shell would show. Reporting the whole data:
                // URL back would be a megabyte of base64 across the C boundary
                // to say something the scheme alone says.
                self.store(&origin, "data:", Some(&bytes))
            }
            Plan::Fetch { origin, url } => {
                let bytes = get(url.clone()).await;
                self.store(&origin, &url, bytes.as_deref())
            }
        }
    }

    /// [`FaviconCache::fulfil`] over the engine's own HTTP client, so the
    /// request carries the same TLS fingerprint, cookies and connection pool
    /// as everything else the page loaded.
    pub async fn fetch(&mut self, plan: Plan, client: &crate::net::HttpClient) -> bool {
        self.fulfil(plan, |url| async move {
            // Redirects are followed because `/favicon.ico` redirecting to a
            // CDN path is the normal case; the cap is the same 5 the script
            // path uses.
            match client.get_follow(&url, 5).await {
                Ok(resp) if resp.ok() => Some(resp.body),
                _ => None,
            }
        })
        .await
    }
}

/// Bytes of a `data:` URL, if it is one and it is base64.
///
/// Percent-encoded (non-base64) data URLs are not decoded: they cannot
/// represent PNG or ICO bytes without escaping most of them, so in practice
/// they only appear for SVG — which this engine cannot rasterise anyway.
fn decode_data_url(href: &str) -> Option<Vec<u8>> {
    // `get` rather than slicing: an href is arbitrary author text and may not
    // have a char boundary at byte 5.
    if !href.get(..5)?.eq_ignore_ascii_case("data:") {
        return None;
    }
    let (meta, payload) = href.get(5..)?.split_once(',')?;
    if !meta.to_ascii_lowercase().contains("base64") {
        return None;
    }
    // Whitespace inside a long inline icon is legal in HTML and fatal to a
    // strict base64 decoder.
    let cleaned: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, cleaned).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn dom(html: &str) -> Dom {
        crate::html_parser::parse_html(html)
    }

    /// A 1x1 opaque red PNG, so decoding is exercised on real bytes rather
    /// than mocked away.
    fn red_png() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 255]).unwrap();
        }
        buf
    }

    #[test]
    fn a_link_rel_icon_is_found_and_made_absolute() {
        let dom = dom(
            r#"<html><head><link rel="icon" href="/assets/fav.png"></head><body></body></html>"#,
        );
        let links = find_icon_links(&dom);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].rel, IconRel::Icon);

        let cache = FaviconCache::new();
        match cache.plan("https://example.com/deep/page.html", &dom) {
            Plan::Fetch { origin, url } => {
                assert_eq!(origin, "https://example.com");
                assert_eq!(url, "https://example.com/assets/fav.png");
            }
            other => panic!("expected a fetch, got {other:?}"),
        }
    }

    #[test]
    fn a_relative_href_resolves_against_the_document_not_the_origin() {
        let dom = dom(r#"<html><head><link rel="shortcut icon" href="fav.png"></head></html>"#);
        let cache = FaviconCache::new();
        match cache.plan("https://example.com/deep/page.html", &dom) {
            Plan::Fetch { url, .. } => assert_eq!(url, "https://example.com/deep/fav.png"),
            other => panic!("expected a fetch, got {other:?}"),
        }
    }

    #[test]
    fn a_base_element_moves_the_resolution_root() {
        let dom = dom(r#"<html><head><base href="https://cdn.example.com/static/">
               <link rel="icon" href="fav.png"></head></html>"#);
        assert_eq!(
            base_href(&dom).as_deref(),
            Some("https://cdn.example.com/static/")
        );
        let cache = FaviconCache::new();
        match cache.plan("https://example.com/page.html", &dom) {
            Plan::Fetch { origin, url } => {
                assert_eq!(url, "https://cdn.example.com/static/fav.png");
                // The cache key follows the *page*, not the icon: the icon is
                // the site's, wherever it is hosted.
                assert_eq!(origin, "https://example.com");
            }
            other => panic!("expected a fetch, got {other:?}"),
        }
    }

    #[test]
    fn the_favicon_ico_fallback_is_used_when_the_page_declares_nothing() {
        let dom = dom("<html><head><title>no icon</title></head><body>hi</body></html>");
        assert!(find_icon_links(&dom).is_empty());
        let cache = FaviconCache::new();
        match cache.plan("https://example.com/some/page?q=1", &dom) {
            Plan::Fetch { url, .. } => assert_eq!(url, "https://example.com/favicon.ico"),
            other => panic!("expected the fallback, got {other:?}"),
        }
    }

    #[test]
    fn the_fallback_keeps_a_non_default_port() {
        let dom = dom("<html><head></head></html>");
        let cache = FaviconCache::new();
        match cache.plan("https://example.com:8443/", &dom) {
            Plan::Fetch { url, .. } => assert_eq!(url, "https://example.com:8443/favicon.ico"),
            other => panic!("expected the fallback, got {other:?}"),
        }
    }

    #[test]
    fn a_rel_that_merely_contains_icon_is_not_an_icon() {
        // `contains("icon")` would match this; tokenising does not.
        let dom = dom(r#"<html><head><link rel="icon-license" href="/nope.png"></head></html>"#);
        assert!(find_icon_links(&dom).is_empty());
    }

    #[test]
    fn a_stylesheet_link_is_not_an_icon() {
        let dom = dom(r#"<html><head><link rel="stylesheet" href="/a.css"></head></html>"#);
        assert!(find_icon_links(&dom).is_empty());
    }

    #[test]
    fn a_nominated_icon_beats_an_apple_touch_icon_however_large() {
        let dom = dom(r#"<html><head>
               <link rel="apple-touch-icon" sizes="180x180" href="/touch.png">
               <link rel="icon" sizes="16x16" href="/fav.png">
               </head></html>"#);
        let links = find_icon_links(&dom);
        assert_eq!(links.len(), 2);
        assert_eq!(choose(&links).unwrap().href, "/fav.png");
    }

    #[test]
    fn an_apple_touch_icon_is_used_when_it_is_all_there_is() {
        let dom = dom(r#"<html><head><link rel="apple-touch-icon" href="/t.png"></head></html>"#);
        let links = find_icon_links(&dom);
        assert_eq!(choose(&links).unwrap().rel, IconRel::AppleTouch);
    }

    #[test]
    fn the_larger_of_two_declared_icons_wins() {
        let dom = dom(r#"<html><head>
               <link rel="icon" sizes="16x16" href="/small.png">
               <link rel="icon" sizes="48x48" href="/big.png">
               </head></html>"#);
        let links = find_icon_links(&dom);
        assert_eq!(choose(&links).unwrap().href, "/big.png");
    }

    #[test]
    fn document_order_breaks_a_tie() {
        let dom = dom(r#"<html><head>
               <link rel="icon" href="/first.png">
               <link rel="icon" href="/second.png">
               </head></html>"#);
        let links = find_icon_links(&dom);
        assert_eq!(choose(&links).unwrap().href, "/first.png");
    }

    #[tokio::test]
    async fn the_cache_prevents_a_second_fetch() {
        // The property the whole module exists for: an icon re-requested on
        // every tab switch is a liveness beacon. Asserted by counting the
        // requests the production path would make, not by inspecting state.
        let dom = dom(r#"<html><head><link rel="icon" href="/fav.png"></head></html>"#);
        let page = "https://example.com/";
        let calls = Cell::new(0usize);
        let mut cache = FaviconCache::new();

        let get = |_url: String| {
            calls.set(calls.get() + 1);
            async { Some(red_png()) }
        };

        let plan = cache.plan(page, &dom);
        assert!(matches!(plan, Plan::Fetch { .. }));
        assert!(cache.fulfil(plan, get).await);
        assert_eq!(calls.get(), 1);

        // A second navigation to the same origin — a tab switch, a reload, a
        // different article on the same site.
        let plan = cache.plan("https://example.com/other/article", &dom);
        assert_eq!(plan, Plan::Cached);
        assert!(!cache.fulfil(plan, get).await, "nothing left to do");
        assert_eq!(calls.get(), 1, "the network must not be touched again");

        assert_eq!(cache.for_url(page).map(|f| f.width), Some(1));
    }

    #[tokio::test]
    async fn an_origin_with_no_icon_is_not_asked_twice() {
        // The negative entry matters more than the positive one: without it,
        // the origin most likely to be re-asked forever is the one that has
        // already said no.
        let dom = dom("<html><head></head><body></body></html>");
        let calls = Cell::new(0usize);
        let mut cache = FaviconCache::new();
        let get = |_url: String| {
            calls.set(calls.get() + 1);
            async { None }
        };

        let plan = cache.plan("https://example.com/", &dom);
        assert!(!cache.fulfil(plan, get).await);
        assert_eq!(calls.get(), 1);

        assert_eq!(cache.plan("https://example.com/again", &dom), Plan::Cached);
        assert!(cache.is_answered("https://example.com"));
        assert!(cache.get("https://example.com").is_none());
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn bytes_that_do_not_decode_still_record_the_negative() {
        let mut cache = FaviconCache::new();
        assert!(!cache.store("https://example.com", "u", Some(b"not an image")));
        assert!(cache.is_answered("https://example.com"));
    }

    #[test]
    fn a_page_with_no_network_origin_is_refused() {
        let dom = dom(r#"<html><head><link rel="icon" href="/fav.png"></head></html>"#);
        let cache = FaviconCache::new();
        assert!(matches!(
            cache.plan("file:///C:/tmp/page.html", &dom),
            Plan::Refused(_)
        ));
        assert!(matches!(cache.plan("about:blank", &dom), Plan::Refused(_)));
    }

    #[test]
    fn a_plaintext_icon_on_a_secure_page_is_upgraded_not_fetched_in_the_clear() {
        let dom =
            dom(r#"<html><head><link rel="icon" href="http://example.com/f.png"></head></html>"#);
        let cache = FaviconCache::new();
        match cache.plan("https://example.com/", &dom) {
            Plan::Fetch { url, .. } => assert_eq!(url, "https://example.com/f.png"),
            other => panic!("expected an upgrade, got {other:?}"),
        }
    }

    #[test]
    fn an_inline_data_icon_never_reaches_the_network() {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, red_png());
        let html = format!(
            r#"<html><head><link rel="icon" href="data:image/png;base64,{b64}"></head></html>"#
        );
        let dom = dom(&html);
        let cache = FaviconCache::new();
        match cache.plan("https://example.com/", &dom) {
            Plan::Inline { origin, bytes } => {
                assert_eq!(origin, "https://example.com");
                assert!(Favicon::decode(&bytes, "x").is_some());
            }
            other => panic!("expected an inline icon, got {other:?}"),
        }
    }

    #[test]
    fn decoding_and_scaling_produces_premultiplied_pixels() {
        // Half-transparent green: premultiplied, the green channel must come
        // down with the alpha. A straight-alpha buffer blitted by a native app
        // shows a bright fringe, which is the visible symptom.
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0, 255, 0, 128].repeat(4)).unwrap();
        }
        let icon = Favicon::decode(&buf, "https://example.com/f.png").unwrap();
        assert_eq!((icon.width, icon.height), (2, 2));

        let mut out = vec![0u8; 16 * 16 * 4];
        assert!(icon.copy_scaled(16, &mut out));
        assert_eq!(out[3], 128, "alpha is untouched");
        assert_eq!(out[1], 128, "green is multiplied down by the alpha");
        assert_eq!(out[0], 0);
    }

    #[test]
    fn a_short_buffer_is_refused_rather_than_partly_filled() {
        let icon = Favicon::decode(&red_png(), "u").unwrap();
        let mut out = vec![0u8; 10];
        assert!(!icon.copy_scaled(16, &mut out));
        assert!(out.iter().all(|b| *b == 0), "and nothing was written");
    }

    #[test]
    fn clearing_forgets_which_origins_were_visited() {
        let mut cache = FaviconCache::new();
        cache.store("https://example.com", "u", Some(&red_png()));
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }
}
