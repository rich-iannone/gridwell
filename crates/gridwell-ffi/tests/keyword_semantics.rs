//! Keyword values mean the same thing in every writer. Before typing, writers
//! compared raw strings (`font_weight == "bold"`), so CSS-valid values such as
//! `"700"` were silently ignored.

use gridwell_ir::{StyleDef, Table};
use gridwell_testkit::{cell, row, CellExt, TableBuilder};

fn table_with_weight(weight: serde_json::Value) -> Table {
    // No header row: header cells are bold anyway and would mask the result.
    let mut t = TableBuilder::new(2)
        .style_def("w", StyleDef::default())
        .body(vec![row(vec![cell("BOLDCELL").style("w"), cell("plain")])])
        .build();
    t.styles.defs.get_mut("w").unwrap().font_weight = Some(serde_json::from_value(weight).unwrap());
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    t
}

/// (writer, render, bold marker that must appear iff the cell is bold)
type WriterCase = (&'static str, fn(&Table) -> String, &'static str);

fn writers() -> Vec<WriterCase> {
    vec![
        (
            "latex",
            |t| gridwell_writer_latex::render_latex(t).unwrap(),
            "\\textbf{BOLDCELL}",
        ),
        (
            "typst",
            |t| gridwell_writer_typst::render_typst(t).unwrap(),
            "weight: \"bold\"",
        ),
        (
            "rtf",
            |t| gridwell_writer_rtf::render_rtf(t).unwrap(),
            "\\b BOLDCELL",
        ),
        (
            "svg",
            |t| gridwell_writer_svg::render_svg(t).unwrap(),
            "font-weight=\"bold\">BOLDCELL",
        ),
        (
            "ansi",
            |t| gridwell_writer_ansi::render_ansi(t).unwrap(),
            "\u{1b}[1m",
        ),
        (
            "docx",
            |t| {
                gridwell_writer_docx::DocxWriter::new()
                    .render_document_xml(t)
                    .unwrap()
            },
            "<w:b/>",
        ),
        (
            "pptx",
            |t| {
                gridwell_writer_pptx::PptxWriter::new()
                    .render_slide_xml(t)
                    .unwrap()
            },
            " b=\"1\"",
        ),
    ]
}

#[test]
fn bold_weights_render_bold_everywhere() {
    for weight in [
        serde_json::json!("bold"),
        serde_json::json!("700"),
        serde_json::json!(700),
        serde_json::json!("bolder"),
    ] {
        let t = table_with_weight(weight.clone());
        for (name, render, marker) in writers() {
            assert!(
                render(&t).contains(marker),
                "{name}: weight {weight} not rendered bold"
            );
        }
    }
}

#[test]
fn normal_weights_do_not_render_bold() {
    for weight in [
        serde_json::json!("normal"),
        serde_json::json!("400"),
        serde_json::json!(500),
    ] {
        let t = table_with_weight(weight.clone());
        for (name, render, marker) in writers() {
            assert!(
                !render(&t).contains(marker),
                "{name}: weight {weight} rendered bold"
            );
        }
    }
}

#[test]
fn html_emits_the_canonical_weight() {
    let t = table_with_weight(serde_json::json!(700));
    let html = gridwell_writer_html::render_html(&t).unwrap();
    assert!(html.contains("font-weight: 700"), "{html}");
}

#[test]
fn xlsx_writes_integers_and_numbers_as_numeric_cells() {
    let t = TableBuilder::new(3)
        .body(vec![row(vec![
            cell("42").typed("integer", serde_json::json!(42)),
            cell("1.5").typed("number", serde_json::json!(1.5)),
            cell("x").typed("string", serde_json::json!("x")),
        ])])
        .build();
    let xml = gridwell_writer_xlsx::XlsxWriter::new()
        .render_sheet_xml(&t)
        .unwrap();
    assert!(xml.contains("<v>42</v>"), "integer not numeric: {xml}");
    assert!(xml.contains("<v>1.5</v>"), "number not numeric: {xml}");
    assert!(xml.contains("<t>x</t>"), "string not text: {xml}");
}
