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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("auto") {
            return Ok(Length::Auto);
        }

        if let Some(num) = s.strip_suffix("px") {
            return parse_num(num).map(Length::Px).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("pt") {
            return parse_num(num).map(Length::Pt).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("rem") {
            return parse_num(num).map(Length::Rem).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("em") {
            return parse_num(num).map(Length::Em).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("in") {
            return parse_num(num).map(Length::In).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("cm") {
            return parse_num(num).map(Length::Cm).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("mm") {
            return parse_num(num).map(Length::Mm).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix('%') {
            return parse_num(num).map(Length::Percent).ok_or_else(|| err(s));
        }
        if let Some(num) = s.strip_suffix("fr") {
            return parse_num(num).map(Length::Fr).ok_or_else(|| err(s));
        }

        Err(LengthParseError(s.to_string()))
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

fn parse_num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
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
        assert!("bogus".parse::<Length>().is_err());
        assert!("".parse::<Length>().is_err());
        assert!("px".parse::<Length>().is_err());
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
