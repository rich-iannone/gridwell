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

    /// To hex string (#RRGGBB or #RRGGBBAA if alpha < 255).
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
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
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        if s.eq_ignore_ascii_case("transparent") {
            return Ok(Color::TRANSPARENT);
        }

        // Named colors (subset — the most common ones)
        if let Some(c) = named_color(s) {
            return Ok(c);
        }

        // Hex
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex).ok_or_else(|| ColorParseError(s.to_string()));
        }

        // rgb()/rgba()
        if let Some(inner) = s
            .strip_prefix("rgba(")
            .and_then(|s| s.strip_suffix(')'))
            .or_else(|| s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')))
        {
            return parse_rgb_func(inner).ok_or_else(|| ColorParseError(s.to_string()));
        }

        Err(ColorParseError(s.to_string()))
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

