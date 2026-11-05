//! Number formats read off a number's display. The IR carries the display text
//! and the value, not a format pattern, so a typed number's format is the one
//! that shows the value as the table does: `0.0` for `76.0`, `#,##0` for
//! `1,250`, `"$"#,##0.00` for `$1,250.00`, `0.0%` for `45.1%`, `0.00,,"M"` for
//! `1.23M`.
//!
//! Excel shows a number's decimal mark and thousands separators as its reader's
//! locale has them, so a display with other marks (`1.234,50`, `1 234,5`) gets
//! the same format as `1,234.50` and `1,234.5`: the cell is a number, shown in
//! the reader's marks.

/// Characters a display may separate thousands with (`.` and `,` also mark
/// decimals).
const GROUP_MARKS: [char; 8] = [
    ',', '.', ' ', '\u{a0}', '\u{202f}', '\u{2009}', '\'', '\u{2019}',
];

/// A number format that shows a value as the table displays it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NumFmt {
    decimals: u8,
    /// Thousands separators.
    grouping: bool,
    /// How many thousands the value is shown in (a trailing `,` each): 2 for
    /// `1.23M`.
    thousands: u8,
    /// The text before and after the number when it's positive: a currency, a
    /// unit, a `%` (which Excel scales by).
    before: String,
    after: String,
    /// The text before and after a negative number, when Excel's own minus
    /// (a `-` before everything) doesn't show it: `−` (U+2212 MINUS SIGN, as gt
    /// writes negatives), a minus after a currency, parentheses.
    negative: Option<(String, String)>,
    /// Room after the number (`_0` a digit's, `_.` a period's), aligning it on
    /// a character: `pad_code` in `render.rs`.
    pub(crate) pad: String,
}

/// One way to read a display's digits and marks: the digits (the integer's
/// and the fraction's, together) and how many are decimals.
struct Reading {
    digits: String,
    decimals: usize,
    grouping: bool,
}

impl NumFmt {
    /// The format that shows `value` as `shown`, if `shown` is a number that
    /// rounds from `value`: an optional sign (`-`, `−` or `+`) or parentheses
    /// around a negative, then digits with optional thousands separators and
    /// decimals, with text before and after it that names what the number is
    /// (letters, a currency symbol, `%`, spaces). Anything else has none: such
    /// a cell keeps its text (`<0.001`, `46 (37, 60)`, `1.2e5`).
    pub(crate) fn infer(shown: &str, value: f64) -> Option<Self> {
        let (parens, inner) = match shown.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            Some(inner) => (true, inner),
            None => (false, shown),
        };
        let first = inner.find(|c: char| c.is_ascii_digit())?;
        let last = inner.rfind(|c: char| c.is_ascii_digit())?;
        let (before, core, after) = (&inner[..first], &inner[first..=last], &inner[last + 1..]);

        let is_sign = |c: char| matches!(c, '-' | '\u{2212}' | '+');
        let signs: Vec<(usize, char)> =
            before.char_indices().filter(|&(_, c)| is_sign(c)).collect();
        if signs.len() > 1 || (parens && !signs.is_empty()) {
            return None;
        }
        let unsigned: String = before.chars().filter(|&c| !is_sign(c)).collect();
        if !affix(&unsigned, false) || !affix(after, true) {
            return None;
        }
        let negative = parens || matches!(signs.first(), Some((_, '-' | '\u{2212}')));
        if negative != (value < 0.0) {
            return None;
        }
        // Excel puts its own `-` before everything else; any other negative
        // gets a section of its own.
        let own_minus = matches!(signs.first(), Some((0, '-')));
        let negative = (negative && !own_minus).then(|| {
            if parens {
                (format!("({before}"), format!("{after})"))
            } else {
                (before.to_owned(), after.to_owned())
            }
        });
        let before = if negative.is_some() || own_minus {
            unsigned
        } else {
            before.to_owned()
        };

        // The value scaled as shown: by 100 for a percentage, in thousands,
        // millions, … for `K`, `M`, … (or those are units: `1.5K` kelvins).
        let mut scales = vec![(1.0, 0u8)];
        if after.contains('%') {
            scales = vec![(100.0, 0)];
        } else if let Some(k) = ["K", "M", "B", "T"].iter().position(|&s| s == after) {
            let k = k as u8 + 1;
            scales.insert(0, (10f64.powi(-3 * i32::from(k)), k));
        }
        for reading in readings(core) {
            if reading.decimals > 30 {
                continue;
            }
            let Ok(shown_value) =
                format!("{}e-{}", reading.digits, reading.decimals).parse::<f64>()
            else {
                continue;
            };
            // The display is the value rounded to its decimals (with slack for
            // binary floating point).
            let half = 0.5 * 10f64.powi(-(reading.decimals as i32));
            for &(factor, thousands) in &scales {
                let v = value.abs() * factor;
                if (v - shown_value).abs() <= half * (1.0 + 1e-9) + v * 1e-15 {
                    return Some(Self {
                        decimals: reading.decimals as u8,
                        grouping: reading.grouping,
                        thousands,
                        before,
                        after: after.to_owned(),
                        negative,
                        pad: String::new(),
                    });
                }
            }
        }
        None
    }

    /// The built-in `numFmtId` with this pattern, if there is one.
    pub(crate) fn builtin(&self) -> Option<u8> {
        if !self.pad.is_empty() || !self.before.is_empty() || self.negative.is_some() {
            return None;
        }
        if self.thousands > 0 {
            return None;
        }
        match (self.decimals, self.grouping, self.after.as_str()) {
            (0, false, "") => Some(1),
            (2, false, "") => Some(2),
            (0, true, "") => Some(3),
            (2, true, "") => Some(4),
            (0, false, "%") => Some(9),
            (2, false, "%") => Some(10),
            _ => None,
        }
    }

    /// The format code: a second section shows a negative as the table does.
    pub(crate) fn code(&self) -> String {
        let mut number = String::from(if self.grouping { "#,##0" } else { "0" });
        if self.decimals > 0 {
            number.push('.');
            number.push_str(&"0".repeat(self.decimals.into()));
        }
        number.push_str(&",".repeat(self.thousands.into()));
        let section = |before: &str, after: &str| {
            format!("{}{number}{}{}", literal(before), literal(after), self.pad)
        };
        let positive = section(&self.before, &self.after);
        match &self.negative {
            Some((before, after)) => format!("{positive};{}", section(before, after)),
            None => positive,
        }
    }
}

/// The ways to read `core` (from a display's first digit to its last) as a
/// number: with no marks, with one mark as either the decimal mark or the
/// thousands separator, or with two, the last the decimal mark.
fn readings(core: &str) -> Vec<Reading> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mut marks: Vec<char> = Vec::new();
    for c in core.chars().filter(|c| !c.is_ascii_digit()) {
        if !GROUP_MARKS.contains(&c) {
            return Vec::new();
        }
        if !marks.contains(&c) {
            marks.push(c);
        }
    }
    let decimal = |mark: char, grouping: Option<char>| -> Option<Reading> {
        let (int, frac) = core.rsplit_once(mark)?;
        if !matches!(mark, '.' | ',') || !digits(frac) {
            return None;
        }
        let int = match grouping {
            Some(g) => grouped(int, g)?,
            None => digits(int).then(|| int.to_owned())?,
        };
        plain_int(&int).then(|| Reading {
            digits: format!("{int}{frac}"),
            decimals: frac.len(),
            grouping: grouping.is_some(),
        })
    };
    match marks[..] {
        [] => plain_int(core)
            .then(|| Reading {
                digits: core.to_owned(),
                decimals: 0,
                grouping: false,
            })
            .into_iter()
            .collect(),
        [mark] => {
            let as_decimal = decimal(mark, None);
            let as_group = grouped(core, mark)
                .filter(|int| plain_int(int))
                .map(|int| Reading {
                    digits: int,
                    decimals: 0,
                    grouping: true,
                });
            as_decimal.into_iter().chain(as_group).collect()
        }
        [a, b] => {
            // The decimal mark is the last mark, and comes once.
            let last = core.chars().rev().find(|c| !c.is_ascii_digit());
            let (mark, group) = if last == Some(b) { (b, a) } else { (a, b) };
            decimal(mark, Some(group)).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

/// The digits of an integer written with `mark` between groups of three (or,
/// as India writes them, of two then a last three), if it is.
fn grouped(int: &str, mark: char) -> Option<String> {
    let groups: Vec<&str> = int.split(mark).collect();
    let (first, rest) = groups.split_first()?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if rest.is_empty() || !groups.iter().all(|g| digits(g)) || first.len() > 3 || *first == "0" {
        return None;
    }
    let threes = rest.iter().all(|g| g.len() == 3);
    let indian = first.len() <= 2
        && rest.last().is_some_and(|g| g.len() == 3)
        && rest[..rest.len() - 1].iter().all(|g| g.len() == 2);
    (threes || indian).then(|| groups.concat())
}

/// An integer part a format shows: no leading zeros (`007`).
fn plain_int(int: &str) -> bool {
    !(int.len() > 1 && int.starts_with('0'))
}

/// Text that can stand beside a number in its display: letters (a unit, a
/// currency code), a currency symbol, spaces, `'`, `’`, `°`, a `.` beside
/// letters (`kr.`), and after the number one `%`. Not `<`, `~`, `±`, `/`,
/// brackets or other digits: those say something else about the number.
fn affix(s: &str, after: bool) -> bool {
    let letters = s.chars().any(char::is_alphabetic);
    s.chars().all(|c| {
        c.is_alphabetic()
            || c.is_whitespace()
            || currency(c)
            || matches!(c, '\'' | '\u{2019}' | '\u{b0}')
            || (c == '.' && letters)
            || (c == '%' && after)
    }) && s.matches('%').count() <= 1
}

/// A currency symbol (Unicode's category Sc).
fn currency(c: char) -> bool {
    matches!(c,
        '$' | '\u{a2}'..='\u{a5}' | '\u{58f}' | '\u{60b}' | '\u{7fe}' | '\u{7ff}'
        | '\u{9f2}' | '\u{9f3}' | '\u{9fb}' | '\u{af1}' | '\u{bf9}' | '\u{e3f}'
        | '\u{17db}' | '\u{20a0}'..='\u{20c0}' | '\u{a838}' | '\u{fdfc}' | '\u{fe69}'
        | '\u{ff04}' | '\u{ffe0}' | '\u{ffe1}' | '\u{ffe5}' | '\u{ffe6}'
        | '\u{11fdd}'..='\u{11fe0}' | '\u{1e2ff}' | '\u{1ecb0}')
}

/// Text shown as is in a format code: quoted, or (with a `"` in it) each
/// character escaped. A `%` stays a percent sign, which scales the value.
fn literal(s: &str) -> String {
    s.split('%')
        .map(|part| {
            if part.is_empty() {
                String::new()
            } else if part.contains('"') {
                part.chars().flat_map(|c| ['\\', c]).collect()
            } else {
                format!("\"{part}\"")
            }
        })
        .collect::<Vec<_>>()
        .join("%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(shown: &str, value: f64) -> Option<String> {
        NumFmt::infer(shown, value).map(|f| f.code())
    }

    #[test]
    fn a_mark_is_the_decimal_mark_or_a_separator_as_the_value_says() {
        assert_eq!(code("1,234", 1234.0).as_deref(), Some("#,##0"));
        assert_eq!(code("1,234", 1.234).as_deref(), Some("0.000"));
        assert_eq!(code("1.234", 1234.0).as_deref(), Some("#,##0"));
        assert_eq!(code("1.234", 1.234).as_deref(), Some("0.000"));
        assert_eq!(code("1.234,50", 1234.5).as_deref(), Some("#,##0.00"));
        assert_eq!(code("1\u{202f}234,5", 1234.5).as_deref(), Some("#,##0.0"));
        assert_eq!(code("1\u{2019}234.50", 1234.5).as_deref(), Some("#,##0.00"));
        assert_eq!(code("12,34,567", 1234567.0).as_deref(), Some("#,##0"));
        // A separator between groups of three, and only before the decimals.
        assert_eq!(code("1,23", 123.0), None);
        assert_eq!(code("1.234.5", 1234.5), None);
        assert_eq!(code("1.234,567,8", 1234.5678), None);
        assert_eq!(code("0,123", 123.0), None);
        assert_eq!(code("1;234", 1234.0), None);
    }

    #[test]
    fn grouped_readings_have_no_leading_zero() {
        assert!(grouped("1,234", ',').is_some());
        assert!(!plain_int("01234") && plain_int("0") && plain_int("10"));
        assert_eq!(grouped("12,34,567", ','), Some("1234567".into()));
        assert_eq!(grouped("1,23,45,678", ','), Some("12345678".into()));
        assert_eq!(grouped("1,234,56,789", ','), None);
        assert_eq!(grouped("123,45,678", ','), None);
        assert_eq!(grouped("1234", ','), None);
    }

    #[test]
    fn literals_are_quoted_and_a_percent_scales() {
        assert_eq!(literal(""), "");
        assert_eq!(literal("$"), "\"$\"");
        assert_eq!(literal(" %"), "\" \"%");
        assert_eq!(literal("%"), "%");
        assert_eq!(literal("in\""), "\\i\\n\\\"");
    }
}
