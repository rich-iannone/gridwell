//! Every format draws a table as this gridwell reads it (`Table::readable`): an
//! unknown node as its fallback, a newer minor version's unknown values left out,
//! a conditional whose selector has an unknown field not at all. What's left out
//! is warned of, after the version and before what a format can't express.

use gridwell_ir::{Ignored, Table};
use gridwell_render::{names, render, warnings, Output, Warning};
use serde_json::{json, Value};

fn table(version: &str, content: Value) -> Value {
    json!({
        "ir_version": version,
        "config": { "table_cols": 2, "stub_cols": 1 },
        "styles": {
            "defs": { "s": { "font_weight": "bold" } },
            "conditionals": [
                { "id": "odd", "selector": { "row_parity": "odd" }, "style": { "color": "#0000FF" } }
            ]
        },
        "column_spec": [{ "id": "a" }, { "id": "b" }],
        "table": {
            "thead": { "rows": [{ "cells": [
                { "content": [{ "type": "text", "value": "A" }] },
                { "content": [{ "type": "text", "value": "B" }] }
            ] }] },
            "tbody": [{ "rows": [
                { "cells": [{ "content": [{ "type": "text", "value": "x" }], "is_stub": true }, { "content": content }] },
                { "cells": [{ "content": [{ "type": "text", "value": "y" }], "is_stub": true }, { "content": [] }] }
            ] }]
        }
    })
}

fn parse(v: &Value) -> Table {
    Table::from_json(&v.to_string()).unwrap()
}

/// Every format's output, by name.
fn outputs(v: &Value) -> Vec<(&'static str, Vec<u8>)> {
    let t = parse(v);
    names()
        .into_iter()
        .map(|f| {
            let out = render(&t, f, None).unwrap_or_else(|e| panic!("{f}: {e}"));
            (f, Output::into_bytes(out))
        })
        .collect()
}

fn assert_same_outputs(a: &Value, b: &Value, what: &str) {
    for ((f, x), (_, y)) in outputs(a).into_iter().zip(outputs(b)) {
        assert!(x == y, "{what}: {f} differs");
    }
}

#[test]
fn every_format_draws_an_unknown_node_as_its_fallback() {
    let fallback = json!([
        { "type": "text", "value": "drawn ", "href": "https://example.com/" },
        { "type": "styled_text", "value": "instead", "style_id": "s" },
        { "type": "line_break" },
        { "type": "raw", "format": "html", "value": "<b>raw</b>" },
        { "type": "sparkline", "fallback": [{ "type": "text", "value": "!" }] },
        { "type": "gauge" }
    ]);
    let with_unknown = table(
        "1.0",
        json!([
            { "type": "text", "value": "a " },
            { "type": "chart", "series": [1, 2, 3], "fallback": fallback },
            { "type": "dial", "fallback": null }
        ]),
    );
    let inline = table(
        "1.0",
        json!([
            { "type": "text", "value": "a " },
            { "type": "text", "value": "drawn ", "href": "https://example.com/" },
            { "type": "styled_text", "value": "instead", "style_id": "s" },
            { "type": "line_break" },
            { "type": "raw", "format": "html", "value": "<b>raw</b>" },
            { "type": "text", "value": "!" }
        ]),
    );
    assert_same_outputs(&with_unknown, &inline, "fallback");
    // Not vacuous: the fallback is drawn.
    let html = render(&parse(&with_unknown), "html", None).unwrap();
    let html = String::from_utf8(Output::into_bytes(html)).unwrap();
    assert!(
        html.contains("instead") && html.contains("<b>raw</b>"),
        "{html}"
    );
    // And it isn't warned of.
    assert_eq!(warnings(&parse(&with_unknown), Some("html")).unwrap(), []);
}

#[test]
fn every_format_draws_a_newer_minor_s_unknown_values_as_unset() {
    let text = json!([{ "type": "text", "value": "v", "href": "tel:+15555550100" }]);
    let mut newer = table("1.3", text);
    newer["config"]["dir"] = json!("sideways");
    newer["config"]["table_width"] = json!("80cqw");
    newer["styles"]["defs"]["s"]["color"] = json!("color(display-p3 1 0 0)");
    newer["column_spec"][1]["align"] = json!("start-edge");
    let mut known = table("1.3", json!([{ "type": "text", "value": "v" }]));
    known["styles"]["defs"]["s"] = json!({ "font_weight": "bold" });
    assert_same_outputs(&newer, &known, "newer values");

    let found = warnings(&parse(&newer), None).unwrap();
    let rules: Vec<&str> = found.iter().map(Warning::rule).collect();
    assert_eq!(rules[0], "NEWER_IR_VERSION");
    assert_eq!(rules[1..6], ["IGNORED_VALUE"; 5], "{rules:?}");
    assert!(
        rules[6..].iter().all(|r| *r == "UNSUPPORTED_FEATURE"),
        "{rules:?}"
    );
    let Warning::Ignored(Ignored::Value { message, .. }) = &found[1] else {
        panic!("{:?}", found[1]);
    };
    assert!(
        message.starts_with("config.table_width is \"80cqw\""),
        "{message}"
    );
    assert_eq!(found[1].as_unsupported(), None);
    assert!(
        found[5]
            .to_string()
            .starts_with("[IGNORED_VALUE] content[0].href of the cell at tbody group 0"),
        "{}",
        found[5]
    );
    assert!(found[5]
        .to_string()
        .ends_with("; the text is drawn without a link"));
}

#[test]
fn every_format_draws_a_conditional_with_an_unknown_selector_field_as_none() {
    let content = json!([{ "type": "text", "value": "v" }]);
    let mut narrowed = table("1.0", content.clone());
    narrowed["styles"]["conditionals"][0]["selector"]["col_parity"] = json!("even");
    let mut none = table("1.0", content);
    none["styles"]["conditionals"] = json!([]);
    assert_same_outputs(&narrowed, &none, "selector");
    let found = warnings(&parse(&narrowed), Some("html")).unwrap();
    assert_eq!(
        found,
        [Warning::Ignored(Ignored::Selector {
            index: 0,
            fields: vec!["col_parity".into()]
        })]
    );
    assert_eq!(
        found[0].to_string(),
        "[IGNORED_SELECTOR] styles.conditionals[0].selector has a field this gridwell \
         doesn't read (\"col_parity\"); the conditional style applies to no row"
    );
    // Not vacuous: the conditional is drawn without the field.
    let plain = table("1.0", json!([{ "type": "text", "value": "v" }]));
    let html = |v: &Value| Output::into_bytes(render(&parse(v), "html", None).unwrap());
    assert_ne!(html(&plain), html(&none));
}
