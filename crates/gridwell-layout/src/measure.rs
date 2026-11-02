//! Estimated text widths, without a font's metrics: what a format laid out
//! elsewhere (a slide's table, whose columns don't fit their content on their own)
//! sizes its columns for, and what SVG layout uses for the characters its fonts
//! don't cover.
//!
//! The estimates are DejaVu Sans' advances, rounded up: wider than most fonts a
//! viewer substitutes, so text sized for them fits.

use unicode_width::UnicodeWidthChar;

/// Bold text is wider; DejaVu Sans Bold runs about 8–11% wider than regular.
pub const BOLD_FACTOR: f64 = 1.12;

/// Estimated advance of `c`, in em (DejaVu-like widths, rounded up).
pub fn char_em(c: char) -> f64 {
    if c.is_ascii() {
        return ascii_em(c);
    }
    match c.width() {
        // Combining marks, zero-width joiners, variation selectors, controls.
        Some(0) | None => 0.0,
        Some(2) if is_emoji(c) => 1.25,
        // East Asian wide / fullwidth characters are one em in CJK fonts.
        Some(2) => 1.04,
        // Accented Latin, Greek, Cyrillic, Arabic, Hebrew, …: wide Latin bound.
        _ => 0.70,
    }
}

/// The estimated width of `s`, in em: its characters' [`char_em`], wider by
/// [`BOLD_FACTOR`] when bold.
pub fn text_em(s: &str, bold: bool) -> f64 {
    let em: f64 = s.chars().map(char_em).sum();
    if bold {
        em * BOLD_FACTOR
    } else {
        em
    }
}

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF)
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
    fn text_adds_its_characters_up() {
        assert_eq!(text_em("", false), 0.0);
        assert!((text_em("1.5", false) - 1.58).abs() < 1e-9);
        assert!((text_em("1.5", true) - 1.58 * BOLD_FACTOR).abs() < 1e-9);
    }
}
