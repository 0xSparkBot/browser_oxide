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
use std::time::{Duration, Instant};

/// Per-runtime state for the humanized clock.
pub struct PerfState {
    /// Process-relative origin; `performance.now()` returns ms since this
    /// instant (matches DOM HighResolutionTime contract for the document).
    origin: Instant,
    /// Wall-clock (UNIX epoch ms) corresponding to `origin`. Read by
    /// `op_perf_time_origin_ms` so JS `performance.timeOrigin` honors the
    /// invariant `timeOrigin + performance.now() ≈ Date.now()`. Real
    /// Chrome maintains this invariant; without it, an earlier JS-side
    /// ad-hoc computation (`Date.now() - <hardcoded nav_end>`) produced a
    /// detectable skew between `performance.timeOrigin + performance.now()`
    /// and `Date.now()`.
    origin_unix_ms: f64,
    /// Last returned value in µs — enforces monotonicity per HRT spec.
    last_us: f64,
    /// Network-backed navigation metadata, when this realm was created from
    /// an HTTP response rather than synthetic HTML/about:blank.
    navigation_timing: Option<crate::net::TimingStats>,
}

impl PerfState {
    pub fn new() -> Self {
        Self::with_seed(0xCAFEF00DDEADBEEF)
    }
    pub fn with_seed(_seed: u64) -> Self {
        let origin_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        Self {
            origin: Instant::now(),
            origin_unix_ms,
            last_us: 0.0,
            navigation_timing: None,
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
        let origin = Instant::now()
            .checked_sub(Duration::from_secs_f64(age_ms / 1000.0))
            .unwrap_or_else(Instant::now);
        Self {
            origin,
            origin_unix_ms: timing.time_origin_unix_ms,
            last_us: 0.0,
            navigation_timing: Some(timing),
        }
    }

    /// Returns elapsed ms since origin on Chrome's non-isolated 100 µs grid.
    pub fn now_ms(&mut self) -> f64 {
        let raw_us = self.origin.elapsed().as_nanos() as f64 / 1000.0;
        let quantized_us = (raw_us / 100.0).floor() * 100.0;
        let value = quantized_us.max(self.last_us);
        self.last_us = value;
        value / 1000.0
    }
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
                start_time: origin_offset + t.request_start_ms,
                duration: t.response_end_ms - t.request_start_ms,
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
            assert!((scaled - scaled.round()).abs() < 1e-9, "value was {scaled}");
        }
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
