use gridwell_ir::{Table, ValidationRule};
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

// ─── Valid fixtures: must parse and validate cleanly ───

#[test]
fn valid_minimal_1x1() {
    let table = load_fixture("minimal/minimal_1x1.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_minimal_no_header() {
    let table = load_fixture("minimal/minimal_no_header.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_colspan_basic() {
    let table = load_fixture("minimal/colspan_basic.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_rowspan_basic() {
    let table = load_fixture("minimal/rowspan_basic.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_footnote_single() {
    let table = load_fixture("minimal/footnote_single.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_row_group_multiple() {
    let table = load_fixture("minimal/row_group_multiple.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_summary_rows() {
    let table = load_fixture("minimal/summary_rows.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_empty_body() {
    let table = load_fixture("minimal/empty_body.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_styles_borders() {
    let table = load_fixture("minimal/styles_borders.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_content_rich() {
    let table = load_fixture("minimal/content_rich.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_unicode_cjk() {
    let table = load_fixture("minimal/unicode_cjk.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_column_widths() {
    let table = load_fixture("minimal/column_widths.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn valid_comprehensive_reference() {
    let table = load_fixture("comprehensive/reference_table.json");
    let errors = table.validate();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

// ─── Invalid fixtures: must parse but fail validation with the expected rule ───

#[test]
fn invalid_span_overflow_right() {
    let table = load_fixture("invalid/span_overflow_right.json");
    let errors = table.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.rule == ValidationRule::SpanOverflowRight),
        "expected SpanOverflowRight error, got: {errors:#?}"
    );
}

#[test]
fn invalid_span_overflow_bottom() {
    let table = load_fixture("invalid/span_overflow_bottom.json");
    let errors = table.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.rule == ValidationRule::SpanOverflowBottom),
        "expected SpanOverflowBottom error, got: {errors:#?}"
    );
}

#[test]
fn invalid_span_overlap() {
    let table = load_fixture("invalid/span_overlap.json");
    let errors = table.validate();
    assert!(
        errors.iter().any(|e| e.rule == ValidationRule::SpanOverlap),
        "expected SpanOverlap error, got: {errors:#?}"
    );
}

#[test]
fn invalid_footnote_ref_missing() {
    let table = load_fixture("invalid/footnote_ref_missing.json");
    let errors = table.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.rule == ValidationRule::FootnoteRefsValid),
        "expected FootnoteRefsValid error, got: {errors:#?}"
    );
}

#[test]
fn invalid_style_ref_missing() {
    let table = load_fixture("invalid/style_ref_missing.json");
    let errors = table.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.rule == ValidationRule::StyleRefsValid),
        "expected StyleRefsValid error, got: {errors:#?}"
    );
}

#[test]
fn invalid_col_count_mismatch() {
    let table = load_fixture("invalid/col_count_mismatch.json");
    let errors = table.validate();
    assert!(
        errors.iter().any(|e| e.rule == ValidationRule::ColCount),
        "expected ColCount error, got: {errors:#?}"
    );
}

#[test]
fn invalid_placeholder_has_content() {
    let table = load_fixture("invalid/placeholder_has_content.json");
    let errors = table.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.rule == ValidationRule::SpanPlaceholderHasContent),
        "expected SpanPlaceholderHasContent error, got: {errors:#?}"
    );
}

#[test]
fn invalid_summary_no_stub() {
    let table = load_fixture("invalid/summary_no_stub.json");
    let errors = table.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.rule == ValidationRule::SummaryRequiresStub),
        "expected SummaryRequiresStub error, got: {errors:#?}"
    );
}
