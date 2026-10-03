use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid length value: {0}")]
pub struct LengthParseError(pub String);

/// A parsed CSS length value.
#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    Px(f64),
    Pt(f64),
    Em(f64),
    Rem(f64),
    In(f64),
    Cm(f64),
    Mm(f64),
    Percent(f64),
    Fr(f64),
    Auto,
}

impl Length {
    /// Convert to points (1px = 0.75pt, 1in = 72pt, 1cm = 28.3465pt, 1mm = 2.83465pt).
    pub fn to_pt(&self, font_size_pt: f64, root_font_size_pt: f64) -> Option<f64> {
        match self {
            Length::Px(v) => Some(v * 0.75),
            Length::Pt(v) => Some(*v),
            Length::Em(v) => Some(v * font_size_pt),
            Length::Rem(v) => Some(v * root_font_size_pt),
            Length::In(v) => Some(v * 72.0),
            Length::Cm(v) => Some(v * 28.346_456_7),
            Length::Mm(v) => Some(v * 2.834_645_67),
            Length::Percent(_) | Length::Fr(_) | Length::Auto => None,
        }
    }

    /// Convert to twips (1pt = 20 twips).
    pub fn to_twips(&self, font_size_pt: f64, root_font_size_pt: f64) -> Option<f64> {
        self.to_pt(font_size_pt, root_font_size_pt)
            .map(|pt| pt * 20.0)
    }

    /// Convert to EMU (1pt = 12700 EMU).
    pub fn to_emu(&self, font_size_pt: f64, root_font_size_pt: f64) -> Option<f64> {
        self.to_pt(font_size_pt, root_font_size_pt)
            .map(|pt| pt * 12700.0)
    }
}

impl FromStr for Length {
    type Err = LengthParseError;

    /// Parse a CSS length: a finite number followed by a unit (case-insensitive),
    /// `auto`, or a unitless `0`. Units: `px pt em rem in cm mm % fr`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        if lower == "auto" {
            return Ok(Length::Auto);
        }
        // CSS allows a bare zero (and only zero) without a unit.
        if let Some(v) = parse_num(&lower) {
            return if v == 0.0 {
                Ok(Length::Px(0.0))
            } else {
                Err(err(s))
            };
        }

        // `rem` before `em`: the longer suffix must win.
        type Make = fn(f64) -> Length;
        const UNITS: [(&str, Make); 9] = [
            ("px", Length::Px),
            ("pt", Length::Pt),
            ("rem", Length::Rem),
            ("em", Length::Em),
            ("in", Length::In),
            ("cm", Length::Cm),
            ("mm", Length::Mm),
            ("%", Length::Percent),
            ("fr", Length::Fr),
        ];
        for (unit, make) in UNITS {
            if let Some(num) = lower.strip_suffix(unit) {
                return parse_num(num).map(make).ok_or_else(|| err(s));
            }
        }
        Err(err(s))
    }
}

impl Length {
    /// The numeric part (`None` for `auto`).
    pub fn value(&self) -> Option<f64> {
        match *self {
            Length::Px(v)
            | Length::Pt(v)
            | Length::Em(v)
            | Length::Rem(v)
            | Length::In(v)
            | Length::Cm(v)
            | Length::Mm(v)
            | Length::Percent(v)
            | Length::Fr(v) => Some(v),
            Length::Auto => None,
        }
    }

    /// True for a value below zero (never for `auto`).
    pub fn is_negative(&self) -> bool {
        self.value().is_some_and(|v| v < 0.0)
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Length::Px(v) => write!(f, "{v}px"),
            Length::Pt(v) => write!(f, "{v}pt"),
            Length::Em(v) => write!(f, "{v}em"),
            Length::Rem(v) => write!(f, "{v}rem"),
            Length::In(v) => write!(f, "{v}in"),
            Length::Cm(v) => write!(f, "{v}cm"),
            Length::Mm(v) => write!(f, "{v}mm"),
            Length::Percent(v) => write!(f, "{v}%"),
            Length::Fr(v) => write!(f, "{v}fr"),
            Length::Auto => write!(f, "auto"),
        }
    }
}

impl Serialize for Length {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Length::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// A finite number. `f64::from_str` alone would also take `inf`, `infinity` and
/// `NaN`. Whitespace inside the value (`1 px`) is rejected, as in CSS.
fn parse_num(s: &str) -> Option<f64> {
    if s.contains(char::is_whitespace) {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn err(s: &str) -> LengthParseError {
    LengthParseError(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_px() {
        assert_eq!("12px".parse::<Length>().unwrap(), Length::Px(12.0));
        assert_eq!("0px".parse::<Length>().unwrap(), Length::Px(0.0));
        assert_eq!("3.5px".parse::<Length>().unwrap(), Length::Px(3.5));
    }

    #[test]
    fn parse_percent() {
        assert_eq!("100%".parse::<Length>().unwrap(), Length::Percent(100.0));
        assert_eq!("50.5%".parse::<Length>().unwrap(), Length::Percent(50.5));
    }

    #[test]
    fn parse_fr() {
        assert_eq!("1fr".parse::<Length>().unwrap(), Length::Fr(1.0));
        assert_eq!("2.5fr".parse::<Length>().unwrap(), Length::Fr(2.5));
    }

    #[test]
    fn parse_auto() {
        assert_eq!("auto".parse::<Length>().unwrap(), Length::Auto);
        assert_eq!("AUTO".parse::<Length>().unwrap(), Length::Auto);
    }

    #[test]
    fn parse_invalid() {
        for s in [
            "bogus",
            "",
            "  ",
            "px",
            "%",
            "12",
            "1.5",
            "-3",
            "12 px",
            "12p x",
            "12pxx",
            "px12",
            "12px;",
            "12px 3px",
            "1,5px",
            "12vw",
            "calc(1px + 2px)",
            "0x10px",
            "auto px",
        ] {
            assert!(s.parse::<Length>().is_err(), "{s:?} should be rejected");
        }
    }

    #[test]
    fn non_finite_numbers_are_rejected() {
        for s in [
            "NaNpx",
            "nanpx",
            "infpx",
            "-infpt",
            "infinityem",
            "inf%",
            "NaN%",
            "inffr",
            "1e400px",
            "-1e400px",
            "infin",
            "nan",
            "inf",
        ] {
            assert!(s.parse::<Length>().is_err(), "{s:?} should be rejected");
        }
    }

    #[test]
    fn unitless_zero_is_zero_px() {
        for s in ["0", "0.0", "-0", "+0", " 0 ", "00", "0e5"] {
            assert_eq!(s.parse::<Length>().unwrap(), Length::Px(0.0), "{s:?}");
        }
    }

    #[test]
    fn units_are_case_insensitive() {
        assert_eq!("12PX".parse::<Length>().unwrap(), Length::Px(12.0));
        assert_eq!("1.5Em".parse::<Length>().unwrap(), Length::Em(1.5));
        assert_eq!("2REM".parse::<Length>().unwrap(), Length::Rem(2.0));
        assert_eq!("1IN".parse::<Length>().unwrap(), Length::In(1.0));
        assert_eq!("3Fr".parse::<Length>().unwrap(), Length::Fr(3.0));
    }

    #[test]
    fn every_unit_parses() {
        let cases = [
            ("1px", Length::Px(1.0)),
            ("1pt", Length::Pt(1.0)),
            ("1em", Length::Em(1.0)),
            ("1rem", Length::Rem(1.0)),
            ("1in", Length::In(1.0)),
            ("1cm", Length::Cm(1.0)),
            ("1mm", Length::Mm(1.0)),
            ("1%", Length::Percent(1.0)),
            ("1fr", Length::Fr(1.0)),
            (".5em", Length::Em(0.5)),
            ("+2pt", Length::Pt(2.0)),
            ("-4px", Length::Px(-4.0)),
            ("1e2px", Length::Px(100.0)),
            ("  7mm\t", Length::Mm(7.0)),
        ];
        for (s, want) in cases {
            assert_eq!(s.parse::<Length>().unwrap(), want, "{s:?}");
        }
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "12px", "1.5em", "2rem", "100%", "1fr", "auto", "0.25in", "-3pt", "7cm", "4mm",
        ] {
            let l: Length = s.parse().unwrap();
            assert_eq!(l.to_string(), s);
            assert_eq!(l.to_string().parse::<Length>().unwrap(), l);
        }
    }

    #[test]
    fn value_and_sign() {
        assert_eq!(Length::Pt(3.0).value(), Some(3.0));
        assert_eq!(Length::Auto.value(), None);
        assert!(Length::Px(-1.0).is_negative());
        assert!(!Length::Px(0.0).is_negative());
        assert!(!Length::Auto.is_negative());
    }

    #[test]
    fn to_pt_conversion() {
        assert_eq!(Length::Px(1.0).to_pt(12.0, 16.0), Some(0.75));
        assert_eq!(Length::Pt(12.0).to_pt(12.0, 16.0), Some(12.0));
        assert_eq!(Length::In(1.0).to_pt(12.0, 16.0), Some(72.0));
        assert_eq!(Length::Em(2.0).to_pt(12.0, 16.0), Some(24.0));
        assert_eq!(Length::Rem(1.0).to_pt(12.0, 16.0), Some(16.0));
        assert_eq!(Length::Auto.to_pt(12.0, 16.0), None);
    }
}
