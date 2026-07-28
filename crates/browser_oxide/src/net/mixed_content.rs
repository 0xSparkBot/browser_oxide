//! Mixed content: refusing plaintext subresources on a secure page.
//!
//! A page served over HTTPS that pulls a script over HTTP has thrown away the
//! guarantee it just paid for. The attacker who could not read the document can
//! read and rewrite the script, and a rewritten script owns the page. Every
//! shipping browser has blocked this class since about 2015; this engine did
//! not, which meant an `https://` padlock here promised strictly less than the
//! same padlock elsewhere.
//!
//! # What is refused, and what is not
//!
//! Browsers split mixed content in two. **Active** content — script, stylesheet,
//! iframe, fetch/XHR, worker — can rewrite the document, and is blocked
//! outright. **Passive** content — images, video, audio — can mislead but not
//! execute, and is historically allowed with a warning, though the web is
//! moving to upgrading it to HTTPS instead.
//!
//! This blocks active content and upgrades passive content, which is where
//! Chrome and Firefox have both landed. Upgrading rather than blocking passive
//! content is the difference between a page that looks broken and a page that
//! looks fine on a server that supports HTTPS — which, by now, is nearly all of
//! them.
//!
//! # What counts as secure
//!
//! `https:` and `wss:`, plus loopback. Loopback is exempt because it cannot be
//! intercepted by a network attacker and because refusing it would break every
//! local development server — the same carve-out the specification makes for
//! "potentially trustworthy origins".

/// What to do with a subresource request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Not mixed content, or not on a secure page. Fetch it as asked.
    Allow,
    /// Passive mixed content: fetch it over HTTPS instead.
    Upgrade,
    /// Active mixed content: refuse.
    Block,
}

/// Is this URL's origin one a network attacker cannot sit in the middle of?
///
/// Scheme first, then the loopback carve-out. Deliberately does not treat
/// `file:` as secure for this purpose: a local page pulling plaintext from the
/// network is still crossing a network an attacker may hold.
pub fn is_secure(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("wss://") {
        return true;
    }
    if let Some(rest) = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("ws://"))
    {
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('@')
            .next()
            .unwrap_or("");
        let host = host.split(':').next().unwrap_or("");
        return host == "localhost"
            || host == "127.0.0.1"
            || host == "[::1]"
            || host == "::1"
            || host.ends_with(".localhost");
    }
    // `data:`, `blob:`, `about:` and friends carry no network hop of their own.
    !lower.starts_with("http://") && !lower.starts_with("ws://")
}

/// Whether a request type can rewrite the document if an attacker controls it.
///
/// The strings are exactly those [`crate::net::blocker::classify_request_type`]
/// produces — Adblock's vocabulary, not Chromium's. This first shipped matching
/// `"sub_frame"`, which Chromium uses and this codebase never emits, so an
/// `http:` iframe on an `https:` page would have fallen through to *passive*
/// and been quietly upgraded instead of refused. It could not bite yet because
/// no call site classifies a subdocument, which is precisely why it would have
/// survived to the one that did. `an_iframe_is_active_in_this_codebases_own_
/// vocabulary` pins the two together.
///
/// Unrecognised types count as active. The extension heuristic falls back to
/// `"xmlhttprequest"` for anything it cannot name, and refusing an unknown
/// plaintext resource on a secure page is the failure direction that cannot
/// cost the user their confidentiality.
fn is_active(request_type: &str) -> bool {
    !matches!(request_type, "image" | "media" | "font")
}

/// Decide what to do with `request_url` loaded by `page_url`.
///
/// `page_url` empty means the caller does not know the document's origin. That
/// returns `Allow` rather than guessing: refusing on an unknown origin would
/// break every request the engine makes before a document exists, and guessing
/// "secure" would invent a guarantee. The gap is real and is why the fetch path
/// should always pass a referer.
pub fn check(page_url: &str, request_url: &str, request_type: &str) -> Verdict {
    if page_url.is_empty() || !is_secure(page_url) {
        return Verdict::Allow;
    }
    if is_secure(request_url) {
        return Verdict::Allow;
    }
    if is_active(request_type) {
        Verdict::Block
    } else {
        Verdict::Upgrade
    }
}

/// Rewrite an `http:`/`ws:` URL to its secure scheme.
pub fn upgrade(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        format!("https://{rest}")
    } else if let Some(rest) = url.strip_prefix("ws://") {
        format!("wss://{rest}")
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two modules must agree on what a subdocument is called.
    ///
    /// Derived from `classify_request_type` rather than written down here: a
    /// literal would restate the bug this test exists to catch.
    #[test]
    fn an_iframe_is_active_in_this_codebases_own_vocabulary() {
        use crate::net::blocker::classify_request_type;

        let iframe = classify_request_type("https://x.example/page", Some("subdocument"));
        assert!(
            is_active(iframe),
            "an iframe classified as {iframe:?} was treated as passive, so plaintext              frames on a secure page would be upgraded rather than refused"
        );
        assert_eq!(
            check("https://bank.example/", "http://evil.example/frame", iframe),
            Verdict::Block
        );

        // And the passive ones stay passive, or every image on the web breaks.
        for hint in ["image", "media", "font"] {
            let kind = classify_request_type("https://x.example/a", Some(hint));
            assert!(
                !is_active(kind),
                "{hint} classified as {kind:?} became active"
            );
        }
    }

    #[test]
    fn active_content_on_a_secure_page_is_blocked() {
        // The case the whole module exists for: an attacker who can rewrite
        // this script owns the page, padlock and all.
        for kind in ["script", "stylesheet", "xmlhttprequest", "sub_frame"] {
            assert_eq!(
                check("https://bank.example/", "http://cdn.example/x.js", kind),
                Verdict::Block,
                "{kind} was allowed over plaintext on a secure page"
            );
        }
    }

    #[test]
    fn passive_content_is_upgraded_rather_than_broken() {
        assert_eq!(
            check("https://bank.example/", "http://cdn.example/a.png", "image"),
            Verdict::Upgrade
        );
        assert_eq!(
            upgrade("http://cdn.example/a.png"),
            "https://cdn.example/a.png"
        );
        assert_eq!(upgrade("ws://x.example/s"), "wss://x.example/s");
    }

    #[test]
    fn a_plaintext_page_is_left_alone() {
        // Nothing to protect: the document itself already came over the wire in
        // the clear, so refusing its subresources would be theatre.
        assert_eq!(
            check("http://old.example/", "http://old.example/x.js", "script"),
            Verdict::Allow
        );
    }

    #[test]
    fn loopback_is_not_mixed_content() {
        // Cannot be intercepted, and refusing it would break every local dev
        // server — the same carve-out the spec makes.
        for local in [
            "http://localhost:3000/x.js",
            "http://127.0.0.1:8080/x.js",
            "http://app.localhost/x.js",
        ] {
            assert_eq!(
                check("https://page.example/", local, "script"),
                Verdict::Allow,
                "{local} was treated as mixed content"
            );
        }
    }

    #[test]
    fn an_unknown_page_origin_does_not_guess() {
        // Guessing "secure" would invent a guarantee; guessing "insecure" would
        // refuse everything the engine fetches before a document exists.
        assert_eq!(
            check("", "http://cdn.example/x.js", "script"),
            Verdict::Allow
        );
    }

    #[test]
    fn secure_schemes_are_recognised_case_insensitively() {
        assert!(is_secure("HTTPS://example.com/"));
        assert!(is_secure("wss://example.com/"));
        assert!(!is_secure("http://example.com/"));
        // Schemes with no network hop of their own.
        assert!(is_secure("data:text/plain,hi"));
        assert!(is_secure("about:blank"));
    }

    #[test]
    fn a_userinfo_host_cannot_smuggle_localhost() {
        // `http://localhost@evil.example/` has host evil.example, not
        // localhost. Parsing the authority naively is how that becomes a
        // bypass.
        assert!(!is_secure("http://localhost@evil.example/x.js"));
        assert_eq!(
            check(
                "https://page.example/",
                "http://localhost@evil.example/x.js",
                "script"
            ),
            Verdict::Block
        );
    }
}
