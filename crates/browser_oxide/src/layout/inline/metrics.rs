//! Font metrics, and the platform convention that decides `line-height: normal`.
//!
//! There is no spec for this. `line-height: normal` comes out of font tables,
//! but *which* tables and how the result is rounded is a per-platform
//! convention, and the three major ones disagree by whole pixels on the same
//! font at the same size.
//!
//! That makes it a fingerprint surface, not a cosmetic one: these numbers reach
//! scripts through `getBoundingClientRect()`, `getClientRects()` and
//! `measureText()`. The convention we follow must therefore match the identity
//! the engine is *presenting*, not the machine it happens to be running on — a
//! stealth profile claiming Chrome on Linux while reporting Windows text
//! metrics is exactly the internal inconsistency the whole fingerprint design
//! exists to avoid.

use ttf_parser::Face;

/// Which platform's metric convention to follow.
///
/// Chosen from the active stealth profile's OS, not from the build host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricsProfile {
    /// FreeType / fontconfig, as Chrome uses on Linux: `hhea` ascender,
    /// descender and lineGap.
    ///
    /// The default, because the engine bundles the Liberation faces and aliases
    /// Arial to Liberation Sans specifically to mimic Chrome on Linux.
    #[default]
    Linux,
    /// GDI / DirectWrite, as Chrome uses on Windows: ascent and descent from
    /// `OS/2.usWin*` rounded independently, and *external leading* rather than
    /// the raw `hhea.lineGap` —
    ///
    /// ```text
    /// external_leading = max(0, hhea.lineGap
    ///                          - ((winAscent + winDescent)
    ///                             - (hhea.ascender - hhea.descender)))
    /// ```
    ///
    /// The gap is only whatever is left after the win metrics have already
    /// grown past the hhea box. Consolas is the case that proves the term is
    /// needed: `lineGap` 350 and overshoot 350 cancel exactly to zero.
    Windows,
    /// CoreText, as Chrome uses on macOS: `hhea` metrics, but the leading is
    /// placed *above* the ascent rather than split.
    MacOs,
}

impl MetricsProfile {
    /// Pick the convention matching an OS name as it appears in a stealth
    /// profile (`"windows"`, `"macos"`, `"linux"`).
    pub fn for_os(os_name: &str) -> Self {
        let os = os_name.to_ascii_lowercase();
        if os.contains("win") {
            Self::Windows
        } else if os.contains("mac") || os.contains("darwin") || os.contains("ios") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// Metrics for one face at one size, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    /// The used value of `line-height: normal`.
    pub line_spacing: f32,
}

impl FontMetrics {
    /// Height of the inline content box — ascent plus descent, no leading.
    /// This, not `line_spacing`, is what `getClientRects()` reports for a text
    /// fragment.
    pub fn content_height(&self) -> f32 {
        self.ascent + self.descent
    }

    /// Ascent and descent after `line_height`'s leading has been distributed.
    ///
    /// The half-leading is *floored*, which is why Arial at 16px (half-leading
    /// 0.5) sits flush against the top of its line box while a face with a
    /// half-leading of 1.0 is inset by a pixel.
    pub fn leaded(&self, line_height: f32) -> (f32, f32) {
        let half = ((line_height - self.content_height()) / 2.0).floor();
        let ascent = self.ascent + half;
        (ascent, line_height - ascent)
    }
}

/// Derive metrics for a face at a pixel size under a platform convention.
pub fn metrics_for(face: &Face<'_>, size_px: f32, profile: MetricsProfile) -> FontMetrics {
    let upem = f32::from(face.units_per_em()).max(1.0);
    let scale = size_px / upem;

    let hhea_ascent = f32::from(face.ascender());
    let hhea_descent = f32::from(-face.descender());
    let hhea_gap = f32::from(face.line_gap());

    let (ascent_u, descent_u, gap_u) = match profile {
        MetricsProfile::Linux | MetricsProfile::MacOs => (hhea_ascent, hhea_descent, hhea_gap),
        MetricsProfile::Windows => {
            let (win_ascent, win_descent) = match face.tables().os2 {
                Some(os2) => (
                    f32::from(os2.windows_ascender()),
                    f32::from(-os2.windows_descender()),
                ),
                None => (hhea_ascent, hhea_descent),
            };
            let hhea_span = hhea_ascent + hhea_descent;
            let win_span = win_ascent + win_descent;
            let external_leading = (hhea_gap - (win_span - hhea_span)).max(0.0);
            (win_ascent, win_descent, external_leading)
        }
    };

    // Blink rounds ascent and descent to whole pixels independently before
    // deriving line spacing. Skipping this is worth several pixels per line and
    // compounds down the page.
    let ascent = (ascent_u * scale).round();
    let descent = (descent_u * scale).round();
    let line_gap = gap_u * scale;
    let line_spacing = (ascent + descent + line_gap).round();

    FontMetrics {
        ascent,
        descent,
        line_gap,
        line_spacing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liberation_sans() -> &'static [u8] {
        include_bytes!("../../canvas/fonts/LiberationSans-Regular.ttf")
    }

    #[test]
    fn line_spacing_is_positive_and_stable() {
        let data = liberation_sans();
        let face = Face::parse(data, 0).expect("bundled face parses");
        for profile in [
            MetricsProfile::Linux,
            MetricsProfile::Windows,
            MetricsProfile::MacOs,
        ] {
            let m = metrics_for(&face, 16.0, profile);
            assert!(m.ascent > 0.0 && m.descent > 0.0, "{profile:?}");
            assert!(
                m.line_spacing >= m.content_height(),
                "{profile:?}: line box must not be shorter than its content"
            );
        }
    }

    #[test]
    fn metrics_scale_with_size() {
        let data = liberation_sans();
        let face = Face::parse(data, 0).unwrap();
        let small = metrics_for(&face, 10.0, MetricsProfile::Linux);
        let large = metrics_for(&face, 40.0, MetricsProfile::Linux);
        assert!(large.ascent > small.ascent * 3.0);
    }

    #[test]
    fn leading_is_split_and_floored() {
        let data = liberation_sans();
        let face = Face::parse(data, 0).unwrap();
        let m = metrics_for(&face, 16.0, MetricsProfile::Linux);
        let (ascent, descent) = m.leaded(m.line_spacing);
        assert_eq!(
            ascent + descent,
            m.line_spacing,
            "leaded ascent + descent must reconstitute the line box exactly"
        );
        assert!(ascent >= m.ascent, "leading must not eat into the ascent");
    }

    #[test]
    fn profile_selection_from_os_name() {
        assert_eq!(MetricsProfile::for_os("Windows"), MetricsProfile::Windows);
        assert_eq!(MetricsProfile::for_os("macOS"), MetricsProfile::MacOs);
        assert_eq!(MetricsProfile::for_os("Linux"), MetricsProfile::Linux);
        // Anything unrecognised follows the bundled font set, which is
        // Liberation — i.e. Chrome on Linux.
        assert_eq!(MetricsProfile::for_os("plan9"), MetricsProfile::Linux);
    }
}
