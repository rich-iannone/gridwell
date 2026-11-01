//! BCP 47 language tags (`en`, `fr-CA`, `zh-Hant-TW`), checked for well-formedness
//! (RFC 5646's syntax; whether a subtag is registered is not checked) and split
//! into the parts formats ask for.

use std::fmt;

/// A well-formed language tag and its main subtags, case-normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTag {
    /// The tag as written.
    pub tag: String,
    /// The primary language, lowercase (`fr`); `None` for a private-use tag
    /// (`x-…`).
    pub language: Option<String>,
    /// The script, title case (`Hant`).
    pub script: Option<String>,
    /// The region, uppercase (`CA`, `419`).
    pub region: Option<String>,
}

/// Why a string is not a language tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTagError(pub &'static str);

impl fmt::Display for LanguageTagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LanguageTagError {}

/// Scripts written right to left.
const RTL_SCRIPTS: &[&str] = &[
    "Adlm", "Arab", "Aran", "Hebr", "Mand", "Mend", "Nkoo", "Rohg", "Samr", "Syrc", "Thaa", "Yezi",
];

/// Languages written right to left when the tag names no script (in their
/// usual script, per CLDR's likely subtags).
const RTL_LANGUAGES: &[&str] = &[
    "ar", "arc", "bal", "bqi", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn",
    "nqo", "pnb", "prs", "ps", "sd", "sdh", "syr", "ug", "ur", "yi",
];

impl LanguageTag {
    /// Parse a tag. Subtags are separated by `-` and matched ignoring case.
    pub fn parse(s: &str) -> Result<Self, LanguageTagError> {
        if s.contains('_') {
            return Err(LanguageTagError(
                "subtags are separated by `-`, not `_` (fr-CA, not fr_CA)",
            ));
        }
        let bad =
            LanguageTagError("it is not a BCP 47 language tag (such as en, fr-CA or zh-Hant-TW)");
        let subtags: Vec<&str> = s.split('-').collect();
        let alnum =
            |t: &str| (1..=8).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric());
        if !subtags.iter().all(|t| alnum(t)) {
            return Err(bad);
        }
        let alpha = |t: &str| t.bytes().all(|b| b.is_ascii_alphabetic());
        let digit = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
        let mut tag = LanguageTag {
            tag: s.to_string(),
            language: None,
            script: None,
            region: None,
        };
        let mut rest = &subtags[..];
        // Private use only: `x-…`.
        if !rest[0].eq_ignore_ascii_case("x") {
            let lang = rest[0];
            if !(alpha(lang) && lang.len() >= 2) {
                return Err(bad);
            }
            tag.language = Some(lang.to_ascii_lowercase());
            rest = &rest[1..];
            // Up to three extended language subtags, after a 2–3 letter language.
            if lang.len() <= 3 {
                let mut n = 0;
                while n < 3 && rest.first().is_some_and(|t| t.len() == 3 && alpha(t)) {
                    rest = &rest[1..];
                    n += 1;
                }
            }
            if let Some(t) = rest.first().filter(|t| t.len() == 4 && alpha(t)) {
                let mut script = t.to_ascii_lowercase();
                script[..1].make_ascii_uppercase();
                tag.script = Some(script);
                rest = &rest[1..];
            }
            if let Some(t) = rest
                .first()
                .filter(|t| (t.len() == 2 && alpha(t)) || (t.len() == 3 && digit(t)))
            {
                tag.region = Some(t.to_ascii_uppercase());
                rest = &rest[1..];
            }
            // Variants: 5–8 characters, or a digit and three more.
            while let Some(t) = rest.first() {
                let variant = (5..=8).contains(&t.len())
                    || (t.len() == 4 && t.as_bytes()[0].is_ascii_digit());
                if !variant {
                    break;
                }
                rest = &rest[1..];
            }
            // Extensions: a singleton other than `x`, then subtags of 2–8.
            while let Some(t) = rest.first() {
                if t.len() != 1 || t.eq_ignore_ascii_case("x") {
                    break;
                }
                let n = rest[1..].iter().take_while(|t| t.len() >= 2).count();
                if n == 0 {
                    return Err(bad);
                }
                rest = &rest[1 + n..];
            }
        }
        // Private use: `x` and at least one subtag; anything else is left over.
        match rest {
            [] => Ok(tag),
            [x, more @ ..] if x.eq_ignore_ascii_case("x") && !more.is_empty() => Ok(tag),
            _ => Err(bad),
        }
    }

    /// Whether the tag's text is written right to left: by its script, else by its
    /// language's usual script.
    pub fn is_rtl(&self) -> bool {
        match (&self.script, &self.language) {
            (Some(script), _) => RTL_SCRIPTS.contains(&script.as_str()),
            (None, Some(lang)) => RTL_LANGUAGES.contains(&lang.as_str()),
            (None, None) => false,
        }
    }
}

/// The direction of a text's first strongly directional character: `Some(true)`
/// for right to left, `Some(false)` for left to right, `None` if it has none
/// (digits, punctuation and symbols are not strong). This is how HTML resolves
/// `dir="auto"`, with letters standing in for Unicode's strong classes: letters of
/// the Hebrew, Arabic, Syriac, Thaana, N'Ko and other right-to-left blocks are
/// right to left, every other letter left to right.
pub fn first_strong_is_rtl(text: &str) -> Option<bool> {
    text.chars().find(|c| c.is_alphabetic()).map(|c| {
        matches!(u32::from(c),
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(s: &str) -> (Option<String>, Option<String>, Option<String>) {
        let t = LanguageTag::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        assert_eq!(t.tag, s);
        (t.language, t.script, t.region)
    }

    fn some(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn well_formed_tags_are_split_into_parts() {
        assert_eq!(parts("en"), (some("en"), None, None));
        assert_eq!(parts("fr-CA"), (some("fr"), None, some("CA")));
        assert_eq!(parts("ZH-hant-tw"), (some("zh"), some("Hant"), some("TW")));
        assert_eq!(parts("es-419"), (some("es"), None, some("419")));
        assert_eq!(parts("sr-Latn"), (some("sr"), some("Latn"), None));
        assert_eq!(parts("zh-yue-HK"), (some("zh"), None, some("HK")));
        assert_eq!(parts("de-CH-1901"), (some("de"), None, some("CH")));
        assert_eq!(parts("sl-rozaj-biske"), (some("sl"), None, None));
        assert_eq!(parts("en-US-u-ca-gregory"), (some("en"), None, some("US")));
        assert_eq!(parts("en-a-bbb-x-a-ccc"), (some("en"), None, None));
        assert_eq!(parts("x-whatever"), (None, None, None));
        assert_eq!(
            parts("qaa-Qaaa-QM-x-southern"),
            (some("qaa"), some("Qaaa"), some("QM"))
        );
        assert_eq!(parts("haw"), (some("haw"), None, None));
    }

    #[test]
    fn malformed_tags_are_rejected() {
        for s in [
            "",
            "-",
            "en-",
            "-en",
            "e",
            "1en",
            "en--US",
            "en US",
            "en.US",
            "toolonglang",
            "en-toolongsubtag",
            "en-a",
            "en-a-x-b",
            "x",
            "en-x",
            "en-US-a",
            "é",
            "en-ÜS",
            "en-12",
            "en-US-Latn",
        ] {
            assert!(LanguageTag::parse(s).is_err(), "{s:?} accepted");
        }
        let e = LanguageTag::parse("fr_CA").unwrap_err();
        assert!(e.0.contains("not `_`"), "{e}");
    }

    #[test]
    fn first_strong_character_decides() {
        assert_eq!(first_strong_is_rtl("123 مرحبا abc"), Some(true));
        assert_eq!(first_strong_is_rtl("(1) שלום"), Some(true));
        assert_eq!(first_strong_is_rtl("١٢٣ abc مرحبا"), Some(false));
        assert_eq!(first_strong_is_rtl("— Ünïcode"), Some(false));
        assert_eq!(first_strong_is_rtl("漢字"), Some(false));
        assert_eq!(first_strong_is_rtl("١٢٣ 456 !?"), None);
        assert_eq!(first_strong_is_rtl(""), None);
    }

    #[test]
    fn direction_comes_from_the_script_else_the_language() {
        let rtl = |s: &str| LanguageTag::parse(s).unwrap().is_rtl();
        for s in [
            "ar",
            "ar-EG",
            "he",
            "fa-IR",
            "ur",
            "yi",
            "ckb",
            "az-Arab",
            "pa-Arab-PK",
        ] {
            assert!(rtl(s), "{s}");
        }
        for s in ["en", "fr-CA", "zh-Hant", "ar-Latn", "ku", "x-ar", "az"] {
            assert!(!rtl(s), "{s}");
        }
    }
}
