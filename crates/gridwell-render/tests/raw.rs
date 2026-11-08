//! The `raw` option, which every format takes: with it off, no raw markup from
//! the IR reaches any format's output, wherever in the table it is; raw HTML is
//! drawn as text and styled runs instead; and a table without raw nodes renders
//! as it does with the option on.

use std::collections::BTreeSet;
use std::io::{Cursor, Read};

use gridwell_ir::Table;
use gridwell_layout::html::without_raw;
use gridwell_render::{render, OutputKind, RenderError, REGISTRY};
use gridwell_testkit::{cell, examples, raw, row, TableBuilder};
use serde_json::{json, Value};

/// In every raw node the tests add, in markup no format may write.
const SECRET: &str = "GW_SECRET_9f3";
/// The text the raw HTML they add is drawn as.
const LOWERED: &str = "GW_LOWERED";

/// Raw markup for every format that writes its own, and some no format writes.
fn raw_nodes() -> Value {
    let node =
        |format: &str, value: String| json!({"type": "raw", "format": format, "value": value});
    json!([
        node(
            "html",
            format!("<i>{LOWERED}</i><script>{SECRET}</script><!-- {SECRET} -->")
        ),
        node("latex", format!("\\input{{{SECRET}}}")),
        node("typst", format!("#read(\"{SECRET}\")")),
        node(
            "rtf",
            format!("{{\\field{{\\*\\fldinst INCLUDETEXT \"{SECRET}\"}}}}")
        ),
        node("markdown", SECRET.to_string()),
    ])
}

/// `v` with [`raw_nodes`] added to every content list but a placeholder's (whose
/// content must be empty). Returns where it added them, without indices
/// (`table.tbody.rows.cells`).
fn add_raw(v: &mut Value, path: &str, places: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            let placeholder = map.get("is_placeholder") == Some(&Value::Bool(true));
            for (key, child) in map.iter_mut() {
                if key == "extensions" {
                    continue;
                }
                let at = format!("{path}.{key}");
                match child {
                    Value::Array(nodes) if key == "content" => {
                        if !placeholder {
                            nodes.extend(raw_nodes().as_array().unwrap().iter().cloned());
                            places.push(path.trim_start_matches('.').to_string());
                        }
                    }
                    _ => add_raw(child, &at, places),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|c| add_raw(c, path, places)),
        _ => {}
    }
}

/// Every example, and a table with extra header lines (no example has them),
/// with raw markup in every content list; and where it is.
fn tables_with_raw() -> Vec<(&'static str, Table, Vec<String>)> {
    let extra_lines = TableBuilder::new(1)
        .title("t")
        .extra_line("one")
        .extra_line("two")
        .body(vec![row(vec![cell("x")])])
        .build();
    examples()
        .into_iter()
        .map(|ex| (ex.name, ex.table()))
        .chain([("extra_lines", extra_lines)])
        .map(|(name, t)| {
            let mut v = serde_json::to_value(t).unwrap();
            let mut places = Vec::new();
            add_raw(&mut v, "", &mut places);
            let t: Table = serde_json::from_value(v).unwrap();
            t.ensure_valid().unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, t, places)
        })
        .collect()
}

/// A format's output as text: an OOXML package's parts, each read whole.
fn text_of(format: &str, bytes: Vec<u8>) -> String {
    if REGISTRY
        .iter()
        .any(|w| w.name() == format && w.kind() == OutputKind::Text)
    {
        return String::from_utf8(bytes).unwrap();
    }
    let mut zip = gridwell_ooxml::zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    for i in 0..zip.len() {
        let mut part = Vec::new();
        zip.by_index(i).unwrap().read_to_end(&mut part).unwrap();
        out.push_str(&String::from_utf8_lossy(&part));
    }
    out
}

fn untrusted() -> Value {
    json!({ "raw": false })
}

#[test]
fn with_raw_off_no_raw_markup_reaches_any_output() {
    let tables = tables_with_raw();
    for (name, t, _) in &tables {
        for w in REGISTRY {
            let out = render(t, w.name(), Some(&untrusted()))
                .unwrap_or_else(|e| panic!("{name} to {}: {e}", w.name()));
            let out = text_of(w.name(), out.into_bytes());
            assert!(
                !out.contains(SECRET),
                "{name} to {}: raw markup in the output",
                w.name()
            );
        }
    }
    // The control: with the option on, the formats that write raw markup write it.
    for (name, t, _) in &tables {
        for format in ["html", "latex", "typst", "rtf", "pandoc", "quarto"] {
            let out = text_of(format, render(t, format, None).unwrap().into_bytes());
            assert!(out.contains(SECRET), "{name} to {format}: no raw markup");
        }
    }
}

#[test]
fn with_raw_off_every_content_list_is_lowered() {
    let mut seen = BTreeSet::new();
    for (name, t, places) in tables_with_raw() {
        let out = serde_json::to_string(&without_raw(&t)).unwrap();
        assert!(!out.contains(SECRET), "{name}");
        assert!(!out.contains(r#""type":"raw""#), "{name}");
        // Each list's raw HTML is drawn: once per list.
        assert_eq!(out.matches(LOWERED).count(), places.len(), "{name}");
        seen.extend(places);
    }
    // The corpus has content in every place a table holds it.
    let places: Vec<&str> = seen.iter().map(String::as_str).collect();
    assert_eq!(
        places,
        [
            "footer.footnotes",
            "footer.source_notes",
            "header.extra_lines",
            "header.subtitle",
            "header.title",
            "table.tbody.label",
            "table.tbody.rows.cells",
            "table.tbody.summary_rows.cells",
            "table.thead.rows.cells",
        ]
    );
}

#[test]
fn raw_off_changes_nothing_without_raw_nodes() {
    let mut compared = 0;
    for ex in examples() {
        let t = ex.table();
        if !matches!(without_raw(&t), std::borrow::Cow::Borrowed(_)) {
            continue;
        }
        compared += 1;
        for w in REGISTRY {
            let on = render(&t, w.name(), None).unwrap();
            let off = render(&t, w.name(), Some(&untrusted())).unwrap();
            assert!(on == off, "{} to {}", ex.name, w.name());
        }
    }
    assert!(compared > 50, "{compared}");
}

/// A one-cell table holding raw HTML.
fn html_cell(html: &str) -> Table {
    TableBuilder::new(1)
        .body(vec![row(vec![gridwell_ir::Cell {
            content: vec![raw("html", html)],
            ..cell("")
        }])])
        .build()
}

#[test]
fn html_draws_raw_html_as_runs_with_their_styles() {
    let t = html_cell("<b>bold</b> <span style=\"color: red\">red</span><script>x</script>");
    for inline in [false, true] {
        let options = json!({ "raw": false, "inline_styles": inline });
        let out = render(&t, "html", Some(&options)).unwrap();
        let out = out.as_text().unwrap();
        // With classes too, a run's style is inline: the table has no class for it.
        assert!(
            out.contains(r#"<span style="font-weight: bold">bold</span> <span style="color: #FF0000">red</span>"#),
            "inline_styles {inline}: {out}"
        );
        assert!(!out.contains("<script>") && !out.contains("<b>"), "{out}");
    }
    let out = render(&t, "html", None).unwrap();
    assert!(out.as_text().unwrap().contains("<b>bold</b>"));
}

#[test]
fn pandoc_and_quarto_pass_no_raw_inline_on() {
    let t = html_cell("<i>x</i>");
    for format in ["pandoc", "quarto"] {
        let on = render(&t, format, None).unwrap();
        assert!(on.as_text().unwrap().contains("RawInline"), "{format}");
        let off = render(&t, format, Some(&untrusted())).unwrap();
        let off = off.as_text().unwrap();
        assert!(
            !off.contains("RawInline") && off.contains("Emph"),
            "{format}: {off}"
        );
    }
}

#[test]
fn every_format_takes_the_option_and_checks_it() {
    let t = html_cell("x");
    for w in REGISTRY {
        let name = w.name();
        assert_eq!(w.default_options()["raw"], json!(true), "{name}");
        // On, explicitly, is the default.
        let on = render(&t, name, Some(&json!({ "raw": true }))).unwrap();
        assert!(on == render(&t, name, None).unwrap(), "{name}");
        for bad in [json!("no"), json!(0), json!(null)] {
            let err = render(&t, name, Some(&json!({ "raw": bad }))).unwrap_err();
            assert!(
                matches!(&err, RenderError::InvalidOptions { message, .. }
                    if message.starts_with("raw: invalid type")),
                "{name} {bad}: {err}"
            );
        }
        // A misspelled option is still an error, naming the options there are.
        let err = render(&t, name, Some(&json!({ "rwa": false }))).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("unknown field `rwa`") && message.contains("`raw`"),
            "{name}: {message}"
        );
    }
    let err = render(&t, "docx", Some(&json!({ "x": 1 }))).unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid options for \"docx\": unknown field `x`, expected `raw`"
    );
}
