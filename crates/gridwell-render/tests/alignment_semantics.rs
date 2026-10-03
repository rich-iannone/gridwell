//! Alignment means the same thing in every writer: a right-aligned column's
//! cells are right-aligned, and a header cell spanning columns (a spanner label)
//! is centred. The grid invariant compares text only, so this is checked here;
//! HTML ignored column alignment entirely until this test existed.

use gridwell_ir::Table;
use gridwell_testkit::{cell, column, placeholder, row, CellExt, ColumnExt, TableBuilder};

fn table() -> Table {
    TableBuilder::new(2)
        .columns(vec![
            column("n", "Name"),
            column("v", "Value").align("right"),
        ])
        .head(row(vec![cell("SPANNER").colspan(2), placeholder()]))
        .head(row(vec![cell("Name"), cell("Value")]))
        .body(vec![row(vec![cell("alpha"), cell("12345")])])
        .build()
}

fn text(format: &str) -> String {
    let t = table();
    match format {
        "docx" => gridwell_writer_docx::DocxWriter::new()
            .render_document_xml(&t)
            .unwrap(),
        "pptx" => gridwell_writer_pptx::PptxWriter::new()
            .render_slide_xml(&t)
            .unwrap(),
        "xlsx" => {
            let w = gridwell_writer_xlsx::XlsxWriter::new();
            w.render_sheet_xml(&t).unwrap() + &w.render_styles_xml(&t).unwrap()
        }
        f => gridwell_render::render(&t, f, None)
            .unwrap()
            .as_text()
            .unwrap()
            .to_string(),
    }
}

/// (format, marker for the right-aligned number, marker for the centred spanner)
const CASES: &[(&str, &str, &str)] = &[
    ("html", "<td class=\"gw__al_right\">12345</td>", "<th class=\"gw__al_center\" colspan=\"2\">SPANNER</th>"),
    ("latex", "\\begin{tabular}{lr}", "\\multicolumn{2}{c}{SPANNER}"),
    ("typst", "align: (left, right,),", "table.cell(colspan: 2, align: center)"),
    ("rtf", "\\pard\\intbl\\qr 12345\\cell", "\\pard\\intbl\\qc\\b SPANNER\\cell"),
    ("docx", "<w:jc w:val=\"right\"/></w:pPr><w:r><w:t xml:space=\"preserve\">12345", "<w:jc w:val=\"center\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">SPANNER"),
    ("pptx", "<a:pPr algn=\"r\"/><a:r><a:rPr lang=\"en-US\"/><a:t>12345", "<a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"en-US\" b=\"1\"/><a:t>SPANNER"),
    ("xlsx", "<alignment horizontal=\"right\"/>", "<alignment horizontal=\"center\"/>"),
    ("svg", "text-anchor=\"end\" xml:space=\"preserve\">12345", "text-anchor=\"middle\" xml:space=\"preserve\"><tspan font-weight=\"bold\">SPANNER"),
];

#[test]
fn right_aligned_columns_and_centred_spanners_in_every_writer() {
    for (format, right, centred) in CASES {
        let out = text(format);
        assert!(
            out.contains(right),
            "{format}: right alignment missing ({right:?}):\n{out}"
        );
        assert!(
            out.contains(centred),
            "{format}: centred spanner missing ({centred:?}):\n{out}"
        );
    }
}

#[test]
fn pandoc_and_quarto_carry_alignment() {
    for format in ["pandoc", "quarto"] {
        let out = text(format);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let s = v.to_string();
        // Column specs: left, right.
        assert!(s.contains(r#"[{"t":"AlignLeft"},{"t":"ColWidthDefault"}],[{"t":"AlignRight"},{"t":"ColWidthDefault"}]"#), "{format}: {s}");
        // The spanner overrides its columns with AlignCenter.
        assert!(s.contains(r#"{"t":"AlignCenter"},1,2"#), "{format}: {s}");
    }
}

#[test]
fn ansi_places_text_by_alignment() {
    let out = text("ansi");
    let plain: String = out
        .split('\u{1b}')
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                p
            } else {
                p.split_once('m').map_or("", |x| x.1)
            }
        })
        .collect();
    let spanner = plain.lines().find(|l| l.contains("SPANNER")).unwrap();
    let inner = spanner.trim_matches('│');
    let (lead, trail) = (
        inner.len() - inner.trim_start().len(),
        inner.len() - inner.trim_end().len(),
    );
    assert!(
        lead.abs_diff(trail) <= 1,
        "spanner not centred: {spanner:?}"
    );
    let value = plain.lines().find(|l| l.contains("12345")).unwrap();
    assert!(
        value.trim_end_matches('│').ends_with("12345 "),
        "not right-aligned: {value:?}"
    );
}

#[test]
fn html_alignment_rules_outrank_typical_page_css() {
    // A bare `.gw__al_right` loses to a host page's `.gw_table td { text-align: left }`
    // (one class + one element beats one class); scoped under the table class it
    // wins. The visual harness wraps output in exactly such a page stylesheet.
    let html = text("html");
    assert!(
        html.contains(".gw_table .gw__al_right { text-align: right }"),
        "{html}"
    );
    assert!(
        html.contains(".gw_table .gw__al_center { text-align: center }"),
        "{html}"
    );
}
