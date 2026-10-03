//! Keyword fields: lenient parsing, verbatim round-trip of unknown values, and the
//! UNKNOWN_VALUE validation rule for every field kind.

use gridwell_ir::{HAlign, Keyword, RowRole, Table, ValidationRule};
use serde_json::{json, Value};

/// A valid 2-column table with one style, one composition, one conditional, a header
/// row, a body row and a summary row, so every keyword field has a place to live.
fn base() -> Value {
    let cell = |s: &str| json!({ "content": [{ "type": "text", "value": s }] });
    json!({
        "ir_version": "1.0",
        "config": { "table_cols": 2, "header_rows": 1, "body_rows": 1, "stub_cols": 1,
                    "page_break_mode": "avoid", "container_overflow": "auto" },
        "styles": {
            "defs": { "s": { "font_weight": "bold", "font_style": "italic", "text_align": "right",
                             "vertical_align": "middle", "text_transform": "none",
                             "text_decoration": "underline", "white_space": "nowrap",
                             "word_break": "normal", "overflow": "hidden", "text_overflow": "ellipsis",
                             "border": { "top": { "width": "1px", "style": "solid", "color": "#000" } } } },
            "compositions": { "c": { "extends": "s", "overrides": { "text_align": "center" } } },
            "conditionals": [{ "id": "z", "selector": { "row_parity": "even", "scope": "tbody" },
                               "style": { "font_weight": "normal" } }]
        },
        "column_spec": [{ "id": "a", "align": "left" }, { "id": "b", "align": "right" }],
        "table": {
            "thead": { "rows": [{ "role": "column_label",
                                  "cells": [cell("A"), { "content": [{ "type": "text", "value": "B" }], "scope": "col" }] }] },
            "tbody": [{
                "rows": [{ "cells": [cell("x"), { "content": [{ "type": "text", "value": "1" }],
                                                  "typed_value": { "type": "integer", "value": 1 },
                                                  "data_type": "integer" }] }],
                "summary_rows": [{ "role": "summary_row", "cells": [cell("Total"), cell("1")] }]
            }]
        }
    })
}

fn errors_for(v: &Value) -> Vec<(ValidationRule, String)> {
    let t = Table::from_json(&v.to_string()).expect("lenient parsing never fails on keywords");
    t.validate()
        .into_iter()
        .map(|e| (e.rule, e.message))
        .collect()
}

#[test]
fn base_table_is_valid() {
    assert_eq!(errors_for(&base()), vec![]);
}

#[test]
fn aliases_and_case_variants_validate_cleanly() {
    let mut v = base();
    v["column_spec"][1]["align"] = json!("  RIGHT ");
    v["config"]["page_break_mode"] = json!("Force_Between_Groups");
    v["styles"]["conditionals"][0]["selector"]["scope"] = json!("body");
    v["table"]["tbody"][0]["summary_rows"][0]["role"] = json!("summary");
    v["styles"]["defs"]["s"]["font_weight"] = json!(700);
    assert_eq!(errors_for(&v), vec![]);
}

/// Sets one field of the base table to a bogus value.
type Mutation = Box<dyn Fn(&mut Value)>;

/// Every keyword field: set it to a bogus value, expect exactly one UNKNOWN_VALUE
/// naming the field and the bad value.
#[test]
fn every_keyword_field_reports_unknown_values() {
    let cases: Vec<(&str, Mutation)> = vec![
        (
            "column_spec[1].align",
            Box::new(|v| v["column_spec"][1]["align"] = json!("diagonal")),
        ),
        (
            "config.page_break_mode",
            Box::new(|v| v["config"]["page_break_mode"] = json!("sometimes")),
        ),
        (
            "config.container_overflow",
            Box::new(|v| v["config"]["container_overflow"] = json!("spill")),
        ),
        (
            "styles.defs.s.font_weight",
            Box::new(|v| v["styles"]["defs"]["s"]["font_weight"] = json!("heavy")),
        ),
        (
            "styles.defs.s.font_style",
            Box::new(|v| v["styles"]["defs"]["s"]["font_style"] = json!("slanty")),
        ),
        (
            "styles.defs.s.text_align",
            Box::new(|v| v["styles"]["defs"]["s"]["text_align"] = json!("middle")),
        ),
        (
            "styles.defs.s.vertical_align",
            Box::new(|v| v["styles"]["defs"]["s"]["vertical_align"] = json!("centre")),
        ),
        (
            "styles.defs.s.text_transform",
            Box::new(|v| v["styles"]["defs"]["s"]["text_transform"] = json!("shout")),
        ),
        (
            "styles.defs.s.text_decoration",
            Box::new(|v| v["styles"]["defs"]["s"]["text_decoration"] = json!("wavy")),
        ),
        (
            "styles.defs.s.white_space",
            Box::new(|v| v["styles"]["defs"]["s"]["white_space"] = json!("squash")),
        ),
        (
            "styles.defs.s.word_break",
            Box::new(|v| v["styles"]["defs"]["s"]["word_break"] = json!("never")),
        ),
        (
            "styles.defs.s.overflow",
            Box::new(|v| v["styles"]["defs"]["s"]["overflow"] = json!("spill")),
        ),
        (
            "styles.defs.s.text_overflow",
            Box::new(|v| v["styles"]["defs"]["s"]["text_overflow"] = json!("dots")),
        ),
        (
            "styles.defs.s.border.top.style",
            Box::new(|v| v["styles"]["defs"]["s"]["border"]["top"]["style"] = json!("wiggly")),
        ),
        (
            "styles.compositions.c.overrides.text_align",
            Box::new(|v| v["styles"]["compositions"]["c"]["overrides"]["text_align"] = json!("up")),
        ),
        (
            "styles.conditionals[0].selector.row_parity",
            Box::new(|v| v["styles"]["conditionals"][0]["selector"]["row_parity"] = json!("third")),
        ),
        (
            "styles.conditionals[0].selector.scope",
            Box::new(|v| v["styles"]["conditionals"][0]["selector"]["scope"] = json!("tfoot")),
        ),
        (
            "styles.conditionals[0].style.font_weight",
            Box::new(|v| v["styles"]["conditionals"][0]["style"]["font_weight"] = json!("1001")),
        ),
        (
            "row role",
            Box::new(|v| v["table"]["thead"]["rows"][0]["role"] = json!("header")),
        ),
        (
            "cell scope",
            Box::new(|v| v["table"]["thead"]["rows"][0]["cells"][1]["scope"] = json!("column")),
        ),
        (
            "typed_value.type",
            Box::new(|v| {
                v["table"]["tbody"][0]["rows"][0]["cells"][1]["typed_value"]["type"] =
                    json!("bignum")
            }),
        ),
        (
            "cell data_type",
            Box::new(|v| {
                v["table"]["tbody"][0]["rows"][0]["cells"][1]["data_type"] = json!("bignum")
            }),
        ),
        (
            "row role",
            Box::new(|v| v["table"]["tbody"][0]["summary_rows"][0]["role"] = json!("subtotal")),
        ),
    ];
    for (field, mutate) in cases {
        let mut v = base();
        mutate(&mut v);
        let errs = errors_for(&v);
        assert_eq!(errs.len(), 1, "{field}: {errs:?}");
        let (rule, msg) = &errs[0];
        assert_eq!(*rule, ValidationRule::UnknownValue, "{field}");
        assert!(msg.starts_with(field), "{field}: message was {msg:?}");
        assert!(msg.contains("which is not one of:"), "{msg}");
    }
}

#[test]
fn all_unknown_values_are_reported_together() {
    let mut v = base();
    v["column_spec"][0]["align"] = json!("a");
    v["column_spec"][1]["align"] = json!("b");
    v["styles"]["defs"]["s"]["font_style"] = json!("c");
    let errs = errors_for(&v);
    assert_eq!(
        errs.iter()
            .filter(|(r, _)| *r == ValidationRule::UnknownValue)
            .count(),
        3,
        "{errs:?}"
    );
}

#[test]
fn unknown_values_round_trip_verbatim_and_known_ones_canonically() {
    let mut v = base();
    v["column_spec"][0]["align"] = json!("Diagonal-ish");
    v["column_spec"][1]["align"] = json!("RIGHT");
    v["table"]["tbody"][0]["summary_rows"][0]["role"] = json!("summary");
    let t = Table::from_json(&v.to_string()).unwrap();
    assert_eq!(
        t.column_spec[0].align,
        HAlign::Unknown("Diagonal-ish".into())
    );
    let back: Value = serde_json::from_str(&t.to_json().unwrap()).unwrap();
    assert_eq!(back["column_spec"][0]["align"], "Diagonal-ish");
    assert_eq!(back["column_spec"][1]["align"], "right");
    assert_eq!(
        back["table"]["tbody"][0]["summary_rows"][0]["role"],
        RowRole::Summary.as_str()
    );
}

#[test]
fn unknown_value_message_lists_allowed_values() {
    let mut v = base();
    v["column_spec"][0]["align"] = json!("diagonal");
    let (_, msg) = &errors_for(&v)[0];
    for allowed in HAlign::ALLOWED {
        assert!(msg.contains(allowed), "{msg}");
    }
}

#[test]
fn wrong_json_type_for_a_keyword_is_still_a_parse_error() {
    let mut v = base();
    v["column_spec"][0]["align"] = json!(42);
    assert!(Table::from_json(&v.to_string()).is_err());
}
