//! Snapshot + validity tests over the shared example corpus.
//!
//! Snapshots capture the unzipped XML (human-reviewable); a separate test
//! confirms the packaged bytes are a valid zip. Review changes with
//! `cargo insta review`; regenerate with `INSTA_UPDATE=always cargo test`.

use gridwell_testkit::examples;
use gridwell_writer_docx::{render_docx, DocxWriter};

#[test]
fn snapshots() {
    let writer = DocxWriter::new();
    for ex in examples() {
        let xml = writer
            .render_document_xml(&ex.table())
            .unwrap_or_else(|e| panic!("render '{}' failed: {e}", ex.name));
        insta::assert_snapshot!(ex.name, xml);
    }
}

#[test]
fn binary_output_is_valid_zip() {
    for ex in examples() {
        let bytes =
            render_docx(&ex.table()).unwrap_or_else(|e| panic!("render '{}' failed: {e}", ex.name));
        assert!(bytes.len() > 100, "{}: output too small", ex.name);
        assert_eq!(&bytes[0..4], b"PK\x03\x04", "{}: bad zip magic", ex.name);
    }
}
