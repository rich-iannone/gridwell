//! A right-to-left table is a right-to-left sheet (`rightToLeft`), which Excel
//! shows mirrored: a border's `left` and `right` are the cell's start and end, so
//! the IR's physical sides swap, while horizontal alignment stays physical.

#[allow(dead_code)]
mod support;

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use support::{attr, cell_xml, render};

/// The `<xf>` of a cell, with its alignment, and its `<border>`.
fn format(sheet: &str, styles: &str, cell_ref: &str) -> (String, String) {
    let s: usize = attr(cell_xml(sheet, cell_ref), "s").map_or(0, |s| s.parse().unwrap());
    let xfs = &styles[styles.find("<cellXfs").unwrap()..styles.find("</cellXfs>").unwrap()];
    let xf = format!("<xf {}", xfs.split("<xf ").nth(s + 1).unwrap().trim_end());
    let border: usize = attr(&xf, "borderId").unwrap().parse().unwrap();
    let borders = &styles[styles.find("<borders").unwrap()..styles.find("</borders>").unwrap()];
    let border = format!(
        "<border>{}",
        borders
            .split("<border>")
            .nth(border + 1)
            .unwrap()
            .trim_end()
    );
    (xf, border)
}

/// The cell whose text is `text`.
fn cell_ref<'s>(sheet: &'s str, text: &str) -> &'s str {
    let end = sheet.find(&format!(">{text}</t>")).unwrap();
    let start = sheet[..end].rfind("<c r=\"").unwrap() + 6;
    &sheet[start..start + sheet[start..].find('"').unwrap()]
}

#[test]
fn a_right_to_left_table_is_a_right_to_left_sheet() {
    let (sheet, ..) = render(&directed(Some(TextDirection::Rtl), None));
    assert!(
        sheet.contains("<sheetView rightToLeft=\"1\" workbookViewId=\"0\"/>"),
        "{sheet}"
    );
    for dir in [None, Some(TextDirection::Ltr), Some(TextDirection::Auto)] {
        let (sheet, ..) = render(&directed(dir, None));
        assert!(!sheet.contains("rightToLeft"), "{sheet}");
    }
}

#[test]
fn borders_swap_and_alignment_stays_physical() {
    let red = "<color rgb=\"FFFF0000\"/>";
    let (sheet, styles, _) = render(&directed(None, None));
    let (xf, border) = format(&sheet, &styles, cell_ref(&sheet, "LEFTRED"));
    assert!(
        border.contains(&format!("<left style=\"medium\">{red}</left><right/>")),
        "{border}"
    );
    assert!(xf.contains("<alignment horizontal=\"right\"/>"), "{xf}");
    // Left is general alignment's side.
    let (xf, _) = format(&sheet, &styles, cell_ref(&sheet, "one"));
    assert!(!xf.contains("<alignment"), "{xf}");

    let (sheet, styles, _) = render(&directed(Some(TextDirection::Rtl), None));
    let (xf, border) = format(&sheet, &styles, cell_ref(&sheet, "LEFTRED"));
    assert!(
        border.contains(&format!("<left/><right style=\"medium\">{red}</right>")),
        "{border}"
    );
    assert!(xf.contains("<alignment horizontal=\"right\"/>"), "{xf}");
    // General alignment would start it on the right.
    let (xf, _) = format(&sheet, &styles, cell_ref(&sheet, "one"));
    assert!(xf.contains("<alignment horizontal=\"left\"/>"), "{xf}");
    // Lines outside the grid start where the text does.
    let (xf, _) = format(&sheet, &styles, cell_ref(&sheet, "Title"));
    assert!(!xf.contains("<alignment"), "{xf}");
}
