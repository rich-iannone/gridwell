//! Dates, times and booleans keep their type, as numbers do (`numbers.rs`): a
//! date or time is a serial (days since 1899-12-30) whose number format shows
//! it as the table does, and a boolean is a boolean cell. A display no format
//! reproduces stays text.

mod support;

use gridwell_testkit::{cell, CellExt};
use serde_json::{json, Value};
use support::{attr, cell_xml, format_code, one_row, render};

/// The cell `A1` of a one-cell table showing `shown`, typed `kind` `value`.
fn a1(shown: &str, kind: &str, value: Value) -> (String, String) {
    let (sheet, styles, _) = render(&one_row(vec![cell(shown).typed(kind, value)]));
    let c = cell_xml(&sheet, "A1").to_owned();
    (c, format_code(&sheet, &styles, "A1"))
}

/// The number a cell holds.
fn number(c: &str) -> f64 {
    assert_eq!(attr(c, "t"), None, "not a number: {c}");
    let v = &c[c.find("<v>").unwrap_or_else(|| panic!("no value: {c}")) + 3..];
    v[..v.find("</v>").unwrap()].parse().unwrap()
}

fn assert_text(shown: &str, c: &str) {
    assert!(
        attr(c, "t") == Some("inlineStr") && c.contains(&format!(">{shown}</t>")),
        "{shown}: not text: {c}"
    );
}

#[test]
fn dates_get_the_format_of_their_display() {
    // Serials and weekdays from Python's `datetime`.
    let cases = [
        ("2024-03-15", "2024-03-15", "yyyy\\-mm\\-dd", 45366.0),
        ("03/15/2024", "2024-03-15", "mm\\/dd\\/yyyy", 45366.0),
        ("3/15/2024", "2024-03-15", "m\\/dd\\/yyyy", 45366.0),
        ("15.03.2024", "2024-03-15", "dd\\.mm\\.yyyy", 45366.0),
        ("15/03/24", "2024-03-15", "dd\\/mm\\/yy", 45366.0),
        ("5 Mar 2024", "2024-03-05", "[$-409]d mmm yyyy", 45356.0),
        (
            "Mar 15, 2024",
            "2024-03-15",
            "[$-409]mmm dd\\, yyyy",
            45366.0,
        ),
        (
            "15 March 2024",
            "2024-03-15",
            "[$-409]dd mmmm yyyy",
            45366.0,
        ),
        ("March 2024", "2024-03-15", "[$-409]mmmm yyyy", 45366.0),
        (
            "Friday, March 15, 2024",
            "2024-03-15",
            "[$-409]dddd\\, mmmm dd\\, yyyy",
            45366.0,
        ),
        ("Fri 15", "2024-03-15", "[$-409]ddd dd", 45366.0),
        // Other words are literal text.
        (
            "15th of March",
            "2024-03-15",
            "[$-409]dd\"th\" \"of\" mmmm",
            45366.0,
        ),
        // The first day Excel counts right.
        ("1900-03-01", "1900-03-01", "yyyy\\-mm\\-dd", 61.0),
    ];
    for (shown, value, code, serial) in cases {
        let (c, got) = a1(shown, "date", json!(value));
        assert_eq!(number(&c), serial, "{shown}: {c}");
        assert_eq!(got, code, "{shown}");
    }
}

#[test]
fn datetimes_and_times_get_formats_too() {
    let cases = [
        (
            "2024-03-15 14:30",
            "datetime",
            "2024-03-15T14:30:00",
            "yyyy\\-mm\\-dd hh\\:mm",
            45366.604166666664,
        ),
        // The day is 14 as well: the run before the minutes is the hour.
        (
            "2024-03-14 14:30",
            "datetime",
            "2024-03-14 14:30:00",
            "yyyy\\-mm\\-dd hh\\:mm",
            45365.604166666664,
        ),
        // Months and minutes alike: each `mm` is read as the one it is.
        (
            "03/15 03:03",
            "datetime",
            "2024-03-15T03:03:00",
            "mm\\/dd hh\\:mm",
            45366.12708333333,
        ),
        // The hour is the month as well: a month next to minutes would be
        // read as minutes.
        (
            "11:58:10",
            "datetime",
            "2024-11-15T11:58:10",
            "hh\\:mm\\:ss",
            45611.49872685185,
        ),
        (
            "Jun 12, 2047 6:07:31 PM",
            "datetime",
            "2047-06-12T18:07:31",
            "[$-409]mmm dd\\, yyyy h\\:mm\\:ss AM/PM",
            53855.755219907405,
        ),
        (
            "3/15/2024 2:30 PM",
            "datetime",
            "2024-03-15T14:30:00",
            "[$-409]m\\/dd\\/yyyy h\\:mm AM/PM",
            45366.604166666664,
        ),
        (
            "2024-03-15T14:30:05.250",
            "datetime",
            "2024-03-15T14:30:05.25",
            "yyyy\\-mm\\-dd\"T\"hh\\:mm\\:ss.000",
            45366.60422743056,
        ),
        // The time isn't shown: the cell still holds it.
        (
            "2024-03-15",
            "datetime",
            "2024-03-15T14:30:00",
            "yyyy\\-mm\\-dd",
            45366.604166666664,
        ),
        // A date alone is its midnight.
        (
            "2024-03-15",
            "datetime",
            "2024-03-15",
            "yyyy\\-mm\\-dd",
            45366.0,
        ),
        ("14:30", "time", "14:30:00", "hh\\:mm", 0.6041666666666666),
        ("14:30", "time", "14:30", "hh\\:mm", 0.6041666666666666),
        (
            "2:30:05 PM",
            "time",
            "14:30:05",
            "[$-409]h\\:mm\\:ss AM/PM",
            0.604224537037037,
        ),
        ("12:00 AM", "time", "00:00:00", "[$-409]hh\\:mm AM/PM", 0.0),
        // Minutes before seconds, with no hour.
        ("30:05", "time", "00:30:05", "mm\\:ss", 0.020891203703703703),
    ];
    for (shown, kind, value, code, serial) in cases {
        let (c, got) = a1(shown, kind, json!(value));
        assert!(
            (number(&c) - serial).abs() < 1e-9,
            "{shown}: {c} is not {serial}"
        );
        assert_eq!(got, code, "{shown}");
    }
}

#[test]
fn a_display_no_date_format_reproduces_stays_text() {
    for (shown, kind, value) in [
        // Value and display disagree.
        ("2024-03-16", "date", json!("2024-03-15")),
        ("Thursday, March 15, 2024", "date", json!("2024-03-15")),
        ("2:30 AM", "time", json!("14:30:00")),
        ("14:31", "datetime", json!("2024-03-15T14:30:00")),
        // A name written another way, which no code shows.
        ("MARCH 15, 2024", "date", json!("2024-03-15")),
        ("mar 15", "date", json!("2024-03-15")),
        ("2:30 pm", "time", json!("14:30:00")),
        // Digits that are no field of the value, or not ASCII digits.
        (
            "\u{0662}\u{0660}\u{0662}\u{0664}-03-15",
            "date",
            json!("2024-03-15"),
        ),
        ("Q1 2024", "date", json!("2024-03-15")),
        ("2024-03-15 (week 11)", "date", json!("2024-03-15")),
        // A date has no time, a time no date.
        ("2024-03-15 00:00", "date", json!("2024-03-15")),
        ("2024-03-15 14:30", "time", json!("14:30:00")),
        // Seconds' fractions must be shown, exactly.
        ("14:30:05", "time", json!("14:30:05.5")),
        ("14:30:05.4", "time", json!("14:30:05.5")),
        ("14:30:05.1235", "time", json!("14:30:05.1235")),
        // Minutes where Excel would read a month (before an hour).
        ("03 14", "datetime", json!("2024-05-15T14:03:00")),
        // Not a value Excel can hold as given.
        ("1900-02-28", "date", json!("1900-02-28")),
        ("2024-02-30", "date", json!("2024-02-30")),
        ("2024-3-15", "date", json!("2024-3-15")),
        (
            "2024-03-15 14:30",
            "datetime",
            json!("2024-03-15T14:30:00Z"),
        ),
        (
            "2024-03-15 14:30",
            "datetime",
            json!("2024-03-15T14:30:00+01:00"),
        ),
        ("24:00", "time", json!("24:00:00")),
        ("2024-03-15", "date", json!(45366)),
        // No field at all.
        ("today", "date", json!("2024-03-15")),
    ] {
        let (c, _) = a1(shown, kind, value);
        assert_text(shown, &c);
    }
}

#[test]
fn booleans_are_boolean_cells() {
    for (shown, value, v) in [
        ("TRUE", true, "1"),
        ("FALSE", false, "0"),
        // Excel shows its own TRUE and FALSE, as it does for typed-in `true`.
        ("True", true, "1"),
        ("false", false, "0"),
    ] {
        let (c, code) = a1(shown, "boolean", json!(value));
        assert_eq!(attr(&c, "t"), Some("b"), "{shown}: {c}");
        assert!(c.contains(&format!("<v>{v}</v>")), "{shown}: {c}");
        assert_eq!(code, "General");
    }
}

#[test]
fn a_boolean_shown_otherwise_stays_text() {
    for (shown, value) in [
        ("Yes", json!(true)),
        ("\u{2713}", json!(true)),
        ("TRUE", json!(false)),
        ("1", json!(true)),
        ("TRUE", json!("TRUE")),
    ] {
        let (c, _) = a1(shown, "boolean", value);
        assert_text(shown, &c);
    }
}

#[test]
fn cells_sharing_a_date_format_share_a_style() {
    let (sheet, styles, _) = render(&one_row(vec![
        cell("2024-03-15").typed("date", json!("2024-03-15")),
        cell("2024-04-01").typed("date", json!("2024-04-01")),
        cell("Apr 1, 2024").typed("date", json!("2024-04-01")),
        cell("0.0").typed("number", json!(0)),
    ]));
    let s = |r| attr(cell_xml(&sheet, r), "s").map(str::to_owned);
    assert_eq!(s("A1"), s("B1"));
    assert_ne!(s("A1"), s("C1"));
    // Two date formats and a number format, each declared once.
    assert_eq!(styles.matches("<numFmt ").count(), 3, "{styles}");
    assert!(
        styles.contains("formatCode=\"[$-409]mmm d\\, yyyy\""),
        "{styles}"
    );
}
