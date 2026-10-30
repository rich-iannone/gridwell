//! The heading and note paragraphs take their size from their paragraph style
//! (`word/styles.xml`), so a relative font size resolves against that style's
//! size, not the document's 11pt, and an unstyled line writes no size at all.

use gridwell_testkit::{cell, row, styled, text, TableBuilder};
use gridwell_writer_docx::DocxWriter;

/// The `w:sz` of each paragraph with style `id`, in order.
fn sizes(xml: &str, id: &str) -> Vec<Option<u32>> {
    xml.split("<w:p>")
        .filter(|p| p.starts_with(&format!("<w:pPr><w:pStyle w:val=\"{id}\"/>")))
        .map(|p| {
            p.split("<w:sz w:val=\"")
                .nth(1)
                .map(|s| s.split('"').next().unwrap().parse().unwrap())
        })
        .collect()
}

fn table(size: Option<&str>) -> gridwell_ir::Table {
    let mut b = TableBuilder::new(1)
        .title("T")
        .subtitle("S")
        .footnote("a", "1", "F")
        .source_note("N")
        .body(vec![row(vec![cell("x")])]);
    if let Some(size) = size {
        b = b.style_def(
            "s",
            serde_json::from_value(serde_json::json!({ "font_size": size })).unwrap(),
        );
    }
    let mut t = b.build();
    if size.is_some() {
        let s = Some("s".to_string());
        let header = t.header.as_mut().unwrap();
        header.title.as_mut().unwrap().style_id = s.clone();
        header.subtitle.as_mut().unwrap().style_id = s.clone();
        let footer = t.footer.as_mut().unwrap();
        footer.footnotes[0].style_id = s.clone();
        footer.source_notes[0].style_id = s;
    }
    t
}

#[test]
fn relative_sizes_resolve_against_the_paragraph_style() {
    let xml = DocxWriter::new()
        .render_document_xml(&table(Some("150%")))
        .unwrap();
    // Title 16pt, subtitle 12pt, notes 9pt; in half-points, times 1.5.
    assert_eq!(sizes(&xml, "Title"), [Some(48)], "{xml}");
    assert_eq!(sizes(&xml, "Subtitle"), [Some(36)], "{xml}");
    assert_eq!(sizes(&xml, "FootnoteText"), [Some(27)], "{xml}");
    assert_eq!(sizes(&xml, "SourceNote"), [Some(27)], "{xml}");

    // An absolute size is itself.
    let xml = DocxWriter::new()
        .render_document_xml(&table(Some("10pt")))
        .unwrap();
    for id in ["Title", "Subtitle", "FootnoteText", "SourceNote"] {
        assert_eq!(sizes(&xml, id), [Some(20)], "{id}: {xml}");
    }
}

#[test]
fn a_span_in_an_unsized_line_resolves_against_the_paragraph_style() {
    let mut t = table(Some("150%"));
    let header = t.header.as_mut().unwrap();
    let title = header.title.as_mut().unwrap();
    title.style_id = None;
    title.content = vec![text("A "), styled("b", "s")];
    let footer = t.footer.as_mut().unwrap();
    footer.source_notes[0].style_id = None;
    footer.source_notes[0].content = vec![styled("n", "s")];
    let xml = DocxWriter::new().render_document_xml(&t).unwrap();
    // The plain run writes no size; the span is 1.5 times the style's.
    let title = xml.split("Title").nth(1).unwrap();
    assert!(
        title.contains("<w:t xml:space=\"preserve\">A </w:t>"),
        "{title}"
    );
    assert_eq!(sizes(&xml, "Title"), [Some(48)], "{xml}");
    assert_eq!(sizes(&xml, "SourceNote"), [Some(27)], "{xml}");
}

#[test]
fn unstyled_lines_take_their_size_from_the_style() {
    let xml = DocxWriter::new().render_document_xml(&table(None)).unwrap();
    for id in ["Title", "Subtitle", "FootnoteText", "SourceNote"] {
        assert_eq!(sizes(&xml, id), [None], "{id}: {xml}");
    }
}
