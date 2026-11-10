//! Content nodes: a node of a type this gridwell doesn't know parses, and is written
//! back whole; a known node is still checked in full.

use gridwell_ir::{ContentNode, Table};
use serde_json::{json, Value};

/// A one-cell table whose cell holds `content`.
fn table(content: Value) -> Value {
    json!({
        "ir_version": "1.0",
        "config": { "table_cols": 1, "header_rows": 0, "body_rows": 1 },
        "styles": {},
        "column_spec": [{ "id": "a" }],
        "table": { "thead": { "rows": [] }, "tbody": [{ "rows": [{ "cells": [{ "content": content }] }] }] }
    })
}

fn round_trip(v: &Value) -> Value {
    let t = Table::from_json(&v.to_string()).expect("parses");
    serde_json::from_str(&t.to_json().expect("serializes")).unwrap()
}

fn node(v: Value) -> Result<ContentNode, serde_json::Error> {
    serde_json::from_value(v)
}

/// One node of each known type, with every field set.
fn known() -> Vec<Value> {
    vec![
        json!({ "type": "text", "value": "a" }),
        json!({ "type": "styled_text", "value": "b", "style_id": "s" }),
        json!({ "type": "line_break" }),
        json!({ "type": "footnote_mark", "ref": "f", "mark_text": "1" }),
        json!({ "type": "image", "src": "data:,", "alt": "x", "width": "1em", "height": "2em" }),
        json!({ "type": "raw", "format": "html", "value": "<b/>" }),
    ]
}

#[test]
fn an_unknown_node_is_written_back_whole() {
    let future = json!({
        "type": "sparkline",
        "values": [1, 2.5, null],
        "options": { "color": "red", "nested": [{ "deep": true }] },
        "label": "trend"
    });
    let v = table(json!([{ "type": "text", "value": "x" }, future.clone()]));
    let back = round_trip(&v);
    let content = &back["table"]["tbody"][0]["rows"][0]["cells"][0]["content"];
    assert_eq!(content[1], future);
    assert_eq!(content[0], json!({ "type": "text", "value": "x" }));
    // And the parsed node holds the whole object.
    match node(future.clone()).unwrap() {
        ContentNode::Unknown(m) => assert_eq!(Value::Object(m), future),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unknown_node_is_valid() {
    let v = table(json!([{ "type": "sparkline", "values": [1] }]));
    let t = Table::from_json(&v.to_string()).unwrap();
    assert!(t.validate().is_empty(), "{:?}", t.validate());
}

#[test]
fn every_known_type_parses_as_itself_and_round_trips() {
    let nodes = known();
    let mut seen: Vec<String> = nodes
        .iter()
        .map(|n| n["type"].as_str().unwrap().to_string())
        .collect();
    seen.sort();
    let mut types = ContentNode::KNOWN_TYPES.map(String::from).to_vec();
    types.sort();
    assert_eq!(seen, types, "known() covers KNOWN_TYPES");
    for n in nodes {
        let parsed = node(n.clone()).unwrap();
        assert!(!matches!(parsed, ContentNode::Unknown(_)), "{n}");
        assert_eq!(serde_json::to_value(&parsed).unwrap(), n);
    }
}

#[test]
fn every_variant_serializes_to_a_known_type() {
    // Exhaustive: a new variant fails to compile here until it's listed, and then
    // fails the test until its type is in KNOWN_TYPES.
    let s = || String::from("x");
    let all = [
        ContentNode::Text {
            value: s(),
            href: None,
        },
        ContentNode::StyledText {
            value: s(),
            style_id: None,
            href: None,
        },
        ContentNode::LineBreak {},
        ContentNode::FootnoteMark {
            reference: s(),
            mark_text: s(),
        },
        ContentNode::Image {
            src: s(),
            alt: None,
            width: None,
            height: None,
        },
        ContentNode::Raw {
            format: s(),
            value: s(),
        },
    ];
    for n in &all {
        match n {
            ContentNode::Text { .. }
            | ContentNode::StyledText { .. }
            | ContentNode::LineBreak {}
            | ContentNode::FootnoteMark { .. }
            | ContentNode::Image { .. }
            | ContentNode::Raw { .. }
            | ContentNode::Unknown(_) => {}
        }
        let v = serde_json::to_value(n).unwrap();
        let t = v["type"].as_str().unwrap();
        assert!(ContentNode::KNOWN_TYPES.contains(&t), "{t}");
    }
    assert_eq!(all.len(), ContentNode::KNOWN_TYPES.len());
}

#[test]
fn a_known_node_is_still_checked_in_full() {
    let bad = [
        (json!({ "type": "text" }), "missing field `value`"),
        (
            json!({ "type": "footnote_mark", "ref": "f" }),
            "missing field `mark_text`",
        ),
        (json!({ "type": "image" }), "missing field `src`"),
        (json!({ "type": "text", "value": 1 }), "invalid type"),
    ];
    for (n, want) in bad {
        let err = node(n.clone()).unwrap_err().to_string();
        assert!(err.contains(want), "{n}: {err}");
        // Through a whole table too.
        let err = Table::from_json(&table(json!([n.clone()])).to_string()).unwrap_err();
        assert!(err.to_string().contains(want), "{n}: {err}");
    }
}

#[test]
fn a_node_needs_a_string_type() {
    let err = node(json!({ "value": "a" })).unwrap_err().to_string();
    assert!(err.contains("missing field `type`"), "{err}");
    for t in [json!(1), json!(null), json!(["text"]), json!({ "a": 1 })] {
        let err = node(json!({ "type": t, "value": "a" }))
            .unwrap_err()
            .to_string();
        assert!(err.contains("`type` must be a string"), "{t}: {err}");
    }
    let err = node(json!("text")).unwrap_err().to_string();
    assert!(err.contains("invalid type"), "{err}");
}
