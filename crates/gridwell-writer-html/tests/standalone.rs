//! `standalone`: a complete UTF-8 document around exactly the fragment, titled
//! after the table. Off by default.

use gridwell_ir::Table;
use gridwell_testkit::{cell, examples, footnote_mark, row, text, TableBuilder};
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};

fn render(t: &Table, standalone: bool, pretty_print: bool, inline_styles: bool) -> String {
    HtmlWriter::with_config(HtmlWriterConfig {
        standalone,
        pretty_print,
        inline_styles,
        ..HtmlWriterConfig::default()
    })
    .render(t)
    .unwrap()
}

fn titled(title: Vec<gridwell_ir::content::ContentNode>) -> Table {
    let mut t = TableBuilder::new(1)
        .title("x")
        .body(vec![row(vec![cell("1")])])
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().content = title;
    t
}

fn title_of(html: &str) -> String {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("head > title").unwrap();
    doc.select(&sel).next().unwrap().text().collect()
}

#[test]
fn off_by_default() {
    assert!(!HtmlWriterConfig::default().standalone);
    for ex in examples() {
        let html = HtmlWriter::new().render(&ex.table()).unwrap();
        assert!(
            html.starts_with("<div class=\"gw_container\""),
            "{}",
            ex.name
        );
        assert!(!html.contains("<!doctype"), "{}", ex.name);
    }
}

#[test]
fn the_document_wraps_exactly_the_fragment() {
    for ex in examples() {
        let t = ex.table();
        for pretty in [true, false] {
            for inline in [false, true] {
                let fragment = render(&t, false, pretty, inline);
                let doc = render(&t, true, pretty, inline);
                let what = format!("{} pretty={pretty} inline={inline}", ex.name);
                let head = doc.find("<body>").unwrap() + "<body>".len();
                let head_end = if pretty { head + 1 } else { head };
                let tail = if pretty {
                    "</body>\n</html>\n"
                } else {
                    "</body></html>"
                };
                assert!(doc.starts_with("<!doctype html>"), "{what}");
                assert!(doc.ends_with(tail), "{what}");
                assert_eq!(&doc[head_end..doc.len() - tail.len()], fragment, "{what}");
                let prelude = &doc[..head];
                assert!(prelude.contains("<meta charset=\"utf-8\">"), "{what}");
                assert!(prelude.contains("<meta name=\"viewport\""), "{what}");
                assert_eq!(prelude.matches("<title>").count(), 1, "{what}");
                if !pretty {
                    assert!(!doc.contains('\n') || fragment.contains('\n'), "{what}");
                }
            }
        }
    }
}

#[test]
fn the_title_is_the_tables_title_as_text() {
    let t = titled(vec![
        text("Sales  by\n region"),
        footnote_mark("f", "1"),
        gridwell_ir::content::ContentNode::LineBreak {},
        text("2026"),
    ]);
    assert_eq!(
        title_of(&render(&t, true, true, false)),
        "Sales by region 2026"
    );

    // Escaped: markup in a title stays text.
    let t = titled(vec![text("</title><script>alert(1)</script>")]);
    let html = render(&t, true, true, false);
    assert!(!html.contains("<script>"), "{html}");
    assert_eq!(title_of(&html), "</title><script>alert(1)</script>");
}

#[test]
fn the_title_falls_back_to_the_aria_label_then_table() {
    let mut t = TableBuilder::new(1)
        .body(vec![row(vec![cell("1")])])
        .build();
    assert_eq!(title_of(&render(&t, true, true, false)), "Table");
    t.config.aria_label = Some("Quarterly <sales>".into());
    let html = render(&t, true, true, false);
    assert_eq!(title_of(&html), "Quarterly <sales>");
    assert!(
        html.contains("<title>Quarterly &lt;sales&gt;</title>"),
        "{html}"
    );
}

#[test]
fn lang_comes_from_the_locale_when_it_is_a_language_tag() {
    let mut t = TableBuilder::new(1)
        .body(vec![row(vec![cell("1")])])
        .build();
    for (locale, lang) in [
        ("en-US", Some("en-US")),
        ("fr_CA", Some("fr-CA")),
        ("de", Some("de")),
        ("", None),
        ("en\" onload=\"x", None),
        ("1en", None),
    ] {
        t.config.locale = locale.into();
        let html = render(&t, true, true, false);
        let first = html.lines().nth(1).unwrap();
        match lang {
            Some(l) => assert_eq!(first, format!("<html lang=\"{l}\">"), "{locale:?}"),
            None => assert_eq!(first, "<html>", "{locale:?}"),
        }
    }
}
