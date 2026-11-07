//! Messages say where: an error about a row or a cell names its section (`thead`,
//! `tbody group 1`, `tbody group 1 summary`), and the location fields agree. Also
//! DUPLICATE_ID, which reports each repeat once.

use gridwell_ir::{Table, ValidationError, ValidationRule};
use serde_json::{json, Value};

/// A valid table: two columns, a stub, a head row, two groups (the second with a
/// summary row) and a footnote.
fn base() -> Value {
    let cell = |s: &str| json!({ "content": [{ "type": "text", "value": s }] });
    let rows = |n: usize| -> Value {
        (0..n)
            .map(|i| json!({ "cells": [cell(&format!("r{i}")), cell("1")] }))
            .collect()
    };
    json!({
        "ir_version": "1.0",
        "config": { "table_cols": 2, "header_rows": 1, "body_rows": 4, "stub_cols": 1 },
        "styles": { "defs": { "s": { "font_weight": "bold" } } },
        "column_spec": [{ "id": "a" }, { "id": "b" }],
        "table": {
            "thead": { "rows": rows(1) },
            "tbody": [
                { "rows": rows(2) },
                { "rows": rows(2), "summary_rows": rows(2) }
            ]
        },
        "footer": { "footnotes": [{ "id": "f", "mark": "1", "content": [] }] }
    })
}

fn errors(v: &Value) -> Vec<ValidationError> {
    Table::from_json(&v.to_string()).expect("parses").validate()
}

/// The three kinds of section, as (pointer to its rows, the name messages use,
/// section, row group).
const SECTIONS: [(&str, &str, &str, Option<u32>); 3] = [
    ("/table/thead/rows", "thead", "thead", None),
    ("/table/tbody/1/rows", "tbody group 1", "tbody", Some(1)),
    (
        "/table/tbody/1/summary_rows",
        "tbody group 1 summary",
        "tbody_summary",
        Some(1),
    ),
];

#[test]
fn base_table_is_valid() {
    assert!(errors(&base()).is_empty(), "{:?}", errors(&base()));
}

#[test]
fn row_and_cell_errors_name_their_section() {
    type Edit = fn(&mut Value);
    // Each breaks the second cell of a section's first row (or the row itself).
    let edits: &[(ValidationRule, Edit)] = &[
        (ValidationRule::SpanZeroValue, |c| c["colspan"] = json!(0)),
        (ValidationRule::SpanOverflowRight, |c| {
            c["colspan"] = json!(2)
        }),
        (ValidationRule::SpanOverflowBottom, |c| {
            c["rowspan"] = json!(3)
        }),
        (ValidationRule::SpanPlaceholderMismatch, |c| {
            c["is_placeholder"] = json!(true);
            c["content"] = json!([]);
        }),
        (ValidationRule::SpanPlaceholderHasContent, |c| {
            c["is_placeholder"] = json!(true)
        }),
        (ValidationRule::StubContiguous, |c| {
            c["is_stub"] = json!(true)
        }),
        (ValidationRule::StyleRefsValid, |c| {
            c["style_id"] = json!("zz")
        }),
        (
            ValidationRule::FootnoteRefsValid,
            |c| c["content"] = json!([{ "type": "footnote_mark", "ref": "zz", "mark_text": "z" }]),
        ),
        (ValidationRule::UnknownValue, |c| c["scope"] = json!("zz")),
        (
            ValidationRule::InvalidLength,
            |c| c["content"] = json!([{ "type": "image", "src": "x.png", "width": "zz" }]),
        ),
    ];
    for (pointer, name, section, group) in SECTIONS {
        for (rule, edit) in edits {
            let mut v = base();
            edit(&mut v.pointer_mut(pointer).unwrap()[0]["cells"][1]);
            let errs: Vec<_> = errors(&v).into_iter().filter(|e| e.rule == *rule).collect();
            assert_eq!(errs.len(), 1, "{rule} in {name}: {:?}", errors(&v));
            let e = &errs[0];
            let at = format!("{name} (row=0, col=1)");
            assert!(
                e.message.contains(&at),
                "{rule}: {:?} lacks {at:?}",
                e.message
            );
            assert_eq!(
                (e.section.as_str(), e.row_group, e.row, e.col),
                (section, group, Some(0), Some(1)),
                "{rule} in {name}"
            );
        }
        // A row: too many cells, an unknown role, a missing style.
        type RowEdit = fn(&mut Value);
        let row_edits: &[(ValidationRule, RowEdit)] = &[
            (ValidationRule::ColCount, |r| {
                r["cells"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({ "content": [] }))
            }),
            (ValidationRule::UnknownValue, |r| r["role"] = json!("zz")),
            (ValidationRule::StyleRefsValid, |r| {
                r["style_id"] = json!("zz")
            }),
        ];
        for (rule, edit) in row_edits {
            let mut v = base();
            edit(&mut v.pointer_mut(pointer).unwrap()[0]);
            let errs: Vec<_> = errors(&v).into_iter().filter(|e| e.rule == *rule).collect();
            assert_eq!(errs.len(), 1, "{rule} in {name}: {:?}", errors(&v));
            let e = &errs[0];
            let at = format!("{name} row 0");
            assert!(
                e.message.contains(&at),
                "{rule}: {:?} lacks {at:?}",
                e.message
            );
            assert_eq!(
                (e.section.as_str(), e.row_group, e.row),
                (section, group, Some(0)),
                "{rule} in {name}"
            );
        }
    }
}

#[test]
fn the_overlap_message_names_both_cells() {
    let mut v = base();
    let rows = v.pointer_mut("/table/tbody/1/summary_rows").unwrap();
    rows[0]["cells"][0]["rowspan"] = json!(2);
    let errs = errors(&v);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(
        errs[0].to_string(),
        "[SPAN_OVERLAP] Grid position at tbody group 1 summary (row=1, col=0) is already \
         claimed by the cell at (row=0, col=0)"
    );
}

#[test]
fn duplicate_footnote_ids_are_reported_once_per_repeat() {
    let mut v = base();
    let note = |id: &str| json!({ "id": id, "mark": "1", "content": [] });
    v["footer"]["footnotes"] = json!([note("f"), note("g"), note("f"), note("f"), note("g")]);
    let got: Vec<String> = errors(&v).iter().map(|e| e.to_string()).collect();
    assert_eq!(
        got,
        [
            "[DUPLICATE_ID] footer.footnotes[2] has the id \"f\" of footer.footnotes[0]: \
             footnote ids must be unique",
            "[DUPLICATE_ID] footer.footnotes[3] has the id \"f\" of footer.footnotes[0]: \
             footnote ids must be unique",
            "[DUPLICATE_ID] footer.footnotes[4] has the id \"g\" of footer.footnotes[1]: \
             footnote ids must be unique",
        ]
    );
}

#[test]
fn a_style_id_in_both_defs_and_compositions_is_reported_once() {
    let mut v = base();
    v["styles"]["defs"]["t"] = json!({});
    v["styles"]["compositions"] = json!({
        "t": { "extends": "s" },
        "s": { "extends": "t" },
        "u": { "extends": "s" }
    });
    let errs = errors(&v);
    let got: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
    assert_eq!(
        got,
        [
            "[DUPLICATE_ID] style id \"s\" is a key of both styles.defs and styles.compositions",
            "[DUPLICATE_ID] style id \"t\" is a key of both styles.defs and styles.compositions",
        ]
    );
    assert!(errs.iter().all(|e| e.section == "styles"));
}

#[test]
fn a_composition_extends_a_definition() {
    let mut v = base();
    v["styles"]["compositions"] = json!({
        "c": { "extends": "s" },
        "d": { "extends": "c" },
        "e": { "extends": "e" },
        "f": { "extends": "nope" }
    });
    let got: Vec<String> = errors(&v).iter().map(|e| e.to_string()).collect();
    let line = |id: &str, base: &str| {
        format!(
            "[STYLE_REFS_VALID] styles.compositions.{id}.extends is \"{base}\", which is not \
             defined in styles.defs (a composition extends a definition; compositions don't \
             chain)"
        )
    };
    assert_eq!(got, [line("d", "c"), line("e", "e"), line("f", "nope")]);
}
