//! A right-to-left table (`a:tblPr rtl`, its paragraphs `rtl`, aligned right by
//! default) and the table's language on every run. Paragraph alignment is
//! physical, so a left-aligned cell says so; an `rtl` table's cell borders are
//! its start and end, so the IR's physical sides swap.

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use gridwell_writer_pptx::PptxWriter;

fn slide(dir: Option<TextDirection>, lang: Option<&str>) -> String {
    PptxWriter::new()
        .render_slide_xml(&directed(dir, lang))
        .unwrap()
}

/// The paragraph whose text is `text`, from `<a:p>` to the text.
fn paragraph<'x>(xml: &'x str, text: &str) -> &'x str {
    let end = xml
        .find(&format!(">{text}</a:t>"))
        .unwrap_or_else(|| panic!("no {text}"));
    &xml[xml[..end].rfind("<a:p>").unwrap()..end]
}

/// The cell whose text is `text`, from its text to `</a:tc>`.
fn cell_props<'x>(xml: &'x str, text: &str) -> &'x str {
    let start = xml.find(&format!(">{text}</a:t>")).unwrap();
    &xml[start..start + xml[start..].find("</a:tc>").unwrap()]
}

#[test]
fn a_right_to_left_table_has_right_to_left_paragraphs() {
    let ltr = slide(None, None);
    assert!(!ltr.contains("rtl="), "{ltr}");
    assert!(ltr.contains("<a:tblPr firstRow=\"1\" bandRow=\"1\"/>"));
    let rtl = slide(Some(TextDirection::Rtl), None);
    assert!(
        rtl.contains("<a:tblPr rtl=\"1\" firstRow=\"1\" bandRow=\"1\"/>"),
        "{rtl}"
    );
    for (text, algn) in [
        ("Title", "r"),
        ("Name", "l"),
        ("Value", "r"),
        ("Group", "r"),
        ("one", "l"),
        ("2", "r"),
        ("Source", "r"),
    ] {
        assert!(
            paragraph(&rtl, text).starts_with(&format!("<a:p><a:pPr algn=\"{algn}\" rtl=\"1\"/>")),
            "{text}: {}",
            paragraph(&rtl, text)
        );
    }
    assert!(!slide(Some(TextDirection::Auto), None).contains("rtl="));
}

#[test]
fn cell_borders_swap() {
    let ltr = slide(None, None);
    let c = cell_props(&ltr, "LEFTRED");
    assert!(
        c.contains("<a:lnL w=\"19050\"><a:solidFill><a:srgbClr val=\"FF0000\"/>"),
        "{c}"
    );
    assert!(!c.contains("<a:lnR"), "{c}");
    let rtl = slide(Some(TextDirection::Rtl), None);
    let c = cell_props(&rtl, "LEFTRED");
    assert!(
        c.contains("<a:lnR w=\"19050\"><a:solidFill><a:srgbClr val=\"FF0000\"/>"),
        "{c}"
    );
    assert!(!c.contains("<a:lnL"), "{c}");
}

#[test]
fn every_run_is_in_the_tables_language() {
    let runs = |xml: &str| xml.matches("<a:rPr ").count();
    let xml = slide(None, Some("ar"));
    assert_eq!(xml.matches("<a:rPr lang=\"ar\"").count(), runs(&xml));
    assert!(runs(&xml) >= 9, "{xml}");
    // Without one, US English, as before.
    let xml = slide(None, None);
    assert_eq!(xml.matches("<a:rPr lang=\"en-US\"").count(), runs(&xml));
    // Escaped.
    let xml = slide(None, Some("a\"b"));
    assert!(xml.contains("<a:rPr lang=\"a&quot;b\""), "{xml}");
}
