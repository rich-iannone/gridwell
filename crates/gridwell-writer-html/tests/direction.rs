//! `config.lang` and `config.dir`: the container's `lang` and `dir`, the
//! standalone document's language, and, in a right-to-left or `auto` table, every
//! cell's alignment written out (so a left-aligned cell stays on the left) and
//! group labels on the start side.

use gridwell_ir::{Table, TextDirection};
use gridwell_testkit::examples::directed;
use gridwell_writer_html::{render_html, HtmlWriter, HtmlWriterConfig};

fn container(html: &str) -> &str {
    &html[..html.find('>').unwrap()]
}

/// The opening tag of the element whose text is `text`.
fn tag_of<'h>(html: &'h str, text: &str) -> &'h str {
    let end = html.find(&format!(">{text}<")).unwrap();
    &html[html[..end].rfind('<').unwrap()..end]
}

fn class_of<'h>(html: &'h str, text: &str) -> &'h str {
    let tag = tag_of(html, text);
    let at = tag.find("class=\"").unwrap() + 7;
    &tag[at..at + tag[at..].find('"').unwrap()]
}

fn group_label_class(html: &str) -> &str {
    let at = html.find("gw_group_label").unwrap();
    &html[at..at + html[at..].find('"').unwrap()]
}

#[test]
fn the_container_has_the_language_and_direction() {
    let html = render_html(&directed(Some(TextDirection::Rtl), Some("ar"))).unwrap();
    assert_eq!(
        container(&html),
        r#"<div class="gw_container" lang="ar" dir="rtl""#
    );
    let html = render_html(&directed(Some(TextDirection::Auto), None)).unwrap();
    assert_eq!(container(&html), r#"<div class="gw_container" dir="auto""#);
    let html = render_html(&directed(Some(TextDirection::Ltr), Some("fr-CA"))).unwrap();
    assert_eq!(
        container(&html),
        r#"<div class="gw_container" lang="fr-CA" dir="ltr""#
    );
    let html = render_html(&directed(None, None)).unwrap();
    assert_eq!(container(&html), r#"<div class="gw_container""#);
}

#[test]
fn the_language_is_escaped() {
    // Not a language tag (validation says so), but written safely regardless.
    let html = render_html(&directed(None, Some("a\"b<c"))).unwrap();
    assert!(
        container(&html).contains(r#"lang="a&quot;b&lt;c""#),
        "{}",
        container(&html)
    );
}

#[test]
fn a_standalone_document_is_in_the_tables_language() {
    let standalone = |t: &Table| {
        HtmlWriter::with_config(HtmlWriterConfig {
            standalone: true,
            ..HtmlWriterConfig::default()
        })
        .render(t)
        .unwrap()
    };
    let html = standalone(&directed(Some(TextDirection::Rtl), Some("ar")));
    assert!(html.contains("<html lang=\"ar\">"), "{html}");
    // Without a language, the locale stands in.
    let mut t = directed(None, None);
    t.config.locale = "de_CH".into();
    assert!(standalone(&t).contains("<html lang=\"de-CH\">"));
    let html = standalone(&directed(None, Some("a\"b")));
    assert!(html.contains("<html lang=\"a&quot;b\">"), "{html}");
}

/// `directed`, with its `Value` column left-aligned.
fn left_values(dir: Option<TextDirection>) -> Table {
    let mut t = directed(dir, None);
    t.column_spec[1].align = "left".into();
    t
}

#[test]
fn with_a_direction_left_alignment_is_written_out() {
    // Left is the default, so it isn't written for a body cell; right is.
    let ltr = render_html(&directed(None, None)).unwrap();
    assert_eq!(class_of(&ltr, "2"), "gw__al_right");
    // Left-to-right, by default or as set, is as before.
    for dir in [None, Some(TextDirection::Ltr)] {
        let html = render_html(&left_values(dir)).unwrap();
        assert!(
            !tag_of(&html, "2").contains("class="),
            "{}",
            tag_of(&html, "2")
        );
        assert!(group_label_class(&html).contains("gw__al_left"));
    }
    for dir in [TextDirection::Rtl, TextDirection::Auto] {
        let html = render_html(&directed(Some(dir.clone()), None)).unwrap();
        assert_eq!(class_of(&html, "2"), "gw__al_right");
        let html = render_html(&left_values(Some(dir))).unwrap();
        assert_eq!(class_of(&html, "2"), "gw__al_left");
        // A group label goes where the text starts.
        assert!(group_label_class(&html).contains("gw__al_start"), "{html}");
        assert!(
            html.contains(".gw__al_start { text-align: start }"),
            "{html}"
        );
    }
}
