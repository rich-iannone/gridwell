use gridwell_ir::Table;
use gridwell_writer_docx::DocxWriter;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
}

fn load_fixture(path: &str) -> Table {
    let full_path = fixtures_dir().join(path);
    let json = fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", full_path.display()));
    Table::from_json(&json)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", full_path.display()))
}

fn render_xml(path: &str) -> String {
    let table = load_fixture(path);
    DocxWriter::new()
        .render_document_xml(&table)
        .unwrap_or_else(|e| panic!("Failed to render {path}: {e}"))
}

fn render_bytes(path: &str) -> Vec<u8> {
    let table = load_fixture(path);
    DocxWriter::new()
        .render(&table)
        .unwrap_or_else(|e| panic!("Failed to render {path}: {e}"))
}

// ─── XML snapshot tests ───

#[test]
fn snapshot_minimal_1x1() {
    insta::assert_snapshot!(render_xml("minimal/minimal_1x1.json"));
}

#[test]
fn snapshot_minimal_no_header() {
    insta::assert_snapshot!(render_xml("minimal/minimal_no_header.json"));
}

#[test]
fn snapshot_colspan_basic() {
    insta::assert_snapshot!(render_xml("minimal/colspan_basic.json"));
}

#[test]
fn snapshot_rowspan_basic() {
    insta::assert_snapshot!(render_xml("minimal/rowspan_basic.json"));
}

#[test]
fn snapshot_footnote_single() {
    insta::assert_snapshot!(render_xml("minimal/footnote_single.json"));
}

#[test]
fn snapshot_row_group_multiple() {
    insta::assert_snapshot!(render_xml("minimal/row_group_multiple.json"));
}

#[test]
fn snapshot_summary_rows() {
    insta::assert_snapshot!(render_xml("minimal/summary_rows.json"));
}

#[test]
fn snapshot_empty_body() {
    insta::assert_snapshot!(render_xml("minimal/empty_body.json"));
}

#[test]
fn snapshot_styles_borders() {
    insta::assert_snapshot!(render_xml("minimal/styles_borders.json"));
}

#[test]
fn snapshot_content_rich() {
    insta::assert_snapshot!(render_xml("minimal/content_rich.json"));
}

#[test]
fn snapshot_unicode_cjk() {
    insta::assert_snapshot!(render_xml("minimal/unicode_cjk.json"));
}

#[test]
fn snapshot_column_widths() {
    insta::assert_snapshot!(render_xml("minimal/column_widths.json"));
}

#[test]
fn snapshot_comprehensive_reference() {
    insta::assert_snapshot!(render_xml("comprehensive/reference_table.json"));
}

// ─── Binary output tests (verify zip is valid) ───

#[test]
fn binary_output_is_valid_zip() {
    let bytes = render_bytes("minimal/minimal_1x1.json");
    assert!(bytes.len() > 100, "docx output too small");
    // Check ZIP magic bytes
    assert_eq!(&bytes[0..4], b"PK\x03\x04");
}
