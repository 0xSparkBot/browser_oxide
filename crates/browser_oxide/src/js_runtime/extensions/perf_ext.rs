//! Chrome-shaped `performance.now()`.
//!
//! Chrome 148 exposes a 100 µs grid without cross-origin isolation. In a
//! tight loop the smallest positive delta is therefore 0.1 ms. Adding fresh
//! sub-grid noise on every call is observably wrong: adjacent calls can then
//! differ by only a few nanoseconds even while the underlying clock remains
//! in the same 100 µs bucket.

use crate::js_runtime::state::DomState;
use deno_core::op2;
use deno_core::OpState;
use std::sync::OnceLock;

/// Per-runtime state for the humanized clock.
pub struct PerfState {
    /// Process-relative origin in microseconds. Blink clamps this absolute
    /// monotonic coordinate before converting it to milliseconds and
    /// subtracting it from the similarly clamped current coordinate.
    origin_monotonic_us: i64,
    /// Wall-clock (UNIX epoch ms) corresponding to `origin`. Read by
    /// `op_perf_time_origin_ms` so JS `performance.timeOrigin` honors the
    /// invariant `timeOrigin + performance.now() ≈ Date.now()`. Real
    /// Chrome maintains this invariant; without it, an earlier JS-side
    /// ad-hoc computation (`Date.now() - <hardcoded nav_end>`) produced a
    /// detectable skew between `performance.timeOrigin + performance.now()`
    /// and `Date.now()`.
    origin_unix_ms: f64,
    /// Last returned value in ms — enforces monotonicity per HRT spec.
    last_ms: f64,
    /// Network-backed navigation metadata, when this realm was created from
    /// an HTTP response rather than synthetic HTML/about:blank.
    navigation_timing: Option<crate::net::TimingStats>,
    /// Per-process Blink-style time-clamper secret.
    clamper_secret: u64,
}

impl PerfState {
    pub fn new() -> Self {
        Self::with_seed(*process_clamper_secret())
    }
    pub fn with_seed(seed: u64) -> Self {
        let origin_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        Self {
            origin_monotonic_us: process_monotonic_us(),
            origin_unix_ms,
            last_ms: 0.0,
            navigation_timing: None,
            clamper_secret: seed,
        }
    }

    pub fn with_navigation(navigation_timing: Option<crate::net::TimingStats>) -> Self {
        let Some(timing) = navigation_timing else {
            return Self::new();
        };
        if !timing.time_origin_unix_ms.is_finite() || timing.time_origin_unix_ms <= 0.0 {
            return Self::new();
        }

        let now_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .unwrap_or(timing.time_origin_unix_ms);
        let age_ms = (now_unix_ms - timing.time_origin_unix_ms).max(0.0);
        Self {
            origin_monotonic_us: process_monotonic_us().saturating_sub((age_ms * 1000.0) as i64),
            origin_unix_ms: timing.time_origin_unix_ms,
            last_ms: 0.0,
            navigation_timing: Some(timing),
            clamper_secret: *process_clamper_secret(),
        }
    }

    /// Returns elapsed ms since origin on Chrome's non-isolated 100 µs grid.
    pub fn now_ms(&mut self) -> f64 {
        let clamped_origin =
            clamp_time_resolution_us(self.origin_monotonic_us, self.clamper_secret);
        let clamped_now = clamp_time_resolution_us(process_monotonic_us(), self.clamper_secret);
        // Deliberately convert each absolute coordinate before subtraction.
        // This ordering is observable in Chromium's IEEE-754 result bits.
        let raw_ms = elapsed_ms_from_clamped(clamped_origin, clamped_now).max(0.0);
        let value = raw_ms.max(self.last_ms);
        self.last_ms = value;
        value
    }
}

fn process_clamper_secret() -> &'static u64 {
    static SECRET: OnceLock<u64> = OnceLock::new();
    SECRET.get_or_init(rand::random)
}

fn clamp_time_resolution_us(time_us: i64, secret: u64) -> i64 {
    const RESOLUTION_US: i64 = 100;
    const LOWER_DIGITS_MOD: i64 = 10_000_000_000;
    let was_negative = time_us < 0;
    let positive = if was_negative {
        time_us.saturating_abs()
    } else {
        time_us
    };
    let lower = positive % LOWER_DIGITS_MOD;
    let upper = positive - lower;
    let mut clamped = lower - lower % RESOLUTION_US;
    let hash = murmur_hash3((clamped as u64) ^ secret);
    let random_bits = (hash & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000;
    let threshold = clamped as f64 + RESOLUTION_US as f64 * (f64::from_bits(random_bits) - 1.0);
    if lower as f64 >= threshold {
        clamped += RESOLUTION_US;
    }
    let value = upper.saturating_add(clamped);
    if was_negative {
        -value
    } else {
        value
    }
}

fn murmur_hash3(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^= value >> 33;
    value
}

fn elapsed_ms_from_clamped(origin_us: i64, current_us: i64) -> f64 {
    current_us as f64 / 1000.0 - origin_us as f64 / 1000.0
}

#[cfg(target_os = "macos")]
fn process_monotonic_us() -> i64 {
    #[repr(C)]
    struct MachTimebaseInfo {
        numerator: u32,
        denominator: u32,
    }
    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
    }
    static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
    let (numerator, denominator) = *TIMEBASE.get_or_init(|| {
        let mut info = MachTimebaseInfo {
            numerator: 0,
            denominator: 0,
        };
        // SAFETY: `info` is a valid writable structure and the OS function
        // initializes it synchronously without retaining the pointer.
        let status = unsafe { mach_timebase_info(&mut info) };
        if status == 0 && info.denominator != 0 {
            (info.numerator, info.denominator)
        } else {
            (1, 1)
        }
    });
    // SAFETY: `mach_absolute_time` takes no pointers and has no preconditions.
    let ticks = unsafe { mach_absolute_time() } as u128;
    let nanos = ticks.saturating_mul(numerator as u128) / denominator as u128;
    (nanos / 1000).min(i64::MAX as u128) as i64
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_monotonic_us() -> i64 {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is a valid writable `timespec`; the OS call does not
    // retain its pointer. Failure falls back to the process-local anchor below.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value) } == 0 {
        value
            .tv_sec
            .saturating_mul(1_000_000)
            .saturating_add(value.tv_nsec / 1000)
    } else {
        fallback_monotonic_us()
    }
}

#[cfg(not(unix))]
fn process_monotonic_us() -> i64 {
    fallback_monotonic_us()
}

#[cfg(any(not(unix), all(unix, not(target_os = "macos"))))]
fn fallback_monotonic_us() -> i64 {
    use std::time::Instant;
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    // Keep the coordinate in the same magnitude as a normally running desktop
    // OS monotonic clock so conversion precision remains browser-shaped.
    200_000_000_000i64.saturating_add(
        ANCHOR
            .get_or_init(Instant::now)
            .elapsed()
            .as_micros()
            .min(i64::MAX as u128) as i64,
    )
}

impl Default for PerfState {
    fn default() -> Self {
        Self::new()
    }
}

#[op2(fast)]
pub fn op_perf_now_humanized(s: &mut OpState) -> f64 {
    let s = s.borrow_mut::<PerfState>();
    s.now_ms()
}

/// Returns the UNIX-epoch ms corresponding to `PerfState.origin` (the
/// process-relative t=0 for `performance.now()`). JS uses this as the
/// `performance.timeOrigin` value so the standard Web Platform invariant
/// `timeOrigin + performance.now() ≈ Date.now()` holds.
#[op2(fast)]
pub fn op_perf_time_origin_ms(s: &mut OpState) -> f64 {
    let s = s.borrow::<PerfState>();
    s.origin_unix_ms
}

#[op2]
#[serde]
pub fn op_perf_get_navigation_timing(state: &mut OpState) -> Option<crate::net::TimingStats> {
    state.borrow::<PerfState>().navigation_timing.clone()
}

#[derive(serde::Serialize)]
pub struct JsResourceTiming {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
    pub fetch_start: f64,
    pub domain_lookup_start: f64,
    pub domain_lookup_end: f64,
    pub connect_start: f64,
    pub connect_end: f64,
    pub secure_connection_start: f64,
    pub request_start: f64,
    pub response_start: f64,
    pub response_end: f64,
    pub transfer_size: u64,
    pub encoded_body_size: u64,
    pub decoded_body_size: u64,
}

#[op2]
#[serde]
pub fn op_perf_get_resource_timings(state: &mut OpState) -> Vec<JsResourceTiming> {
    let perf_origin_unix_ms = state.borrow::<PerfState>().origin_unix_ms;
    let dom_state = state.borrow::<DomState>();
    dom_state
        .resource_timings
        .iter()
        .map(|t| {
            let origin_offset = if t.time_origin_unix_ms > 0.0 {
                t.time_origin_unix_ms - perf_origin_unix_ms
            } else {
                0.0
            };
            JsResourceTiming {
                name: t.name.clone(),
                entry_type: "resource".to_string(),
                // Resource Timing starts when fetching begins, not when the
                // request bytes finally reach the wire. The TimingStats wall
                // clock is captured at fetch dispatch, so its offset from this
                // realm's performance origin is the entry startTime.
                start_time: origin_offset,
                duration: t.response_end_ms,
                fetch_start: origin_offset,
                domain_lookup_start: origin_offset + t.dns_start_ms,
                domain_lookup_end: origin_offset + t.dns_end_ms,
                connect_start: origin_offset + t.connect_start_ms,
                connect_end: origin_offset + t.connect_end_ms,
                secure_connection_start: origin_offset + t.tls_start_ms,
                request_start: origin_offset + t.request_start_ms,
                response_start: origin_offset + t.response_start_ms,
                response_end: origin_offset + t.response_end_ms,
                transfer_size: t.transfer_size,
                encoded_body_size: t.encoded_body_size,
                decoded_body_size: t.decoded_body_size,
            }
        })
        .collect()
}

deno_core::extension!(
    perf_extension,
    ops = [
        op_perf_now_humanized,
        op_perf_get_navigation_timing,
        op_perf_get_resource_timings,
        op_perf_time_origin_ms,
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_stay_on_the_100_microsecond_grid() {
        let mut s = PerfState::with_seed(7);
        for _ in 0..10_000 {
            let scaled = s.now_ms() * 10.0;
            assert!((scaled - scaled.round()).abs() < 1e-6, "value was {scaled}");
        }
    }

    #[test]
    fn absolute_clock_conversion_matches_chromium_delta_bits() {
        let first = elapsed_ms_from_clamped(200_000_000_000, 200_000_123_600);
        let second = elapsed_ms_from_clamped(200_000_000_000, 200_000_123_700);
        let delta = second - first;
        assert_eq!(delta.to_bits(), 0x3fb9_9999_8000_0000);
        assert_eq!(delta, 0.099_999_994_039_535_52);
    }

    #[test]
    fn values_are_monotonic() {
        let mut s = PerfState::with_seed(0xBEEF);
        let mut previous = s.now_ms();
        for _ in 0..10_000 {
            let value = s.now_ms();
            assert!(value >= previous, "{value} went backwards from {previous}");
            previous = value;
        }
    }
}
