//! `quarto_disable_processing` marks the table, and only the table, so Quarto
//! leaves it as written; off (the default) the output has no Quarto attribute.

use gridwell_ir::Table;
use gridwell_testkit::examples;
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};
use scraper::{Html, Selector};

const ATTR: &str = "data-quarto-disable-processing";

fn render(t: &Table, quarto_disable_processing: bool, inline_styles: bool) -> String {
    HtmlWriter::with_config(HtmlWriterConfig {
        quarto_disable_processing,
        inline_styles,
        ..HtmlWriterConfig::default()
    })
    .render(t)
    .unwrap()
}

#[test]
fn the_table_alone_is_marked_and_nothing_else_changes() {
    let marked = Selector::parse(&format!("[{ATTR}]")).unwrap();
    for ex in examples() {
        let t = ex.table();
        for inline in [false, true] {
            let off = render(&t, false, inline);
            let on = render(&t, true, inline);
            assert!(!off.contains("data-quarto"), "{}", ex.name);
            let doc = Html::parse_fragment(&on);
            let hits: Vec<_> = doc.select(&marked).collect();
            assert_eq!(hits.len(), 1, "{}", ex.name);
            assert_eq!(hits[0].value().name(), "table", "{}", ex.name);
            assert_eq!(hits[0].value().attr(ATTR), Some("true"), "{}", ex.name);
            assert_eq!(
                on.replacen(&format!(" {ATTR}=\"true\""), "", 1),
                off,
                "{}",
                ex.name
            );
        }
    }
}

#[test]
fn the_option_is_off_unless_asked_for() {
    assert!(!HtmlWriterConfig::default().quarto_disable_processing);
    let c: HtmlWriterConfig =
        serde_json::from_str(r#"{"quarto_disable_processing": true}"#).unwrap();
    assert!(c.quarto_disable_processing);
}
