//! Estimated text widths, without a font's metrics: what a format laid out
//! elsewhere (a slide's table, whose columns don't fit their content on their own)
//! sizes its columns for, and what SVG layout uses for the characters its fonts
//! don't cover.
//!
//! The estimates are DejaVu Sans' advances, rounded up: wider than most fonts a
//! viewer substitutes, so text sized for them fits.
//!
//! Text is measured by grapheme cluster (what a reader sees as one character), so
//! an emoji sequence (a family joined with ZWJ, a skin tone, a flag, a keycap) is
//! one emoji wide, as a viewer draws it, not the sum of the code points in it.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

/// Bold text is wider; DejaVu Sans Bold runs about 8–11% wider than regular.
pub const BOLD_FACTOR: f64 = 1.12;

/// Estimated advance of an emoji, in em: Noto Color Emoji's is 1.245em, Apple
/// Color Emoji's about 1.2em.
pub const EMOJI_EM: f64 = 1.25;

/// Estimated advance of `c`, in em (DejaVu-like widths, rounded up).
pub fn char_em(c: char) -> f64 {
    if c.is_ascii() {
        return ascii_em(c);
    }
    match c.width() {
        // Combining marks, zero-width joiners, variation selectors, controls.
        Some(0) | None => 0.0,
        Some(2) if is_emoji_char(c) => EMOJI_EM,
        // East Asian wide / fullwidth characters are one em in CJK fonts.
        Some(2) => 1.04,
        // Accented Latin, Greek, Cyrillic, Arabic, Hebrew, …: wide Latin bound.
        _ => 0.70,
    }
}

/// Estimated advance of the grapheme cluster `g`, in em: [`EMOJI_EM`] for an
/// [emoji](is_emoji), else the sum of its characters' [`char_em`] (a letter's
/// combining marks are free).
pub fn cluster_em(g: &str) -> f64 {
    if is_emoji(g) {
        EMOJI_EM
    } else {
        g.chars().map(char_em).sum()
    }
}

/// `s`'s grapheme clusters, the pieces a line breaks between: each one's first
/// character (a space, a hyphen, a letter) and its [`cluster_em`].
pub fn clusters(s: &str) -> impl Iterator<Item = (char, f64)> + '_ {
    s.graphemes(true)
        .filter_map(|g| Some((g.chars().next()?, cluster_em(g))))
}

/// The estimated width of `s`, in em: its grapheme clusters' [`cluster_em`],
/// wider by [`BOLD_FACTOR`] when bold.
pub fn text_em(s: &str, bold: bool) -> f64 {
    let em: f64 = clusters(s).map(|(_, em)| em).sum();
    if bold {
        em * BOLD_FACTOR
    } else {
        em
    }
}

/// Whether the grapheme cluster `g` is drawn as one emoji: an emoji (with any
/// skin tone, or other emoji joined to it with ZWJ: 👍🏽, 👨‍👩‍👧), a flag (a pair
/// of regional indicators, or a tag sequence), or a character asked to look like
/// one with VS16 (❤️, ©️) or made a keycap (1️⃣).
pub fn is_emoji(g: &str) -> bool {
    let Some(first) = g.chars().next() else {
        return false;
    };
    (first.width() == Some(2) && is_emoji_char(first))
        || is_regional_indicator(first)
        || g.chars().any(|c| matches!(c, '\u{FE0F}' | '\u{20E3}'))
}

fn is_emoji_char(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF)
}

/// Half of a flag (🇨🇦 is 🇨 and 🇦); drawn alone, it is a boxed letter, as wide.
fn is_regional_indicator(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// Advance widths (in em) for printable ASCII: DejaVu Sans, rounded up, raised where
/// calibration showed a wider glyph: Arial/Helvetica on macOS, and the pinned harness
/// image's fallback fonts (`*`, `/`, `\\`, `I`, `M`, `|`). See the SVG writer's
/// tests/calibration.rs.
fn ascii_em(c: char) -> f64 {
    match c {
        ' ' => 0.32,
        'i' | 'j' | 'l' | '\'' | '.' | ',' => 0.30,
        'I' => 0.35,
        ':' | ';' => 0.34,
        '/' | '\\' => 0.38,
        'f' | 't' | '-' | '(' | ')' | '[' | ']' | '!' | '`' => 0.40,
        'r' | '"' => 0.46,
        '|' => 0.50,
        'J' | '_' => 0.58,
        's' | 'z' | 'c' | '?' | '*' => 0.56,
        'k' | 'v' | 'x' | 'y' | 'F' | 'L' => 0.60,
        'a' | 'e' | 'o' | 'T' => 0.64,
        'P' | 'Y' | 'E' | 'S' => 0.70,
        'b' | 'd' | 'g' | 'h' | 'n' | 'p' | 'q' | 'u' | '$' | '{' | '}' => 0.64,
        '0'..='9' => 0.64,
        'A' | 'B' | 'K' | 'V' | 'X' | 'Z' => 0.70,
        'R' | 'C' => 0.76,
        'U' | 'N' | 'H' => 0.76,
        'D' | 'G' | 'O' | 'Q' | '&' => 0.80,
        '#' | '+' | '=' | '<' | '>' | '~' | '^' | 'w' => 0.84,
        'M' => 0.90,
        '%' | 'm' | 'W' => 1.0,
        '@' => 1.06,
        // Control characters take no space (they are not rendered).
        c if c.is_ascii_control() => 0.0,
        _ => 0.64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_printable_ascii_has_a_width() {
        for c in (0x20u8..0x7f).map(char::from) {
            assert!(char_em(c) > 0.0, "{c:?}");
            assert!(char_em(c) <= 1.1, "{c:?}");
        }
    }

    #[test]
    fn cjk_is_wider_than_latin_and_combining_marks_are_free() {
        assert_eq!(char_em('東'), 1.04);
        assert!(char_em('😀') > 1.0);
        assert_eq!(char_em('\u{0301}'), 0.0); // combining acute
        assert_eq!(char_em('\u{200D}'), 0.0); // ZWJ
        assert!(char_em('é') >= char_em('e'));
    }

    #[test]
    fn an_emoji_sequence_is_one_emoji_wide() {
        for g in [
            "😀",
            "👍🏽",                                                       // a skin tone
            "👨\u{200D}👩\u{200D}👧\u{200D}👦",                         // a family, joined with ZWJ
            "🏳\u{FE0F}\u{200D}🌈",                                      // a rainbow flag
            "🇨🇦", // a flag: two regional indicators
            "🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}", // a tag flag
            "❤\u{FE0F}", // text by default, emoji with VS16
            "©\u{FE0F}",
            "1\u{FE0F}\u{20E3}", // a keycap
            "#\u{20E3}",
        ] {
            assert_eq!(g.graphemes(true).count(), 1, "{g}");
            assert!(is_emoji(g), "{g}");
            assert_eq!(text_em(g, false), EMOJI_EM, "{g}");
            assert_eq!(text_em(g, true), EMOJI_EM * BOLD_FACTOR, "{g}");
        }
        assert_eq!(text_em("🇨🇦🇺🇸", false), 2.0 * EMOJI_EM);
        assert_eq!(text_em("a👍🏽b", false), EMOJI_EM + 2.0 * char_em('a'));
        // A lone regional indicator is drawn as a boxed letter.
        assert!(is_emoji("🇨"));
    }

    #[test]
    fn other_clusters_are_their_characters() {
        // Not emoji: text, a text-presentation symbol, VS15 (text style), a
        // letter with combining marks, a Devanagari conjunct with ZWJ.
        for g in ["a", "東", "é", "☺", "❤\u{FE0E}", "e\u{0301}", "क्\u{200D}"] {
            assert!(!is_emoji(g), "{g}");
            assert_eq!(cluster_em(g), g.chars().map(char_em).sum::<f64>(), "{g}");
        }
        assert!(!is_emoji(""));
        assert_eq!(text_em("e\u{0301}", false), char_em('e'));
        assert_eq!(cluster_em("😀"), char_em('😀'));
    }

    #[test]
    fn text_adds_its_characters_up() {
        assert_eq!(text_em("", false), 0.0);
        assert!((text_em("1.5", false) - 1.58).abs() < 1e-9);
        assert!((text_em("1.5", true) - 1.58 * BOLD_FACTOR).abs() < 1e-9);
    }
}
