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

// ─── Invalid fixtures: one per rule, each producing exactly its `_expect` ───

/// Every invalid fixture with the rule ids it declares in `_expect`.
fn invalid_fixtures() -> Vec<(String, Table, Vec<String>)> {
    let dir = fixtures_dir().join("invalid");
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let json = fs::read_to_string(&path).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
        let expect: Vec<String> = serde_json::from_value(raw["_expect"].clone())
            .unwrap_or_else(|_| panic!("{} has no _expect list", path.display()));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        out.push((name, Table::from_json(&json).unwrap(), expect));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_invalid_fixture_reports_exactly_its_expected_rules() {
    for (name, table, expect) in invalid_fixtures() {
        let mut got: Vec<String> = table
            .validate()
            .iter()
            .map(|e| e.rule.id().to_string())
            .collect();
        got.sort();
        got.dedup();
        let mut want = expect.clone();
        want.sort();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn every_rule_has_an_invalid_fixture() {
    use ValidationRule::*;
    let covered: std::collections::HashSet<String> = invalid_fixtures()
        .into_iter()
        .flat_map(|(_, _, e)| e)
        .collect();
    // Exhaustive: a new rule fails to compile here until it is listed (and then
    // fails at run time until a fixture produces it).
    let all = [
        ColCount,
        RowCount,
        ColspecLength,
        StubContiguous,
        StyleRefsValid,
        FootnoteRefsValid,
        SpanOverflowRight,
        SpanOverflowBottom,
        SpanOverlap,
        SpanGap,
        SpanPlaceholderHasContent,
        SpanPlaceholderMismatch,
        SpanZeroValue,
        SummaryRequiresStub,
        LimitExceeded,
        UnknownValue,
        InvalidColor,
        InvalidLength,
    ];
    for rule in all {
        match rule {
            ColCount
            | RowCount
            | ColspecLength
            | StubContiguous
            | StyleRefsValid
            | FootnoteRefsValid
            | SpanOverflowRight
            | SpanOverflowBottom
            | SpanOverlap
            | SpanPlaceholderHasContent
            | SpanPlaceholderMismatch
            | SpanZeroValue
            | SummaryRequiresStub
            | LimitExceeded
            | UnknownValue
            | InvalidColor
            | InvalidLength => {
                assert!(
                    covered.contains(rule.id()),
                    "no invalid fixture produces {rule}"
                );
            }
            // With valid cell counts every position holds a cell object, so a gap
            // only arises from the span grid on rows that are too short; see
            // `span_gap_is_reported_by_the_span_grid_for_short_rows`.
            SpanGap => assert!(!covered.contains(rule.id())),
        }
    }
}

#[test]
fn span_gap_is_reported_by_the_span_grid_for_short_rows() {
    // Two columns, but the row has one cell object: position (0,1) has no cell.
    let row: gridwell_ir::Row = serde_json::from_value(serde_json::json!({
        "cells": [{ "content": [{ "type": "text", "value": "a" }] }]
    }))
    .unwrap();
    let (_, errors) = gridwell_ir::span::OccupancyGrid::materialize(&[row], 2, "tbody", Some(0));
    let rules: Vec<_> = errors.iter().map(|e| (e.rule, e.col)).collect();
    assert_eq!(rules, vec![(ValidationRule::SpanGap, Some(1))]);
}

#[test]
fn valid_fixtures_and_corpus_have_no_stub_or_placeholder_errors() {
    for dir in ["minimal", "comprehensive"] {
        for entry in fs::read_dir(fixtures_dir().join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let t = Table::from_json(&fs::read_to_string(&path).unwrap()).unwrap();
                assert!(
                    t.validate().is_empty(),
                    "{}: {:?}",
                    path.display(),
                    t.validate()
                );
            }
        }
    }
}
