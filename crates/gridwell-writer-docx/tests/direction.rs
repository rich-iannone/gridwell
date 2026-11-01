//! A right-to-left table (`bidiVisual`, its paragraphs `bidi`) and the document's
//! language (`w:lang` in the default run properties). Word reads a right-to-left
//! paragraph's `left` and `right` as its start and end, and a `bidiVisual`
//! table's cell borders likewise, so the IR's physical sides are swapped.

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use gridwell_writer_docx::DocxWriter;

fn document(dir: Option<TextDirection>) -> String {
    DocxWriter::new()
        .render_document_xml(&directed(dir, None))
        .unwrap()
}

/// The XML of the cell whose text is `text`, from `<w:tc>` to the text.
fn cell<'x>(xml: &'x str, text: &str) -> &'x str {
    let end = xml
        .find(&format!(">{text}</w:t>"))
        .unwrap_or_else(|| panic!("no cell {text}"));
    &xml[xml[..end].rfind("<w:tc>").unwrap()..end]
}

/// The XML of the paragraph whose text is `text`, from `<w:p>` to the text.
fn paragraph<'x>(xml: &'x str, text: &str) -> &'x str {
    let end = xml.find(&format!(">{text}</w:t>")).unwrap();
    &xml[xml[..end].rfind("<w:p>").unwrap()..end]
}

#[test]
fn a_right_to_left_table_is_bidi_visual_with_bidi_paragraphs() {
    let ltr = document(None);
    assert!(!ltr.contains("bidiVisual") && !ltr.contains("<w:bidi/>"));
    let rtl = document(Some(TextDirection::Rtl));
    let tbl_pr = &rtl[rtl.find("<w:tblPr>").unwrap()..rtl.find("</w:tblPr>").unwrap()];
    // CT_TblPr order: tblStyle, …, bidiVisual, tblW.
    assert!(
        tbl_pr.contains("<w:tblStyle w:val=\"TableGrid\"/><w:bidiVisual/><w:tblW"),
        "{tbl_pr}"
    );
    for text in ["Title", "Name", "one", "2", "Group", "Source"] {
        assert!(
            paragraph(&rtl, text).contains("<w:bidi/>"),
            "{text}: {}",
            paragraph(&rtl, text)
        );
    }
    // `auto` with no right-to-left text is left to right.
    assert!(!document(Some(TextDirection::Auto)).contains("bidiVisual"));
}

#[test]
fn alignment_is_physical_so_its_sides_swap() {
    let ltr = document(None);
    assert!(paragraph(&ltr, "2").contains("<w:jc w:val=\"right\"/>"));
    assert!(!paragraph(&ltr, "one").contains("<w:jc"));
    let rtl = document(Some(TextDirection::Rtl));
    // Right is the end of a right-to-left paragraph; left must be written.
    assert!(
        paragraph(&rtl, "2").contains("<w:jc w:val=\"left\"/>"),
        "{}",
        paragraph(&rtl, "2")
    );
    assert!(paragraph(&rtl, "one").contains("<w:jc w:val=\"right\"/>"));
    // Lines outside the table start where the text does.
    assert!(!paragraph(&rtl, "Title").contains("<w:jc"));
}

#[test]
fn borders_are_physical_so_their_sides_swap() {
    let red = "w:color=\"FF0000\"";
    let ltr = document(None);
    assert!(
        cell(&ltr, "LEFTRED").contains(&format!(
            "<w:left w:val=\"single\" w:sz=\"12\" w:space=\"0\" {red}/>"
        )),
        "{}",
        cell(&ltr, "LEFTRED")
    );
    let rtl = document(Some(TextDirection::Rtl));
    let c = cell(&rtl, "LEFTRED");
    assert!(
        c.contains(&format!(
            "<w:right w:val=\"single\" w:sz=\"12\" w:space=\"0\" {red}/>"
        )),
        "{c}"
    );
    assert!(!c.contains("<w:left "), "{c}");
}

#[test]
fn the_language_is_the_documents() {
    let styles = |lang: Option<&str>| DocxWriter::new().render_styles_xml(&directed(None, lang));
    let lang = |xml: &str| {
        xml.find("<w:lang ")
            .map(|i| xml[i..i + xml[i..].find("/>").unwrap() + 2].to_string())
    };
    assert_eq!(lang(&styles(None)), None);
    assert_eq!(
        lang(&styles(Some("fr-CA"))).unwrap(),
        "<w:lang w:val=\"fr-CA\"/>"
    );
    // Right-to-left text is complex script, East Asian text has its own slot.
    assert_eq!(
        lang(&styles(Some("ar-EG"))).unwrap(),
        "<w:lang w:val=\"ar-EG\" w:bidi=\"ar-EG\"/>"
    );
    assert_eq!(
        lang(&styles(Some("zh-Hant"))).unwrap(),
        "<w:lang w:val=\"zh-Hant\" w:eastAsia=\"zh-Hant\"/>"
    );
    // It is in the default run properties.
    let xml = styles(Some("ar"));
    let defaults = &xml[xml.find("<w:rPrDefault>").unwrap()..xml.find("</w:rPrDefault>").unwrap()];
    assert!(defaults.contains("<w:lang "), "{defaults}");
    // A malformed tag (validation reports it) isn't written.
    assert_eq!(lang(&styles(Some("fr_CA"))), None);
}

#[test]
fn bold_covers_complex_script_text() {
    let xml = document(None);
    // The column labels are bold.
    assert!(
        paragraph(&xml, "Name").contains("<w:b/><w:bCs/>"),
        "{}",
        paragraph(&xml, "Name")
    );
}
