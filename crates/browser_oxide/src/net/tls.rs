//! BoringSSL TLS configuration with Chrome 147 fingerprint.
//!
//! Configures TLS to produce a ClientHello identical to Chrome 147,
//! including cipher suites, curves, signature algorithms, extensions,
//! and certificate compression — all in the exact order that produces
//! the correct JA3/JA4 fingerprint.
//!
//! Extension *order* is the one deliberate exception. Chrome permutes it
//! on every handshake, so there is no single order to reproduce; see the
//! "Chrome desktop/Android extension order" note further down this file.

use crate::stealth::{DeviceClass, StealthProfile};
use boring2::ssl::{
    CertCompressionAlgorithm, ConnectConfiguration, SslConnector, SslCurve, SslMethod, SslOptions,
    SslVersion,
};
use boring2::x509::store::X509StoreBuilder;
use boring2::x509::X509;
use foreign_types::ForeignTypeRef;
use tokio::net::TcpStream;
use tokio_boring2::SslStream;

use crate::net::error::NetError;

/// The Chrome major version whose **verified-real** ClientHello / H2
/// fingerprint these constants reproduce, byte-exact.
///
/// **Why this is 147 while every desktop preset's UA advertises Chrome
/// 148 — and why that is NOT an incoherent skew:**
///
/// 1. Chrome's TLS ClientHello is **version-stable across majors**. It
///    only changes on a deliberate TLS-stack change; the last such change
///    was the MLKEM768 post-quantum rollout at Chrome 131. There was no
///    TLS-stack change between 147 and 148 (consecutive majors, ~1 month
///    apart, May 2026), so the bytes real Chrome 148 puts on the wire are
///    identical to this verified-real 147 capture: the byte-exact Chrome
///    147 and 148 values are the same values.
/// 2. **JA4 does not encode the Chrome version.** JA4 = TLS-version +
///    sorted cipher/extension counts + ALPN + sorted sigalgs. None of
///    those differ 147↔148. A "JA4-vs-UA cross-check" verifies
///    the JA4 corresponds to *a Chrome* consistent with the UA *family*
///    — it cannot, even in principle, detect a 147-vs-148 minor/major
///    label difference.
/// 3. UA=148 is a **deliberate, A/B-tested** decision: real Chrome
///    stable IS 148 (chromiumdash; shipped early May 2026), and the
///    147→148 UA bump *recovered* several previously-blocked sites in
///    our measurement. Rolling the UA back to 147 would re-introduce
///    those regressions and advertise an outdated browser (its own
///    soft-deny signal). So the coherent state is UA=148 + these
///    (wire-identical) 147-reference bytes.
///
/// This constant exists so the coherence is **machine-checked** (see the
/// `tls_fingerprint_vectors_no_silent_drift` test) and the rationale is
/// one `grep` away — the silent-drift hazard the plan flags is removed
/// without changing a single wire byte or UA.
pub const TLS_CHROME_MAJOR: u32 = 147;

/// The Chrome major every desktop Chrome preset's `user_agent`
/// advertises. Intentionally != [`TLS_CHROME_MAJOR`]; see that
/// constant's docs for why this is wire-coherent, not a skew.
pub const UA_CHROME_MAJOR: u32 = 148;

/// Chrome 147 cipher suite list (order is critical for JA3 fingerprint).
const CIPHER_LIST: &str = concat!(
    "TLS_AES_128_GCM_SHA256",
    ":TLS_AES_256_GCM_SHA384",
    ":TLS_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    ":TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    ":TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    ":TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
    ":TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
    ":TLS_RSA_WITH_AES_128_GCM_SHA256",
    ":TLS_RSA_WITH_AES_256_GCM_SHA384",
    ":TLS_RSA_WITH_AES_128_CBC_SHA",
    ":TLS_RSA_WITH_AES_256_CBC_SHA",
);

/// Chrome 147 signature algorithms (order matters).
const SIGALGS_LIST: &str = concat!(
    "ecdsa_secp256r1_sha256",
    ":rsa_pss_rsae_sha256",
    ":rsa_pkcs1_sha256",
    ":ecdsa_secp384r1_sha384",
    ":rsa_pss_rsae_sha384",
    ":rsa_pkcs1_sha384",
    ":rsa_pss_rsae_sha512",
    ":rsa_pkcs1_sha512",
);

/// Chrome desktop elliptic curves (Chrome 131+ uses MLKEM768).
const CURVES_DESKTOP: &[SslCurve] = &[
    SslCurve::X25519_MLKEM768,
    SslCurve::X25519,
    SslCurve::SECP256R1,
    SslCurve::SECP384R1,
];

/// Chrome Android elliptic curves. Kyber768Draft00 (deprecated) was the
/// canonical Chrome 124-130 PQ curve; Chrome 131+ desktop replaced it with
/// MLKEM768 (codepoint 4588). A reference Chrome 131 Android capture
/// shows no PQ at all (just 29/23/24), but Chrome Android shares the
/// desktop codebase and by Chrome 147+ should have rolled MLKEM — verify
/// against a fresh Pixel capture if regressions appear.
const CURVES_ANDROID: &[SslCurve] = CURVES_DESKTOP;

/// iOS Safari 18 cipher suite list (20 ciphers, Apple's order). Per a
/// reference Safari iOS 18 TLS capture.
/// Distinct from Chrome desktop (15 ciphers): includes 3DES_EDE_CBC_SHA at
/// the tail and an extra RSA_WITH_3DES_EDE_CBC_SHA. Cipher order matters
/// for JA3.
const CIPHER_LIST_SAFARI_IOS: &str = concat!(
    "TLS_AES_128_GCM_SHA256",
    ":TLS_AES_256_GCM_SHA384",
    ":TLS_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    ":TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    ":TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    ":TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
    ":TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
    ":TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
    ":TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
    ":TLS_RSA_WITH_AES_256_GCM_SHA384",
    ":TLS_RSA_WITH_AES_128_GCM_SHA256",
    ":TLS_RSA_WITH_AES_256_CBC_SHA",
    ":TLS_RSA_WITH_AES_128_CBC_SHA",
    ":TLS_ECDHE_ECDSA_WITH_3DES_EDE_CBC_SHA",
    ":TLS_ECDHE_RSA_WITH_3DES_EDE_CBC_SHA",
    ":TLS_RSA_WITH_3DES_EDE_CBC_SHA",
);

/// iOS Safari signature algorithms (10 entries, includes the duplicated
/// `rsa_pss_rsae_sha384` Apple quirk we must reproduce verbatim).
/// Reference Safari TLS captures include the duplicate.
const SIGALGS_LIST_SAFARI_IOS: &str = concat!(
    "ecdsa_secp256r1_sha256",
    ":rsa_pss_rsae_sha256",
    ":rsa_pkcs1_sha256",
    ":ecdsa_secp384r1_sha384",
    ":rsa_pss_rsae_sha384",
    ":rsa_pss_rsae_sha384",
    ":rsa_pkcs1_sha384",
    ":rsa_pss_rsae_sha512",
    ":rsa_pkcs1_sha512",
    ":rsa_pkcs1_sha1",
);

/// iOS Safari 18 elliptic curves. No PQ (MLKEM lands in iOS 26 per Apple's
/// PQC support page). Adds P-521 vs Chrome desktop. Order per safari_18.0_iOS.yaml.
const CURVES_SAFARI_IOS: &[SslCurve] = &[
    SslCurve::X25519,
    SslCurve::SECP256R1,
    SslCurve::SECP384R1,
    SslCurve::SECP521R1,
];

/// iOS Safari 18 extension permutation. Indices into BoringSSL's internal
/// `BORING_SSLEXTENSION_PERMUTATION` table — see boring2 ssl/mod.rs for the
/// canonical ordering. Per reference Safari iOS 18 TLS captures, real
/// Safari emits its extensions in a FIXED order (no Fisher-Yates shuffle),
/// roughly: server_name, extended_master_secret, renegotiate, supported_groups,
/// ec_point_formats, ALPN, status_request, signature_algorithms,
/// signed_certificate_timestamp, key_share, psk_key_exchange_modes,
/// supported_versions, cert_compression. (GREASE and PADDING are auto-emitted
/// by BoringSSL outside the permutation table; PADDING positional ordering
/// requires raw extension injection — deferred.)
const SAFARI_IOS_EXTENSION_PERMUTATION: &[u8] = &[
    0,  // server_name
    2,  // extended_master_secret
    3,  // renegotiate
    4,  // supported_groups
    5,  // ec_point_formats
    7,  // application_layer_protocol_negotiation (ALPN)
    8,  // status_request
    9,  // signature_algorithms
    11, // certificate_timestamp
    14, // key_share
    15, // psk_key_exchange_modes
    17, // supported_versions
    21, // cert_compression (compress_certificate, type 27). boring2 kExtensions
        // index is 21 (per `ExtensionType::BORING_SSLEXTENSION_PERMUTATION` in
        // boring2 ssl/mod.rs, the authoritative mirror of the C table);
        // the previous `22` is the PADDING slot — a live TLS-fingerprint capture
        // showed this index emitting ext 0x15 (padding) instead of 0x1b
        // here, giving JA4 t13d2013h2 vs real iOS-18 Safari's t13d2014h2. With
        // 21, compress_certificate is emitted and BoringSSL auto-appends padding
        // last by ClientHello length → the 14-extension Safari JA4.
];

/// Firefox 135 (NSS) cipher suite list — 17 ciphers, NSS order. Distinct
/// from Chrome's 15: NSS leads TLS1.3 with AES-128-GCM, CHACHA20, AES-256-GCM
/// (CHACHA before AES-256), then the ECDHE-ECDSA/RSA GCM pairs, then the CBC
/// block (ECDSA before RSA, 256 before 128 in NSS's CBC ordering), then the
/// two RSA-GCM and two RSA-CBC fallbacks. Yields the Firefox JA4 cipher hash
/// `5b57614c22b0` (vs Chrome's). Per reference Firefox TLS captures.
const CIPHER_LIST_FIREFOX: &str = concat!(
    "TLS_AES_128_GCM_SHA256",
    ":TLS_CHACHA20_POLY1305_SHA256",
    ":TLS_AES_256_GCM_SHA384",
    ":TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    ":TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    ":TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    ":TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    ":TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
    ":TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
    ":TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
    ":TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
    ":TLS_RSA_WITH_AES_128_GCM_SHA256",
    ":TLS_RSA_WITH_AES_256_GCM_SHA384",
    ":TLS_RSA_WITH_AES_128_CBC_SHA",
    ":TLS_RSA_WITH_AES_256_CBC_SHA",
);

/// Firefox 135 (NSS) signature algorithms — 11 entries, NSS order: the three
/// ECDSA curves first, then RSA-PSS, then RSA-PKCS1, then the SHA-1 tail
/// (ecdsa_sha1, rsa_pkcs1_sha1). Yields the Firefox JA4 sigalg hash
/// `3d5424432f57`.
const SIGALGS_LIST_FIREFOX: &str = concat!(
    "ecdsa_secp256r1_sha256",
    ":ecdsa_secp384r1_sha384",
    ":ecdsa_secp521r1_sha512",
    ":rsa_pss_rsae_sha256",
    ":rsa_pss_rsae_sha384",
    ":rsa_pss_rsae_sha512",
    ":rsa_pkcs1_sha256",
    ":rsa_pkcs1_sha384",
    ":rsa_pkcs1_sha512",
    ":ecdsa_sha1",
    ":rsa_pkcs1_sha1",
);

/// Firefox 135 supported_groups. NSS appends the two FFDHE groups
/// (ffdhe2048, ffdhe3072) after the EC curves — a hard Firefox signature no
/// Chrome build sends. X25519MLKEM768 leads (Firefox shipped PQ key-share by
/// default in 132+). P-521 present (Chrome desktop omits it).
const CURVES_FIREFOX: &[SslCurve] = &[
    SslCurve::X25519_MLKEM768,
    SslCurve::X25519,
    SslCurve::SECP256R1,
    SslCurve::SECP384R1,
    SslCurve::SECP521R1,
    SslCurve::FFDHE2048,
    SslCurve::FFDHE3072,
];

/// Firefox 135 delegated_credentials (ext 0x22) sigalg list — Firefox-only.
/// The four ECDSA sigalgs NSS advertises in the delegated-credential ext.
const FIREFOX_DELEGATED_CREDENTIALS: &str = concat!(
    "ecdsa_secp256r1_sha256",
    ":ecdsa_secp384r1_sha384",
    ":ecdsa_secp521r1_sha512",
    ":ecdsa_sha1",
);

/// Firefox 135 record_size_limit (ext 0x1c) value: 0x4001 (16385).
const FIREFOX_RECORD_SIZE_LIMIT: u16 = 0x4001;

/// Firefox 135 extension order (indices into BoringSSL's
/// `BORING_SSLEXTENSION_PERMUTATION` table — same index space the Chrome and
/// Safari permutations use). FIXED order every handshake (NSS does not
/// Fisher-Yates shuffle). 15 extensions → the Firefox `t13d1715h2` JA4 count.
/// Index map (from `ExtensionType::BORING_SSLEXTENSION_PERMUTATION`):
/// 0=SNI, 1=ECH, 2=ext_master_secret, 3=renegotiate, 4=supported_groups,
/// 5=ec_point_formats, 6=session_ticket, 7=ALPN, 8=status_request,
/// 9=signature_algorithms, 14=key_share, 15=psk_kex_modes, 17=supported_versions,
/// 22=delegated_credentials, 25=record_size_limit. Order verified against a
/// reference Firefox 135 TLS capture — iterate if the JA4 ext-hash diverges.
const FIREFOX_EXTENSION_PERMUTATION: &[u8] = &[
    0,  // server_name
    2,  // extended_master_secret
    3,  // renegotiation_info
    4,  // supported_groups
    5,  // ec_point_formats
    6,  // session_ticket
    7,  // ALPN
    8,  // status_request
    22, // delegated_credentials (0x22) — Firefox-only
    14, // key_share
    17, // supported_versions
    9,  // signature_algorithms
    15, // psk_key_exchange_modes
    25, // record_size_limit (0x1c) — Firefox-only (boring2 perm-table index 25)
    1,  // encrypted_client_hello (ECH grease)
];

/// ALPN protocols: h2 + http/1.1
const ALPN_PROTOS: &[u8] = b"\x02h2\x08http/1.1";

// ---------------------------------------------------------------------------
// Chrome desktop/Android extension order: permuted per handshake, by
// BoringSSL, and deliberately not pinned by us.
// ---------------------------------------------------------------------------
//
// Chrome has shuffled its ClientHello extension order on every connection
// since Chrome 110 — visible on the wire from 20 January 2023, as an
// anti-ossification measure. Fastly measured the result in the wild as
// "nearly 15 factorial" orderings, so that "each connection ... practically
// [has] a unique JA3 fingerprint". There is therefore no "the" Chrome
// extension order to reproduce, and a *stable* order is by itself positive
// evidence of not-Chrome.
//
// This engine used to reproduce that mechanism at the wrong granularity. It
// called `set_permute_extensions(false)` and installed a single
// Fisher-Yates-shuffled order onto the `SSL_CTX`, once, in
// `chrome_connector()`. That connector is held as `Arc<SslConnector>` by
// `HttpClient` and reused for every connection the client makes, so one
// draw from 16! ≈ 2×10¹³ orderings — roughly 44 bits — was chosen at client
// construction and then replayed verbatim to every origin the session
// contacted. That is a session-unique identifier carried *below* HTTP,
// where no cookie policy, storage partition or blocklist can reach it, and
// readable by every server, CDN and passive observer that computes JA3 or
// an ordered JA4 variant. (JA4 proper sorts the extension list precisely to
// survive Chrome's permutation, so a JA4-only observer never saw it. JA3
// does not sort, and Cloudflare exposes `ja3_hash` alongside `ja4`.)
//
// WHY PER-HANDSHAKE PERMUTATION AND NOT A FIXED, FLEET-WIDE ORDER
//
// The rest of the privacy work here is converging on the Tor strategy —
// normalise, do not randomise (`docs/FINGERPRINT_SURFACE.md` §1; W3C's
// fingerprinting guidance; the WWW 2025 *Breaking the Shield* finding that
// randomisation-based defences fare worse than fixed outputs). A fixed
// order is the obvious reading of that recommendation, and it is the wrong
// answer here, for two reasons.
//
//  1. The objection to randomisation is that it manufactures a
//     configuration nobody has ever seen and holds it long enough to
//     identify. Neither half applies to a per-handshake permutation: the
//     observable distribution is exactly real Chrome's, and the value is
//     discarded before anything can be joined to it. Randomness re-drawn
//     per connection *destroys* a linkable identifier rather than creating
//     one. The defect was never that the order was random; it was that it
//     was random only once.
//  2. Normalisation and impersonation conflict only when the fleet-wide
//     constant cannot match a real browser. On this surface the constant is
//     precisely what cannot match: real Chrome has no fixed extension
//     order, so shipping one would give every browser_oxide user the same
//     wire-visible "not Chrome, and specifically this engine" label — a
//     static signature an anti-bot vendor enumerates once and matches
//     forever, with no anonymity set beyond browser_oxide's own users.
//     Per-handshake permutation puts the user inside Chrome's population,
//     which is what the impersonation strategy and the anonymity-set
//     argument both want.
//
// So the two philosophies agree here, and the fixed-order option is the one
// that satisfies neither. Safari and Firefox keep an explicit fixed order
// below because *those* browsers genuinely emit one — Apple's stack and NSS
// do not shuffle. That is impersonation fidelity, and it is fleet-wide
// constant for the same reason the real browsers' orders are.
//
// MECHANISM
//
// `SSL_CTX_set_permute_extensions(ctx, 1)` makes BoringSSL run
// `ssl_setup_extension_permutation()` once per handshake off the
// `SSL_HANDSHAKE`, seeded from `RAND_bytes` (boringssl `ssl/extensions.cc`,
// called from `ssl/handshake_client.cc`). It is mutually exclusive with the
// boring2 fork's `SSL_CTX_set_extension_permutation()`: the fork consults
// the context permutation only on the `!permute_extensions` path, so a
// fixed order and a per-handshake shuffle can never both be in force.
//
// BoringSSL permutes all 26 slots of its `kExtensions` table rather than a
// curated subset, but a slot emits bytes only when the matching feature is
// configured, so the extension *set* — and hence the JA4 `t13d1516h2`
// extension count — is unchanged; only the order moves. Handing the slot
// list back to BoringSSL also fixes a latent bug in the curated 16-index
// list: it omitted the `cookie` slot, so a second ClientHello following a
// HelloRetryRequest would have silently dropped the server's cookie.

/// Build an `SslConnector` configured with the TLS fingerprint matching
/// `profile.device_class`. Currently all variants share Chrome 147 desktop
/// configuration; this also branches for Android and iOS Safari.
pub fn chrome_connector(profile: &StealthProfile) -> Result<SslConnector, NetError> {
    // Per-device_class branching.
    //  - Desktop / Android: shared Chrome 147 cipher/sigalg/extension config,
    //    with the extension order permuted per handshake by BoringSSL.
    //    Android only diverges in the curves list (Kyber768Draft00 vs MLKEM).
    //  - MobileIOS: distinct Safari 18 cipher/sigalg/curves + a fixed
    //    extension order (Safari does not permute) + zlib cert compression +
    //    SslOptions::NO_TICKET. Per-connection ALPS and ECH grease are also
    //    skipped — see configure_connection() below.
    let is_safari_ios = profile.device_class == DeviceClass::MobileIOS;
    // Firefox wire class: a desktop profile whose browser family is Firefox
    // emits an NSS-class ClientHello (no GREASE, FFDHE groups,
    // delegated_credentials + record_size_limit, fixed extension order)
    // instead of Chrome's. Without this a firefox_135_* profile put a
    // Chrome JA4 under a Firefox UA — an incoherent identity that any JA4↔UA
    // cross-check would flag.
    let is_firefox = profile.browser_name == "Firefox";
    let curves: &[SslCurve] = if is_firefox {
        CURVES_FIREFOX
    } else {
        match profile.device_class {
            DeviceClass::MobileAndroid => CURVES_ANDROID,
            DeviceClass::MobileIOS => CURVES_SAFARI_IOS,
            DeviceClass::Desktop => CURVES_DESKTOP,
        }
    };
    let cipher_list: &str = if is_safari_ios {
        CIPHER_LIST_SAFARI_IOS
    } else if is_firefox {
        CIPHER_LIST_FIREFOX
    } else {
        CIPHER_LIST
    };
    let sigalgs_list: &str = if is_safari_ios {
        SIGALGS_LIST_SAFARI_IOS
    } else if is_firefox {
        SIGALGS_LIST_FIREFOX
    } else {
        SIGALGS_LIST
    };
    let mut builder =
        SslConnector::builder(SslMethod::tls()).map_err(|e| NetError::Tls(e.to_string()))?;

    // Cipher suites (per device_class)
    builder
        .set_cipher_list(cipher_list)
        .map_err(|e| NetError::Tls(e.to_string()))?;

    // Elliptic curves (per device_class)
    builder
        .set_curves(curves)
        .map_err(|e| NetError::Tls(e.to_string()))?;

    // Signature algorithms (per device_class)
    builder
        .set_sigalgs_list(sigalgs_list)
        .map_err(|e| NetError::Tls(e.to_string()))?;

    // ALPN
    builder
        .set_alpn_protos(ALPN_PROTOS)
        .map_err(|e| NetError::Tls(e.to_string()))?;

    // TLS version range. Safari iOS 18.x advertises 4 versions (1.0, 1.1,
    // 1.2, 1.3) in supported_versions per reference Safari iOS captures —
    // visible as a length-difference on the extension. Servers still
    // negotiate 1.3 because no real server speaks 1.0/1.1 anymore, but the
    // ClientHello must advertise all four to fingerprint as Safari.
    let min_version = if is_safari_ios {
        SslVersion::TLS1
    } else {
        SslVersion::TLS1_2
    };
    builder
        .set_min_proto_version(Some(min_version))
        .map_err(|e| NetError::Tls(e.to_string()))?;
    builder
        .set_max_proto_version(Some(SslVersion::TLS1_3))
        .map_err(|e| NetError::Tls(e.to_string()))?;

    // GREASE: Chrome sprinkles GREASE across cipher/group/extension lists;
    // NSS-class Firefox sends NONE. The visible no-GREASE shape is itself a
    // Firefox tell, so disable it for the Firefox arm.
    builder.set_grease_enabled(!is_firefox);

    // Extension order. Chrome permutes per handshake; Safari and Firefox emit
    // a fixed order. See the "Chrome desktop/Android extension order" note
    // above for why we permute rather than pin a fleet-wide Chrome order.
    //
    // The two settings are mutually exclusive in BoringSSL — the fork
    // consults the context permutation only when permute_extensions is off —
    // so exactly one arm below takes effect, and setting both would silently
    // make the fixed order dead.
    let fixed_extension_order: Option<&[u8]> = if is_safari_ios {
        Some(SAFARI_IOS_EXTENSION_PERMUTATION)
    } else if is_firefox {
        // Firefox/NSS emits a FIXED extension order every handshake, like
        // Safari — use the Firefox order verbatim.
        Some(FIREFOX_EXTENSION_PERMUTATION)
    } else {
        None
    };
    builder.set_permute_extensions(fixed_extension_order.is_none());
    if let Some(order) = fixed_extension_order {
        // Safari's PADDING positional ordering still requires raw extension
        // injection (deferred); BoringSSL auto-emits PADDING when the
        // ClientHello length crosses ~512 bytes, which our Safari profile
        // typically does.
        builder
            .set_extension_permutation_indices(order)
            .map_err(|e| NetError::Tls(e.to_string()))?;
    }

    builder.enable_ocsp_stapling();
    builder.enable_signed_cert_timestamps();

    // Chrome 131+ and Firefox 132+ both send two key shares
    // (X25519MLKEM768 + X25519).
    builder.set_key_shares_limit(2);

    // Firefox-only extensions: delegated_credentials (0x22) and
    // record_size_limit (0x1c). Both are hard Firefox/NSS signatures absent
    // from every Chrome build. boring2 4.15 exposes them as builder methods.
    if is_firefox {
        builder
            .set_delegated_credentials(FIREFOX_DELEGATED_CREDENTIALS)
            .map_err(|e| NetError::Tls(e.to_string()))?;
        builder.set_record_size_limit(FIREFOX_RECORD_SIZE_LIMIT);
    }

    // Certificate compression. Chrome desktop+Android = Brotli (algo 2).
    // iOS Safari = Zlib (algo 1). Firefox 135 advertises zlib THEN brotli in
    // its compress_certificate ext (NSS order).
    if is_firefox {
        builder
            .add_cert_compression_alg(CertCompressionAlgorithm::Zlib)
            .map_err(|e| NetError::Tls(e.to_string()))?;
        builder
            .add_cert_compression_alg(CertCompressionAlgorithm::Brotli)
            .map_err(|e| NetError::Tls(e.to_string()))?;
    } else {
        let cert_compress_alg = if is_safari_ios {
            CertCompressionAlgorithm::Zlib
        } else {
            CertCompressionAlgorithm::Brotli
        };
        builder
            .add_cert_compression_alg(cert_compress_alg)
            .map_err(|e| NetError::Tls(e.to_string()))?;
    }

    // iOS Safari does not send the session_ticket extension at all.
    // SslOptions::NO_TICKET tells BoringSSL to omit the extension entirely
    // (vs sending it with a stale ticket).
    if is_safari_ios {
        builder.set_options(SslOptions::NO_TICKET);
    }

    // Load Mozilla root certificates into the certificate store
    let mut cert_store = X509StoreBuilder::new().map_err(|e| NetError::Tls(e.to_string()))?;
    for cert_der in webpki_root_certs::TLS_SERVER_ROOT_CERTS {
        let x509 = X509::from_der(cert_der.as_ref())
            .map_err(|e| NetError::Tls(format!("failed to parse root cert: {e}")))?;
        let _ = cert_store.add_cert(x509);
    }
    builder.set_cert_store(cert_store.build());

    Ok(builder.build())
}

/// Configure a per-connection TLS session with ALPS, ECH GREASE, and SNI.
/// Per-`profile.device_class` branching:
///  - Desktop / Android: ECH grease + ALPS HTTP/2 SETTINGS payload
///  - MobileIOS: skip BOTH (Safari has neither)
pub fn configure_connection(
    connector: &SslConnector,
    profile: &StealthProfile,
    domain: &str,
) -> Result<ConnectConfiguration, NetError> {
    let mut config = connector
        .configure()
        .map_err(|e| NetError::Tls(e.to_string()))?;

    let is_safari_ios = profile.device_class == DeviceClass::MobileIOS;
    let is_firefox = profile.browser_name == "Firefox";

    if !is_safari_ios {
        // ECH GREASE — Chrome desktop+Android AND Firefox all send it.
        // Safari does not.
        config.set_enable_ech_grease(true);
    }

    if !is_safari_ios && !is_firefox {
        // Application-layer settings (ALPS) for HTTP/2.
        // Chrome 147 Headless sends 4 settings: 1, 2, 4, 6.
        // Safari has no ALPS extension at all — skip entirely on iOS.
        // Firefox has no ALPS extension either — skip for the Firefox arm.
        let alps_payload: &[u8] = &[
            // SETTINGS frame (Length 24, Type 4, Flags 0, Stream 0)
            0x00, 0x00, 0x18, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, // ID 1: 65536
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, // ID 2: 0
            0x00, 0x02, 0x00, 0x00, 0x00, 0x00, // ID 4: 6291456
            0x00, 0x04, 0x00, 0x60, 0x00, 0x00, // ID 6: 262144
            0x00, 0x06, 0x00, 0x04, 0x00, 0x00,
            // Empty ACCEPT_CH frame (Length 0, Type 0x89, Flags 0, Stream 0)
            0x00, 0x00, 0x00, 0x89, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        // SAFETY: BoringSSL's `SSL_add_application_settings` reads the
        // ALPN name (`b"h2"`, length 2) and the ALPS payload buffer
        // (`alps_payload` — a contiguous static slice we built above);
        // both are valid, contiguous, non-null, and live for the
        // entire call. `config.as_ptr()` returns a non-null pointer
        // to the live `SslContext` we own here. BoringSSL only reads
        // these buffers; it copies the data into the SSL_CTX, no
        // ownership transfer.
        unsafe {
            if boring_sys2::SSL_add_application_settings(
                config.as_ptr(),
                b"h2".as_ptr(),
                2,
                alps_payload.as_ptr(),
                alps_payload.len(),
            ) != 1
            {
                return Err(NetError::Tls("failed to add ALPS settings".into()));
            }
        }
        config.set_alps_use_new_codepoint(true);
    }

    // SNI is the same for all profiles.
    let sni_domain = domain.trim_start_matches('[').trim_end_matches(']');
    if sni_domain.parse::<std::net::IpAddr>().is_ok() {
        config.set_use_server_name_indication(false);
    } else {
        config
            .set_hostname(sni_domain)
            .map_err(|e| NetError::Tls(e.to_string()))?;
    }

    Ok(config)
}

/// Establish a TLS connection to `domain` using the provided `SslConnector`.
pub async fn connect_tls(
    connector: &SslConnector,
    profile: &StealthProfile,
    domain: &str,
    stream: TcpStream,
) -> Result<SslStream<TcpStream>, NetError> {
    let config = configure_connection(connector, profile, domain)?;
    let sni_domain = domain.trim_start_matches('[').trim_end_matches(']');

    tokio_boring2::connect(config, sni_domain, stream)
        .await
        .map_err(|e| NetError::Tls(format!("TLS handshake failed: {e}")))
}

/// Returns the negotiated ALPN protocol from a TLS stream, if any.
pub fn negotiated_alpn(stream: &SslStream<TcpStream>) -> Option<&[u8]> {
    stream.ssl().selected_alpn_protocol()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Self-verifying JA4 drift guard + UA/TLS coherence assert.
    /// Network-free.
    ///
    /// Pins every JA4 input (cipher list, sigalg list, supported-groups
    /// order, extension count) byte-/element-exact to the verified-real
    /// Chrome reference so the fingerprint can never silently drift
    /// again (any edit to
    /// the constants fails this test loudly), and machine-checks that
    /// the deliberate UA=148 / TLS-ref=147 split is the documented,
    /// wire-coherent one (see [`TLS_CHROME_MAJOR`] docs).
    #[test]
    fn tls_fingerprint_vectors_no_silent_drift() {
        // --- JA4 input 1: cipher suites (order is JA4-significant) ---
        const EXPECT_CIPHERS: &str = "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:\
TLS_CHACHA20_POLY1305_SHA256:TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256:\
TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256:TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384:\
TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384:TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256:\
TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256:TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA:\
TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA:TLS_RSA_WITH_AES_128_GCM_SHA256:\
TLS_RSA_WITH_AES_256_GCM_SHA384:TLS_RSA_WITH_AES_128_CBC_SHA:\
TLS_RSA_WITH_AES_256_CBC_SHA";
        assert_eq!(
            CIPHER_LIST, EXPECT_CIPHERS,
            "Chrome cipher list drifted from the verified-real reference \
             — JA4 cipher hash would change"
        );

        // --- JA4 input 2: signature algorithms (order is JA4-significant) ---
        const EXPECT_SIGALGS: &str = "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:\
rsa_pkcs1_sha256:ecdsa_secp384r1_sha384:rsa_pss_rsae_sha384:rsa_pkcs1_sha384:\
rsa_pss_rsae_sha512:rsa_pkcs1_sha512";
        assert_eq!(
            SIGALGS_LIST, EXPECT_SIGALGS,
            "Chrome sigalg list drifted — JA4 sigalg hash would change"
        );

        // --- JA4 input 3: supported groups / curves order ---
        assert_eq!(
            CURVES_DESKTOP,
            &[
                SslCurve::X25519_MLKEM768,
                SslCurve::X25519,
                SslCurve::SECP256R1,
                SslCurve::SECP384R1,
            ],
            "Chrome desktop curve order drifted (post-quantum MLKEM768 \
             must lead) — JA4 supported_groups would change"
        );

        // --- JA4 input 4: extension count (16 — JA4 `c` digit) ---
        // The extension *set* is no longer a hand-curated list; BoringSSL
        // derives it from the builder config (see the extension-order note in
        // this file). CHROME_CLIENTHELLO_EXTENSIONS records what that config
        // is expected to put on the wire, and
        // `chrome_extension_order_is_redrawn_per_handshake` checks it against
        // a real ClientHello rather than against another constant.
        assert_eq!(
            CHROME_CLIENTHELLO_EXTENSIONS.len(),
            16,
            "Chrome extension count drifted — JA4 extension-count digit \
             would change"
        );

        // --- UA / TLS coherence (the deliberate, wire-equivalent split) ---
        assert_eq!(TLS_CHROME_MAJOR, 147);
        assert_eq!(UA_CHROME_MAJOR, 148);
        // The split is intentional and wire-coherent: Chrome's
        // ClientHello did not rev 147→148, JA4 cannot encode the Chrome
        // version, and UA=148 is the A/B-tested current-Chrome value.
        // (Rationale in TLS_CHROME_MAJOR docs.)

        fn ua_chrome_major(ua: &str) -> Option<u32> {
            let i = ua.find("Chrome/")? + "Chrome/".len();
            ua[i..].split('.').next()?.parse().ok()
        }

        for profile in [
            crate::stealth::presets::chrome_148_macos(),
            crate::stealth::presets::chrome_148_windows(),
        ] {
            assert_eq!(
                ua_chrome_major(&profile.user_agent),
                Some(UA_CHROME_MAJOR),
                "desktop Chrome preset UA major must equal UA_CHROME_MAJOR \
                 (the coherence single-source-of-truth); UA was {:?}",
                profile.user_agent
            );
            assert_eq!(
                profile.tls_impersonate, "chrome_147",
                "desktop Chrome preset TLS profile must be the verified-real \
                 chrome_147 reference (wire-equivalent to Chrome \
                 {UA_CHROME_MAJOR}); see TLS_CHROME_MAJOR docs"
            );
        }
    }

    /// Capture the first 5 bytes of our outbound ClientHello (the TLS
    /// record header) and assert the record version is 0x0301 (TLS 1.0).
    /// Source-code analysis of `boringssl/src/ssl/ssl_aead_ctx.cc:168-173`
    /// confirms `RecordVersion()` returns `TLS1_VERSION` (0x0301) for the
    /// initial ClientHello (null cipher, version_ == 0). This test verifies
    /// it empirically — a BoringSSL source patch for the TLS 1.0 record
    /// version is **NOT NEEDED**.
    #[tokio::test]
    async fn safari_ios_emits_tls_1_0_record_version() {
        use tokio::io::AsyncReadExt;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Background server that just reads the first 5 bytes and reports.
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 5];
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                stream.read_exact(&mut buf),
            )
            .await
            .unwrap()
            .unwrap();
            buf
        });

        // Connect with iOS Safari profile.
        let profile = crate::stealth::presets::iphone_15_pro_safari_18();
        let connector = chrome_connector(&profile).expect("connector");
        let tcp = TcpStream::connect(addr).await.unwrap();
        // We expect the handshake to fail (server doesn't respond), but the
        // ClientHello is sent before that. Race the timeout against the
        // server's read.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            connect_tls(&connector, &profile, "localhost", tcp),
        )
        .await;

        let bytes = tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("server timeout")
            .expect("server task");

        let content_type = bytes[0];
        let record_version = ((bytes[1] as u16) << 8) | (bytes[2] as u16);

        // Content type 22 = TLS handshake
        assert_eq!(
            content_type, 22,
            "expected TLS handshake (22), got {content_type}"
        );

        // Record version: real Safari sends 0x0301 (TLS 1.0); BoringSSL
        // emits the same for null-cipher (initial ClientHello).
        assert_eq!(
            record_version, 0x0301,
            "iOS Safari record version: got 0x{record_version:04x}, expected 0x0301 (TLS 1.0). \
             If this is 0x0303 then a BoringSSL source patch IS needed; if 0x0301 then \
             our current build already matches Safari."
        );
    }

    /// Same record-version check for desktop Chrome profile. Real Chrome
    /// also sends 0x0301 (TLS 1.0) record version for the initial ClientHello
    /// — TLS-version selection happens in the inner extension, not the outer
    /// record header. This test confirms the BoringSSL behavior is uniform
    /// across desktop and Safari profiles.
    #[tokio::test]
    async fn desktop_chrome_emits_tls_1_0_record_version() {
        use tokio::io::AsyncReadExt;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 5];
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                stream.read_exact(&mut buf),
            )
            .await
            .unwrap()
            .unwrap();
            buf
        });

        let profile = crate::stealth::presets::chrome_148_macos();
        let connector = chrome_connector(&profile).expect("connector");
        let tcp = TcpStream::connect(addr).await.unwrap();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            connect_tls(&connector, &profile, "localhost", tcp),
        )
        .await;

        let bytes = tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("server timeout")
            .expect("server task");

        let record_version = ((bytes[1] as u16) << 8) | (bytes[2] as u16);
        assert_eq!(
            record_version, 0x0301,
            "Chrome desktop record version: got 0x{record_version:04x}, expected 0x0301."
        );
    }

    /// The 16 extension types a desktop Chrome ClientHello carries, as wire
    /// codepoints, sorted. GREASE and PADDING are excluded: BoringSSL emits
    /// both outside the permutation, and JA4 excludes them from its count.
    /// This is the JA4 `t13d15**16**h2` extension count.
    const CHROME_CLIENTHELLO_EXTENSIONS: &[u16] = &[
        0,     // server_name
        5,     // status_request
        10,    // supported_groups
        11,    // ec_point_formats
        13,    // signature_algorithms
        16,    // application_layer_protocol_negotiation
        18,    // signed_certificate_timestamp
        23,    // extended_master_secret
        27,    // compress_certificate
        35,    // session_ticket
        43,    // supported_versions
        45,    // psk_key_exchange_modes
        51,    // key_share
        17613, // application_settings (new codepoint)
        65037, // encrypted_client_hello (GREASE ECH)
        65281, // renegotiation_info
    ];

    /// True for the 16 GREASE codepoints (RFC 8701): both bytes equal and of
    /// the form `0x?A`.
    fn is_grease(ext_type: u16) -> bool {
        (ext_type >> 8) == (ext_type & 0x00ff) && (ext_type & 0x000f) == 0x000a
    }

    /// Pull the extension types, in wire order, out of one captured
    /// ClientHello record payload (i.e. the bytes after the 5-byte TLS record
    /// header). GREASE and PADDING (21) are dropped, matching JA4's rules and
    /// leaving exactly the extensions the permutation governs.
    fn clienthello_extension_types(record_payload: &[u8]) -> Vec<u16> {
        let be16 = |b: &[u8], i: usize| ((b[i] as u16) << 8) | b[i + 1] as u16;

        assert_eq!(
            record_payload[0], 1,
            "expected a ClientHello handshake message"
        );
        let hs_len = ((record_payload[1] as usize) << 16)
            | ((record_payload[2] as usize) << 8)
            | record_payload[3] as usize;
        // A Chrome ClientHello with an MLKEM768 key share runs ~1.9 kB, far
        // under the 16 kB record limit, so it always arrives whole. Assert it
        // rather than silently parsing a fragment.
        assert_eq!(
            hs_len,
            record_payload.len() - 4,
            "ClientHello spans more than one TLS record — parser would need \
             to reassemble"
        );
        let body = &record_payload[4..];

        let mut p = 2 + 32; // legacy_version + random
        p += 1 + body[p] as usize; // legacy_session_id
        p += 2 + be16(body, p) as usize; // cipher_suites
        p += 1 + body[p] as usize; // legacy_compression_methods
        let ext_end = p + 2 + be16(body, p) as usize;
        p += 2;

        let mut types = Vec::new();
        while p + 4 <= ext_end {
            let ext_type = be16(body, p);
            let ext_len = be16(body, p + 2) as usize;
            p += 4 + ext_len;
            if !is_grease(ext_type) && ext_type != 21 {
                types.push(ext_type);
            }
        }
        assert_eq!(p, ext_end, "extension block did not parse cleanly");
        types
    }

    /// Drive one real handshake attempt from `connector` against a throwaway
    /// listener and return the extension types its ClientHello carried, in
    /// wire order. The handshake never completes — the listener reads the
    /// ClientHello and hangs up — which is all we need, because the
    /// permutation is chosen while the ClientHello is being built.
    async fn capture_clienthello_extensions(
        connector: &SslConnector,
        profile: &StealthProfile,
    ) -> Vec<u16> {
        use tokio::io::AsyncReadExt;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut header = [0u8; 5];
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                stream.read_exact(&mut header),
            )
            .await
            .expect("timed out reading TLS record header")
            .unwrap();
            let mut payload = vec![0u8; ((header[3] as usize) << 8) | header[4] as usize];
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                stream.read_exact(&mut payload),
            )
            .await
            .expect("timed out reading ClientHello")
            .unwrap();
            payload
        });

        let tcp = TcpStream::connect(addr).await.unwrap();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            connect_tls(connector, profile, "localhost", tcp),
        )
        .await;

        let payload = tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .expect("capture server timed out")
            .expect("capture server task");
        clienthello_extension_types(&payload)
    }

    /// **Regression test for the per-session TLS supercookie.**
    ///
    /// Two handshakes from **one** `SslConnector` must not present the same
    /// extension order. That "one connector" is the whole point: `HttpClient`
    /// builds a connector once in `new()` and holds it as `Arc<SslConnector>`
    /// for every `connect_tls` it ever makes (`net/mod.rs`), so a connector
    /// reused across two handshakes *is* a client reused across two origins.
    ///
    /// The previous test compared two calls of a shuffle helper, which was
    /// non-deterministic in isolation while the order the wire actually saw
    /// was fixed for the client's lifetime — so it passed throughout.
    ///
    /// The false-failure risk is one in 16! (≈ 5×10⁻¹⁴) per run, which is far
    /// below the flake floor of anything else in this suite.
    #[tokio::test]
    async fn chrome_extension_order_is_redrawn_per_handshake() {
        let profile = crate::stealth::presets::chrome_148_macos();
        let connector = chrome_connector(&profile).expect("connector");

        let first = capture_clienthello_extensions(&connector, &profile).await;
        let second = capture_clienthello_extensions(&connector, &profile).await;

        // The set must be Chrome's, unchanged — permuting must not add or
        // drop an extension, or the JA4 count digit moves.
        for (label, observed) in [("first", &first), ("second", &second)] {
            let mut sorted = (*observed).clone();
            sorted.sort_unstable();
            assert_eq!(
                sorted, CHROME_CLIENTHELLO_EXTENSIONS,
                "{label} ClientHello carried a different extension set than \
                 the Chrome reference — JA4 would change"
            );
        }

        // ...and the order must be redrawn, as real Chrome has done since 110.
        assert_ne!(
            first, second,
            "two handshakes from the SAME connector sent an identical \
             extension order: the permutation is fixed for the client's \
             lifetime and is therefore a ~44-bit cross-site identifier"
        );
    }

    /// The mirror of the above: profiles that impersonate a browser which
    /// genuinely does *not* permute must stay byte-stable across handshakes.
    /// Without this, "fix the supercookie" could be satisfied by permuting
    /// everything, which would break Safari and Firefox fidelity.
    #[tokio::test]
    async fn safari_and_firefox_extension_order_is_fixed_per_handshake() {
        for profile in [
            crate::stealth::presets::iphone_15_pro_safari_18(),
            crate::stealth::presets::firefox_135_windows(),
        ] {
            let connector = chrome_connector(&profile).expect("connector");
            let first = capture_clienthello_extensions(&connector, &profile).await;
            let second = capture_clienthello_extensions(&connector, &profile).await;
            assert_eq!(
                first, second,
                "{} emits a fixed extension order in reality; ours varied \
                 between handshakes",
                profile.browser_name
            );
            assert!(
                !first.is_empty(),
                "captured no extensions for {}",
                profile.browser_name
            );
        }
    }
}
