//! CSS `font-size` values: a [`Length`] or one of the size keywords.

use crate::length::Length;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid font size: {0}")]
pub struct FontSizeParseError(pub String);

/// A parsed `font-size`.
#[derive(Debug, Clone, PartialEq)]
pub enum FontSize {
    /// An explicit length (`12px`, `1.2em`, `90%`, …).
    Length(Length),
    /// An absolute-size keyword (`small`, `x-large`, …) and its CSS size in px.
    Keyword(&'static str, f64),
    /// `smaller`: one step down from the parent size.
    Smaller,
    /// `larger`: one step up from the parent size.
    Larger,
}

/// The CSS absolute-size keywords and their sizes in px (CSS Fonts 4, with a
/// 16px `medium`).
pub const FONT_SIZE_KEYWORDS: &[(&str, f64)] = &[
    ("xx-small", 9.0),
    ("x-small", 10.0),
    ("small", 13.0),
    ("medium", 16.0),
    ("large", 18.0),
    ("x-large", 24.0),
    ("xx-large", 32.0),
    ("xxx-large", 48.0),
];

impl FontSize {
    /// The size in pt, given the parent's and the root's font sizes in pt.
    /// `smaller`/`larger` scale the parent by 1/1.2 and 1.2, as browsers do.
    pub fn to_pt(&self, parent_pt: f64, root_pt: f64) -> Option<f64> {
        match self {
            FontSize::Length(Length::Percent(p)) => Some(parent_pt * p / 100.0),
            FontSize::Length(l) => l.to_pt(parent_pt, root_pt),
            FontSize::Keyword(_, px) => Some(px * 0.75),
            FontSize::Smaller => Some(parent_pt / 1.2),
            FontSize::Larger => Some(parent_pt * 1.2),
        }
    }
}

impl std::fmt::Display for FontSize {
    /// CSS form: the keyword, or the normalized length.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontSize::Length(l) => write!(f, "{l}"),
            FontSize::Keyword(name, _) => f.write_str(name),
            FontSize::Smaller => f.write_str("smaller"),
            FontSize::Larger => f.write_str("larger"),
        }
    }
}

impl FromStr for FontSize {
    type Err = FontSizeParseError;

    /// A length that makes sense as a font size (not `auto`, `fr`, or negative),
    /// or a keyword (case-insensitive).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim().to_ascii_lowercase();
        if let Some(&(name, px)) = FONT_SIZE_KEYWORDS.iter().find(|(k, _)| *k == t) {
            return Ok(FontSize::Keyword(name, px));
        }
        match t.as_str() {
            "smaller" => return Ok(FontSize::Smaller),
            "larger" => return Ok(FontSize::Larger),
            _ => {}
        }
        match t.parse::<Length>() {
            Ok(Length::Auto | Length::Fr(_)) | Err(_) => Err(FontSizeParseError(s.to_string())),
            Ok(l) if l.is_negative() => Err(FontSizeParseError(s.to_string())),
            Ok(l) => Ok(FontSize::Length(l)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_lengths() {
        assert_eq!(
            "small".parse::<FontSize>().unwrap(),
            FontSize::Keyword("small", 13.0)
        );
        assert_eq!(
            " X-Large ".parse::<FontSize>().unwrap(),
            FontSize::Keyword("x-large", 24.0)
        );
        assert_eq!("smaller".parse::<FontSize>().unwrap(), FontSize::Smaller);
        assert_eq!("LARGER".parse::<FontSize>().unwrap(), FontSize::Larger);
        assert_eq!(
            "12px".parse::<FontSize>().unwrap(),
            FontSize::Length(Length::Px(12.0))
        );
        assert_eq!(
            "90%".parse::<FontSize>().unwrap(),
            FontSize::Length(Length::Percent(90.0))
        );
    }

    #[test]
    fn rejects_non_sizes() {
        for s in ["auto", "1fr", "-2px", "big", "", "12", "NaNpx", "x small"] {
            assert!(s.parse::<FontSize>().is_err(), "{s:?}");
        }
    }

    #[test]
    fn to_pt() {
        let pt = |s: &str| {
            let v = s.parse::<FontSize>().unwrap().to_pt(12.0, 10.0).unwrap();
            (v * 1e9).round() / 1e9
        };
        assert_eq!(pt("medium"), 12.0);
        assert_eq!(pt("16px"), 12.0);
        assert_eq!(pt("2em"), 24.0);
        assert_eq!(pt("2rem"), 20.0);
        assert_eq!(pt("50%"), 6.0);
        assert_eq!(pt("larger"), 14.4);
        assert_eq!(pt("smaller"), 10.0);
    }
}
