//! Isolate-level timezone and locale override.
//!
//! WHY this module exists instead of more JavaScript patches.
//!
//! The stealth profile claims an IANA timezone. Until this module existed, that
//! claim was implemented entirely as a JavaScript veneer over `Intl` and a
//! handful of `Date.prototype` methods. V8 itself was never told anything, so
//! every accessor the veneer had not thought of — `getHours`, `getDate`,
//! `getDay`, `getMonth`, `getFullYear`, `toLocaleTimeString`,
//! `toLocaleDateString`, and the whole `setFullYear`/`setHours` family — went
//! on computing local time in the *host* zone. On a Los Angeles host running a
//! preset claiming `Europe/Moscow`, `getHours()` returned 15 while
//! `toString()` returned hour 1.
//!
//! That is worse than not spoofing at all. Subtracting one clock from the other
//! recovers the host's true UTC offset exactly, and the disagreement between
//! two accessors that can never disagree in a real browser is itself a reliable
//! "this browser is lying" signal — which is precisely what commercial VPN and
//! bot detection keys on.
//!
//! Patching accessors one at a time cannot fix this: the list is open-ended and
//! grows with each V8 release. The fix has to land underneath JavaScript, at
//! the point where V8 asks what local time is.
//!
//! V8 asks ICU. With `--icu-timezone-data` (on by default) V8's `DateCache`
//! resolves local time through `ICUTimezoneCache`, which reads ICU's *default*
//! timezone. Set that before the isolate exists, and every `Date` accessor —
//! including ones added by future V8 versions — reports the profile's zone with
//! no JavaScript involved.
//!
//! The same reasoning applies to the default *locale*: `toLocaleTimeString()`
//! with no arguments formats in ICU's default locale, so a Moscow-claiming
//! profile on a Canadian-English host rendered `"3:00:08 p.m."` where `en-US`
//! requires `"3:00:08 PM"`. `v8::icu::set_default_locale` is exposed by the
//! `v8` crate, so that half needs no FFI of our own.
//!
//! ## Why the raw ICU symbol
//!
//! The `v8` crate (149.4.0, ICU 77) exposes `icu::set_default_locale` but no
//! timezone equivalent. It does, however, already link ICU's C API into the
//! same static archive under ICU's standard version-suffix renaming — the
//! crate's own `icu.rs` declares `udata_setCommonData_77` exactly this way. We
//! declare `ucal_setDefaultTimeZone_77` by the same convention rather than
//! reimplementing the veneer, and pin the expectation with a test: if a future
//! `v8` bump changes the ICU version the link fails loudly at build time rather
//! than silently reverting to a host-timezone leak.
//!
//! ## Process-global scope
//!
//! ICU's default zone and locale are per *process*, not per isolate. That
//! matches how this engine is deployed — one process holds one session and
//! therefore one stealth profile (see `docs/ADR-009-PROCESS-MODEL.md`). Workers
//! run on their own threads inside that process and so need no parallel
//! JavaScript patch of their own; `create_worker_runtime` still calls
//! `install_process_defaults` before building the worker isolate, because what
//! is per-isolate is V8's *cache* of the ICU defaults, not the defaults. If two
//! profiles claiming different zones are ever driven from one process we log a
//! warning, because the second one silently wins for both.

use std::sync::Mutex;

// SAFETY of the declaration itself: this is ICU 77's stable C API as linked
// into the `v8` crate's static library. `zone_id` is a NUL-terminated UTF-16
// string; `status` is a `UErrorCode` (an `i32`) that must be pre-set to
// `U_ZERO_ERROR` (0) or ICU returns immediately without doing anything.
unsafe extern "C" {
    fn ucal_setDefaultTimeZone_77(zone_id: *const u16, status: *mut i32);
    fn ucal_getDefaultTimeZone_77(result: *mut u16, capacity: i32, status: *mut i32) -> i32;
}

/// What ICU installs when it does not recognise an identifier. It does *not*
/// report an error in that case, so a typo'd zone would otherwise sail through
/// and leave every `Date` accessor on GMT while the profile claimed otherwise.
const ICU_UNKNOWN_ZONE: &str = "Etc/Unknown";

/// The zone most recently pushed into ICU, so a second runtime in the same
/// process can detect that it is about to change a global out from under a
/// live isolate.
static APPLIED_ZONE: Mutex<Option<String>> = Mutex::new(None);

/// Read back ICU's process-wide default zone, so we can tell an accepted
/// identifier from one ICU quietly turned into `Etc/Unknown`.
fn icu_default_timezone() -> String {
    // IANA identifiers are far shorter than this; ICU truncates rather than
    // overruns, and a truncated read would only ever fail the equality check.
    let mut buf = [0u16; 128];
    let mut status: i32 = 0;
    // SAFETY: `buf` is a live, correctly sized UTF-16 buffer and we pass its
    // true capacity; `status` is a live, zero-initialised `UErrorCode`.
    let len =
        unsafe { ucal_getDefaultTimeZone_77(buf.as_mut_ptr(), buf.len() as i32, &mut status) };
    if status > 0 || len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())])
}

/// Push `zone` into ICU's process-wide default.
///
/// Returns the canonical identifier ICU actually installed, or `None` when the
/// identifier was not usable — in which case local time is still the host's and
/// the caller must say so rather than assume the override took.
fn set_icu_default_timezone(zone: &str) -> Option<String> {
    // ICU wants NUL-terminated UTF-16.
    let utf16: Vec<u16> = zone.encode_utf16().chain(std::iter::once(0)).collect();
    let mut status: i32 = 0;
    // SAFETY: `utf16` is NUL-terminated and outlives the call; ICU copies the
    // identifier. `status` is a live, zero-initialised `UErrorCode`.
    unsafe {
        ucal_setDefaultTimeZone_77(utf16.as_ptr(), &mut status);
    }
    // UErrorCode: positive values are failures, negative ones are warnings.
    // ICU does not report unknown zones through it at all, hence the readback.
    if status > 0 {
        return None;
    }
    let installed = icu_default_timezone();
    if installed.is_empty() || installed == ICU_UNKNOWN_ZONE {
        None
    } else {
        Some(installed)
    }
}

/// Point ICU's process-wide defaults at the profile's timezone and locale.
///
/// `zone` is an IANA identifier (`"Europe/Moscow"`); `locale` is a BCP 47 tag
/// (`"ru-RU"`). Either may be empty, meaning "leave the host default alone".
///
/// **Call this before the isolate that must observe it is created**, and note
/// that "created" includes the bootstrap scripts deno_core runs inside
/// `JsRuntime::new`.
///
/// The ordering is not a nicety, it is the whole fix for the locale half. V8
/// resolves `icu::Locale::getDefault()` once per isolate and caches the answer
/// in `Isolate::default_locale()`; every later no-argument `toLocaleTimeString`
/// / `Intl` construction reads the cache, not ICU. V8 can be told to drop that
/// cache — `Isolate::LocaleConfigurationChangeNotification` exists — but the
/// `v8` crate binds no such method, so there is no way back once the isolate
/// has looked. Setting ICU first is the only ordering that works, and getting
/// it wrong is silent: ICU reports the profile's locale, `Intl` agrees because
/// the bootstrap rewrites `resolvedOptions`, and only `toLocaleTimeString()`
/// with no arguments quietly keeps spelling the host's "a.m.".
pub fn install_process_defaults(zone: &str, locale: &str) {
    if !zone.is_empty() {
        let mut applied = APPLIED_ZONE.lock().unwrap_or_else(|e| e.into_inner());
        if applied.as_deref().is_some_and(|prev| prev != zone) {
            // Two profiles, one process. Whoever ran last wins for every
            // isolate, so the earlier one now contradicts its own claim —
            // exactly the inconsistency this module exists to remove.
            tracing::warn!(
                previous = %applied.as_deref().unwrap_or(""),
                requested = %zone,
                "stealth timezone changed process-wide; isolates created earlier \
                 will now report the new zone (ICU's default is per-process)"
            );
        }
        match set_icu_default_timezone(zone) {
            Some(installed) => *applied = Some(installed),
            None => tracing::warn!(
                timezone = %zone,
                "ICU rejected the profile timezone; Date accessors will report \
                 the host zone and contradict the profile"
            ),
        }
    }

    if !locale.is_empty() {
        // Drives the no-argument forms of `toLocaleString` / `toLocaleTimeString`
        // / `toLocaleDateString` and the default locale of every `Intl`
        // constructor — including `ListFormat`, `Segmenter`, `DisplayNames` and
        // `DurationFormat`, which no JS patch covers.
        deno_core::v8::icu::set_default_locale(locale);
    }
}

/// Drop `isolate`'s cached local-time offsets so its next `Date` operation
/// re-reads ICU's default zone.
///
/// Call after the isolate exists. `install_process_defaults` running first is
/// what makes the isolate correct; this makes it correct *independently* of
/// when V8 happens to build its per-isolate `DateCache`, which is an
/// implementation detail of a vendored V8 that nothing here pins.
pub fn resync_isolate(isolate: &mut deno_core::v8::Isolate) {
    // `Skip`, emphatically not `Redetect`, despite the name reading the wrong
    // way round. `Redetect` makes V8 call
    // `icu::TimeZone::adoptDefault(icu::TimeZone::detectHostTimeZone())`, which
    // throws away the zone we installed and puts the HOST's back — reinstating
    // the leak. `Skip` clears the isolate's cached offsets and zone names
    // without re-detecting, so the next `Date` operation reads the ICU default.
    isolate.date_time_configuration_change_notification(deno_core::v8::TimeZoneDetection::Skip);
}

/// The timezone currently installed process-wide, if any.
///
/// Bootstrap JS consults this (through `op_native_timezone`) to decide whether
/// the legacy `Date.prototype` veneer is still needed.
pub fn applied_timezone() -> Option<String> {
    APPLIED_ZONE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stealth::{presets, StealthProfile};
    // `scope_with_context!` expands to unqualified `v8::` paths.
    use deno_core::v8;

    /// Reads every timezone-revealing surface off ONE `Date` instance, so the
    /// readings cannot disagree merely because the clock moved between them.
    ///
    /// Deliberately samples three instants: now, a January instant and a July
    /// one. The last two straddle DST in most zones, which catches an override
    /// that is a fixed offset rather than a real zone.
    const PROBE_JS: &str = r#"(() => {
        const out = [];
        for (const ms of [Date.now(), 1735689600000, 1751328000000]) {
            const d = new Date(ms);
            const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
            const locale = Intl.DateTimeFormat().resolvedOptions().locale;

            // What the claimed zone says the wall-clock hour is, asked of a
            // formatter that is told the zone explicitly.
            const parts = new Intl.DateTimeFormat('en-US', {
                timeZone: tz, hour: '2-digit', hour12: false,
                minute: '2-digit', weekday: 'short', day: '2-digit',
            }).formatToParts(d);
            const part = (t) => parts.find((p) => p.type === t).value;

            // What getTimezoneOffset() implies the wall-clock hour is.
            const offset = d.getTimezoneOffset();
            const shifted = new Date(ms - offset * 60000);

            // "Wed Jul 29 2026 01:00:08 GMT+0300 (Moscow Standard Time)"
            const ts = /(\d{2}):(\d{2}):(\d{2}) GMT([+-]\d{4})/.exec(d.toString());

            out.push({
                tz,
                locale,
                getHours: d.getHours(),
                getMinutes: d.getMinutes(),
                getDate: d.getDate(),
                // Compared as a NAME so a wrong-by-a-whole-day override cannot
                // hide behind modular arithmetic; the formatter is pinned to
                // 'en-US' above, so the spelling is stable.
                getDay: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][d.getDay()],
                intlWeekday: part('weekday'),
                intlHour: Number(part('hour')) % 24,
                intlDay: Number(part('day')),
                offsetHour: shifted.getUTCHours(),
                offsetDate: shifted.getUTCDate(),
                toStringHour: Number(ts[1]),
                toStringMinute: Number(ts[2]),
                toTimeStringHour: Number(d.toTimeString().slice(0, 2)),
                // Default-argument locale formatting must be identical to
                // asking explicitly for the profile's locale and zone. This is
                // the accessor that leaked BOTH the host hour and the host
                // locale's am/pm spelling.
                localeTimeMatches:
                    d.toLocaleTimeString() === d.toLocaleTimeString(locale, { timeZone: tz }),
                localeDateMatches:
                    d.toLocaleDateString() === d.toLocaleDateString(locale, { timeZone: tz }),
                localeTime: d.toLocaleTimeString(),
            });
        }
        return JSON.stringify(out);
    })()"#;

    /// Assert that no two timezone-revealing APIs disagree.
    ///
    /// A test that checked one accessor would have passed against the bug this
    /// exists to prevent: `Intl`, `getTimezoneOffset` and `toString` were all
    /// correct on their own while `getHours` returned the host's hour.
    fn assert_consistent(json: &str, expected_tz: &str, realm: &str) {
        let samples: Vec<serde_json::Value> =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("{realm}: bad probe JSON: {e}"));
        assert_eq!(samples.len(), 3, "{realm}: expected three sampled instants");

        for (i, s) in samples.iter().enumerate() {
            let n = |k: &str| {
                s[k].as_i64()
                    .unwrap_or_else(|| panic!("{realm}[{i}]: {k} missing"))
            };
            let at = format!("{realm}, instant {i}");

            assert_eq!(
                s["tz"].as_str(),
                Some(expected_tz),
                "{at}: Intl reports the wrong zone"
            );
            assert_eq!(
                n("getHours"),
                n("intlHour"),
                "{at}: getHours disagrees with Intl in the claimed zone — this is \
                 the host-timezone leak (getHours={}, Intl={})",
                n("getHours"),
                n("intlHour"),
            );
            assert_eq!(
                n("getHours"),
                n("offsetHour"),
                "{at}: getHours disagrees with getTimezoneOffset"
            );
            assert_eq!(
                n("getHours"),
                n("toStringHour"),
                "{at}: getHours disagrees with Date.prototype.toString"
            );
            assert_eq!(
                n("getHours"),
                n("toTimeStringHour"),
                "{at}: getHours disagrees with Date.prototype.toTimeString"
            );
            assert_eq!(
                n("getMinutes"),
                n("toStringMinute"),
                "{at}: getMinutes disagrees with Date.prototype.toString"
            );
            assert_eq!(
                n("getDate"),
                n("intlDay"),
                "{at}: getDate disagrees with Intl in the claimed zone"
            );
            assert_eq!(
                n("getDate"),
                n("offsetDate"),
                "{at}: getDate disagrees with getTimezoneOffset"
            );
            assert_eq!(
                s["getDay"].as_str(),
                s["intlWeekday"].as_str(),
                "{at}: getDay disagrees with Intl in the claimed zone"
            );
            assert_eq!(
                s["localeTimeMatches"].as_bool(),
                Some(true),
                "{at}: toLocaleTimeString() with no arguments does not match the \
                 profile's locale+zone — it returned {:?}",
                s["localeTime"],
            );
            assert_eq!(
                s["localeDateMatches"].as_bool(),
                Some(true),
                "{at}: toLocaleDateString() with no arguments does not match the \
                 profile's locale+zone"
            );
        }
    }

    fn moscow_profile() -> StealthProfile {
        presets::chrome_148_ru()
    }

    /// UTC+14 with no DST. Whatever the host zone is — including
    /// `Europe/Moscow` on a Russian developer's machine — this one differs, so
    /// the suite cannot pass by accident the way the original probe did when
    /// the macOS preset happened to match the host.
    fn far_side_profile() -> StealthProfile {
        presets::with_locale(
            presets::chrome_148_windows(),
            "en-US",
            &["en-US", "en"],
            "Pacific/Kiritimati",
        )
    }

    /// The whole design rests on ICU 77's C API being present in the `v8`
    /// crate's archive under the `_77` suffix. If a `v8` bump moves to ICU 78
    /// this test's *link* fails, which is the loud failure we want — silently
    /// losing the override would reinstate the host-timezone leak.
    #[test]
    fn icu_accepts_iana_zone_and_rejects_nonsense() {
        assert_eq!(
            set_icu_default_timezone("Europe/Moscow").as_deref(),
            Some("Europe/Moscow"),
            "ICU should accept a valid IANA identifier"
        );
        assert_eq!(
            set_icu_default_timezone("Not/AZone"),
            None,
            "an unknown identifier must be reported, not silently turned into \
             Etc/Unknown while the profile keeps claiming a real zone"
        );
        // Leave the process in a defined state for whatever runs next.
        assert!(set_icu_default_timezone("UTC").is_some());
    }

    #[test]
    fn page_realm_timezone_accessors_all_agree() {
        for profile in [moscow_profile(), far_side_profile()] {
            let expected = profile.timezone.clone();
            let dom = crate::html_parser::parse_html("<html><body></body></html>");
            let mut rt = crate::js_runtime::BrowserJsRuntime::with_profile(dom, profile);
            let json = rt.execute_script(PROBE_JS, None).expect("probe script");
            assert_consistent(&json, &expected, "page");
        }
    }

    /// Workers were entirely unpatched: `worker_bootstrap.js` overrode only
    /// `resolvedOptions`, so the realm contradicted itself outright. Sensors
    /// re-read time inside a Worker for exactly this reason, so the fix is only
    /// a fix if it reaches here.
    #[test]
    fn worker_realm_timezone_accessors_all_agree() {
        for profile in [moscow_profile(), far_side_profile()] {
            let expected = profile.timezone.clone();
            let mut rt = crate::js_runtime::runtime::create_worker_runtime(Some(profile), true);
            let global = rt
                .execute_script("<anonymous>", PROBE_JS.to_string())
                .expect("worker probe script");
            let json = {
                let ctx = rt.main_context();
                v8::scope_with_context!(scope, rt.v8_isolate(), ctx);
                let local = v8::Local::new(scope, global);
                local
                    .to_string(scope)
                    .expect("probe result should stringify")
                    .to_rust_string_lossy(scope)
            };
            assert_consistent(&json, &expected, "worker");
        }
    }

    /// The point of moving the override under JavaScript is that accessors
    /// nobody patched now behave too. `getFullYear`/`getMonth` and the
    /// `setHours` family were never in the veneer.
    #[test]
    fn unpatched_date_accessors_follow_the_profile_zone() {
        let profile = far_side_profile();
        let dom = crate::html_parser::parse_html("<html><body></body></html>");
        let mut rt = crate::js_runtime::BrowserJsRuntime::with_profile(dom, profile);

        // 2025-01-01T00:00:00Z is 2025-01-01 14:00 in Kiritimati (UTC+14):
        // same date, but the year/month boundary makes a host-zone reading
        // west of UTC land in 2024.
        let parts = rt
            .execute_script(
                "(() => { const d = new Date(1735689600000); \
                  return [d.getFullYear(), d.getMonth(), d.getDate(), d.getHours()].join(','); })()",
                None,
            )
            .expect("accessor script");
        assert_eq!(
            parts, "2025,0,1,14",
            "unpatched Date accessors must read local time in the profile's zone"
        );

        // `setHours` writes through the same local-time conversion. Setting
        // local midnight and reading it back must round-trip.
        let round_trip = rt
            .execute_script(
                "(() => { const d = new Date(1735689600000); d.setHours(0, 0, 0, 0); \
                  return d.getHours() + ':' + d.getDate(); })()",
                None,
            )
            .expect("setter script");
        assert_eq!(
            round_trip, "0:1",
            "Date setters must use the same local zone as the getters"
        );
    }
}
