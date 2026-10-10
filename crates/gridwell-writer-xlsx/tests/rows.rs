//! Multi-line and large text stays visible: cells with line breaks wrap (Excel
//! shows a line break only when wrapping is on) and rows are tall enough for
//! their lines and font sizes. Without this, LibreOffice drew the lines of a
//! multi-line cell on top of each other, or only the last one.

use gridwell_ir::StyleDef;
use gridwell_testkit::{cell, cell_content, line_break, row, text, CellExt, TableBuilder};
use gridwell_writer_xlsx::XlsxWriter;

/// The `<row …>` start tag of row `r` (1-based).
fn row_tag(sheet: &str, r: usize) -> &str {
    let start = sheet.find(&format!("<row r=\"{r}\"")).unwrap();
    &sheet[start..start + sheet[start..].find('>').unwrap()]
}

fn row_height(sheet: &str, r: usize) -> Option<f64> {
    let tag = row_tag(sheet, r);
    let i = tag.find(" ht=\"")? + 5;
    tag[i..i + tag[i..].find('"').unwrap()].parse().ok()
}

/// The `<xf>` (in `cellXfs`) a cell's `s` index points at.
fn xf_of<'a>(sheet: &str, styles: &'a str, cell_ref: &str) -> &'a str {
    let c = &sheet[sheet.find(&format!("<c r=\"{cell_ref}\"")).unwrap()..];
    let tag = &c[..c.find('>').unwrap()];
    let s: usize = tag
        .find(" s=\"")
        .map(|i| {
            tag[i + 4..i + 4 + tag[i + 4..].find('"').unwrap()]
                .parse()
                .unwrap()
        })
        .unwrap_or(0);
    let xfs = &styles[styles.find("<cellXfs").unwrap()..];
    xfs.split("<xf ").nth(s + 1).unwrap()
}

#[test]
fn line_breaks_wrap_and_rows_fit_their_lines_and_sizes() {
    let t = TableBuilder::new(2)
        .style_def(
            "big",
            StyleDef {
                font_size: Some("24pt".into()),
                ..Default::default()
            },
        )
        .head(row(vec![cell("H1"), cell("H2")]))
        .body(vec![
            row(vec![
                cell("plain"),
                cell_content(vec![
                    text("one"),
                    line_break(),
                    text("two"),
                    line_break(),
                    text("three"),
                ]),
            ]),
            row(vec![cell("BIG").style("big"), cell("x")]),
            row(vec![cell("short"), cell("y")]),
        ])
        .build();
    let w = XlsxWriter::new();
    let sheet = w.render_sheet_xml(&t).unwrap();
    let styles = w.render_styles_xml(&t).unwrap();

    // The three-line cell wraps; its row fits three 11pt lines.
    assert!(
        xf_of(&sheet, &styles, "B2").contains("wrapText=\"1\""),
        "{styles}"
    );
    let h = row_height(&sheet, 2).expect("explicit height for the 3-line row");
    assert!(h >= 3.0 * 11.0 * 1.3, "{h}");
    // 24pt text needs a taller row than the default 15pt.
    let h = row_height(&sheet, 3).expect("explicit height for the 24pt row");
    assert!(h >= 24.0 * 1.3, "{h}");
    // A plain row keeps Excel's default (no explicit height), and single-line
    // cells don't wrap.
    assert_eq!(row_height(&sheet, 4), None, "{}", row_tag(&sheet, 4));
    assert!(
        !xf_of(&sheet, &styles, "A4").contains("wrapText"),
        "{styles}"
    );
}
