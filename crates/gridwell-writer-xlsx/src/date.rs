//! Dates and times of day as spreadsheet serials (days since 1899-12-30, the
//! time of day a fraction of one), shown through a number format read off the
//! display: `yyyy\-mm\-dd` for `2024-03-15`, `[$-409]mmm d\, yyyy` for
//! `Mar 15, 2024`. The IR carries the display text and an ISO 8601 value, not a
//! pattern, so the format is worked out from the two. A display no format
//! reproduces has none: such a cell keeps its text.

use gridwell_ir::ValueType;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// A typed date, datetime or time of day.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Moment {
    /// `None` for a time of day.
    date: Option<Date>,
    /// `None` for a date.
    time: Option<Time>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Date {
    year: i32,
    month: u32,
    day: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Time {
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
}

/// `s` as a number, if it is exactly `len` ASCII digits.
fn digits(s: &str, len: usize) -> Option<u32> {
    if s.len() == len && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `YYYY-MM-DD`.
fn parse_date(s: &str) -> Option<Date> {
    let mut parts = s.split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let date = Date {
        year: digits(y, 4)? as i32,
        month: digits(m, 2)?,
        day: digits(d, 2)?,
    };
    ((1..=12).contains(&date.month)
        && (1..=days_in_month(date.year, date.month)).contains(&date.day))
    .then_some(date)
}

/// `HH:MM`, `HH:MM:SS` or `HH:MM:SS.fff` (up to nine digits); no offset.
fn parse_time(s: &str) -> Option<Time> {
    let (hms, frac) = match s.split_once('.') {
        Some((hms, f)) => (hms, Some(f)),
        None => (s, None),
    };
    let mut parts = hms.split(':');
    let hour = digits(parts.next()?, 2)?;
    let minute = digits(parts.next()?, 2)?;
    let second = parts.next().map(|p| digits(p, 2));
    if parts.next().is_some() || (frac.is_some() && second.is_none()) {
        return None;
    }
    let nanos = match frac {
        Some(f) if (1..=9).contains(&f.len()) => {
            digits(f, f.len())? * 10u32.pow(9 - f.len() as u32)
        }
        Some(_) => return None,
        None => 0,
    };
    let second = second.unwrap_or(Some(0))?;
    (hour < 24 && minute < 60 && second < 60).then_some(Time {
        hour,
        minute,
        second,
        nanos,
    })
}

/// Days from 1970-01-01 to a proleptic Gregorian date.
fn days_from_civil(Date { year, month, day }: Date) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let (m, d) = (i64::from(month), i64::from(day));
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// A numeric field of a date or time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

/// One piece of a format code.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    /// A numeric field and its code (`yyyy`, `m`, `ss`, …).
    Field(Field, &'static str),
    /// Fractional seconds: this many digits after the seconds' `.`.
    Fraction(usize),
    /// A month or weekday name, or AM/PM.
    Name(&'static str),
    /// Text shown as is.
    Literal(String),
}

/// A display split into runs of ASCII digits, runs of letters, and single
/// other characters.
fn tokenize(shown: &str) -> Vec<&str> {
    let class = |c: char| {
        if c.is_ascii_digit() {
            0
        } else if c.is_alphabetic() {
            1
        } else {
            2
        }
    };
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut chars = shown.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|&(_, n)| n);
        if class(c) == 2 || next.is_none_or(|n| class(n) != class(c)) {
            let end = i + c.len_utf8();
            tokens.push(&shown[start..end]);
            start = end;
        }
    }
    tokens
}

impl Moment {
    /// The typed value `value` of type `kind`: a date (`2024-03-15`), a
    /// datetime (`2024-03-15T14:30:00`, or with a space; a date alone is its
    /// midnight) or a time of day (`14:30:00`). Anything else (another type, an
    /// offset, a date Excel can't show) is `None`.
    pub(crate) fn parse(kind: &ValueType, value: &str) -> Option<Self> {
        let moment = match kind {
            ValueType::Date => Self {
                date: Some(parse_date(value)?),
                time: None,
            },
            ValueType::Datetime => {
                let (date, time) = match value.split_once(['T', ' ']) {
                    Some((d, t)) => (d, Some(parse_time(t)?)),
                    None => (value, None),
                };
                Self {
                    date: Some(parse_date(date)?),
                    time: Some(time.unwrap_or_default()),
                }
            }
            ValueType::Time => Self {
                date: None,
                time: Some(parse_time(value)?),
            },
            _ => return None,
        };
        // Excel counts a 1900-02-29 that never was: its serials, and weekdays,
        // are wrong before 1900-03-01.
        let shown = moment.date.is_none_or(|d| {
            d >= Date {
                year: 1900,
                month: 3,
                day: 1,
            }
        });
        shown.then_some(moment)
    }

    /// The spreadsheet serial: days since 1899-12-30 (a time of day alone has
    /// none), plus the fraction of the day.
    pub(crate) fn serial(&self) -> f64 {
        let epoch = Date {
            year: 1899,
            month: 12,
            day: 30,
        };
        let days = self
            .date
            .map_or(0, |d| days_from_civil(d) - days_from_civil(epoch));
        let seconds = self.time.map_or(0.0, |t| {
            f64::from(t.hour * 3600 + t.minute * 60 + t.second) + f64::from(t.nanos) / 1e9
        });
        days as f64 + seconds / 86_400.0
    }

    /// The format code that shows this moment as `shown`, if there is one.
    /// Each run of digits in `shown` must be one of the moment's fields (the
    /// year as `yyyy` or `yy`, the others padded or not), and each word that
    /// is a month or weekday name, or AM/PM, must be the moment's (in English:
    /// such a code pins `[$-409]`, en-US, so the names don't follow the
    /// reader's locale); other text is literal. A value with fractions of a
    /// second must show them.
    pub(crate) fn format(&self, shown: &str) -> Option<String> {
        let tokens = tokenize(shown);
        let twelve = tokens.iter().any(|t| matches!(*t, "AM" | "PM"));
        let mut parts = Vec::new();
        if !self.search(&tokens, twelve, &mut parts) {
            return None;
        }
        let names = parts.iter().any(|p| matches!(p, Part::Name(_)));
        let mut code = String::from(if names { "[$-409]" } else { "" });
        for (i, part) in parts.iter().enumerate() {
            match part {
                Part::Field(_, c) | Part::Name(c) => code.push_str(c),
                Part::Fraction(n) => code.push_str(&"0".repeat(*n)),
                // The decimal point of the seconds.
                Part::Literal(s) if matches!(parts.get(i + 1), Some(Part::Fraction(_))) => {
                    code.push_str(s)
                }
                Part::Literal(s) if s.chars().all(char::is_alphabetic) => {
                    code.push('"');
                    code.push_str(s);
                    code.push('"');
                }
                // Escaped: Excel would put the reader's own separators in
                // place of a bare `/` or `:`.
                Part::Literal(s) => {
                    for c in s.chars() {
                        if c != ' ' {
                            code.push('\\');
                        }
                        code.push(c);
                    }
                }
            }
        }
        Some(code)
    }

    /// Assign `tokens` to parts, trying each candidate field of a run of
    /// digits in turn until the whole format is one Excel reads as meant.
    fn search(&self, tokens: &[&str], twelve: bool, parts: &mut Vec<Part>) -> bool {
        let Some((&token, rest)) = tokens.split_first() else {
            return self.valid(parts);
        };
        let candidates = if token.bytes().all(|b| b.is_ascii_digit()) {
            self.fields(token, twelve, parts)
        } else if token.chars().all(char::is_alphabetic) {
            match self.word(token) {
                Some(part) => vec![part],
                None => return false,
            }
        } else if token.chars().any(char::is_numeric) {
            // A digit no field can show.
            return false;
        } else {
            vec![Part::Literal(token.to_owned())]
        };
        for part in candidates {
            parts.push(part);
            if self.search(rest, twelve, parts) {
                return true;
            }
            parts.pop();
        }
        false
    }

    /// The fields that show the digits `run`, in order of preference, after
    /// `parts`. Each field is used once.
    fn fields(&self, run: &str, twelve: bool, parts: &[Part]) -> Vec<Part> {
        let mut out = Vec::new();
        // Digits after `ss.` are its fraction.
        if let (Some(t), [.., Part::Field(Field::Second, _), Part::Literal(dot)]) =
            (self.time, parts)
        {
            if dot == "." && run.len() <= 3 {
                let shown = digits(run, run.len()).unwrap_or(u32::MAX);
                if shown.checked_mul(10u32.pow(9 - run.len() as u32)) == Some(t.nanos) {
                    out.push(Part::Fraction(run.len()));
                }
            }
        }
        let used = |f: Field| {
            parts
                .iter()
                .any(|p| matches!(p, Part::Field(g, _) if *g == f))
        };
        let numbered = |v: u32, short: &'static str, long: &'static str| {
            if run.len() == 2 && run == format!("{v:02}") {
                Some(long)
            } else if run.len() == 1 && run == v.to_string() {
                Some(short)
            } else {
                None
            }
        };
        let mut push = |f: Field, code: Option<&'static str>| {
            if let Some(code) = code.filter(|_| !used(f)) {
                out.push(Part::Field(f, code));
            }
        };
        if let Some(d) = self.date {
            let year = if run.len() == 4 && run == format!("{:04}", d.year) {
                Some("yyyy")
            } else if run.len() == 2 && run == format!("{:02}", d.year % 100) {
                Some("yy")
            } else {
                None
            };
            push(Field::Year, year);
            push(Field::Month, numbered(d.month, "m", "mm"));
            push(Field::Day, numbered(d.day, "d", "dd"));
        }
        if let Some(t) = self.time {
            let hour = if twelve {
                (t.hour + 11) % 12 + 1
            } else {
                t.hour
            };
            push(Field::Hour, numbered(hour, "h", "hh"));
            push(Field::Minute, numbered(t.minute, "m", "mm"));
            push(Field::Second, numbered(t.second, "s", "ss"));
        }
        out
    }

    /// The part a word shows: a name or AM/PM, as this moment's; literal text
    /// unless it names some other month, weekday or half of the day, or is
    /// one written another way (`MARCH`, `pm`), which no code shows.
    fn word(&self, w: &str) -> Option<Part> {
        if let Some(d) = self.date {
            let month = MONTHS[d.month as usize - 1];
            let weekday = WEEKDAYS[(days_from_civil(d) + 4).rem_euclid(7) as usize];
            for (name, long, short) in [(month, "mmmm", "mmm"), (weekday, "dddd", "ddd")] {
                if w == name {
                    return Some(Part::Name(long));
                }
                if w == &name[..3] {
                    return Some(Part::Name(short));
                }
            }
        }
        if let Some(t) = self.time {
            if w == if t.hour < 12 { "AM" } else { "PM" } {
                return Some(Part::Name("AM/PM"));
            }
        }
        let named = MONTHS
            .iter()
            .chain(&WEEKDAYS)
            .flat_map(|n| [*n, &n[..3]])
            .chain(["AM", "PM"])
            .any(|n| n.eq_ignore_ascii_case(w));
        (!named).then(|| Part::Literal(w.to_owned()))
    }

    /// Whether Excel reads `parts` as meant: some field is shown, a fraction
    /// of a second is, and each `m` or `mm` is the month or minute it stands
    /// for. Excel reads one as minutes right after an hour or right before
    /// seconds (the codes on either side, past any literal text), and readers
    /// differ on one next to another minute or a second, so a month is next
    /// to no time field at all.
    fn valid(&self, parts: &[Part]) -> bool {
        let code = |p: &&Part| !matches!(p, Part::Literal(_));
        if !parts.iter().any(|p| code(&p)) {
            return false;
        }
        let fraction = parts.iter().any(|p| matches!(p, Part::Fraction(_)));
        if self.time.is_some_and(|t| t.nanos != 0) && !fraction {
            return false;
        }
        parts.iter().enumerate().all(|(i, p)| {
            let field = match p {
                Part::Field(f @ (Field::Month | Field::Minute), _) => *f,
                _ => return true,
            };
            let before = parts[..i].iter().rev().find(code);
            let after = parts[i + 1..].iter().find(code);
            if field == Field::Minute {
                return matches!(before, Some(Part::Field(Field::Hour, _)))
                    || matches!(after, Some(Part::Field(Field::Second, _)));
            }
            let time = |p: Option<&Part>| {
                matches!(
                    p,
                    Some(Part::Field(Field::Hour | Field::Minute | Field::Second, _))
                )
            };
            !time(before) && !time(after)
        })
    }
}
