//! A right-to-left table: its rows `\rtlrow` (the first cell on the right), its
//! paragraphs `\rtlpar`, and lines that start where the text does `\qr`.
//! Alignment (`\ql`, `\qr`) is physical, so a left-aligned cell says so; a
//! row's cell borders are its start and end, so the IR's physical sides swap.

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use gridwell_writer_rtf::render_rtf;

fn rtf(dir: Option<TextDirection>) -> String {
    render_rtf(&directed(dir, None)).unwrap()
}

/// The paragraph whose text is `text`, from `\pard` to the text.
fn paragraph<'r>(rtf: &'r str, text: &str) -> &'r str {
    let end = rtf
        .find(&format!(" {text}"))
        .unwrap_or_else(|| panic!("no {text}"));
    &rtf[rtf[..end].rfind("\\pard").unwrap()..end]
}

/// The row definition (`\trowd` to its first paragraph) before `text`.
fn row_def<'r>(rtf: &'r str, text: &str) -> &'r str {
    let end = rtf.find(&format!(" {text}")).unwrap();
    let start = rtf[..end].rfind("\\trowd").unwrap();
    &rtf[start..start + rtf[start..].find("\\pard").unwrap()]
}

#[test]
fn rows_and_paragraphs_run_right_to_left() {
    let ltr = rtf(None);
    assert!(!ltr.contains("\\rtlrow") && !ltr.contains("\\rtlpar"));
    let rtl = rtf(Some(TextDirection::Rtl));
    for text in ["Name", "one", "LEFTRED", "Group"] {
        assert!(
            row_def(&rtl, text).starts_with("\\trowd\\rtlrow"),
            "{text}: {}",
            row_def(&rtl, text)
        );
    }
    for text in ["Title", "Name", "one", "2", "Group", "Source"] {
        assert!(
            paragraph(&rtl, text).contains("\\rtlpar"),
            "{text}: {}",
            paragraph(&rtl, text)
        );
    }
    assert!(!rtf(Some(TextDirection::Auto)).contains("\\rtlrow"));
}

#[test]
fn alignment_is_physical() {
    let ltr = rtf(None);
    assert!(paragraph(&ltr, "2").contains("\\qr"));
    assert!(
        !paragraph(&ltr, "one").contains("\\q"),
        "{}",
        paragraph(&ltr, "one")
    );
    let rtl = rtf(Some(TextDirection::Rtl));
    assert!(paragraph(&rtl, "2").contains("\\qr"));
    assert!(
        paragraph(&rtl, "one").contains("\\ql"),
        "{}",
        paragraph(&rtl, "one")
    );
    // The title, group label and notes start on the right.
    for text in ["Title", "Group", "Source"] {
        assert!(
            paragraph(&rtl, text).contains("\\rtlpar\\qr"),
            "{text}: {}",
            paragraph(&rtl, text)
        );
    }
}

#[test]
fn cell_borders_swap() {
    let ltr = rtf(None);
    let def = row_def(&ltr, "one");
    assert!(def.contains("\\clbrdrl\\brdrs\\brdrw30"), "{def}");
    assert!(!def.contains("\\clbrdrr"), "{def}");
    let rtl = rtf(Some(TextDirection::Rtl));
    let def = row_def(&rtl, "one");
    assert!(def.contains("\\clbrdrr\\brdrs\\brdrw30"), "{def}");
    assert!(!def.contains("\\clbrdrl"), "{def}");
}
