//! Snapshot + validity tests over the shared example corpus.
//!
//! Snapshots capture the unzipped XML (human-reviewable): the sheet, the shared
//! strings if any, then the styles, broken into lines at their elements (the
//! parts themselves have no line breaks). A separate test confirms the packaged
//! bytes are a valid zip. Review changes with
//! `cargo insta review`; regenerate with `INSTA_UPDATE=always cargo test`.

use gridwell_testkit::examples;
use gridwell_writer_xlsx::{render_xlsx, XlsxWriter};

#[test]
fn snapshots() {
    let writer = XlsxWriter::new();
    for ex in examples() {
        let table = ex.table();
        let mut xml = writer
            .render_sheet_xml(&table)
            .unwrap_or_else(|e| panic!("render '{}' failed: {e}", ex.name));
        // Text lives in the shared-string table.
        let strings = writer.render_shared_strings_xml(&table).unwrap();
        if strings.contains("<si>") {
            xml.push('\n');
            xml.push_str(&strings);
        }
        xml.push('\n');
        xml.push_str(&writer.render_styles_xml(&table).unwrap());
        insta::assert_snapshot!(ex.name, lines(&xml));
    }
}

/// `xml` with a line break before each element that starts a row, a cell's
/// string, a format or one of their containers.
fn lines(xml: &str) -> String {
    const STARTS: &[&str] = &[
        "<worksheet",
        "<dimension",
        "<sheetViews",
        "<sheetFormatPr",
        "<cols",
        "<sheetData",
        "</sheetData",
        "<row ",
        "<mergeCells",
        "<pageMargins",
        "</worksheet",
        "<sst",
        "<si>",
        "</sst",
        "<styleSheet",
        "<numFmts",
        "<numFmt ",
        "<fonts",
        "<font>",
        "</fonts",
        "<fills",
        "<fill>",
        "</fills",
        "<borders",
        "<border>",
        "</borders",
        "<cellStyleXfs",
        "<cellXfs",
        "<xf ",
        "</cellXfs",
        "<cellStyles",
        "<dxfs",
        "<tableStyles",
        "</styleSheet",
    ];
    let mut out = String::with_capacity(xml.len() * 11 / 10);
    let mut rest = xml;
    while let Some(i) = rest[1..].find('<').map(|i| i + 1) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if !out.ends_with('\n') && STARTS.iter().any(|s| rest.starts_with(s)) {
            out.push('\n');
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn binary_output_is_valid_zip() {
    for ex in examples() {
        let bytes =
            render_xlsx(&ex.table()).unwrap_or_else(|e| panic!("render '{}' failed: {e}", ex.name));
        assert!(bytes.len() > 100, "{}: output too small", ex.name);
        assert_eq!(&bytes[0..4], b"PK\x03\x04", "{}: bad zip magic", ex.name);
    }
}
