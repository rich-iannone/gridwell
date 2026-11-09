//! A right-to-left table (its columns written as they are seen, the first on the
//! right, and its paragraphs `rtl`, aligned right by default) and the table's
//! language on every run. Paragraph alignment and cell borders are physical, so
//! a left-aligned cell says so, and a border keeps its side.

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
    // Not `rtl="1"`, which LibreOffice and Apple's importer ignore.
    let rtl = slide(Some(TextDirection::Rtl), None);
    assert!(
        rtl.contains("<a:tblPr firstRow=\"1\" bandRow=\"1\"/>"),
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
fn right_to_left_the_columns_are_written_as_they_are_seen() {
    let order = |xml: &str, texts: &[&str]| {
        let at: Vec<usize> = texts
            .iter()
            .map(|t| xml.find(&format!(">{t}</a:t>")).unwrap())
            .collect();
        at.windows(2).all(|w| w[0] < w[1])
    };
    let ltr = slide(None, None);
    assert!(order(
        &ltr,
        &["Name", "Value", "one", "LEFTRED", "two", "2"]
    ));
    let rtl = slide(Some(TextDirection::Rtl), None);
    assert!(
        order(&rtl, &["Value", "Name", "LEFTRED", "one", "2", "two"]),
        "{rtl}"
    );
}

#[test]
fn cell_borders_keep_their_sides() {
    let red = "<a:lnL w=\"19050\"><a:solidFill><a:srgbClr val=\"FF0000\"/>";
    let stub = "w=\"19050\"><a:solidFill><a:srgbClr val=\"D3D3D3\"/>";
    for dir in [None, Some(TextDirection::Rtl)] {
        let xml = slide(dir.clone(), None);
        let c = cell_props(&xml, "LEFTRED");
        assert!(c.contains(red), "{dir:?}: {c}");
        assert!(!c.contains("<a:lnR"), "{dir:?}: {c}");
    }
    // The rule at the stub's end: its right, or right to left its left. Left to
    // right, beside LEFTRED's own border, none.
    let ltr = slide(None, None);
    let c = cell_props(&ltr, "two");
    assert!(c.contains(&format!("<a:lnR {stub}")), "{c}");
    assert!(!c.contains("<a:lnL"), "{c}");
    assert!(!cell_props(&ltr, "one").contains("<a:lnR"));
    let rtl = slide(Some(TextDirection::Rtl), None);
    for text in ["one", "two"] {
        let c = cell_props(&rtl, text);
        assert!(c.contains(&format!("<a:lnL {stub}")), "{text}: {c}");
        assert!(!c.contains("<a:lnR"), "{text}: {c}");
    }
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
