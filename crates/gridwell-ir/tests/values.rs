//! INVALID_COLOR and INVALID_LENGTH: every colour and length field is checked, with a
//! message naming the field and the value, and valid forms of every kind pass.

use gridwell_ir::{Table, ValidationRule};
use serde_json::{json, Value};

/// A valid table with a value in every colour and length field.
fn base() -> Value {
    let cell = |s: &str| json!({ "content": [{ "type": "text", "value": s }] });
    let image = |w: &str| json!({ "type": "image", "src": "x.png", "width": w, "height": "auto" });
    let style = || {
        json!({
            "color": "#333", "background_color": "rgba(0, 0, 0, 0.1)", "font_size": "12px",
            "indent": "-1em", "min_width": "10%", "max_width": "200px",
            "padding": { "top": "0", "right": "4px", "bottom": "0.5em", "left": "2%" },
            "border": { "bottom": { "width": "2px", "style": "solid", "color": "steelblue" } }
        })
    };
    json!({
        "ir_version": "1.0",
        "config": { "table_cols": 2, "header_rows": 1, "body_rows": 1, "stub_cols": 1,
                    "table_width": "100%", "container_width": "900px", "container_height": "auto" },
        "header": { "title": { "content": [{ "type": "text", "value": "T" }, image("1in")] },
                    "subtitle": { "content": [image("16px")] },
                    "extra_lines": [{ "content": [image("2em")] }] },
        "styles": {
            "defs": { "s": style() },
            "compositions": { "c": { "extends": "s", "overrides": style() } },
            "conditionals": [{ "id": "z", "selector": { "row_parity": "even" }, "style": style() }]
        },
        "column_spec": [
            { "id": "a", "width": "1fr", "min_width": "40px", "max_width": "50%" },
            { "id": "b", "width": "auto" }
        ],
        "table": {
            "thead": { "rows": [{ "cells": [cell("A"), cell("B")] }] },
            "tbody": [{
                "label": { "content": [image("8px")] },
                "rows": [{ "cells": [cell("x"), { "content": [image("20px")] }] }],
                "summary_rows": [{ "cells": [cell("t"), { "content": [image("20px")] }] }]
            }]
        },
        "footer": {
            "footnotes": [{ "id": "f", "mark": "1", "content": [image("1cm")] }],
            "source_notes": [{ "content": [image("5mm")] }]
        }
    })
}

fn errors_for(v: &Value) -> Vec<(ValidationRule, String)> {
    let t = Table::from_json(&v.to_string()).expect("parses");
    t.validate()
        .into_iter()
        .map(|e| (e.rule, e.message))
        .collect()
}

#[test]
fn base_table_is_valid() {
    assert_eq!(errors_for(&base()), vec![]);
}

/// Sets one field of the base table to a value.
type Mutation = Box<dyn Fn(&mut Value, Value)>;

fn at(path: &'static [&'static str]) -> Mutation {
    Box::new(move |v, x| {
        let mut node = v;
        for (i, key) in path.iter().enumerate() {
            node = match key.parse::<usize>() {
                Ok(n) => &mut node[n],
                Err(_) => &mut node[*key],
            };
            if i == path.len() - 1 {
                *node = x.clone();
                return;
            }
        }
    })
}

fn color_fields() -> Vec<(&'static str, Mutation)> {
    vec![
        ("styles.defs.s.color", at(&["styles", "defs", "s", "color"])),
        (
            "styles.defs.s.background_color",
            at(&["styles", "defs", "s", "background_color"]),
        ),
        (
            "styles.defs.s.border.bottom.color",
            at(&["styles", "defs", "s", "border", "bottom", "color"]),
        ),
        (
            "styles.compositions.c.overrides.color",
            at(&["styles", "compositions", "c", "overrides", "color"]),
        ),
        (
            "styles.conditionals[0].style.background_color",
            at(&["styles", "conditionals", "0", "style", "background_color"]),
        ),
    ]
}

/// (field path in messages, setter, accepts negative, accepts %, accepts fr, accepts auto)
type LengthField = (&'static str, Mutation, bool, bool, bool, bool);

fn length_fields() -> Vec<LengthField> {
    vec![
        (
            "config.table_width",
            at(&["config", "table_width"]),
            false,
            true,
            false,
            true,
        ),
        (
            "config.container_width",
            at(&["config", "container_width"]),
            false,
            true,
            false,
            true,
        ),
        (
            "config.container_height",
            at(&["config", "container_height"]),
            false,
            true,
            false,
            true,
        ),
        (
            "column_spec[0].width",
            at(&["column_spec", "0", "width"]),
            false,
            true,
            true,
            true,
        ),
        (
            "column_spec[0].min_width",
            at(&["column_spec", "0", "min_width"]),
            false,
            true,
            false,
            false,
        ),
        (
            "column_spec[0].max_width",
            at(&["column_spec", "0", "max_width"]),
            false,
            true,
            false,
            false,
        ),
        (
            "styles.defs.s.indent",
            at(&["styles", "defs", "s", "indent"]),
            true,
            true,
            false,
            false,
        ),
        (
            "styles.defs.s.min_width",
            at(&["styles", "defs", "s", "min_width"]),
            false,
            true,
            false,
            false,
        ),
        (
            "styles.defs.s.max_width",
            at(&["styles", "defs", "s", "max_width"]),
            false,
            true,
            false,
            false,
        ),
        (
            "styles.defs.s.padding.right",
            at(&["styles", "defs", "s", "padding", "right"]),
            false,
            true,
            false,
            false,
        ),
        (
            "styles.compositions.c.overrides.padding.top",
            at(&["styles", "compositions", "c", "overrides", "padding", "top"]),
            false,
            true,
            false,
            false,
        ),
        (
            "styles.conditionals[0].style.border.bottom.width",
            at(&[
                "styles",
                "conditionals",
                "0",
                "style",
                "border",
                "bottom",
                "width",
            ]),
            false,
            false,
            false,
            false,
        ),
        (
            "header.title.content[1].width",
            at(&["header", "title", "content", "1", "width"]),
            false,
            true,
            false,
            true,
        ),
        (
            "header.subtitle.content[0].height",
            at(&["header", "subtitle", "content", "0", "height"]),
            false,
            true,
            false,
            true,
        ),
        (
            "header.extra_lines[0].content[0].width",
            at(&["header", "extra_lines", "0", "content", "0", "width"]),
            false,
            true,
            false,
            true,
        ),
        (
            "label.content[0].width",
            at(&["table", "tbody", "0", "label", "content", "0", "width"]),
            false,
            true,
            false,
            true,
        ),
        (
            "cell content[0].width",
            at(&[
                "table", "tbody", "0", "rows", "0", "cells", "1", "content", "0", "width",
            ]),
            false,
            true,
            false,
            true,
        ),
        (
            "cell content[0].height",
            at(&[
                "table",
                "tbody",
                "0",
                "summary_rows",
                "0",
                "cells",
                "1",
                "content",
                "0",
                "height",
            ]),
            false,
            true,
            false,
            true,
        ),
        (
            "footer.footnotes[0].content[0].width",
            at(&["footer", "footnotes", "0", "content", "0", "width"]),
            false,
            true,
            false,
            true,
        ),
        (
            "footer.source_notes[0].content[0].width",
            at(&["footer", "source_notes", "0", "content", "0", "width"]),
            false,
            true,
            false,
            true,
        ),
    ]
}

fn expect_one(v: &Value, rule: ValidationRule, field: &str, value: &str) {
    let errs = errors_for(v);
    assert_eq!(errs.len(), 1, "{field} = {value:?}: {errs:#?}");
    let (r, msg) = &errs[0];
    assert_eq!(*r, rule, "{msg}");
    assert!(
        msg.starts_with(&format!("{field} is \"{value}\"")),
        "message should name the field and value: {msg}"
    );
}

#[test]
fn every_color_field_rejects_garbage() {
    for bad in [
        "nope",
        "#12345",
        "#ééé",
        "rgb(1, 2)",
        "red; x: y",
        "",
        "currentColor",
    ] {
        for (field, set) in color_fields() {
            let mut v = base();
            set(&mut v, json!(bad));
            expect_one(&v, ValidationRule::InvalidColor, field, bad);
        }
    }
}

#[test]
fn every_color_field_accepts_every_form() {
    for good in [
        "red",
        "RebeccaPurple",
        "transparent",
        "#abc",
        "#abcd",
        "#aabbcc",
        "#aabbccdd",
        "rgb(1, 2, 3)",
        "rgba(1, 2, 3, 0.5)",
        "rgb(1 2 3 / 50%)",
        "hsl(120 50% 50%)",
        "hsla(120deg, 50%, 50%, 0.5)",
    ] {
        for (field, set) in color_fields() {
            let mut v = base();
            set(&mut v, json!(good));
            assert_eq!(errors_for(&v), vec![], "{field} = {good:?}");
        }
    }
}

#[test]
fn every_length_field_enforces_its_forms() {
    for (field, set, negative, percent, fr, auto) in length_fields() {
        let cases = [
            ("12px", true),
            ("0", true),
            ("1.5em", true),
            ("2REM", true),
            ("1in", true),
            ("3cm", true),
            ("4mm", true),
            ("9pt", true),
            ("-2px", negative),
            ("50%", percent),
            ("2fr", fr),
            ("auto", auto),
            ("12", false),
            ("12 px", false),
            ("NaNpx", false),
            ("infpx", false),
            ("12vw", false),
            ("", false),
            ("1px; color: red", false),
        ];
        for (value, ok) in cases {
            let mut v = base();
            set(&mut v, json!(value));
            if ok {
                assert_eq!(errors_for(&v), vec![], "{field} = {value:?}");
            } else {
                expect_one(&v, ValidationRule::InvalidLength, field, value);
            }
        }
    }
}

#[test]
fn font_size_accepts_keywords_and_lengths() {
    for good in [
        "12px", "1.2em", "90%", "small", "XX-Large", "smaller", "larger", "0",
    ] {
        let mut v = base();
        v["styles"]["defs"]["s"]["font_size"] = json!(good);
        assert_eq!(errors_for(&v), vec![], "{good:?}");
    }
    for bad in ["huge", "-1px", "1fr", "auto", "12", "x small"] {
        let mut v = base();
        v["styles"]["defs"]["s"]["font_size"] = json!(bad);
        expect_one(
            &v,
            ValidationRule::InvalidLength,
            "styles.defs.s.font_size",
            bad,
        );
    }
}

#[test]
fn messages_say_why_and_what_is_expected() {
    let mut v = base();
    v["styles"]["defs"]["s"]["padding"]["left"] = json!("1fr");
    let (_, msg) = &errors_for(&v)[0];
    assert!(msg.contains("fr units are not allowed here"), "{msg}");
    assert!(msg.contains("expected a non-negative length"), "{msg}");
    assert!(
        msg.contains("a percentage") && !msg.contains("auto"),
        "{msg}"
    );

    let mut v = base();
    v["column_spec"][1]["width"] = json!("-5px");
    let (_, msg) = &errors_for(&v)[0];
    assert!(msg.contains("it is negative"), "{msg}");
    assert!(msg.contains("a fraction (fr), or auto"), "{msg}");

    let mut v = base();
    v["styles"]["defs"]["s"]["border"]["bottom"]["width"] = json!("10%");
    let (_, msg) = &errors_for(&v)[0];
    assert!(msg.contains("percentages are not allowed here"), "{msg}");
}

#[test]
fn errors_carry_locations() {
    let mut v = base();
    v["table"]["tbody"][0]["summary_rows"][0]["cells"][1]["content"][0]["width"] = json!("x");
    v["column_spec"][1]["width"] = json!("x");
    let t = Table::from_json(&v.to_string()).unwrap();
    let errs = t.validate();
    assert_eq!(errs.len(), 2, "{errs:#?}");
    let col = errs.iter().find(|e| e.section == "column_spec").unwrap();
    assert_eq!((col.row_group, col.row, col.col), (None, None, Some(1)));
    let cell = errs.iter().find(|e| e.section == "tbody_summary").unwrap();
    assert_eq!(
        (cell.row_group, cell.row, cell.col),
        (Some(0), Some(0), Some(1))
    );
}

#[test]
fn every_bad_value_is_reported_and_order_is_deterministic() {
    let mut v = base();
    for (_, set) in color_fields() {
        set(&mut v, json!("nope"));
    }
    for (_, set, ..) in length_fields() {
        set(&mut v, json!("nope"));
    }
    let a = errors_for(&v);
    assert_eq!(
        a.len(),
        color_fields().len() + length_fields().len(),
        "{a:#?}"
    );
    for _ in 0..5 {
        assert_eq!(errors_for(&v), a);
    }
}
