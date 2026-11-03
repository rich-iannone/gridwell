//! What the Python and R bindings show a table as: a one-line description, and
//! HTML with a fresh id for each display, so several tables in one notebook or
//! document keep their styles apart (`table_id` scopes the style block).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use gridwell_ir::Table;
use gridwell_layout::text::plain_text;

use crate::RenderError;

/// The longest title a description quotes, in characters.
const TITLE_CHARS: usize = 40;

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A one-line description of a table: its size, row groups, summary rows and
/// title, and whether it's invalid. For instance `3 columns, 12 rows in 2 groups,
/// 2 summary rows, titled "Sales"`.
pub fn describe(table: &Table) -> String {
    let body = &table.table.tbody;
    let rows: usize = body.iter().map(|g| g.rows.len()).sum();
    let summaries: usize = body.iter().map(|g| g.summary_rows.len()).sum();
    let mut out = format!(
        "{}, {}",
        count(table.config.table_cols as usize, "column", "columns"),
        count(rows, "row", "rows")
    );
    if body.len() > 1 || body.iter().any(|g| g.label.is_some()) {
        out.push_str(&format!(" in {}", count(body.len(), "group", "groups")));
    }
    if summaries > 0 {
        out.push_str(&format!(
            ", {}",
            count(summaries, "summary row", "summary rows")
        ));
    }
    let title = table.header.as_ref().and_then(|h| h.title.as_ref());
    let title = title
        .map(|t| plain_text(&t.content, " "))
        .unwrap_or_default();
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if !title.is_empty() {
        let mut short: String = title.chars().take(TITLE_CHARS).collect();
        if short.len() < title.len() {
            short.push('…');
        }
        out.push_str(&format!(", titled {short:?}"));
    }
    let errors = table.validate().len();
    if errors > 0 {
        out.push_str(&format!("; invalid ({})", count(errors, "error", "errors")));
    }
    out
}

/// A fresh HTML id for one display of a table, `gw-` and 10 hex digits. Random
/// without touching any language's random state (R's seed stays as it was).
pub fn display_id() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or_default(),
    );
    format!("gw-{:010x}", h.finish() & 0xff_ffff_ffff)
}

/// The HTML a notebook or an HTML document shows a table as: scoped to a fresh
/// [`display_id`], and marked so Quarto doesn't rewrite it with Bootstrap's table
/// classes.
pub fn display_html(table: &Table) -> Result<String, RenderError> {
    let options = serde_json::json!({
        "table_id": display_id(),
        "quarto_disable_processing": true,
    });
    let out = crate::render(table, "html", Some(&options))?;
    Ok(out.as_text().expect("html is a text format").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn table(v: serde_json::Value) -> Table {
        serde_json::from_value(v).unwrap()
    }

    fn cells(n: usize) -> serde_json::Value {
        json!({ "cells": vec![json!({ "content": [] }); n] })
    }

    #[test]
    fn descriptions_count_what_the_table_shows() {
        let base = |tbody: serde_json::Value, header: serde_json::Value| {
            let rows: usize = tbody
                .as_array()
                .unwrap()
                .iter()
                .map(|g| g["rows"].as_array().unwrap().len())
                .sum();
            table(json!({
                "ir_version": "1.0",
                "config": { "table_cols": 2, "body_rows": rows, "stub_cols": 1 },
                "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
                "column_spec": [{ "id": "a" }, { "id": "b" }],
                "header": header,
                "table": { "thead": { "rows": [] }, "tbody": tbody }
            }))
        };
        let t = base(
            json!([{ "rows": [cells(2), cells(2), cells(2)] }]),
            json!(null),
        );
        assert_eq!(describe(&t), "2 columns, 3 rows");
        let t = base(
            json!([
                { "label": { "content": [{ "type": "text", "value": "G" }] },
                  "rows": [cells(2)], "summary_rows": [cells(2)] },
                { "rows": [cells(2), cells(2)] }
            ]),
            json!({ "title": { "content": [
                { "type": "text", "value": "Sales  by" },
                { "type": "line_break" },
                { "type": "text", "value": "\"region\"" }
            ] } }),
        );
        assert_eq!(
            describe(&t),
            r#"2 columns, 3 rows in 2 groups, 1 summary row, titled "Sales by \"region\"""#
        );
        // One labeled group is still a group; a long title is cut short.
        let long = "x".repeat(TITLE_CHARS + 1);
        let t = base(
            json!([{ "label": { "content": [] }, "rows": [cells(2)] }]),
            json!({ "title": { "content": [{ "type": "text", "value": long }] } }),
        );
        let expected = format!("titled \"{}…\"", "x".repeat(TITLE_CHARS));
        assert!(describe(&t).starts_with("2 columns, 1 row in 1 group, "));
        assert!(describe(&t).ends_with(&expected), "{}", describe(&t));
        // Rows of the wrong width make it invalid.
        let t = base(json!([{ "rows": [cells(1)] }]), json!(null));
        assert!(describe(&t).contains("; invalid ("), "{}", describe(&t));
    }

    #[test]
    fn display_html_is_scoped_and_left_alone_by_quarto() {
        let t = table(json!({
            "ir_version": "1.0",
            "config": { "table_cols": 1, "body_rows": 1 },
            "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
            "column_spec": [{ "id": "a" }],
            "table": { "thead": { "rows": [] }, "tbody": [{ "rows": [cells(1)] }] }
        }));
        let (a, b) = (display_html(&t).unwrap(), display_html(&t).unwrap());
        let id = |html: &str| html[9..22].to_string();
        assert!(a.starts_with("<div id=\"gw-"), "{a}");
        assert_ne!(id(&a), id(&b));
        let options = format!(
            r#"{{"table_id": "{}", "quarto_disable_processing": true}}"#,
            id(&a)
        );
        assert_eq!(a, crate::render_text(&t, "html", Some(&options)).unwrap());
        assert!(a.contains("<table class=\"gw_table\" data-quarto-disable-processing=\"true\">"));
        // An invalid table is an error, not HTML.
        let mut bad = t.clone();
        bad.config.table_cols = 2;
        assert!(display_html(&bad).is_err());
    }

    #[test]
    fn display_ids_are_fresh_html_ids() {
        let ids: std::collections::HashSet<String> = (0..100).map(|_| display_id()).collect();
        assert_eq!(ids.len(), 100);
        for id in ids {
            assert_eq!(id.len(), 13, "{id}");
            assert!(id.starts_with("gw-"));
            assert!(id[3..].chars().all(|c| c.is_ascii_hexdigit()), "{id}");
        }
    }
}
