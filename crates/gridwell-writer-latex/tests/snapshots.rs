use gridwell_ir::Table;
use gridwell_writer_latex::LatexWriter;
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

fn render(path: &str) -> String {
    let table = load_fixture(path);
    LatexWriter::new()
        .render(&table)
        .unwrap_or_else(|e| panic!("Failed to render {path}: {e}"))
}

#[test]
fn snapshot_minimal_1x1() {
    insta::assert_snapshot!(render("minimal/minimal_1x1.json"));
}

#[test]
fn snapshot_minimal_no_header() {
    insta::assert_snapshot!(render("minimal/minimal_no_header.json"));
}

#[test]
fn snapshot_colspan_basic() {
    insta::assert_snapshot!(render("minimal/colspan_basic.json"));
}

#[test]
fn snapshot_rowspan_basic() {
    insta::assert_snapshot!(render("minimal/rowspan_basic.json"));
}

#[test]
fn snapshot_footnote_single() {
    insta::assert_snapshot!(render("minimal/footnote_single.json"));
}

#[test]
fn snapshot_row_group_multiple() {
    insta::assert_snapshot!(render("minimal/row_group_multiple.json"));
}

#[test]
fn snapshot_summary_rows() {
    insta::assert_snapshot!(render("minimal/summary_rows.json"));
}

#[test]
fn snapshot_empty_body() {
    insta::assert_snapshot!(render("minimal/empty_body.json"));
}

#[test]
fn snapshot_styles_borders() {
    insta::assert_snapshot!(render("minimal/styles_borders.json"));
}

#[test]
fn snapshot_content_rich() {
    insta::assert_snapshot!(render("minimal/content_rich.json"));
}

#[test]
fn snapshot_unicode_cjk() {
    insta::assert_snapshot!(render("minimal/unicode_cjk.json"));
}

#[test]
fn snapshot_column_widths() {
    insta::assert_snapshot!(render("minimal/column_widths.json"));
}

#[test]
fn snapshot_comprehensive_reference() {
    insta::assert_snapshot!(render("comprehensive/reference_table.json"));
}
