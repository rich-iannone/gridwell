//! CSS colours: parsing every form a table author is likely to write, normalizing
//! to RGBA so writers never see the source text.
//!
//! Accepted (case-insensitive, surrounding whitespace ignored):
//!
//! - the 148 CSS named colours, plus `transparent`;
//! - hex: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`;
//! - `rgb()` / `rgba()`, legacy comma-separated (`rgb(255, 0, 0)`,
//!   `rgba(255, 0, 0, 0.5)`) or modern space-separated with an optional `/ alpha`
//!   (`rgb(255 0 0 / 50%)`); channels are numbers 0–255 or percentages;
//! - `hsl()` / `hsla()`, in either syntax; the hue is a number of degrees or carries
//!   a `deg`, `grad`, `rad` or `turn` unit.
//!
//! Out-of-range channels clamp (as in CSS); non-finite numbers are rejected.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid color value: {0}")]
pub struct ColorParseError(pub String);

/// An RGBA color (0–255 per channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Fully opaque (alpha 255).
    pub fn is_opaque(&self) -> bool {
        self.a == 255
    }

    /// Fully transparent (alpha 0): paints nothing.
    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }

    /// To hex string (#RRGGBB or #RRGGBBAA if alpha < 255).
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// `RRGGBB` without the `#` or alpha, as OOXML, DrawingML and LaTeX's `HTML`
    /// model want it.
    pub fn to_rrggbb(&self) -> String {
        format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// To rgb()/rgba() CSS functional notation.
    pub fn to_css_rgb(&self) -> String {
        if self.a == 255 {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            let alpha = self.a as f64 / 255.0;
            format!("rgba({}, {}, {}, {:.3})", self.r, self.g, self.b, alpha)
        }
    }

    /// The most compact CSS form understood everywhere: `#RRGGBB` when opaque,
    /// `rgba(…)` otherwise (8-digit hex is not universally supported).
    pub fn to_css(&self) -> String {
        if self.is_opaque() {
            self.to_hex()
        } else {
            self.to_css_rgb()
        }
    }
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let err = || ColorParseError(s.to_string());

        if s.eq_ignore_ascii_case("transparent") {
            return Ok(Color::TRANSPARENT);
        }
        if let Some(c) = named_color(s) {
            return Ok(c);
        }
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex).ok_or_else(err);
        }
        parse_function(s).ok_or_else(err)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Color::from_str(&s).map_err(serde::de::Error::custom)
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    // Checked first so the byte slicing below is always on char boundaries.
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nibble = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some(Color::rgb(nibble(0)?, nibble(1)?, nibble(2)?)),
        4 => Some(Color::new(nibble(0)?, nibble(1)?, nibble(2)?, nibble(3)?)),
        6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color::new(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// `rgb()`, `rgba()`, `hsl()`, `hsla()`.
fn parse_function(s: &str) -> Option<Color> {
    let open = s.find('(')?;
    let name = s[..open].trim().to_ascii_lowercase();
    let inner = s[open + 1..].strip_suffix(')')?;
    let (channels, alpha) = split_args(inner)?;
    let [c1, c2, c3] = channels[..] else {
        return None;
    };
    let a = match alpha {
        Some(a) => alpha_byte(a)?,
        None => 255,
    };
    match name.as_str() {
        "rgb" | "rgba" => Some(Color::new(
            rgb_channel(c1)?,
            rgb_channel(c2)?,
            rgb_channel(c3)?,
            a,
        )),
        "hsl" | "hsla" => {
            let (r, g, b) = hsl_to_rgb(hue(c1)?, fraction(c2)?, fraction(c3)?);
            Some(Color::new(r, g, b, a))
        }
        _ => None,
    }
}

/// Split function arguments into three channels and an optional alpha, accepting
/// either `a, b, c[, alpha]` or `a b c[ / alpha]` (not a mix of the two).
fn split_args(inner: &str) -> Option<(Vec<&str>, Option<&str>)> {
    if inner.contains(',') {
        if inner.contains('/') {
            return None;
        }
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || p.contains(char::is_whitespace))
        {
            return None;
        }
        match parts.len() {
            3 => Some((parts, None)),
            4 => Some((parts[..3].to_vec(), Some(parts[3]))),
            _ => None,
        }
    } else {
        let mut halves = inner.split('/');
        let channels: Vec<&str> = halves.next()?.split_whitespace().collect();
        let alpha = match halves.next() {
            Some(a) => {
                let a = a.trim();
                if a.is_empty() || a.contains(char::is_whitespace) {
                    return None;
                }
                Some(a)
            }
            None => None,
        };
        if halves.next().is_some() {
            return None;
        }
        Some((channels, alpha))
    }
}

/// A finite number. (`f64::from_str` also accepts `inf` and `NaN`.)
fn number(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `50%` → 0.5; a bare number is taken as a percentage too (CSS Color 4's modern
/// `hsl()` allows it).
fn fraction(s: &str) -> Option<f64> {
    let v = match s.strip_suffix('%') {
        Some(p) => number(p)?,
        None => number(s)?,
    };
    Some((v / 100.0).clamp(0.0, 1.0))
}

/// An `rgb()` channel: a number 0–255 or a percentage.
fn rgb_channel(s: &str) -> Option<u8> {
    let v = match s.strip_suffix('%') {
        Some(p) => number(p)? / 100.0 * 255.0,
        None => number(s)?,
    };
    Some(v.clamp(0.0, 255.0).round() as u8)
}

/// An alpha value: a number 0–1 or a percentage.
fn alpha_byte(s: &str) -> Option<u8> {
    let v = match s.strip_suffix('%') {
        Some(p) => number(p)? / 100.0,
        None => number(s)?,
    };
    Some((v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A hue in degrees, normalized to [0, 360).
fn hue(s: &str) -> Option<f64> {
    let lower = s.to_ascii_lowercase();
    let deg = if let Some(v) = lower.strip_suffix("deg") {
        number(v)?
    } else if let Some(v) = lower.strip_suffix("grad") {
        number(v)? * 0.9
    } else if let Some(v) = lower.strip_suffix("rad") {
        number(v)?.to_degrees()
    } else if let Some(v) = lower.strip_suffix("turn") {
        number(v)? * 360.0
    } else {
        number(&lower)?
    };
    Some(deg.rem_euclid(360.0))
}

/// CSS Color 4's `hsl()` conversion; `s` and `l` in [0, 1].
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        let v = l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (f(0.0), f(8.0), f(4.0))
}

fn named_color(name: &str) -> Option<Color> {
    let lower = name.to_ascii_lowercase();
    let i = NAMED_COLORS
        .binary_search_by(|(n, _)| (*n).cmp(lower.as_str()))
        .ok()?;
    let v = NAMED_COLORS[i].1;
    Some(Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// The CSS Color 4 named colours, sorted by name (for binary search).
pub const NAMED_COLORS: &[(&str, u32)] = &[
    ("aliceblue", 0xF0F8FF),
    ("antiquewhite", 0xFAEBD7),
    ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4),
    ("azure", 0xF0FFFF),
    ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4),
    ("black", 0x000000),
    ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF),
    ("blueviolet", 0x8A2BE2),
    ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887),
    ("cadetblue", 0x5F9EA0),
    ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E),
    ("coral", 0xFF7F50),
    ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC),
    ("crimson", 0xDC143C),
    ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B),
    ("darkcyan", 0x008B8B),
    ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B),
    ("darkmagenta", 0x8B008B),
    ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00),
    ("darkorchid", 0x9932CC),
    ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A),
    ("darkseagreen", 0x8FBC8F),
    ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F),
    ("darkslategrey", 0x2F4F4F),
    ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3),
    ("deeppink", 0xFF1493),
    ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222),
    ("floralwhite", 0xFFFAF0),
    ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF),
    ("gainsboro", 0xDCDCDC),
    ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700),
    ("goldenrod", 0xDAA520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xADFF2F),
    ("grey", 0x808080),
    ("honeydew", 0xF0FFF0),
    ("hotpink", 0xFF69B4),
    ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082),
    ("ivory", 0xFFFFF0),
    ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA),
    ("lavenderblush", 0xFFF0F5),
    ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD),
    ("lightblue", 0xADD8E6),
    ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF),
    ("lightgoldenrodyellow", 0xFAFAD2),
    ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90),
    ("lightgrey", 0xD3D3D3),
    ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A),
    ("lightseagreen", 0x20B2AA),
    ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0),
    ("lime", 0x00FF00),
    ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6),
    ("magenta", 0xFF00FF),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66CDAA),
    ("mediumblue", 0x0000CD),
    ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB),
    ("mediumseagreen", 0x3CB371),
    ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A),
    ("mediumturquoise", 0x48D1CC),
    ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xF5FFFA),
    ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5),
    ("navajowhite", 0xFFDEAD),
    ("navy", 0x000080),
    ("oldlace", 0xFDF5E6),
    ("olive", 0x808000),
    ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500),
    ("orangered", 0xFF4500),
    ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA),
    ("palegreen", 0x98FB98),
    ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093),
    ("papayawhip", 0xFFEFD5),
    ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F),
    ("pink", 0xFFC0CB),
    ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xFF0000),
    ("rosybrown", 0xBC8F8F),
    ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513),
    ("salmon", 0xFA8072),
    ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57),
    ("seashell", 0xFFF5EE),
    ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0),
    ("skyblue", 0x87CEEB),
    ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F),
    ("steelblue", 0x4682B4),
    ("tan", 0xD2B48C),
    ("teal", 0x008080),
    ("thistle", 0xD8BFD8),
    ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0),
    ("violet", 0xEE82EE),
    ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF),
    ("whitesmoke", 0xF5F5F5),
    ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Color {
        s.parse::<Color>()
            .unwrap_or_else(|e| panic!("{s:?} should parse: {e}"))
    }

    fn bad(s: &str) {
        assert!(s.parse::<Color>().is_err(), "{s:?} should be rejected");
    }

    #[test]
    fn parse_hex_6() {
        assert_eq!(c("#FF0000"), Color::rgb(255, 0, 0));
        assert_eq!(c("#333333"), Color::rgb(51, 51, 51));
        assert_eq!(c("#abcdef"), Color::rgb(0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn parse_hex_3() {
        assert_eq!(c("#F00"), Color::rgb(255, 0, 0));
        assert_eq!(c("#0a8"), Color::rgb(0, 0xAA, 0x88));
    }

    #[test]
    fn parse_hex_4() {
        assert_eq!(c("#F008"), Color::new(255, 0, 0, 0x88));
        assert_eq!(c("#0000"), Color::TRANSPARENT);
    }

    #[test]
    fn parse_hex_8() {
        assert_eq!(c("#FF000080"), Color::new(255, 0, 0, 128));
    }

    #[test]
    fn hex_rejects_bad_lengths_and_digits() {
        for s in [
            "#",
            "#F",
            "#FF",
            "#FFFFF",
            "#FFFFFFF",
            "#FFFFFFFFF",
            "#GG0000",
            "#12345G",
            "# FFF",
            "#+FF",
            "FF0000",
        ] {
            bad(s);
        }
    }

    #[test]
    fn non_ascii_hex_is_rejected_not_a_panic() {
        // Byte lengths 3, 4, 6 and 8: these used to slice inside a code point.
        for s in [
            "#é0",
            "#éé",
            "#ééé",
            "#éééé",
            "#€00",
            "#0€0",
            "#😀",
            "#😀😀",
            "#a😀a",
        ] {
            bad(s);
        }
    }

    #[test]
    fn parse_rgb_legacy() {
        assert_eq!(c("rgb(255, 0, 0)"), Color::rgb(255, 0, 0));
        assert_eq!(c("rgb(255,0,0)"), Color::rgb(255, 0, 0));
        assert_eq!(c("RGB( 1 , 2 , 3 )"), Color::rgb(1, 2, 3));
        assert_eq!(c("rgb(100%, 50%, 0%)"), Color::rgb(255, 128, 0));
        assert_eq!(c("rgb(127.6, 0, 0)"), Color::rgb(128, 0, 0));
    }

    #[test]
    fn parse_rgba_legacy() {
        assert_eq!(c("rgba(255, 0, 0, 0.5)"), Color::new(255, 0, 0, 128));
        assert_eq!(c("rgba(255, 0, 0, 50%)"), Color::new(255, 0, 0, 128));
        assert_eq!(c("rgba(0, 0, 0, 0)"), Color::TRANSPARENT);
        // rgb() with four arguments and rgba() with three are aliases in CSS 4.
        assert_eq!(c("rgb(255, 0, 0, 1)"), Color::rgb(255, 0, 0));
        assert_eq!(c("rgba(255, 0, 0)"), Color::rgb(255, 0, 0));
    }

    #[test]
    fn parse_rgb_modern() {
        assert_eq!(c("rgb(255 0 0)"), Color::rgb(255, 0, 0));
        assert_eq!(c("rgb(255 0 0 / 0.5)"), Color::new(255, 0, 0, 128));
        assert_eq!(c("rgb(255 0 0/50%)"), Color::new(255, 0, 0, 128));
        assert_eq!(c("rgb(  10   20   30  )"), Color::rgb(10, 20, 30));
        assert_eq!(c("rgba(0% 100% 0% / 25%)"), Color::new(0, 255, 0, 64));
    }

    #[test]
    fn out_of_range_channels_clamp() {
        assert_eq!(c("rgb(300, -5, 255)"), Color::rgb(255, 0, 255));
        assert_eq!(c("rgb(150% 0 0)"), Color::rgb(255, 0, 0));
        assert_eq!(c("rgba(0, 0, 0, 7)"), Color::rgb(0, 0, 0));
        assert_eq!(c("rgba(0, 0, 0, -1)"), Color::TRANSPARENT);
    }

    #[test]
    fn parse_hsl() {
        assert_eq!(c("hsl(0, 100%, 50%)"), Color::rgb(255, 0, 0));
        assert_eq!(c("hsl(120, 100%, 50%)"), Color::rgb(0, 255, 0));
        assert_eq!(c("hsl(240 100% 50%)"), Color::rgb(0, 0, 255));
        assert_eq!(c("hsl(0, 0%, 0%)"), Color::rgb(0, 0, 0));
        assert_eq!(c("hsl(0, 0%, 100%)"), Color::rgb(255, 255, 255));
        assert_eq!(c("hsl(0 0% 50%)"), Color::rgb(128, 128, 128));
        // rebeccapurple is defined as hsl(270, 50%, 40%).
        assert_eq!(c("hsl(270, 50%, 40%)"), c("rebeccapurple"));
        assert_eq!(c("hsla(120, 100%, 25%, 0.5)"), Color::new(0, 128, 0, 128));
        assert_eq!(c("hsl(120 100% 25% / 50%)"), Color::new(0, 128, 0, 128));
    }

    #[test]
    fn hsl_hue_units_and_wrapping() {
        let green = Color::rgb(0, 255, 0);
        assert_eq!(c("hsl(120deg 100% 50%)"), green);
        assert_eq!(c("hsl(480 100% 50%)"), green);
        assert_eq!(c("hsl(-240 100% 50%)"), green);
        assert_eq!(c("hsl(0.3333333333turn 100% 50%)"), green);
        assert_eq!(c("hsl(133.3333333grad 100% 50%)"), green);
        assert_eq!(c("hsl(2.0943951rad 100% 50%)"), green);
        assert_eq!(c("HSL(120DEG 100% 50%)"), green);
    }

    #[test]
    fn hsl_matches_browser_values() {
        // Spot checks against values computed by browsers.
        assert_eq!(c("hsl(39, 100%, 50%)"), Color::rgb(255, 166, 0));
        assert_eq!(c("hsl(210, 50%, 60%)"), Color::rgb(102, 153, 204));
        assert_eq!(c("hsl(300, 76%, 72%)"), Color::rgb(238, 129, 238));
    }

    #[test]
    fn malformed_functions_are_rejected() {
        for s in [
            "rgb()",
            "rgb(1, 2)",
            "rgb(1, 2, 3, 4, 5)",
            "rgb(1 2)",
            "rgb(1 2 3 4)",
            "rgb(1, 2, 3",
            "rgb 1, 2, 3)",
            "rgb(1, 2 3)",
            "rgb(1, 2, 3 / 0.5)",
            "rgb(1 2 3 / )",
            "rgb(1 2 3 / 0.5 / 0.5)",
            "rgb(1,, 2, 3)",
            "rgb(a, b, c)",
            "rgb(NaN, 0, 0)",
            "rgb(inf, 0, 0)",
            "rgb(0, 0, 0, NaN)",
            "hsl(NaN, 50%, 50%)",
            "hsl(infdeg, 50%, 50%)",
            "hsl(1foo, 50%, 50%)",
            "cmyk(0, 0, 0, 0)",
            "url(x)",
            "rgb(1, 2, 3)x",
            "red; background: blue",
            "red}</style><script>",
            "expression(alert(1))",
        ] {
            bad(s);
        }
    }

    #[test]
    fn parse_named() {
        assert_eq!(c("red"), Color::rgb(255, 0, 0));
        assert_eq!(c("Black"), Color::rgb(0, 0, 0));
        assert_eq!(c("  CornflowerBlue "), Color::rgb(0x64, 0x95, 0xED));
        assert_eq!(c("rebeccapurple"), Color::rgb(0x66, 0x33, 0x99));
        assert_eq!(c("green"), Color::rgb(0, 128, 0));
        assert_eq!(c("lime"), Color::rgb(0, 255, 0));
        assert_eq!(c("grey"), c("gray"));
        assert_eq!(c("darkslategrey"), c("darkslategray"));
    }

    #[test]
    fn named_table_is_complete_sorted_and_unique() {
        assert_eq!(NAMED_COLORS.len(), 148);
        for w in NAMED_COLORS.windows(2) {
            assert!(w[0].0 < w[1].0, "{} !< {}", w[0].0, w[1].0);
        }
        for (name, v) in NAMED_COLORS {
            assert!(name.bytes().all(|b| b.is_ascii_lowercase()), "{name}");
            assert!(*v <= 0xFF_FFFF);
            let upper = name.to_ascii_uppercase();
            assert_eq!(c(name), c(&upper));
            assert_eq!(c(name).to_hex(), format!("#{v:06X}"));
        }
    }

    #[test]
    fn parse_transparent() {
        assert_eq!(c("transparent"), Color::TRANSPARENT);
        assert_eq!(c("TRANSPARENT"), Color::TRANSPARENT);
        assert!(c("transparent").is_transparent());
    }

    #[test]
    fn parse_invalid() {
        for s in ["", "   ", "nope", "currentcolor", "inherit", "redd", "re d"] {
            bad(s);
        }
    }

    #[test]
    fn to_hex_format() {
        assert_eq!(Color::rgb(255, 0, 0).to_hex(), "#FF0000");
        assert_eq!(Color::new(255, 0, 0, 128).to_hex(), "#FF000080");
        assert_eq!(Color::new(1, 2, 3, 4).to_rrggbb(), "010203");
    }

    #[test]
    fn to_css_round_trips() {
        assert_eq!(Color::rgb(1, 2, 3).to_css(), "#010203");
        assert_eq!(
            Color::new(255, 0, 0, 128).to_css(),
            "rgba(255, 0, 0, 0.502)"
        );
        for s in [
            "#123456",
            "rgba(10, 20, 30, 0.5)",
            "transparent",
            "hsl(200 40% 30% / 0.25)",
        ] {
            let color = c(s);
            assert_eq!(c(&color.to_css()), color, "{s}");
            assert_eq!(c(&color.to_hex()), color, "{s}");
        }
    }
}
