//! Reading what this gridwell may not know (`Table::readable`): a newer minor IR
//! version's values, content nodes of unknown types (and their fallbacks), and
//! selectors with fields this gridwell doesn't read. Also the required fields IR
//! 1.0 relaxes, and the forms of `string` and `duration` typed values.

use gridwell_ir::{Ignored, Table, ValidationError, ValidationRule};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::BTreeSet;

fn maximal() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../xtask/tests/docs/maximal.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn parse(v: &Value) -> Table {
    Table::from_json(&v.to_string()).expect("parses")
}

fn at(mut v: Value, version: &str) -> Value {
    v["ir_version"] = json!(version);
    v
}

/// Every string leaf of `v`, as a JSON pointer.
fn string_leaves(v: &Value, path: String, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let k = k.replace('~', "~0").replace('/', "~1");
                string_leaves(x, format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                string_leaves(x, format!("{path}/{i}"), out);
            }
        }
        Value::String(_) => out.push(path),
        _ => {}
    }
}

/// Every leaf of `v` (any JSON value but an object or array), as a JSON pointer.
fn leaves(v: &Value, path: String, out: &mut BTreeSet<(String, String)>) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, x) in m {
                leaves(x, format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) if !a.is_empty() => {
            for (i, x) in a.iter().enumerate() {
                leaves(x, format!("{path}/{i}"), out);
            }
        }
        _ => {
            out.insert((path, v.to_string()));
        }
    }
}

/// The JSON pointers at which `a` and `b` differ.
fn differences(a: &Value, b: &Value) -> BTreeSet<String> {
    let (mut x, mut y) = (BTreeSet::new(), BTreeSet::new());
    leaves(a, String::new(), &mut x);
    leaves(b, String::new(), &mut y);
    x.symmetric_difference(&y).map(|(p, _)| p.clone()).collect()
}

fn readable_json(t: &Table) -> Value {
    serde_json::to_value(&*t.readable().table).unwrap()
}

/// Whether a newer minor version may mean the error: an unknown keyword value, a
/// color, length or font size that doesn't parse, or a link of another scheme.
fn lenient(e: &ValidationError) -> bool {
    match e.rule {
        ValidationRule::UnknownValue
        | ValidationRule::InvalidColor
        | ValidationRule::InvalidLength => true,
        ValidationRule::InvalidHref => e.message.contains(gridwell_core::link::OTHER_SCHEME),
        _ => false,
    }
}

/// What a value of the field at `pointer` affects when it's left out: the
/// conditional style its selector picks, the typed value it types, or itself.
fn owner(pointer: &str) -> String {
    if let Some(i) = pointer.find("/selector/") {
        return pointer[..i].to_string();
    }
    if let Some(p) = pointer
        .strip_suffix("/type")
        .filter(|p| p.ends_with("/typed_value"))
    {
        return p.to_string();
    }
    pointer.to_string()
}

#[test]
fn every_value_a_newer_minor_version_may_add_is_ignored_with_the_error_it_would_be() {
    let base = maximal();
    let base_11 = parse(&at(base.clone(), "1.1"));
    assert!(base_11.validate().is_empty());
    let readable_base = readable_json(&base_11);
    let mut leaves = Vec::new();
    string_leaves(&base, String::new(), &mut leaves);
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut ignored_fields = 0;
    for pointer in &leaves {
        if pointer == "/ir_version" {
            continue;
        }
        for bad in ["future-value", "ftp://example.com/x", "3qq"] {
            let mut v = base.clone();
            *v.pointer_mut(pointer).unwrap() = json!(bad);
            let Ok(t10) = Table::from_json(&v.to_string()) else {
                continue;
            };
            let errors = t10.validate();
            let t11 = parse(&at(v, "1.1"));
            let errors_11 = t11.validate();
            // What a newer version can't have added is still an error.
            let kept: Vec<String> = errors
                .iter()
                .filter(|e| !lenient(e))
                .map(|e| e.to_string())
                .collect();
            let got: Vec<String> = errors_11.iter().map(|e| e.to_string()).collect();
            assert_eq!(got, kept, "{pointer} = {bad}");
            let ignorable: Vec<&ValidationError> = errors.iter().filter(|e| lenient(e)).collect();
            if !kept.is_empty() {
                continue;
            }
            let readable = t11.readable();
            match ignorable.as_slice() {
                [] => {
                    assert!(readable.ignored.is_empty(), "{pointer} = {bad}");
                }
                [e] => {
                    seen.insert(e.rule.id());
                    ignored_fields += 1;
                    let [Ignored::Value {
                        rule,
                        message,
                        consequence,
                    }] = &readable.ignored[..]
                    else {
                        panic!("{pointer} = {bad}: {:?}", readable.ignored);
                    };
                    assert_eq!((*rule, message), (e.rule, &e.message), "{pointer} = {bad}");
                    assert!(!consequence.is_empty());
                    assert!(matches!(readable.table, Cow::Owned(_)));
                    // Only that field (or what it picks) is left out.
                    let got = serde_json::to_value(&*readable.table).unwrap();
                    let owner = owner(pointer);
                    // (A list it empties differs too.)
                    for d in differences(&got, &readable_base) {
                        assert!(
                            d == owner
                                || d.starts_with(&format!("{owner}/"))
                                || owner.starts_with(&format!("{d}/")),
                            "{pointer} = {bad}: {d} differs"
                        );
                    }
                    assert!(
                        got.pointer(pointer).is_none_or(|x| x != bad),
                        "{pointer} = {bad}: still there"
                    );
                    // What's left reads with nothing left out.
                    let again = Table::from_json(&serde_json::to_string(&got).unwrap()).unwrap();
                    assert!(again.validate().is_empty(), "{pointer} = {bad}");
                    assert!(again.readable().ignored.is_empty(), "{pointer} = {bad}");
                }
                more => panic!("{pointer} = {bad}: {more:?}"),
            }
        }
    }
    // Not vacuous: each kind is found, and many fields.
    for rule in [
        "UNKNOWN_VALUE",
        "INVALID_COLOR",
        "INVALID_LENGTH",
        "INVALID_HREF",
    ] {
        assert!(seen.contains(rule), "no field gave {rule}");
    }
    assert!(ignored_fields > 150, "{ignored_fields}");
}

#[test]
fn ignored_values_say_what_is_drawn_instead() {
    let mut v = at(maximal(), "1.3");
    v["config"]["dir"] = json!("sideways");
    v["config"]["page_break_mode"] = json!("never");
    v["column_spec"][0]["width"] = json!("10vw");
    v["styles"]["conditionals"][0]["selector"]["scope"] = json!("tfoot");
    v["table"]["tbody"][0]["rows"][0]["cells"][1]["typed_value"]["type"] = json!("money");
    v["header"]["title"]["content"][0]["href"] = json!("tel:+15555550100");
    let t = parse(&v);
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    let shown: Vec<String> = t.readable().ignored.iter().map(|i| i.to_string()).collect();
    assert_eq!(
        shown,
        [
            "[IGNORED_VALUE] config.dir is \"sideways\", which is not one of: ltr, rtl, auto; it is left out",
            "[IGNORED_VALUE] config.page_break_mode is \"never\", which is not one of: avoid, \
             allow, force-between-groups; the default, avoid, is used",
            "[IGNORED_VALUE] column_spec[0].width is \"10vw\", which is not valid here: it does \
             not parse (expected a non-negative length (px, pt, em, rem, in, cm, mm), or a \
             percentage, or a fraction (fr), or auto); the default, auto, is used",
            "[IGNORED_VALUE] styles.conditionals[0].selector.scope is \"tfoot\", which is not one \
             of: tbody, thead, table; the conditional style applies to no row",
            "[IGNORED_VALUE] header.title.content[0].href is \"tel:+15555550100\", which is not a \
             link gridwell writes: its scheme is not http, https or mailto (expected an http, \
             https or mailto URL); the text is drawn without a link",
            "[IGNORED_VALUE] the typed_value.type of the cell at tbody group 0 (row=0, col=1) is \
             \"money\", which is not one of: string, number, integer, boolean, date, datetime, \
             time, duration; the typed value is left out",
        ]
    );
    let r = t.readable();
    let rt = &r.table;
    assert_eq!(rt.config.dir, None);
    assert_eq!(rt.config.page_break_mode, gridwell_ir::PageBreakMode::Avoid);
    assert_eq!(rt.column_spec[0].width, "auto");
    assert!(rt.styles.conditionals.is_empty());
    assert!(rt.table.tbody[0].rows[0].cells[1].typed_value.is_none());
    // The table itself keeps them, and writes them back.
    assert_eq!(
        serde_json::to_value(&t).unwrap()["config"]["dir"],
        json!("sideways")
    );
    // The same values in an IR 1.0 table are errors.
    let t10 = parse(&at(v, "1.0"));
    assert_eq!(t10.validate().len(), 6, "{:?}", t10.validate());
}

#[test]
fn a_table_of_this_version_reads_as_itself() {
    for version in ["1.0", "1.1"] {
        let t = parse(&at(maximal(), version));
        let r = t.readable();
        assert!(r.ignored.is_empty());
        assert!(matches!(r.table, Cow::Borrowed(_)), "{version}");
        let v = t.validated().unwrap();
        assert!(v.ignored().is_empty());
    }
}

#[test]
fn an_older_minor_version_is_read_strictly() {
    // "1.0" and "01.00" are this version; no older 1.x minor exists, but the rule
    // is the same: only a newer minor is lenient.
    for version in ["1.0", "01.00", "1.00"] {
        let mut v = at(maximal(), version);
        v["config"]["dir"] = json!("sideways");
        assert_eq!(parse(&v).validate().len(), 1, "{version}");
    }
}

// ─── Content nodes of unknown types: their fallback ───

/// A one-cell table whose cell holds `content`, with a style `s` and footnote `f`.
fn one_cell(version: &str, content: Value) -> Value {
    json!({
        "ir_version": version,
        "config": { "table_cols": 1 },
        "styles": { "defs": { "s": { "font_weight": "bold" } } },
        "column_spec": [{ "id": "a" }],
        "table": { "thead": { "rows": [] }, "tbody": [{ "rows": [{ "cells": [{ "content": content }] }] }] },
        "footer": { "footnotes": [{ "id": "f", "mark": "1", "content": [] }] }
    })
}

fn cell_content(t: &Table) -> Value {
    serde_json::to_value(&t.table.tbody[0].rows[0].cells[0].content).unwrap()
}

#[test]
fn an_unknown_node_is_read_as_its_fallback() {
    let content = json!([
        { "type": "text", "value": "a" },
        { "type": "chart", "data": [1, 2], "fallback": [
            { "type": "text", "value": "b" },
            { "type": "sparkline", "fallback": [{ "type": "text", "value": "c" }] },
            { "type": "gauge" },
            { "type": "line_break" }
        ] },
        { "type": "badge", "fallback": null },
        { "type": "dial" },
        { "type": "text", "value": "d" }
    ]);
    let t = parse(&one_cell("1.0", content.clone()));
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    // Written back whole (a null fallback too).
    assert_eq!(cell_content(&t), content);
    let r = t.readable();
    assert!(r.ignored.is_empty());
    assert_eq!(
        cell_content(&r.table),
        json!([
            { "type": "text", "value": "a" },
            { "type": "text", "value": "b" },
            { "type": "text", "value": "c" },
            { "type": "line_break" },
            { "type": "text", "value": "d" }
        ])
    );
    let v = t.validated().unwrap();
    assert_eq!(cell_content(v.table()), cell_content(&r.table));
}

#[test]
fn a_fallback_is_validated_as_content() {
    let content = json!([
        { "type": "text", "value": "a" },
        { "type": "chart", "fallback": [
            { "type": "text", "value": "x", "href": "javascript:alert(1)" },
            { "type": "styled_text", "value": "y", "style_id": "nope" },
            { "type": "footnote_mark", "ref": "nope", "mark_text": "2" },
            { "type": "image", "src": "data:,", "width": "-1px" },
            { "type": "dial", "fallback": [{ "type": "text", "value": "z", "href": "" }] }
        ] }
    ]);
    let t = parse(&one_cell("1.0", content));
    let errors: Vec<(ValidationRule, String)> = t
        .validate()
        .into_iter()
        .map(|e| (e.rule, e.message))
        .collect();
    let rules: Vec<ValidationRule> = errors.iter().map(|e| e.0).collect();
    assert_eq!(
        rules,
        [
            ValidationRule::StyleRefsValid,
            ValidationRule::FootnoteRefsValid,
            ValidationRule::InvalidHref,
            ValidationRule::InvalidLength,
            ValidationRule::InvalidHref,
        ],
        "{errors:#?}"
    );
    let messages: Vec<&str> = errors.iter().map(|e| e.1.as_str()).collect();
    let cell = "of the cell at tbody group 0 (row=0, col=0)";
    assert!(
        messages[2].starts_with(&format!(
            "content[1].fallback[0].href {cell} is \"javascript:"
        )),
        "{}",
        messages[2]
    );
    assert!(
        messages[3].starts_with(&format!("content[1].fallback[3].width {cell} is \"-1px\"")),
        "{}",
        messages[3]
    );
    assert!(
        messages[4].starts_with(&format!(
            "content[1].fallback[4].fallback[0].href {cell} is \"\""
        )),
        "{}",
        messages[4]
    );
}

#[test]
fn a_newer_versions_fallback_is_read_leniently_too() {
    let content = json!([{ "type": "chart", "fallback": [
        { "type": "text", "value": "x", "href": "tel:1" },
        { "type": "image", "src": "data:,", "width": "3vw" }
    ] }]);
    let t = parse(&one_cell("1.2", content));
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    let r = t.readable();
    let fields: Vec<String> = r
        .ignored
        .iter()
        .map(|i| i.to_string().split(" is ").next().unwrap().to_string())
        .collect();
    let cell = "of the cell at tbody group 0 (row=0, col=0)";
    assert_eq!(
        fields,
        [
            format!("[IGNORED_VALUE] content[0].fallback[0].href {cell}"),
            format!("[IGNORED_VALUE] content[0].fallback[1].width {cell}"),
        ]
    );
    assert_eq!(
        cell_content(&r.table),
        json!([
            { "type": "text", "value": "x" },
            { "type": "image", "src": "data:,", "alt": null, "width": null, "height": null }
        ])
    );
}

#[test]
fn a_fallback_that_isnt_content_doesnt_parse() {
    for (fallback, why) in [
        (json!("text"), "invalid type: string"),
        (json!({ "type": "text", "value": "a" }), "invalid type: map"),
        (json!([{ "type": "text" }]), "missing field `value`"),
        (json!([{ "value": "a" }]), "missing field `type`"),
    ] {
        let v = one_cell("1.0", json!([{ "type": "chart", "fallback": fallback }]));
        let e = Table::from_json(&v.to_string()).unwrap_err().to_string();
        assert!(
            e.contains("the fallback of a content node of type \"chart\": ") && e.contains(why),
            "{e}"
        );
    }
    // A known node's `fallback` is a field it doesn't have, as before.
    let v = one_cell(
        "1.0",
        json!([{ "type": "text", "value": "a", "fallback": [] }]),
    );
    let t = parse(&v);
    assert_eq!(cell_content(&t), json!([{ "type": "text", "value": "a" }]));
}

// ─── Selectors with fields this gridwell doesn't read ───

fn with_selector(selector: Value) -> Value {
    let mut v = maximal();
    v["styles"]["conditionals"] = json!([
        { "id": "a", "selector": { "row_parity": "odd" }, "style": { "color": "red" } },
        { "id": "b", "selector": selector, "style": { "color": "blue" } }
    ]);
    v
}

#[test]
fn a_selector_with_a_field_this_gridwell_doesnt_read_matches_no_row() {
    let selector = json!({ "row_parity": "even", "col_parity": "odd", "group_id": "g1" });
    let v = with_selector(selector.clone());
    let t = parse(&v);
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    // Written back whole (with the field it leaves out written as null).
    let back = serde_json::to_value(&t).unwrap();
    let mut written = selector.clone();
    written["scope"] = Value::Null;
    assert_eq!(back["styles"]["conditionals"][1]["selector"], written);
    let r = t.readable();
    assert_eq!(
        r.ignored,
        [Ignored::Selector {
            index: 1,
            fields: vec!["col_parity".into(), "group_id".into()]
        }]
    );
    assert_eq!(
        r.ignored[0].to_string(),
        "[IGNORED_SELECTOR] styles.conditionals[1].selector has fields this gridwell doesn't \
         read (\"col_parity\", \"group_id\"); the conditional style applies to no row"
    );
    let ids: Vec<&str> = r
        .table
        .styles
        .conditionals
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(ids, ["a"]);

    let t = parse(&with_selector(json!({ "nth": 3 })));
    assert_eq!(
        t.readable().ignored[0].to_string(),
        "[IGNORED_SELECTOR] styles.conditionals[1].selector has a field this gridwell doesn't \
         read (\"nth\"); the conditional style applies to no row"
    );
    assert_eq!(t.readable().ignored[0].rule(), Ignored::IGNORED_SELECTOR);
}

#[test]
fn a_dropped_conditional_s_style_isnt_reported() {
    // In a newer minor version, its style's unknown values don't matter: it
    // applies to no row.
    let mut v = at(with_selector(json!({ "col_parity": "odd" })), "1.1");
    v["styles"]["conditionals"][1]["style"]["color"] = json!("color(display-p3 1 0 0)");
    let t = parse(&v);
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    assert_eq!(t.readable().ignored.len(), 1);
}

// ─── Required fields IR 1.0 relaxes ───

#[test]
fn styles_content_and_row_counts_may_be_left_out() {
    let v = json!({
        "ir_version": "1.0",
        "config": { "table_cols": 1 },
        "column_spec": [{ "id": "a" }],
        "table": { "thead": { "rows": [{ "cells": [{}] }] }, "tbody": [{ "rows": [{ "cells": [{}] }] }] }
    });
    let t = parse(&v);
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    assert_eq!(t.config.header_rows, None);
    assert_eq!(t.config.body_rows, None);
    assert!(t.styles.defs.is_empty() && t.styles.conditionals.is_empty());
    assert!(t.table.thead.rows[0].cells[0].content.is_empty());
}

#[test]
fn row_counts_are_checked_when_given() {
    let base = json!({
        "ir_version": "1.0",
        "config": { "table_cols": 1 },
        "column_spec": [{ "id": "a" }],
        "table": { "thead": { "rows": [] }, "tbody": [{ "rows": [{ "cells": [{}] }] }] }
    });
    for (field, n, ok) in [
        ("header_rows", 0, true),
        ("header_rows", 1, false),
        ("body_rows", 1, true),
        ("body_rows", 2, false),
        ("body_rows", 0, false),
    ] {
        let mut v = base.clone();
        v["config"][field] = json!(n);
        let errors = parse(&v).validate();
        assert_eq!(errors.is_empty(), ok, "{field} = {n}: {errors:?}");
        if !ok {
            assert_eq!(errors[0].rule, ValidationRule::RowCount);
        }
        v["config"][field] = Value::Null;
        assert!(parse(&v).validate().is_empty(), "{field} = null");
    }
}

#[test]
fn what_stays_required_is_required() {
    let v = maximal();
    for pointer in [
        "/ir_version",
        "/config/table_cols",
        "/column_spec",
        "/column_spec/0/id",
        "/table/thead",
        "/table/tbody",
        "/table/thead/rows/0/cells",
        "/table/tbody/0/rows",
        "/footer/footnotes/0/id",
        "/footer/footnotes/0/mark",
        "/header/title/content/0/value",
    ] {
        let mut v = v.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        let parent = if parent.is_empty() {
            &mut v
        } else {
            v.pointer_mut(parent).unwrap()
        };
        parent.as_object_mut().unwrap().remove(key).unwrap();
        assert!(Table::from_json(&v.to_string()).is_err(), "{pointer}");
    }
}

// ─── Typed values ───

fn typed(value_type: &str, value: Value) -> Vec<ValidationError> {
    let content = json!([{ "type": "text", "value": "x" }]);
    let mut v = one_cell("1.0", content);
    v["table"]["tbody"][0]["rows"][0]["cells"][0]["typed_value"] =
        json!({ "type": value_type, "value": value });
    parse(&v).validate()
}

#[test]
fn a_string_value_is_a_json_string() {
    assert!(typed("string", json!("12")).is_empty());
    assert!(typed("string", json!("")).is_empty());
    for (value, kind) in [
        (json!(12), "a number"),
        (json!(true), "a boolean"),
        (json!(null), "null"),
        (json!(["a"]), "an array"),
        (json!({ "a": 1 }), "an object"),
    ] {
        let errors = typed("string", value);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].rule, ValidationRule::InvalidTypedValue);
        assert_eq!(
            errors[0].message,
            format!(
                "typed_value.value of the cell at tbody group 0 (row=0, col=0) is {kind}, but a \
                 string value is a JSON string"
            )
        );
        assert_eq!((errors[0].row, errors[0].col), (Some(0), Some(0)));
    }
    // Other types aren't checked.
    assert!(typed("number", json!("12")).is_empty());
    assert!(typed("date", json!(12)).is_empty());
}

#[test]
fn a_duration_value_is_an_iso_8601_duration() {
    for good in [
        "PT1H30M",
        "P1D",
        "P1DT12H",
        "PT0S",
        "P0D",
        "PT0.5S",
        "PT1.25S",
        "P1Y2M3W4DT5H6M7.5S",
        "P2W",
        "-PT5M",
        "-P1Y",
        "PT36H",
        "P0000001D",
    ] {
        assert!(typed("duration", json!(good)).is_empty(), "{good}");
    }
    for bad in [
        "", "-", "P", "PT", "P1DT", "T1H", "1:30", "01:30:00", "pt1h", "Pt1H", "PT1h", "PT1.5M",
        "P1.5D", "PT1H1H", "PT30M1H", "P1M1Y", "P1D1W", "PT.5S", "PT1.S", "PT1..5S", "P1H", "PT1D",
        "PT-1H", "P-1D", "--P1D", "+PT1H", "P1D T1H", " PT1H", "PT1H ", "P１D", "PT1,5S", "PTS",
        "PYD", "P1Y2", "P1DTT1H", "P1D-T1H",
    ] {
        let errors = typed("duration", json!(bad));
        assert_eq!(errors.len(), 1, "{bad:?}");
        assert_eq!(errors[0].rule, ValidationRule::InvalidTypedValue);
        assert_eq!(
            errors[0].message,
            format!(
                "typed_value.value of the cell at tbody group 0 (row=0, col=0) is {bad:?}, which \
                 is not an ISO 8601 duration (such as \"PT1H30M\")"
            ),
            "{bad:?}"
        );
    }
    let errors = typed("duration", json!(5400));
    assert_eq!(
        errors[0].message,
        "typed_value.value of the cell at tbody group 0 (row=0, col=0) is a number, but a \
         duration value is an ISO 8601 duration string (such as \"PT1H30M\")"
    );
}

#[test]
fn a_typed_value_s_form_is_checked_in_a_newer_minor_version_too() {
    // `string` and `duration` are IR 1.0 types, so their forms are too.
    let mut v = one_cell("1.4", json!([{ "type": "text", "value": "x" }]));
    v["table"]["tbody"][0]["rows"][0]["cells"][0]["typed_value"] =
        json!({ "type": "duration", "value": "90 minutes" });
    let errors = parse(&v).validate();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].rule, ValidationRule::InvalidTypedValue);
}

#[test]
fn every_rule_is_listed_once_in_order() {
    assert_eq!(
        ValidationRule::ALL.last(),
        Some(&ValidationRule::InvalidTypedValue)
    );
    assert_eq!(
        ValidationRule::InvalidTypedValue.id(),
        "INVALID_TYPED_VALUE"
    );
}
