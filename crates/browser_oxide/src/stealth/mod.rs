//! Stealth fingerprint profiles for browser_oxide.
//!
//! Provides consistent browser identities — UA string, screen, locale,
//! GPU vendor/renderer, TLS impersonation label, behavioural input model
//! — so the engine reports a coherent "I am Chrome 148 on macOS" surface
//! rather than a default headless fingerprint.

pub mod behavior;
pub mod config;
pub mod gpu;
pub mod presets;
pub mod profile;

pub use behavior::{BehaviorProfile, Handedness, MousePoint, ScrollStyle, WheelTick};
pub use config::{ConfigError, ConfigFormat};
pub use gpu::GpuProfile;
pub use presets::*;
pub use profile::{DeviceClass, StealthProfile};

/// Human-timing pause for behavioral stealth (tab-switch dwell, post-challenge
/// jitter, per-point mouse + per-keystroke input pacing). A no-op unless the
/// `slowdowns` feature is enabled, so the default build spends zero time here
/// and runs at full speed. The duration is consumed in both builds so call
/// sites keep their computed values "used" under `-D warnings`.
#[cfg(feature = "slowdowns")]
pub(crate) async fn stealth_delay(dur: std::time::Duration) {
    tokio::time::sleep(dur).await;
}

#[cfg(not(feature = "slowdowns"))]
pub(crate) async fn stealth_delay(_dur: std::time::Duration) {}
