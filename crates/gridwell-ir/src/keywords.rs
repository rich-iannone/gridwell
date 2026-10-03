//! Typed keyword values for the IR's enumerated fields (alignment, roles, border
//! styles, CSS-like style keywords, …).
//!
//! Every keyword type parses **leniently**: matching ignores ASCII case and
//! surrounding whitespace, known aliases map to one canonical spelling, and an
//! unrecognized value becomes `Unknown(original)` instead of a parse error. Unknown
//! values round-trip unchanged and are reported by the validator
//! ([`ValidationRule::UnknownValue`](crate::ValidationRule::UnknownValue)) with their
//! location and the allowed values, so a producer sees *every* bad value at once
//! rather than the first serde error.
//!
//! Known values serialize as their canonical spelling (`ALLOWED`).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Common interface of keyword types, used by validation and error messages.
pub trait Keyword {
    /// Canonical spellings of the known values.
    const ALLOWED: &'static [&'static str];
    /// The value as written in canonical form (or verbatim if unknown).
    fn as_str(&self) -> &str;
    /// `false` for `Unknown(_)`.
    fn is_known(&self) -> bool;
}

macro_rules! keyword {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $canon:literal $(| $alias:literal)* ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(from = "String", into = "String")]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// Not a recognized value; kept verbatim and reported by validation.
            Unknown(String),
        }

        impl $name {
            /// Parse leniently (see the module docs). Never fails.
            pub fn parse(s: &str) -> Self {
                match s.trim().to_ascii_lowercase().as_str() {
                    $( $canon $(| $alias)* => Self::$variant, )+
                    _ => Self::Unknown(s.to_string()),
                }
            }
        }

        impl Keyword for $name {
            const ALLOWED: &'static [&'static str] = &[$($canon),+];
            fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $canon, )+
                    Self::Unknown(s) => s,
                }
            }
            fn is_known(&self) -> bool {
                !matches!(self, Self::Unknown(_))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self::parse(&s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::parse(s)
            }
        }

        impl From<$name> for String {
            fn from(v: $name) -> String {
                v.as_str().to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

keyword! {
    /// Horizontal alignment of a column (`column_spec[].align`) or of text
    /// (`text_align`). `Char` aligns on `align_char` (e.g. a decimal point).
    HAlign {
        Left = "left" | "start",
        Center = "center" | "centre",
        Right = "right" | "end",
        Justify = "justify",
        Char = "char",
    }
}

// Not derivable: the enum comes from `keyword!`, and only some keywords have a default.
#[allow(clippy::derivable_impls)]
impl Default for HAlign {
    fn default() -> Self {
        HAlign::Left
    }
}

keyword! {
    /// Vertical alignment (`vertical_align`).
    VAlign {
        Top = "top",
        Middle = "middle",
        Bottom = "bottom",
        Baseline = "baseline",
        Super = "super",
        Sub = "sub",
    }
}

keyword! {
    /// The role of a row. Column-label and spanner rows belong in `thead`; summary
    /// rows in a group's `summary_rows`.
    RowRole {
        ColumnLabel = "column_label",
        SpannerLabel = "spanner_label",
        Summary = "summary_row" | "summary",
        GrandSummary = "grand_summary_row" | "grand_summary",
    }
}

keyword! {
    /// The HTML `scope` of a header cell.
    CellScope {
        Col = "col",
        ColGroup = "colgroup",
        Row = "row",
        RowGroup = "rowgroup",
    }
}

keyword! {
    /// How a paginating format may break a table across pages.
    PageBreakMode {
        Avoid = "avoid",
        Allow = "allow",
        ForceBetweenGroups = "force-between-groups" | "force_between_groups",
    }
}

// Not derivable: the enum comes from `keyword!`, and only some keywords have a default.
#[allow(clippy::derivable_impls)]
impl Default for PageBreakMode {
    fn default() -> Self {
        PageBreakMode::Avoid
    }
}

keyword! {
    /// Border line style.
    BorderStyle {
        None = "none",
        Hidden = "hidden",
        Solid = "solid",
        Dashed = "dashed",
        Dotted = "dotted",
        Double = "double",
    }
}

keyword! {
    /// Font style.
    FontStyle {
        Normal = "normal",
        Italic = "italic",
        Oblique = "oblique",
    }
}

impl FontStyle {
    /// Whether the style renders slanted (italic or oblique).
    pub fn is_italic(&self) -> bool {
        matches!(self, Self::Italic | Self::Oblique)
    }
}

keyword! {
    /// Text transform.
    TextTransform {
        None = "none",
        Uppercase = "uppercase",
        Lowercase = "lowercase",
        Capitalize = "capitalize",
    }
}

keyword! {
    /// Text decoration line.
    TextDecoration {
        None = "none",
        Underline = "underline",
        Overline = "overline",
        LineThrough = "line-through" | "line_through",
    }
}

keyword! {
    /// White-space handling.
    WhiteSpace {
        Normal = "normal",
        Nowrap = "nowrap",
        Pre = "pre",
        PreWrap = "pre-wrap",
        PreLine = "pre-line",
        BreakSpaces = "break-spaces",
    }
}

keyword! {
    /// Word breaking.
    WordBreak {
        Normal = "normal",
        BreakAll = "break-all",
        KeepAll = "keep-all",
        BreakWord = "break-word",
    }
}

keyword! {
    /// Overflow handling (cells and the container).
    Overflow {
        Visible = "visible",
        Hidden = "hidden",
        Clip = "clip",
        Scroll = "scroll",
        Auto = "auto",
    }
}

keyword! {
    /// Text overflow marker.
    TextOverflow {
        Clip = "clip",
        Ellipsis = "ellipsis",
    }
}

keyword! {
    /// Row parity for conditional styles (1-based, like CSS `:nth-child`).
    RowParity {
        Even = "even",
        Odd = "odd",
    }
}

keyword! {
    /// Which part of the table a conditional style applies to.
    SelectorScope {
        Tbody = "tbody" | "body",
        Thead = "thead" | "head",
        Table = "table" | "all",
    }
}

keyword! {
    /// Type of a cell's raw value (`typed_value.type`, `data_type`).
    ValueType {
        String = "string" | "str" | "text" | "character",
        Number = "number" | "float" | "double" | "numeric",
        Integer = "integer" | "int",
        Boolean = "boolean" | "bool" | "logical",
        Date = "date",
        Datetime = "datetime" | "timestamp",
        Time = "time",
        Duration = "duration",
    }
}

/// Font weight: a keyword or a CSS numeric weight (1–1000). Accepts a JSON string
/// (`"bold"`, `"700"`) or number (`700`); serializes as a string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "FontWeightRepr", into = "String")]
pub enum FontWeight {
    Normal,
    Bold,
    Bolder,
    Lighter,
    /// A numeric weight, 1–1000 (400 = normal, 700 = bold).
    Numeric(u16),
    /// Not a recognized value; kept verbatim and reported by validation.
    Unknown(String),
}

impl FontWeight {
    /// Parse leniently. Never fails.
    pub fn parse(s: &str) -> Self {
        let t = s.trim().to_ascii_lowercase();
        match t.as_str() {
            "normal" => Self::Normal,
            "bold" => Self::Bold,
            "bolder" => Self::Bolder,
            "lighter" => Self::Lighter,
            n => match n.parse::<u16>() {
                Ok(w) if (1..=1000).contains(&w) => Self::Numeric(w),
                _ => Self::Unknown(s.to_string()),
            },
        }
    }

    /// Whether the weight renders bold (bold, bolder, or ≥ 600).
    pub fn is_bold(&self) -> bool {
        match self {
            Self::Bold | Self::Bolder => true,
            Self::Numeric(w) => *w >= 600,
            _ => false,
        }
    }
}

impl Keyword for FontWeight {
    const ALLOWED: &'static [&'static str] = &["normal", "bold", "bolder", "lighter", "1–1000"];
    fn as_str(&self) -> &str {
        match self {
            Self::Normal => "normal",
            Self::Bold => "bold",
            Self::Bolder => "bolder",
            Self::Lighter => "lighter",
            // Numeric values are formatted by `Display`; `as_str` is only used for
            // messages, where the number itself is not needed.
            Self::Numeric(_) => "numeric",
            Self::Unknown(s) => s,
        }
    }
    fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

/// Wire form of [`FontWeight`]: string or number.
#[derive(Deserialize)]
#[serde(untagged)]
enum FontWeightRepr {
    Text(String),
    Number(f64),
}

impl From<FontWeightRepr> for FontWeight {
    fn from(r: FontWeightRepr) -> Self {
        match r {
            FontWeightRepr::Text(s) => Self::parse(&s),
            FontWeightRepr::Number(n) if n.fract() == 0.0 && (1.0..=1000.0).contains(&n) => {
                Self::Numeric(n as u16)
            }
            FontWeightRepr::Number(n) => Self::Unknown(n.to_string()),
        }
    }
}

impl From<String> for FontWeight {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&str> for FontWeight {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<FontWeight> for String {
    fn from(v: FontWeight) -> String {
        v.to_string()
    }
}

impl fmt::Display for FontWeight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric(w) => write!(f, "{w}"),
            other => f.write_str(other.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_is_case_and_space_insensitive() {
        assert_eq!(HAlign::parse("  RIGHT "), HAlign::Right);
        assert_eq!(BorderStyle::parse("Dashed"), BorderStyle::Dashed);
    }

    #[test]
    fn aliases_map_to_canonical_spelling() {
        assert_eq!(RowRole::parse("summary"), RowRole::Summary);
        assert_eq!(RowRole::Summary.as_str(), "summary_row");
        assert_eq!(
            RowRole::parse("grand_summary").to_string(),
            "grand_summary_row"
        );
        assert_eq!(SelectorScope::parse("body"), SelectorScope::Tbody);
        assert_eq!(HAlign::parse("centre"), HAlign::Center);
    }

    #[test]
    fn unknown_values_are_preserved_verbatim() {
        let v = HAlign::parse("Middle-ish");
        assert_eq!(v, HAlign::Unknown("Middle-ish".into()));
        assert!(!v.is_known());
        assert_eq!(v.as_str(), "Middle-ish");
    }

    #[test]
    fn serde_round_trips_known_canonically_and_unknown_verbatim() {
        let known: HAlign = serde_json::from_str("\"Right\"").unwrap();
        assert_eq!(serde_json::to_string(&known).unwrap(), "\"right\"");
        let unknown: HAlign = serde_json::from_str("\"diagonal\"").unwrap();
        assert_eq!(serde_json::to_string(&unknown).unwrap(), "\"diagonal\"");
    }

    #[test]
    fn non_string_json_is_a_parse_error() {
        assert!(serde_json::from_str::<HAlign>("3").is_err());
    }

    #[test]
    fn allowed_lists_every_canonical_value_once() {
        for allowed in [
            HAlign::ALLOWED,
            RowRole::ALLOWED,
            BorderStyle::ALLOWED,
            ValueType::ALLOWED,
        ] {
            let mut v = allowed.to_vec();
            v.sort();
            v.dedup();
            assert_eq!(v.len(), allowed.len());
        }
        // Every canonical spelling parses back to a known value.
        for s in HAlign::ALLOWED {
            assert!(HAlign::parse(s).is_known(), "{s}");
        }
        for s in RowRole::ALLOWED {
            assert!(RowRole::parse(s).is_known(), "{s}");
        }
    }

    #[test]
    fn font_weight_keywords_and_numbers() {
        assert_eq!(FontWeight::parse("bold"), FontWeight::Bold);
        assert_eq!(FontWeight::parse("700"), FontWeight::Numeric(700));
        assert_eq!(FontWeight::parse("700").to_string(), "700");
        assert!(FontWeight::parse("600").is_bold());
        assert!(!FontWeight::parse("500").is_bold());
        assert!(FontWeight::Bolder.is_bold());
        assert!(!FontWeight::Normal.is_bold());
        assert!(!FontWeight::parse("0").is_known());
        assert!(!FontWeight::parse("1001").is_known());
        assert!(!FontWeight::parse("heavy").is_known());
        let w: FontWeight = serde_json::from_str("\"900\"").unwrap();
        assert_eq!(serde_json::to_string(&w).unwrap(), "\"900\"");
        // JSON numbers are accepted too, and normalized to strings.
        let w: FontWeight = serde_json::from_str("700").unwrap();
        assert_eq!(w, FontWeight::Numeric(700));
        assert_eq!(serde_json::to_string(&w).unwrap(), "\"700\"");
        let w: FontWeight = serde_json::from_str("650.5").unwrap();
        assert!(!w.is_known());
    }

    #[test]
    fn defaults() {
        assert_eq!(HAlign::default(), HAlign::Left);
        assert_eq!(PageBreakMode::default(), PageBreakMode::Avoid);
    }
}
