//! Typed numbers keep their display: the cell holds the value, and its number
//! format shows it as the table does (`76.0`, not `76`). A display no number
//! format can reproduce stays text. Rich text (footnote marks, styled runs) goes
//! in the shared-string table, where spreadsheet apps (Numbers among them) honor
//! run formatting; inline strings lose it there.

use gridwell_ir::Table;
use gridwell_testkit::{cell, cell_content, footnote_mark, row, text, CellExt, TableBuilder};
use gridwell_writer_xlsx::XlsxWriter;
use serde_json::json;

/// The value of attribute `name` in the start tag `tag`.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

/// The `<c>` element for `cell_ref`.
fn cell_xml<'a>(sheet: &'a str, cell_ref: &str) -> &'a str {
    let start = sheet
        .find(&format!("<c r=\"{cell_ref}\""))
        .unwrap_or_else(|| panic!("no {cell_ref} in {sheet}"));
    let rest = &sheet[start..];
    let open = rest.find('>').unwrap();
    if rest[..open].ends_with('/') {
        return &rest[..=open];
    }
    &rest[..rest.find("</c>").unwrap() + 4]
}

/// The number format code of a cell (`General` without one).
fn format_code(sheet: &str, styles: &str, cell_ref: &str) -> String {
    let c = cell_xml(sheet, cell_ref);
    let s: usize = attr(c, "s").map_or(0, |s| s.parse().unwrap());
    let xfs = &styles[styles.find("<cellXfs").unwrap()..];
    let xf = xfs.split("<xf ").nth(s + 1).unwrap();
    let id: u32 = attr(&format!(" {xf}"), "numFmtId")
        .unwrap()
        .parse()
        .unwrap();
    match id {
        0 => "General".into(),
        1 => "0".into(),
        2 => "0.00".into(),
        3 => "#,##0".into(),
        4 => "#,##0.00".into(),
        _ => {
            let fmt = styles
                .split("<numFmt ")
                .skip(1)
                .find(|f| attr(&format!(" {f}"), "numFmtId") == Some(&id.to_string()))
                .unwrap_or_else(|| panic!("numFmt {id} missing: {styles}"));
            attr(&format!(" {fmt}"), "formatCode")
                .unwrap()
                .replace("&quot;", "\"")
        }
    }
}

fn one_row(cells: Vec<gridwell_ir::Cell>) -> Table {
    TableBuilder::new(cells.len() as u32)
        .body(vec![row(cells)])
        .build()
}

fn render(t: &Table) -> (String, String, String) {
    let w = XlsxWriter::new();
    (
        w.render_sheet_xml(t).unwrap(),
        w.render_styles_xml(t).unwrap(),
        w.render_shared_strings_xml(t).unwrap(),
    )
}

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
    ] {
        let t = one_row(vec![cell(shown).typed("number", value)]);
        let (sheet, _, _) = render(&t);
        let c = cell_xml(&sheet, "A1");
        assert!(
            c.contains("t=\"inlineStr\"")
                && c.contains(&format!(">{}</t>", shown.replace('<', "&lt;"))),
            "{shown}: {c}"
        );
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
    // Plain text stays inline.
    assert!(
        cell_xml(&sheet, "B1").contains("t=\"inlineStr\""),
        "{sheet}"
    );
    // The cell's mark and the footer's mark are rich shared strings.
    for (r, idx) in [("A1", 0), ("A3", 1)] {
        let c = cell_xml(&sheet, r);
        assert!(
            c.contains("t=\"s\"") && c.contains(&format!("<v>{idx}</v>")),
            "{r}: {c}"
        );
    }
    assert!(!sheet.contains("<rPr>"), "rich text left inline: {sheet}");
    let items: Vec<&str> = sst.split("<si>").skip(1).collect();
    assert_eq!(items.len(), 2, "{sst}");
    assert!(sst.contains("count=\"2\" uniqueCount=\"2\""), "{sst}");
    for item in items {
        assert!(item.contains("<vertAlign val=\"superscript\"/><"), "{item}");
        assert!(item.contains(">1</t></r>"), "{item}");
    }
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
