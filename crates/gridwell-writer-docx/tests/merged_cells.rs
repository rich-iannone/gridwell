//! Merged cells: each piece carries the borders on its outline (Word draws a
//! vertically merged cell's bottom from its last piece) and the cell's vertical
//! alignment; plain cells get neither of their own, and `w:tcPr` keeps schema
//! order.

use gridwell_testkit::examples::styled_merge;
use gridwell_writer_docx::DocxWriter;

/// The `<w:tc>`s of each `<w:tr>`.
fn cells(xml: &str) -> Vec<Vec<&str>> {
    xml.split("<w:tr>")
        .skip(1)
        .map(|r| r.split("<w:tc>").skip(1).collect())
        .collect()
}

/// Which of top, right, bottom, left the cell's own borders set.
fn sides(tc: &str) -> [bool; 4] {
    let b = tc
        .split("<w:tcBorders>")
        .nth(1)
        .map_or("", |b| b.split("</w:tcBorders>").next().unwrap());
    ["top", "right", "bottom", "left"].map(|s| b.contains(&format!("<w:{s} ")))
}

#[test]
fn merged_cells_are_outlined_once() {
    let xml = DocxWriter::new()
        .render_document_xml(&styled_merge())
        .unwrap();
    let rows = cells(&xml);
    // Header, group label, then the two rows the merge spans.
    let (first, last) = (rows[2][0], rows[3][0]);
    assert!(first.contains("<w:vMerge w:val=\"restart\"/>"), "{first}");
    assert_eq!(sides(first), [true, true, false, true], "{first}");
    assert!(last.contains("<w:vMerge/>"), "{last}");
    assert_eq!(sides(last), [false, true, true, true], "{last}");
    assert_eq!(sides(rows[2][1]), [false; 4]);
    assert_eq!(sides(rows[3][1]), [false; 4]);
    // 4.5pt in eighths; the schema puts borders after the merge, before shading.
    assert!(first.contains(
        "<w:vMerge w:val=\"restart\"/><w:tcBorders><w:top w:val=\"single\" w:sz=\"36\" \
         w:space=\"0\" w:color=\"00AA00\"/>"
    ));
    let label = rows[1][0];
    assert_eq!(sides(label), [true; 4], "{label}");
    assert!(label.contains("w:val=\"dashed\" w:sz=\"6\" w:space=\"0\" w:color=\"0000BB\""));
    assert!(
        label.find("</w:tcBorders>") < label.find("<w:shd "),
        "{label}"
    );
}

#[test]
fn every_piece_of_a_merged_cell_is_aligned() {
    let xml = DocxWriter::new()
        .render_document_xml(&styled_merge())
        .unwrap();
    let rows = cells(&xml);
    // `w:vAlign` comes after the borders and shading, last of what is set.
    for piece in [rows[2][0], rows[3][0]] {
        assert!(
            piece.contains("</w:tcBorders><w:vAlign w:val=\"bottom\"/></w:tcPr>"),
            "{piece}"
        );
    }
    for plain in [rows[1][0], rows[2][1], rows[3][1]] {
        assert!(!plain.contains("<w:vAlign"), "{plain}");
    }
}
