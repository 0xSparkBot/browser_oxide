/// A CSS color value.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Rgba {
        r: u8,
        g: u8,
        b: u8,
        a: f32,
    },
    Hsl {
        h: f64,
        s: f64,
        l: f64,
        a: f32,
    },
    Oklch {
        l: f64,
        c: f64,
        h: f64,
        a: f32,
    },
    Oklab {
        l: f64,
        a_axis: f64,
        b_axis: f64,
        alpha: f32,
    },
    Lab {
        l: f64,
        a_axis: f64,
        b_axis: f64,
        alpha: f32,
    },
    Lch {
        l: f64,
        c: f64,
        h: f64,
        a: f32,
    },
    CurrentColor,
    Transparent,
}

impl Color {
    /// Get as sRGB RGBA bytes.
    ///
    /// CSS Lab/LCH are defined relative to D50 and chromatically adapted to
    /// D65 before the sRGB matrix is applied. OKLab/OKLCH are already D65.
    /// The matrices/constants below are the CSS Color 4 reference transforms;
    /// keeping them here makes layout/canvas color resolution deterministic
    /// instead of collapsing every non-sRGB color to black.
    pub fn to_rgba(&self) -> (u8, u8, u8, f32) {
        match self {
            Color::Rgba { r, g, b, a } => (*r, *g, *b, *a),
            Color::Hsl { h, s, l, a } => {
                let (r, g, b) = hsl_to_srgb(*h, *s, *l);
                (to_byte(r), to_byte(g), to_byte(b), a.clamp(0.0, 1.0))
            }
            Color::Lab {
                l,
                a_axis,
                b_axis,
                alpha,
            } => {
                let (r, g, b) = lab_to_srgb(*l, *a_axis, *b_axis);
                (to_byte(r), to_byte(g), to_byte(b), alpha.clamp(0.0, 1.0))
            }
            Color::Lch { l, c, h, a } => {
                let angle = h.to_radians();
                let (r, g, b) = lab_to_srgb(*l, *c * angle.cos(), *c * angle.sin());
                (to_byte(r), to_byte(g), to_byte(b), a.clamp(0.0, 1.0))
            }
            Color::Oklab {
                l,
                a_axis,
                b_axis,
                alpha,
            } => {
                let (r, g, b) = oklab_to_srgb(*l, *a_axis, *b_axis);
                (to_byte(r), to_byte(g), to_byte(b), alpha.clamp(0.0, 1.0))
            }
            Color::Oklch { l, c, h, a } => {
                let angle = h.to_radians();
                let (r, g, b) = oklab_to_srgb(*l, *c * angle.cos(), *c * angle.sin());
                (to_byte(r), to_byte(g), to_byte(b), a.clamp(0.0, 1.0))
            }
            Color::Transparent => (0, 0, 0, 0.0),
            Color::CurrentColor => (0, 0, 0, 1.0), // resolved by computed style
        }
    }
}

fn to_byte(channel: f64) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn srgb_encode(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn hsl_to_srgb(h: f64, s_percent: f64, l_percent: f64) -> (f64, f64, f64) {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = (s_percent / 100.0).clamp(0.0, 1.0);
    let l = (l_percent / 100.0).clamp(0.0, 1.0);
    if s == 0.0 {
        return (l, l, l);
    }

    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

fn lab_to_srgb(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    const EPSILON: f64 = 216.0 / 24_389.0;
    const KAPPA: f64 = 24_389.0 / 27.0;
    fn finv(t: f64) -> f64 {
        let t3 = t * t * t;
        if t3 > EPSILON {
            t3
        } else {
            (116.0 * t - 16.0) / KAPPA
        }
    }

    let f1 = (l.clamp(0.0, 100.0) + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let xd50 = finv(f0) * 0.964_22;
    let yd50 = finv(f1);
    let zd50 = finv(f2) * 0.825_21;

    // Bradford chromatic adaptation from D50 to D65.
    let x = 0.955_473_452_704_218_2 * xd50 - 0.023_098_536_874_261_423 * yd50
        + 0.063_259_308_661_021_7 * zd50;
    let y = -0.028_369_706_963_208_136 * xd50
        + 1.009_995_458_005_822_6 * yd50
        + 0.021_041_398_966_943_008 * zd50;
    let z = 0.012_314_001_688_319_899 * xd50 - 0.020_507_696_433_477_912 * yd50
        + 1.330_365_936_608_075_3 * zd50;
    xyz_d65_to_srgb(x, y, z)
}

fn xyz_d65_to_srgb(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let r = 3.240_969_941_904_522_6 * x - 1.537_383_177_570_094 * y - 0.498_610_760_293_003_4 * z;
    let g =
        -0.969_243_636_280_879_6 * x + 1.875_967_501_507_720_2 * y + 0.041_555_057_407_175_59 * z;
    let b =
        0.055_630_079_696_993_66 * x - 0.203_976_958_888_976_52 * y + 1.056_971_514_242_878_6 * z;
    (srgb_encode(r), srgb_encode(g), srgb_encode(b))
}

fn oklab_to_srgb(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let l = l.clamp(0.0, 1.0);
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;

    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3;
    (srgb_encode(r), srgb_encode(g), srgb_encode(b))
}

/// Resolve a named CSS color. Returns None if not a valid name.
pub fn named_color(name: &str) -> Option<Color> {
    let rgba = |r, g, b| Some(Color::Rgba { r, g, b, a: 1.0 });

    match name.to_ascii_lowercase().as_str() {
        "transparent" => Some(Color::Transparent),
        "currentcolor" => Some(Color::CurrentColor),

        // CSS Level 1
        "black" => rgba(0, 0, 0),
        "silver" => rgba(192, 192, 192),
        "gray" | "grey" => rgba(128, 128, 128),
        "white" => rgba(255, 255, 255),
        "maroon" => rgba(128, 0, 0),
        "red" => rgba(255, 0, 0),
        "purple" => rgba(128, 0, 128),
        "fuchsia" | "magenta" => rgba(255, 0, 255),
        "green" => rgba(0, 128, 0),
        "lime" => rgba(0, 255, 0),
        "olive" => rgba(128, 128, 0),
        "yellow" => rgba(255, 255, 0),
        "navy" => rgba(0, 0, 128),
        "blue" => rgba(0, 0, 255),
        "teal" => rgba(0, 128, 128),
        "aqua" | "cyan" => rgba(0, 255, 255),

        // CSS Level 2+
        "orange" => rgba(255, 165, 0),
        "aliceblue" => rgba(240, 248, 255),
        "antiquewhite" => rgba(250, 235, 215),
        "aquamarine" => rgba(127, 255, 212),
        "azure" => rgba(240, 255, 255),
        "beige" => rgba(245, 245, 220),
        "bisque" => rgba(255, 228, 196),
        "blanchedalmond" => rgba(255, 235, 205),
        "blueviolet" => rgba(138, 43, 226),
        "brown" => rgba(165, 42, 42),
        "burlywood" => rgba(222, 184, 135),
        "cadetblue" => rgba(95, 158, 160),
        "chartreuse" => rgba(127, 255, 0),
        "chocolate" => rgba(210, 105, 30),
        "coral" => rgba(255, 127, 80),
        "cornflowerblue" => rgba(100, 149, 237),
        "cornsilk" => rgba(255, 248, 220),
        "crimson" => rgba(220, 20, 60),
        "darkblue" => rgba(0, 0, 139),
        "darkcyan" => rgba(0, 139, 139),
        "darkgoldenrod" => rgba(184, 134, 11),
        "darkgray" | "darkgrey" => rgba(169, 169, 169),
        "darkgreen" => rgba(0, 100, 0),
        "darkkhaki" => rgba(189, 183, 107),
        "darkmagenta" => rgba(139, 0, 139),
        "darkolivegreen" => rgba(85, 106, 47),
        "darkorange" => rgba(255, 140, 0),
        "darkorchid" => rgba(153, 50, 204),
        "darkred" => rgba(139, 0, 0),
        "darksalmon" => rgba(233, 150, 122),
        "darkseagreen" => rgba(143, 188, 143),
        "darkslateblue" => rgba(72, 61, 139),
        "darkslategray" | "darkslategrey" => rgba(47, 79, 79),
        "darkturquoise" => rgba(0, 206, 209),
        "darkviolet" => rgba(148, 0, 211),
        "deeppink" => rgba(255, 20, 147),
        "deepskyblue" => rgba(0, 191, 255),
        "dimgray" | "dimgrey" => rgba(105, 105, 105),
        "dodgerblue" => rgba(30, 144, 255),
        "firebrick" => rgba(178, 34, 34),
        "floralwhite" => rgba(255, 250, 240),
        "forestgreen" => rgba(34, 139, 34),
        "gainsboro" => rgba(220, 220, 220),
        "ghostwhite" => rgba(248, 248, 255),
        "gold" => rgba(255, 215, 0),
        "goldenrod" => rgba(218, 165, 32),
        "greenyellow" => rgba(173, 255, 47),
        "honeydew" => rgba(240, 255, 240),
        "hotpink" => rgba(255, 105, 180),
        "indianred" => rgba(205, 92, 92),
        "indigo" => rgba(75, 0, 130),
        "ivory" => rgba(255, 255, 240),
        "khaki" => rgba(240, 230, 140),
        "lavender" => rgba(230, 230, 250),
        "lavenderblush" => rgba(255, 240, 245),
        "lawngreen" => rgba(124, 252, 0),
        "lemonchiffon" => rgba(255, 250, 205),
        "lightblue" => rgba(173, 216, 230),
        "lightcoral" => rgba(240, 128, 128),
        "lightcyan" => rgba(224, 255, 255),
        "lightgoldenrodyellow" => rgba(250, 250, 210),
        "lightgray" | "lightgrey" => rgba(211, 211, 211),
        "lightgreen" => rgba(144, 238, 144),
        "lightpink" => rgba(255, 182, 193),
        "lightsalmon" => rgba(255, 160, 122),
        "lightseagreen" => rgba(32, 178, 170),
        "lightskyblue" => rgba(135, 206, 250),
        "lightslategray" | "lightslategrey" => rgba(119, 136, 153),
        "lightsteelblue" => rgba(176, 196, 222),
        "lightyellow" => rgba(255, 255, 224),
        "limegreen" => rgba(50, 205, 50),
        "linen" => rgba(250, 240, 230),
        "mediumaquamarine" => rgba(102, 205, 170),
        "mediumblue" => rgba(0, 0, 205),
        "mediumorchid" => rgba(186, 85, 211),
        "mediumpurple" => rgba(147, 111, 219),
        "mediumseagreen" => rgba(60, 179, 113),
        "mediumslateblue" => rgba(123, 104, 238),
        "mediumspringgreen" => rgba(0, 250, 154),
        "mediumturquoise" => rgba(72, 209, 204),
        "mediumvioletred" => rgba(199, 21, 133),
        "midnightblue" => rgba(25, 25, 112),
        "mintcream" => rgba(245, 255, 250),
        "mistyrose" => rgba(255, 228, 225),
        "moccasin" => rgba(255, 228, 181),
        "navajowhite" => rgba(255, 222, 173),
        "oldlace" => rgba(253, 245, 230),
        "olivedrab" => rgba(107, 142, 35),
        "orangered" => rgba(255, 69, 0),
        "orchid" => rgba(218, 112, 214),
        "palegoldenrod" => rgba(238, 232, 170),
        "palegreen" => rgba(152, 251, 152),
        "paleturquoise" => rgba(175, 238, 238),
        "palevioletred" => rgba(219, 112, 147),
        "papayawhip" => rgba(255, 239, 213),
        "peachpuff" => rgba(255, 218, 185),
        "peru" => rgba(205, 133, 63),
        "pink" => rgba(255, 192, 203),
        "plum" => rgba(221, 160, 221),
        "powderblue" => rgba(176, 224, 230),
        "rebeccapurple" => rgba(102, 51, 153),
        "rosybrown" => rgba(188, 143, 143),
        "royalblue" => rgba(65, 105, 225),
        "saddlebrown" => rgba(139, 69, 19),
        "salmon" => rgba(250, 128, 114),
        "sandybrown" => rgba(244, 164, 96),
        "seagreen" => rgba(46, 139, 87),
        "seashell" => rgba(255, 245, 238),
        "sienna" => rgba(160, 82, 45),
        "skyblue" => rgba(135, 206, 235),
        "slateblue" => rgba(106, 90, 205),
        "slategray" | "slategrey" => rgba(112, 128, 144),
        "snow" => rgba(255, 250, 250),
        "springgreen" => rgba(0, 255, 127),
        "steelblue" => rgba(70, 130, 180),
        "tan" => rgba(210, 180, 140),
        "thistle" => rgba(216, 191, 216),
        "tomato" => rgba(255, 99, 71),
        "turquoise" => rgba(64, 224, 208),
        "violet" => rgba(238, 130, 238),
        "wheat" => rgba(245, 222, 179),
        "whitesmoke" => rgba(245, 245, 245),
        "yellowgreen" => rgba(154, 205, 50),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_colors_work() {
        assert!(
            matches!(named_color("red"), Some(Color::Rgba { r: 255, g: 0, b: 0, a }) if a == 1.0)
        );
        assert!(matches!(
            named_color("RED"),
            Some(Color::Rgba {
                r: 255,
                g: 0,
                b: 0,
                ..
            })
        ));
        assert!(matches!(
            named_color("transparent"),
            Some(Color::Transparent)
        ));
        assert!(matches!(
            named_color("currentcolor"),
            Some(Color::CurrentColor)
        ));
        assert!(named_color("notacolor").is_none());
    }

    #[test]
    fn rebeccapurple() {
        assert!(matches!(
            named_color("rebeccapurple"),
            Some(Color::Rgba {
                r: 102,
                g: 51,
                b: 153,
                ..
            })
        ));
    }
}

#[cfg(test)]
mod color4_tests {
    use super::Color;

    #[test]
    fn color4_to_srgb_matches_chrome_canvas_pixels() {
        let cases = [
            (
                Color::Hsl {
                    h: 120.0,
                    s: 100.0,
                    l: 25.0,
                    a: 1.0,
                },
                (0, 128, 0, 1.0),
            ),
            (
                Color::Lab {
                    l: 50.0,
                    a_axis: 40.0,
                    b_axis: 30.0,
                    alpha: 1.0,
                },
                (187, 88, 70, 1.0),
            ),
            (
                Color::Lch {
                    l: 50.0,
                    c: 50.0,
                    h: 40.0,
                    a: 1.0,
                },
                (185, 89, 67, 1.0),
            ),
            (
                Color::Oklab {
                    l: 0.6,
                    a_axis: 0.1,
                    b_axis: 0.05,
                    alpha: 1.0,
                },
                (186, 100, 92, 1.0),
            ),
            (
                Color::Oklch {
                    l: 0.6,
                    c: 0.15,
                    h: 40.0,
                    a: 1.0,
                },
                (200, 91, 50, 1.0),
            ),
        ];
        for (color, expected) in cases {
            assert_eq!(color.to_rgba(), expected);
        }
    }
}
