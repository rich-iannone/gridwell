//! The table's language and direction are a `text` set rule scoped to the table
//! (a content block around it), with the direction always explicit; with a
//! direction the heading lines go on the start side. Alignment stays physical.

use gridwell_ir::TextDirection;
use gridwell_testkit::examples::directed;
use gridwell_writer_typst::render_typst;

fn typst(dir: Option<TextDirection>, lang: Option<&str>) -> String {
    render_typst(&directed(dir, lang)).unwrap()
}

#[test]
fn language_and_direction_are_set_for_the_table_only() {
    let out = typst(Some(TextDirection::Rtl), Some("ar-EG"));
    assert!(
        out.starts_with("#[\n#set text(lang: \"ar\", region: \"EG\", dir: rtl)\n"),
        "{out}"
    );
    assert!(out.ends_with("]\n"), "{out}");
    // `auto` is resolved from the text; the language never sets the direction.
    assert!(typst(Some(TextDirection::Auto), None).starts_with("#[\n#set text(dir: ltr)\n"));
    assert!(typst(None, Some("he")).starts_with("#[\n#set text(lang: \"he\", dir: ltr)\n"));
    // Typst's language is 2–3 letters and its region 2.
    assert!(typst(None, Some("es-419")).starts_with("#[\n#set text(lang: \"es\", dir: ltr)\n"));
    assert!(typst(None, Some("x-private")).starts_with("#[\n#set text(dir: ltr)\n"));
    // Neither: as before, no block.
    let out = typst(None, None);
    assert!(
        !out.contains("#set text(") && !out.starts_with("#["),
        "{out}"
    );
}

#[test]
fn with_a_direction_heading_lines_start_where_the_text_does() {
    assert!(typst(None, None).contains("#align(left)[#text(size: 16pt, weight: \"bold\")[Title]]"));
    for dir in [TextDirection::Rtl, TextDirection::Ltr] {
        let out = typst(Some(dir), None);
        assert!(
            out.contains("#align(start)[#text(size: 16pt, weight: \"bold\")[Title]]"),
            "{out}"
        );
    }
    // Column alignment is physical.
    assert!(typst(Some(TextDirection::Rtl), None).contains("align: (left, right,)"));
}
