//! Textual snapshot regression tests over the shared example corpus.
//!
//! One named snapshot per example (`snapshots/snapshots__<name>.snap`), rendered
//! without the base stylesheet so each shows only what the IR itself produces
//! (`tests/base_styles.rs` checks that the default output is exactly this plus the
//! base rules), and one snapshot of a default render (`default-look`) that
//! includes it.
//! Review changes with `cargo insta review`; regenerate with
//! `INSTA_UPDATE=always cargo test`.

use gridwell_testkit::examples;
use gridwell_writer_html::{render_html, HtmlWriter, HtmlWriterConfig};

#[test]
fn snapshots() {
    let writer = HtmlWriter::with_config(HtmlWriterConfig {
        base_styles: false,
        ..HtmlWriterConfig::default()
    });
    for ex in examples() {
        let out = writer
            .render(&ex.table())
            .unwrap_or_else(|e| panic!("render '{}' failed: {e}", ex.name));
        insta::assert_snapshot!(ex.name, out);
    }
}

#[test]
fn default_look() {
    let ex = examples()
        .into_iter()
        .find(|e| e.name == "financial-report")
        .expect("financial-report example");
    insta::assert_snapshot!("default-look", render_html(&ex.table()).unwrap());
}
