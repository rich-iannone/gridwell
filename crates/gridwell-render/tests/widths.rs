//! Column widths in the formats that give every column one (RTF, DOCX, PPTX): the
//! same widths in each, from `gridwell_layout`'s [`column_widths_pt`]. Absolute
//! widths are themselves; `%` and `fr` divide the table's width, which is
//! `table_width` or the line (6in in RTF and DOCX, the slide's text width in
//! PPTX); a column without a width gets the format's default, unless the table has
//! a width and no column is `fr`, when such columns share what the others leave.
//! `em` is each format's body text size.
//!
//! [`column_widths_pt`]: gridwell_layout::ResolvedTable::column_widths_pt

use gridwell_ir::Table;
use gridwell_testkit::{cell, column, row, ColumnExt, TableBuilder};

/// One row, a column per width (`""` for none), in a table `table_width` wide.
fn table(table_width: Option<&str>, widths: &[&str]) -> Table {
    let cols = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let c = column(&format!("c{i}"), "C");
            if w.is_empty() {
                c
            } else {
                c.width(w)
            }
        })
        .collect();
    let mut b = TableBuilder::new(widths.len() as u32).columns(cols);
    if let Some(w) = table_width {
        b = b.table_width(w);
    }
    b.body(vec![row(widths.iter().map(|_| cell("x")).collect())])
        .build()
}

/// The number after each `key`.
fn numbers(s: &str, key: &str) -> Vec<u64> {
    s.split(key)
        .skip(1)
        .map(|r| {
            let end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
            r[..end].parse().unwrap()
        })
        .collect()
}

/// RTF column widths in twips, from the `\cellx` right edges of the row with the
/// most cells (a merged cell has one edge).
fn rtf(t: &Table) -> Vec<u64> {
    let src = gridwell_writer_rtf::render_rtf(t).unwrap();
    let edges = src
        .split("\\trowd")
        .skip(1)
        .map(|r| numbers(r.split('\n').next().unwrap(), "\\cellx"))
        .max_by_key(Vec::len)
        .unwrap();
    std::iter::once(0)
        .chain(edges)
        .collect::<Vec<_>>()
        .windows(2)
        .map(|e| e[1] - e[0])
        .collect()
}

/// DOCX column widths in dxa, checking the table's width is their sum.
fn docx(t: &Table) -> Vec<u64> {
    let xml = gridwell_writer_docx::DocxWriter::new()
        .render_document_xml(t)
        .unwrap();
    let cols = numbers(&xml, "<w:gridCol w:w=\"");
    assert_eq!(
        numbers(&xml, "<w:tblW w:w=\""),
        [cols.iter().sum::<u64>()],
        "{xml}"
    );
    cols
}

/// PPTX column widths in EMU, checking the table's frame is their sum, centered
/// on the slide (at its left edge when wider).
fn pptx(t: &Table) -> Vec<u64> {
    const SLIDE_W: u64 = 9_144_000;
    let xml = gridwell_writer_pptx::PptxWriter::new()
        .render_slide_xml(t)
        .unwrap();
    let cols = numbers(&xml, "<a:gridCol w=\"");
    let frame = xml.split("<p:graphicFrame>").nth(1).unwrap();
    let cx = numbers(frame, "<a:ext cx=\"")[0];
    let x = numbers(frame, "<a:off x=\"")[0];
    assert_eq!(cx, cols.iter().sum::<u64>(), "{xml}");
    assert_eq!(x, SLIDE_W.saturating_sub(cx) / 2, "{xml}");
    cols
}

fn twips(pt: &[f64]) -> Vec<u64> {
    pt.iter().map(|p| (p * 20.0).round() as u64).collect()
}

fn emu(pt: &[f64]) -> Vec<u64> {
    pt.iter().map(|p| (p * 12700.0).round() as u64).collect()
}

/// A table with an absolute width is as wide in every format.
#[test]
fn an_absolute_table_width_is_the_same_everywhere() {
    for (width, cols, want) in [
        ("300pt", &["", "60pt", ""][..], &[120.0, 60.0, 120.0][..]),
        (
            "400pt",
            &["100pt", "25%", "1fr", "3fr"],
            &[100.0, 100.0, 50.0, 150.0],
        ),
        (
            "400px",
            &["2cm", ""],
            &[2.0 / 2.54 * 72.0, 300.0 - 2.0 / 2.54 * 72.0],
        ),
        // Rounded, not truncated.
        ("100pt", &["", "", ""], &[100.0 / 3.0; 3]),
    ] {
        let t = table(Some(width), cols);
        assert_eq!(rtf(&t), twips(want), "RTF {width} {cols:?}");
        assert_eq!(docx(&t), twips(want), "DOCX {width} {cols:?}");
        assert_eq!(pptx(&t), emu(want), "PPTX {width} {cols:?}");
    }
}

/// `%` and `fr` divide the line when the table has no absolute width; a column
/// without a width, or an `fr` column the others leave no room for, gets the
/// format's default; `em` is the body text's size.
#[test]
fn the_line_default_and_em_are_each_formats_own() {
    // (table width, columns, RTF and DOCX widths at a 432pt line, PPTX at 648pt)
    type Case<'a> = (Option<&'a str>, &'a [&'a str], &'a [f64], &'a [f64]);
    let cases: [Case; 5] = [
        (None, &["", ""], &[108.0, 108.0], &[72.0, 72.0]),
        (None, &["30%", "1fr"], &[129.6, 302.4], &[194.4, 453.6]),
        (Some("50%"), &["25%", ""], &[54.0, 162.0], &[81.0, 243.0]),
        (
            Some("100pt"),
            &["80pt", "30pt", "1fr"],
            &[80.0, 30.0, 108.0],
            &[80.0, 30.0, 72.0],
        ),
        (
            Some("100%"),
            &["10%", "", "3fr"],
            &[43.2, 108.0, 280.8],
            &[64.8, 72.0, 511.2],
        ),
    ];
    for (width, cols, line, slide) in cases {
        let t = table(width, cols);
        assert_eq!(rtf(&t), twips(line), "RTF {width:?} {cols:?}");
        assert_eq!(docx(&t), twips(line), "DOCX {width:?} {cols:?}");
        assert_eq!(pptx(&t), emu(slide), "PPTX {width:?} {cols:?}");
    }
    // RTF's text is 12pt, DOCX's 11pt, PPTX's 18pt.
    let t = table(Some("4in"), &["2em", ""]);
    assert_eq!(rtf(&t), twips(&[24.0, 264.0]));
    assert_eq!(docx(&t), twips(&[22.0, 266.0]));
    assert_eq!(pptx(&t), emu(&[36.0, 252.0]));
}

/// Widths too wide for a format's integers are scaled down to fit them, in
/// proportion, and thin ones are a unit wide.
#[test]
fn extreme_widths_fit_each_formats_integers() {
    let max = u64::from(i32::MAX as u32);
    let t = table(None, &["1e300pt", "1e300pt", "1e-300pt"]);
    for (name, got) in [("RTF", rtf(&t)), ("DOCX", docx(&t)), ("PPTX", pptx(&t))] {
        assert!(got.iter().sum::<u64>() <= max, "{name} {got:?}");
        assert_eq!(got[0], got[1], "{name} {got:?}");
        assert!(got[0] > max / 2 - 2, "{name} {got:?}");
        assert_eq!(got[2], 1, "{name} {got:?}");
    }
    // Just over the PPTX limit (about 169,000pt): only PPTX scales.
    let t = table(None, &["100000pt", "100000pt"]);
    assert_eq!(rtf(&t), [2_000_000; 2]);
    assert_eq!(docx(&t), [2_000_000; 2]);
    let p = pptx(&t);
    assert_eq!(p[0], p[1]);
    assert_eq!(p.iter().sum::<u64>(), max - 1);
}

/// Every corpus example has as many widths as visible columns, and RTF and DOCX
/// agree on them.
#[test]
fn rtf_and_docx_agree_on_every_example() {
    for ex in gridwell_testkit::examples() {
        let t = ex.table();
        let visible = t.column_spec.iter().filter(|c| !c.hidden).count();
        let d = docx(&t);
        assert_eq!(d.len(), visible, "{}", ex.name);
        assert_eq!(rtf(&t), d, "{}", ex.name);
        assert_eq!(pptx(&t).len(), visible, "{}", ex.name);
    }
}
