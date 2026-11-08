//! Merged cells: DrawingML keeps a `<a:tc>` for every grid position, and each
//! one carries the borders of the cell covering it on the sides that lie on that
//! cell's outline, before its fill. The origin alone carries the alignment.

use gridwell_testkit::examples::styled_merge;
use gridwell_writer_pptx::{PptxConfig, PptxWriter};

/// The slide without the default look's rules: only the table's own borders.
fn bare_slide() -> String {
    PptxWriter::with_config(PptxConfig { base_styles: false })
        .render_slide_xml(&styled_merge())
        .unwrap()
}

/// The `<a:tc>`s of each `<a:tr>`.
fn cells(xml: &str) -> Vec<Vec<&str>> {
    xml.split("<a:tr ")
        .skip(1)
        .map(|r| {
            let mut tcs: Vec<&str> = r.split("</a:tc>").collect();
            tcs.pop();
            tcs
        })
        .collect()
}

/// Which of top, right, bottom, left the cell draws.
fn sides(tc: &str) -> [bool; 4] {
    ["lnT", "lnR", "lnB", "lnL"].map(|s| tc.contains(&format!("<a:{s} ")))
}

#[test]
fn merged_cells_are_outlined_once() {
    let xml = bare_slide();
    let rows = cells(&xml);
    // Header, group label, then the two rows the merge spans.
    let (r0, r1) = (&rows[2], &rows[3]);
    assert!(r0[0].contains("gridSpan=\"2\" rowSpan=\"2\""), "{}", r0[0]);
    assert_eq!(sides(r0[0]), [true, false, false, true], "{}", r0[0]);
    assert_eq!(sides(r0[1]), [true, true, false, false], "{}", r0[1]);
    assert_eq!(sides(r1[0]), [false, false, true, true], "{}", r1[0]);
    assert_eq!(sides(r1[1]), [false, true, true, false], "{}", r1[1]);
    assert_eq!(sides(r0[2]), [false; 4]);
    assert_eq!(sides(r1[2]), [false; 4]);
    // 4.5pt in EMU.
    assert!(r0[0].contains(
        "><a:lnL w=\"57150\"><a:solidFill><a:srgbClr val=\"00AA00\"/></a:solidFill></a:lnL>"
    ));
    // The label spans the row: its first piece takes the left edge, the last
    // the right, every piece the top and bottom; the lines come before the fill.
    let label = &rows[1];
    assert_eq!(sides(label[0]), [true, false, true, true]);
    assert_eq!(sides(label[1]), [true, false, true, false]);
    assert_eq!(sides(label[2]), [true, true, true, false]);
    assert!(
        label[0].contains("<a:prstDash val=\"dash\"/></a:lnL>"),
        "{}",
        label[0]
    );
    assert!(label[0].find("</a:lnB>") < label[0].find("<a:solidFill><a:srgbClr val=\"F0F0F0\""));
}

#[test]
fn a_merged_cell_is_anchored_by_its_origin() {
    let xml = bare_slide();
    let rows = cells(&xml);
    assert!(
        rows[2][0].contains("<a:tcPr anchor=\"b\"><a:lnL "),
        "{}",
        rows[2][0]
    );
    let anchored: Vec<(usize, usize)> = rows
        .iter()
        .enumerate()
        .flat_map(|(r, tcs)| tcs.iter().enumerate().map(move |(c, tc)| (r, c, tc)))
        .filter(|(_, _, tc)| tc.contains(" anchor="))
        .map(|(r, c, _)| (r, c))
        .collect();
    assert_eq!(anchored, [(2, 0)]);
}
