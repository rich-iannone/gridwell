//! Text width estimation and line wrapping for SVG layout.
//!
//! SVG has no layout engine: every glyph position is decided here, without access to
//! the fonts the viewer will use. So the estimate is deliberately **conservative**:
//! per-character advances follow DejaVu Sans, the widest of the common sans-serif
//! fonts a viewer is likely to substitute for `Arial, sans-serif` (Arial, Helvetica,
//! Liberation Sans and Noto Sans are all narrower). Overestimating costs a little
//! whitespace; underestimating makes text overprint the next column.
//!
//! `tests/calibration.rs` checks the estimate against real rendering with
//! `rsvg-convert`.

use unicode_width::UnicodeWidthChar;

/// Bold text is wider; DejaVu Sans Bold runs about 8–11% wider than regular.
pub const BOLD_FACTOR: f64 = 1.12;

/// Advance width of one character, in em.
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

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF)
}

/// Advance widths (in em) for printable ASCII: DejaVu Sans, rounded up, raised where
/// calibration showed a wider glyph: Arial/Helvetica on macOS, and the pinned harness
/// image's fallback fonts (`*`, `/`, `\\`, `I`, `M`, `|`). See tests/calibration.rs.
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

/// Extra width per visible glyph, in px. At small sizes renderers hint glyph
/// advances to whole device pixels, which adds up to ~0.5px per glyph at 1× zoom on
/// top of the proportional width (a 24-character footnote at 11.9px ran ~8px wider
/// than its scaled advances). Calibrated by `tests/calibration.rs` at real sizes.
pub const HINTING_SLACK_PX: f64 = 0.5;

/// Estimated rendered width of `s` in px at `font_size` px.
pub fn text_width(s: &str, font_size: f64, bold: bool) -> f64 {
    let (em, glyphs) = s.chars().fold((0.0, 0usize), |(em, n), c| {
        let w = char_em(c);
        (em + w, n + usize::from(w > 0.0))
    });
    em * font_size * if bold { BOLD_FACTOR } else { 1.0 } + glyphs as f64 * HINTING_SLACK_PX
}

/// A run of text with uniform formatting, as measured and emitted by the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
}

/// Formatting of a [`Run`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    /// A validated `#rrggbb` fill, if any.
    pub color: Option<String>,
    /// Superscript (footnote marks): smaller and raised.
    pub sup: bool,
}

/// Superscripts are drawn at this fraction of the base size.
pub const SUP_SCALE: f64 = 0.7;

impl Run {
    pub fn width(&self, font_size: f64) -> f64 {
        let size = if self.style.sup {
            font_size * SUP_SCALE
        } else {
            font_size
        };
        text_width(&self.text, size, self.style.bold)
    }
}

/// One laid-out line: runs to draw left to right, and its estimated width.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    pub runs: Vec<Run>,
    pub width: f64,
}

impl Line {
    /// Concatenated text of all runs.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// Width of the widest line.
pub fn max_width(lines: &[Line]) -> f64 {
    lines.iter().map(|l| l.width).fold(0.0, f64::max)
}

/// Lay out a paragraph of runs into lines no wider than `max_width` (when possible).
///
/// Breaks at spaces; a word wider than `max_width` on its own is broken between
/// characters. Runs keep their formatting across breaks. With `max_width` = infinity
/// this measures the paragraph's natural (unwrapped) width.
pub fn wrap(runs: &[Run], font_size: f64, max_width: f64) -> Vec<Line> {
    // Column widths are sized to exactly a cell's natural width and recovered by
    // subtraction, so allow for floating-point noise when comparing against them.
    let max_width = max_width + 1e-6;
    // Split runs into words (pieces separated by spaces); a word may span several
    // runs, e.g. "pass" + superscript "1".
    let mut words: Vec<Vec<Run>> = vec![Vec::new()];
    for run in runs {
        for (i, part) in run.text.split(' ').enumerate() {
            if i > 0 {
                words.push(Vec::new());
            }
            if !part.is_empty() {
                words.last_mut().unwrap().push(Run {
                    text: part.to_string(),
                    style: run.style.clone(),
                });
            }
        }
    }
    words.retain(|w| !w.is_empty());

    let mut lines: Vec<Line> = Vec::new();
    let mut current = Line::default();

    let push_piece = |line: &mut Line, piece: Run| {
        line.width += piece.width(font_size);
        match line.runs.last_mut() {
            Some(last) if last.style == piece.style => last.text.push_str(&piece.text),
            _ => line.runs.push(piece),
        }
    };
    // The space before a word takes the previous run's style (minus superscript):
    // measure it that way, since a bold space is wider than a regular one.
    let space_after = |line: &Line| {
        line.runs.last().map(|last| Run {
            text: " ".into(),
            style: RunStyle {
                sup: false,
                ..last.style.clone()
            },
        })
    };

    for word in words {
        let word_w: f64 = word.iter().map(|r| r.width(font_size)).sum();
        let lead = space_after(&current).map_or(0.0, |s| s.width(font_size));

        if !current.runs.is_empty() && current.width + lead + word_w > max_width {
            lines.push(std::mem::take(&mut current));
        }

        if let Some(space) = space_after(&current) {
            push_piece(&mut current, space);
        }

        if word_w <= max_width || !current.runs.is_empty() && current.width + word_w <= max_width {
            for piece in word {
                push_piece(&mut current, piece);
            }
            continue;
        }

        // The word alone is too wide: break it between characters.
        for piece in word {
            for ch in piece.text.chars() {
                let ch_run = Run {
                    text: ch.to_string(),
                    style: piece.style.clone(),
                };
                let w = ch_run.width(font_size);
                if !current.runs.is_empty() && current.width + w > max_width {
                    lines.push(std::mem::take(&mut current));
                }
                push_piece(&mut current, ch_run);
            }
        }
    }

    if !current.runs.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> Vec<Run> {
        vec![Run {
            text: s.into(),
            style: RunStyle::default(),
        }]
    }

    #[test]
    fn widths_are_positive_and_scale_with_size() {
        let w14 = text_width("Hello", 14.0, false);
        assert!(w14 > 0.0);
        // Proportional part doubles with the size; the per-glyph slack does not.
        let slack = 5.0 * HINTING_SLACK_PX;
        assert!((text_width("Hello", 28.0, false) - slack - 2.0 * (w14 - slack)).abs() < 1e-9);
        assert_eq!(text_width("", 14.0, false), 0.0);
        // Zero-width characters get no slack.
        assert_eq!(text_width("\u{0301}\u{200D}", 14.0, false), 0.0);
        assert!(text_width("Hello", 14.0, true) > w14);
    }

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
    fn natural_width_is_one_line() {
        let lines = wrap(&plain("a b c"), 14.0, f64::INFINITY);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text(), "a b c");
        assert!((lines[0].width - text_width("a b c", 14.0, false)).abs() < 1e-9);
    }

    #[test]
    fn wraps_at_spaces_within_the_limit() {
        let text = "clamped between 80 and 200px";
        let limit = text_width("clamped between", 14.0, false) + 0.1;
        let lines = wrap(&plain(text), 14.0, limit);
        assert_eq!(
            lines.iter().map(Line::text).collect::<Vec<_>>(),
            vec!["clamped between", "80 and 200px"]
        );
        assert!(lines.iter().all(|l| l.width <= limit));
    }

    #[test]
    fn breaks_overlong_words_between_characters() {
        let url = "https://example.com/very/long/path";
        let limit = 80.0;
        let lines = wrap(&plain(url), 14.0, limit);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.width <= limit + 1e-9), "{lines:?}");
        assert_eq!(lines.iter().map(Line::text).collect::<String>(), url);
    }

    #[test]
    fn a_word_spanning_runs_stays_together_and_keeps_styles() {
        let runs = vec![
            Run {
                text: "A ".into(),
                style: RunStyle::default(),
            },
            Run {
                text: "pass".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
            },
            Run {
                text: "1".into(),
                style: RunStyle {
                    sup: true,
                    ..Default::default()
                },
            },
        ];
        let lines = wrap(&runs, 14.0, f64::INFINITY);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text(), "A pass1");
        assert_eq!(lines[0].runs.len(), 3);
        assert!(lines[0].runs[2].style.sup);
        // Narrow limit: "pass¹" moves to line 2 as a unit.
        let narrow =
            text_width("pass", 14.0, true) + 1.0 + text_width("1", 14.0 * SUP_SCALE, false);
        let lines = wrap(&runs, 14.0, narrow);
        assert_eq!(
            lines.iter().map(Line::text).collect::<Vec<_>>(),
            vec!["A", "pass1"]
        );
    }

    #[test]
    fn wrapping_never_loses_characters() {
        let text = "The quick brown fox jumps over the lazy dog, twice: 1234567890!";
        for limit in [10.0, 40.0, 75.0, 120.0, 400.0] {
            let lines = wrap(&plain(text), 14.0, limit);
            let rejoined: String = lines.iter().map(Line::text).collect::<Vec<_>>().join(" ");
            assert_eq!(
                rejoined.replace(' ', ""),
                text.replace(' ', ""),
                "limit {limit}"
            );
        }
    }

    #[test]
    fn text_exactly_as_wide_as_the_limit_is_not_wrapped() {
        // Regression: a column sized to a cell's natural width must not wrap it, even
        // when the limit is recovered through float arithmetic (x + w - x != w).
        for word in ["mid", "Revenue", "1,250.3", "販売実績"] {
            let w = text_width(word, 14.0, false);
            let noisy = (100.1 + w) - 100.1;
            assert_eq!(wrap(&plain(word), 14.0, noisy).len(), 1, "{word}");
        }
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        let lines = wrap(&plain(""), 14.0, 100.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].width, 0.0);
    }
}
