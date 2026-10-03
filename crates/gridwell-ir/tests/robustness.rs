//! Validator robustness: hostile or malformed IR must produce errors, never panics,
//! overflow, or unbounded allocation.

use gridwell_ir::validation::{validate_with_limits, Limits};
use gridwell_ir::{Table, ValidationRule};
use serde_json::{json, Value};

fn text_cell(s: &str) -> Value {
    json!({ "content": [{ "type": "text", "value": s }] })
}

fn span_cell(s: &str, colspan: u64, rowspan: u64) -> Value {
    json!({
        "content": [{ "type": "text", "value": s }],
        "colspan": colspan,
        "rowspan": rowspan
    })
}

fn placeholder() -> Value {
    json!({ "content": [], "is_placeholder": true })
}

/// Build a table JSON with one body group. `table_cols` is taken as-is so tests can
/// declare values that disagree with the actual rows.
fn table_json(table_cols: u64, thead: Vec<Vec<Value>>, body: Vec<Vec<Value>>) -> Value {
    table_json_with_summary(table_cols, 0, thead, body, vec![])
}

fn table_json_with_summary(
    table_cols: u64,
    stub_cols: u64,
    thead: Vec<Vec<Value>>,
    body: Vec<Vec<Value>>,
    summary: Vec<Vec<Value>>,
) -> Value {
    let spec_len = thead.first().or(body.first()).map(|r| r.len()).unwrap_or(0);
    let column_spec: Vec<Value> = (0..spec_len)
        .map(|i| json!({ "id": format!("c{i}") }))
        .collect();
    let row = |cells: Vec<Value>| json!({ "cells": cells });
    json!({
        "ir_version": "1.0",
        "config": {
            "table_cols": table_cols,
            "header_rows": thead.len(),
            "body_rows": body.len(),
            "stub_cols": stub_cols
        },
        "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
        "column_spec": column_spec,
        "table": {
            "thead": { "rows": thead.into_iter().map(row).collect::<Vec<_>>() },
            "tbody": [{
                "rows": body.into_iter().map(row).collect::<Vec<_>>(),
                "summary_rows": summary.into_iter().map(row).collect::<Vec<_>>()
            }]
        }
    })
}

fn parse(v: Value) -> Table {
    Table::from_json(&v.to_string()).expect("test IR should parse")
}

fn rules(table: &Table) -> Vec<ValidationRule> {
    table.validate().into_iter().map(|e| e.rule).collect()
}

// ─── Integer overflow in span arithmetic ───

#[test]
fn colspan_u32_max_reports_overflow_without_panicking() {
    let t = parse(table_json(
        2,
        vec![],
        vec![vec![text_cell("a"), span_cell("b", u32::MAX as u64, 1)]],
    ));
    let r = rules(&t);
    assert!(r.contains(&ValidationRule::SpanOverflowRight), "{r:?}");
}

#[test]
fn rowspan_u32_max_reports_overflow_without_panicking() {
    let t = parse(table_json(
        1,
        vec![],
        vec![
            vec![text_cell("a")],
            vec![span_cell("b", 1, u32::MAX as u64)],
        ],
    ));
    let r = rules(&t);
    assert!(r.contains(&ValidationRule::SpanOverflowBottom), "{r:?}");
}

#[test]
fn overflow_message_does_not_wrap() {
    let t = parse(table_json(
        2,
        vec![],
        vec![vec![text_cell("a"), span_cell("b", u32::MAX as u64, 1)]],
    ));
    let err = t
        .validate()
        .into_iter()
        .find(|e| e.rule == ValidationRule::SpanOverflowRight)
        .expect("overflow error");
    // col 1 + colspan u32::MAX - 1 == 4294967295 as a true (non-wrapped) value.
    assert!(err.message.contains("4294967295"), "{}", err.message);
}

#[test]
fn colspan_above_u32_is_a_parse_error_not_a_panic() {
    let v = table_json(1, vec![], vec![vec![span_cell("a", u64::MAX, 1)]]);
    assert!(Table::from_json(&v.to_string()).is_err());
}

// ─── Unbounded allocation from declared dimensions ───

#[test]
fn huge_declared_table_cols_does_not_allocate_a_huge_grid() {
    // One real cell, but table_cols claims u32::MAX. Allocating the occupancy grid
    // for that would need tens of GB; the validator must refuse instead.
    let t = parse(table_json(
        u32::MAX as u64,
        vec![],
        vec![vec![text_cell("a")]],
    ));
    let r = rules(&t);
    assert!(r.contains(&ValidationRule::LimitExceeded), "{r:?}");
}

#[test]
fn declared_cols_disagreeing_with_rows_skips_span_grid() {
    // Rows have 1 cell, config says 3. COL_COUNT reports it; the span grid would only
    // add noise (gaps) on a structurally broken section, so it is skipped.
    let t = parse(table_json(3, vec![], vec![vec![text_cell("a")]]));
    let r = rules(&t);
    assert!(r.contains(&ValidationRule::ColCount), "{r:?}");
    assert!(!r.contains(&ValidationRule::SpanGap), "{r:?}");
}

#[test]
fn table_cols_over_default_limit_is_rejected() {
    let n = Limits::default().max_table_cols as usize + 1;
    let cells: Vec<Value> = (0..n).map(|_| text_cell("x")).collect();
    let t = parse(table_json(n as u64, vec![], vec![cells]));
    let errs = t.validate();
    let limit = errs
        .iter()
        .find(|e| e.rule == ValidationRule::LimitExceeded)
        .unwrap_or_else(|| panic!("expected LIMIT_EXCEEDED, got {errs:?}"));
    assert!(limit.message.contains("table_cols"), "{}", limit.message);
}

#[test]
fn table_cols_at_default_limit_is_accepted() {
    let n = Limits::default().max_table_cols as usize;
    let cells: Vec<Value> = (0..n).map(|_| text_cell("x")).collect();
    let t = parse(table_json(n as u64, vec![], vec![cells]));
    let errs = t.validate();
    assert!(errs.is_empty(), "{:?}", &errs[..errs.len().min(3)]);
}

#[test]
fn rows_per_section_limit_is_enforced() {
    let limits = Limits {
        max_rows_per_section: 3,
        ..Limits::default()
    };
    let body: Vec<Vec<Value>> = (0..4).map(|_| vec![text_cell("x")]).collect();
    let t = parse(table_json(1, vec![], body));
    let r: Vec<_> = validate_with_limits(&t, &limits)
        .into_iter()
        .map(|e| e.rule)
        .collect();
    assert_eq!(r, vec![ValidationRule::LimitExceeded]);

    let body: Vec<Vec<Value>> = (0..3).map(|_| vec![text_cell("x")]).collect();
    let t = parse(table_json(1, vec![], body));
    assert!(validate_with_limits(&t, &limits).is_empty());
}

#[test]
fn total_cells_limit_is_enforced() {
    let limits = Limits {
        max_total_cells: 5,
        ..Limits::default()
    };
    let body: Vec<Vec<Value>> = (0..3)
        .map(|_| vec![text_cell("x"), text_cell("y")])
        .collect();
    let t = parse(table_json(2, vec![], body));
    let r: Vec<_> = validate_with_limits(&t, &limits)
        .into_iter()
        .map(|e| e.rule)
        .collect();
    assert_eq!(r, vec![ValidationRule::LimitExceeded]);
}

#[test]
fn zero_table_cols_with_no_rows_is_reported_not_panicking() {
    let t = parse(table_json(0, vec![], vec![]));
    // An empty table is structurally consistent; just make sure nothing panics.
    let _ = t.validate();
}

// ─── Summary rows get the same span checks as data rows ───

#[test]
fn summary_row_span_overflow_is_reported() {
    let t = parse(table_json_with_summary(
        2,
        1,
        vec![],
        vec![vec![text_cell("a"), text_cell("b")]],
        vec![vec![text_cell("Total"), span_cell("x", 2, 1)]],
    ));
    let errs = t.validate();
    assert!(
        errs.iter()
            .any(|e| e.rule == ValidationRule::SpanOverflowRight && e.section == "tbody_summary"),
        "{errs:?}"
    );
}

#[test]
fn summary_row_gap_is_reported() {
    // A placeholder with nothing spanning over it leaves an unowned grid position.
    let t = parse(table_json_with_summary(
        2,
        1,
        vec![],
        vec![vec![text_cell("a"), text_cell("b")]],
        vec![vec![text_cell("Total"), placeholder()]],
    ));
    let r = rules(&t);
    assert!(r.contains(&ValidationRule::SpanGap), "{r:?}");
}

#[test]
fn summary_row_placeholder_content_is_reported() {
    let mut ph = placeholder();
    ph["content"] = json!([{ "type": "text", "value": "oops" }]);
    let t = parse(table_json_with_summary(
        2,
        1,
        vec![],
        vec![vec![text_cell("a"), text_cell("b")]],
        vec![vec![span_cell("Total", 2, 1), ph]],
    ));
    let errs = t.validate();
    assert!(
        errs.iter()
            .any(|e| e.rule == ValidationRule::SpanPlaceholderHasContent
                && e.section == "tbody_summary"),
        "{errs:?}"
    );
}

#[test]
fn valid_summary_row_with_colspan_passes() {
    let t = parse(table_json_with_summary(
        3,
        1,
        vec![],
        vec![vec![text_cell("a"), text_cell("b"), text_cell("c")]],
        vec![vec![
            text_cell("Total"),
            span_cell("x", 2, 1),
            placeholder(),
        ]],
    ));
    let errs = t.validate();
    assert!(errs.is_empty(), "{errs:?}");
}

// ─── Rule identifiers serialize as documented ───

#[test]
fn limit_exceeded_serializes_screaming_snake() {
    let s = serde_json::to_string(&ValidationRule::LimitExceeded).unwrap();
    assert_eq!(s, "\"LIMIT_EXCEEDED\"");
}

// ─── ensure_valid / display ───

#[test]
fn every_rule_displays_as_its_serialized_id() {
    use ValidationRule::*;
    for rule in [
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
    ] {
        // Exhaustive: a new rule fails to compile here until it's added above.
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
            | SpanGap
            | SpanPlaceholderHasContent
            | SpanPlaceholderMismatch
            | SpanZeroValue
            | SummaryRequiresStub
            | LimitExceeded
            | UnknownValue
            | InvalidColor
            | InvalidLength => {}
        }
        let serialized = serde_json::to_string(&rule).unwrap();
        assert_eq!(format!("\"{rule}\""), serialized);
    }
}

#[test]
fn ensure_valid_ok_for_valid_table() {
    let t = parse(table_json(1, vec![], vec![vec![text_cell("a")]]));
    assert!(t.ensure_valid().is_ok());
}

#[test]
fn ensure_valid_lists_errors_and_truncates() {
    // 12 rows with the wrong cell count → 12 COL_COUNT errors (+ ROW_COUNT is fine).
    let body: Vec<Vec<Value>> = (0..12).map(|_| vec![text_cell("a")]).collect();
    let mut v = table_json(1, vec![], body);
    v["config"]["table_cols"] = json!(2);
    v["column_spec"] = json!([{ "id": "a" }, { "id": "b" }]);
    let err = parse(v).ensure_valid().unwrap_err();
    assert_eq!(err.errors.len(), 12);
    let msg = err.to_string();
    assert!(
        msg.starts_with("table IR failed validation with 12 errors"),
        "{msg}"
    );
    assert_eq!(msg.matches("[COL_COUNT]").count(), 10, "{msg}");
    assert!(msg.ends_with("… and 2 more"), "{msg}");
}

#[test]
fn footnote_refs_are_checked_everywhere_content_appears() {
    let mark = || json!({ "type": "footnote_mark", "ref": "nope", "mark_text": "*" });
    let base = || {
        json!({
            "ir_version": "1.0",
            "config": { "table_cols": 1, "header_rows": 1, "body_rows": 1, "stub_cols": 1 },
            "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
            "header": { "title": { "content": [] }, "subtitle": { "content": [] },
                        "extra_lines": [{ "content": [] }] },
            "column_spec": [{ "id": "a" }],
            "table": {
                "thead": { "rows": [{ "cells": [{ "content": [] }] }] },
                "tbody": [{ "label": { "content": [] },
                            "rows": [{ "cells": [{ "content": [] }] }],
                            "summary_rows": [{ "cells": [{ "content": [] }] }] }]
            },
            "footer": { "footnotes": [{ "id": "f", "mark": "1", "content": [] }],
                        "source_notes": [{ "content": [] }] }
        })
    };
    assert!(parse(base()).validate().is_empty());
    let places: Vec<(&str, Vec<&str>)> = vec![
        ("title", vec!["header", "title", "content"]),
        ("subtitle", vec!["header", "subtitle", "content"]),
        ("extra line", vec!["header", "extra_lines", "0", "content"]),
        (
            "head cell",
            vec!["table", "thead", "rows", "0", "cells", "0", "content"],
        ),
        ("label", vec!["table", "tbody", "0", "label", "content"]),
        (
            "body cell",
            vec!["table", "tbody", "0", "rows", "0", "cells", "0", "content"],
        ),
        (
            "summary cell",
            vec![
                "table",
                "tbody",
                "0",
                "summary_rows",
                "0",
                "cells",
                "0",
                "content",
            ],
        ),
        ("footnote", vec!["footer", "footnotes", "0", "content"]),
        (
            "source note",
            vec!["footer", "source_notes", "0", "content"],
        ),
    ];
    for (what, path) in places {
        let mut v = base();
        let mut node = &mut v;
        for key in &path {
            node = match key.parse::<usize>() {
                Ok(i) => &mut node[i],
                Err(_) => &mut node[*key],
            };
        }
        node.as_array_mut().unwrap().push(mark());
        let rules: Vec<_> = parse(v).validate().into_iter().map(|e| e.rule).collect();
        assert_eq!(rules, vec![ValidationRule::FootnoteRefsValid], "{what}");
    }
}
