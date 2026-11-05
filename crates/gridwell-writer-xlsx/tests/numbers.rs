//! Typed numbers keep their display: the cell holds the value, and its number
//! format shows it as the table does (`76.0`, not `76`). A display no number
//! format can reproduce stays text. Text goes in the shared-string table, where
//! spreadsheet apps (Numbers among them) honor rich text's run formatting
//! (footnote marks, styled runs); inline strings lose it there.

#[allow(dead_code)]
mod support;

use gridwell_testkit::{cell, cell_content, footnote_mark, row, text, CellExt, TableBuilder};
use serde_json::json;
use support::{attr, cell_xml, format_code, one_row, render, string_item, text_of};

#[test]
fn typed_numbers_get_the_format_of_their_display() {
    let cases = [
        ("76.0", json!(76), "0.0"),
        ("75.2", json!(75.2), "0.0"),
        // The display rounds the value: the cell keeps the precise value.
        ("75.2", json!(75.21), "0.0"),
        ("3.50", json!(3.5), "0.00"),
        ("53", json!(53), "0"),
        ("1,250.30", json!(1250.3), "#,##0.00"),
        ("1,250", json!(1250), "#,##0"),
        ("0.125", json!(0.125), "0.000"),
        ("-1.5", json!(-1.5), "0.0"),
        // gt writes negatives with U+2212 MINUS SIGN.
        ("\u{2212}1.5", json!(-1.5), "0.0;\"\u{2212}\"0.0"),
        ("\u{2212}1,234", json!(-1234), "#,##0;\"\u{2212}\"#,##0"),
        // Other marks (a locale's) are the reader's in Excel.
        ("1.234,50", json!(1234.5), "#,##0.00"),
        (
            "\u{2212}98.765,43",
            json!(-98765.43),
            "#,##0.00;\"\u{2212}\"#,##0.00",
        ),
        ("1\u{202f}234,5", json!(1234.5), "#,##0.0"),
        ("1\u{2019}234.50", json!(1234.5), "#,##0.00"),
        ("12,34,567", json!(1234567), "#,##0"),
        ("0,125", json!(0.125), "0.000"),
        ("1.234", json!(1234), "#,##0"),
        // A currency, a unit, a sign of its own, parentheses.
        ("$49.95", json!(49.95), "\"$\"0.00"),
        ("-$5.00", json!(-5), "\"$\"0.00"),
        ("\u{2212}$5.00", json!(-5), "\"$\"0.00;\"\u{2212}$\"0.00"),
        ("$-5.00", json!(-5), "\"$\"0.00;\"$-\"0.00"),
        ("($5.00)", json!(-5), "\"$\"0.00;\"($\"0.00\")\""),
        ("\u{20ac}49,95", json!(49.95), "\"\u{20ac}\"0.00"),
        (
            "49,95\u{a0}\u{20ac}",
            json!(49.95),
            "0.00\"\u{a0}\u{20ac}\"",
        ),
        ("CA$1,250", json!(1250), "\"CA$\"#,##0"),
        ("1,234.50 kg", json!(1234.5), "#,##0.00\" kg\""),
        ("5,00 kr.", json!(5), "0.00\" kr.\""),
        ("+1,234.50", json!(1234.5), "\"+\"#,##0.00"),
        // A percentage is the value scaled by 100.
        ("45.1%", json!(0.451), "0.0%"),
        ("\u{2212}1.2%", json!(-0.0123), "0.0%;\"\u{2212}\"0.0%"),
        ("45,1\u{a0}%", json!(0.451), "0.0\"\u{a0}\"%"),
        ("45.1% up", json!(0.451), "0.0%\" up\""),
        // Thousands, millions, … (or a unit, as the value says).
        ("1.23M", json!(1230000), "0.00,,\"M\""),
        ("12.3K", json!(12300), "0.0,\"K\""),
        ("1.5K", json!(1.5), "0.0\"K\""),
        ("2B", json!(2e9), "0,,,\"B\""),
    ];
    for (shown, value, want) in cases {
        let t = one_row(vec![cell(shown).typed("number", value.clone())]);
        let (sheet, styles, _) = render(&t);
        let c = cell_xml(&sheet, "A1");
        assert!(
            c.contains(&format!("<v>{}</v>", value.as_f64().unwrap())),
            "{shown}: not numeric: {c}"
        );
        assert_eq!(
            format_code(&sheet, &styles, "A1"),
            want,
            "{shown}\n{styles}"
        );
    }
}

#[test]
fn integers_get_formats_too() {
    let t = one_row(vec![cell("12,345").typed("integer", json!(12345))]);
    let (sheet, styles, _) = render(&t);
    assert!(cell_xml(&sheet, "A1").contains("<v>12345</v>"), "{sheet}");
    assert_eq!(format_code(&sheet, &styles, "A1"), "#,##0");
}

#[test]
fn a_display_no_format_reproduces_stays_text() {
    for (shown, value) in [
        // Value and display disagree.
        ("75.2", json!(99)),
        ("-1.5", json!(1.5)),
        // Not a plain number.
        ("<0.001", json!(0.001)),
        ("46 (37, 60)", json!(46)),
        ("1.2e5", json!(120000)),
        (".5", json!(0.5)),
        // Text that says more about the number, or other numbers.
        ("~5", json!(5)),
        ("\u{b1}5", json!(5)),
        ("5/8", json!(5)),
        ("1.23 \u{d7} 10^5", json!(123000)),
        ("1.23 \u{d7} 10\u{2075}", json!(123000)),
        ("451.2\u{2030}", json!(0.4512)),
        ("1.2 m2", json!(1.2)),
        ("[5]", json!(5)),
        ("5)", json!(-5)),
        ("(-5)", json!(-5)),
        ("--5", json!(-5)),
        ("5-", json!(-5)),
        ("5%%", json!(0.05)),
        ("%5", json!(0.05)),
        // (A `%` before the number would scale it in the format: 500.)
        ("%5", json!(5)),
        (". 5", json!(5)),
        // Marks that aren't a number's.
        ("1,23", json!(123)),
        ("1.234.5", json!(1234.5)),
        ("1.234,567,8", json!(1234.5678)),
        ("0,123", json!(123)),
        ("007", json!(7)),
        ("1;234", json!(1234)),
        ("1_234", json!(1234)),
        // A scale the value doesn't have.
        ("45.1%", json!(45.1)),
        ("1.23M", json!(1.24)),
        ("$5.00", json!(-5)),
        ("($5.00)", json!(5)),
    ] {
        let t = one_row(vec![cell(shown).typed("number", value)]);
        let (sheet, _, sst) = render(&t);
        assert_eq!(text_of(&sheet, &sst, "A1"), shown, "{sheet}");
    }
}

#[test]
fn cells_sharing_a_format_share_a_style() {
    let t = one_row(vec![
        cell("76.0").typed("number", json!(76)),
        cell("77.5").typed("number", json!(77.5)),
        cell("53").typed("number", json!(53)),
    ]);
    let (sheet, styles, _) = render(&t);
    let s = |r| attr(cell_xml(&sheet, r), "s").map(str::to_owned);
    assert_eq!(s("A1"), s("B1"));
    assert_ne!(s("A1"), s("C1"));
    // One custom format, declared once, and applied.
    assert_eq!(styles.matches("<numFmt ").count(), 1, "{styles}");
    assert!(styles.contains("<numFmts count=\"1\">"), "{styles}");
    assert!(styles.contains("applyNumberFormat=\"1\""), "{styles}");
}

#[test]
fn a_typed_number_with_a_footnote_mark_keeps_the_mark() {
    // A numeric cell can't show a superscript mark: the cell is (rich) text.
    let t = one_row(vec![cell_content(vec![
        text("90"),
        footnote_mark("fn1", "1"),
    ])
    .typed("number", json!(90))]);
    let (sheet, _, sst) = render(&t);
    let c = cell_xml(&sheet, "A1");
    assert!(c.contains("t=\"s\""), "{c}");
    assert!(sst.contains("<vertAlign val=\"superscript\"/>"), "{sst}");
}

#[test]
fn rich_text_lives_in_shared_strings() {
    let t = TableBuilder::new(2)
        .body(vec![row(vec![
            cell_content(vec![text("Mean (SD)"), footnote_mark("fn1", "1")]),
            cell("plain"),
        ])])
        .footnote("fn1", "1", "SD = standard deviation.")
        .build();
    let (sheet, _, sst) = render(&t);
    // Plain text is a plain string.
    assert_eq!(string_item(&sheet, &sst, "B1"), "<t>plain</t>", "{sst}");
    // The cell's mark and the footer's mark are rich strings.
    assert!(!sheet.contains("<rPr>"), "rich text left inline: {sheet}");
    assert!(sst.contains("count=\"3\" uniqueCount=\"3\""), "{sst}");
    for r in ["A1", "A3"] {
        let item = string_item(&sheet, &sst, r);
        assert!(
            item.contains("<rPr><vertAlign val=\"superscript\"/><"),
            "{item}"
        );
        assert!(item.contains(">1</t></r>"), "{item}");
    }
    assert_eq!(text_of(&sheet, &sst, "A3"), "1 SD = standard deviation.");
}

#[test]
fn identical_rich_strings_are_shared_once() {
    let mark = || cell_content(vec![text("x"), footnote_mark("fn1", "1")]);
    let t = TableBuilder::new(2)
        .body(vec![row(vec![mark(), mark()])])
        .footnote("fn1", "1", "note")
        .build();
    let (sheet, _, sst) = render(&t);
    assert_eq!(attr(cell_xml(&sheet, "A1"), "t"), Some("s"));
    assert!(cell_xml(&sheet, "A1").contains("<v>0</v>"), "{sheet}");
    assert!(cell_xml(&sheet, "B1").contains("<v>0</v>"), "{sheet}");
    // Two references to the shared one, one to the footer's.
    assert!(sst.contains("count=\"3\" uniqueCount=\"2\""), "{sst}");
}

#[test]
fn built_in_formats_serve_where_they_can() {
    for (shown, value, id) in [
        ("53", json!(53), Some("1")),
        ("3.50", json!(3.5), Some("2")),
        ("1,250", json!(1250), Some("3")),
        ("1,250.30", json!(1250.3), Some("4")),
        ("45%", json!(0.45), Some("9")),
        ("45.12%", json!(0.4512), Some("10")),
        ("$5", json!(5), None),
        ("1,250.3", json!(1250.3), None),
    ] {
        let t = one_row(vec![cell(shown).typed("number", value)]);
        let (_, styles, _) = render(&t);
        let used = styles
            .split("<cellXfs")
            .nth(1)
            .and_then(|x| x.split("numFmtId=\"").nth(2))
            .and_then(|x| x.split('"').next());
        match id {
            Some(id) => {
                assert_eq!(used, Some(id), "{shown}\n{styles}");
                assert!(!styles.contains("<numFmt "), "{shown}\n{styles}");
            }
            None => assert!(styles.contains("<numFmt "), "{shown}\n{styles}"),
        }
    }
}
