//! The format registry: complete and consistent metadata, output identical to the
//! writers' own entry points, options that take effect, and every way options or
//! format names can be wrong reported as the right error.

use gridwell_ir::Table;
use gridwell_render::{
    find, names, render, render_binary, render_text, render_with_json_options, Output, OutputKind,
    RenderError, REGISTRY,
};
use gridwell_testkit::{cell, examples, row, TableBuilder};
use serde_json::{json, Value};

fn table() -> Table {
    TableBuilder::new(2)
        .title("Title")
        .head(row(vec![cell("A"), cell("B")]))
        .body(vec![row(vec![cell("1"), cell("2")])])
        .footnote("f", "1", "note")
        .build()
}

fn invalid_table() -> Table {
    let mut t = table();
    t.config.table_cols = 3;
    t
}

#[test]
fn registry_metadata_is_complete_and_consistent() {
    assert_eq!(
        names(),
        [
            "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto", "docx", "xlsx",
            "pptx"
        ]
    );
    let mut seen = std::collections::HashSet::new();
    for w in REGISTRY {
        assert!(seen.insert(w.name()), "duplicate {}", w.name());
        assert_eq!(w.name(), w.name().to_ascii_lowercase());
        assert!(!w.description().is_empty());
        assert!(!w.extension().is_empty() && !w.extension().starts_with('.'));
        assert!(w.media_type().contains('/'));
        assert!(w.default_options().is_object(), "{}", w.name());
        // The output kind matches what the writer produces.
        let out = render(&table(), w.name(), None).unwrap();
        assert_eq!(out.kind(), w.kind(), "{}", w.name());
        if w.kind() == OutputKind::Binary {
            assert!(
                out.into_bytes().starts_with(b"PK\x03\x04"),
                "{} is not a zip",
                w.name()
            );
        }
    }
}

#[test]
fn registry_output_equals_the_writers_own_entry_points() {
    type Direct = fn(&Table) -> Output;
    let direct: Vec<(&str, Direct)> = vec![
        ("html", |t| {
            gridwell_writer_html::render_html(t).unwrap().into()
        }),
        ("latex", |t| {
            gridwell_writer_latex::render_latex(t).unwrap().into()
        }),
        ("typst", |t| {
            gridwell_writer_typst::render_typst(t).unwrap().into()
        }),
        ("rtf", |t| {
            gridwell_writer_rtf::render_rtf(t).unwrap().into()
        }),
        ("svg", |t| {
            gridwell_writer_svg::render_svg(t).unwrap().into()
        }),
        ("ansi", |t| {
            gridwell_writer_ansi::render_ansi(t).unwrap().into()
        }),
        ("pandoc", |t| {
            gridwell_writer_pandoc::render_pandoc(t).unwrap().into()
        }),
        ("quarto", |t| {
            gridwell_writer_quarto::render_quarto(t).unwrap().into()
        }),
        ("docx", |t| {
            gridwell_writer_docx::render_docx(t).unwrap().into()
        }),
        ("xlsx", |t| {
            gridwell_writer_xlsx::render_xlsx(t).unwrap().into()
        }),
        ("pptx", |t| {
            gridwell_writer_pptx::render_pptx(t).unwrap().into()
        }),
    ];
    assert_eq!(direct.len(), REGISTRY.len());
    for ex in examples() {
        let t = ex.table();
        for (name, f) in &direct {
            let via_registry = render(&t, name, None).unwrap();
            assert!(via_registry == f(&t), "{name} differs for {}", ex.name);
            // Explicit defaults are the same as no options.
            let defaults = find(name).unwrap().default_options();
            assert!(
                render(&t, name, Some(&defaults)).unwrap() == via_registry,
                "{name}"
            );
            assert!(
                render(&t, name, Some(&Value::Null)).unwrap() == via_registry,
                "{name}"
            );
        }
    }
}

#[test]
fn format_names_are_case_and_whitespace_insensitive() {
    assert_eq!(find(" HTML ").unwrap().name(), "html");
    assert_eq!(find("Docx").unwrap().name(), "docx");
    for bad in ["", "htm", "html5", "tex", "md", "pdf", "x-html"] {
        assert!(
            matches!(find(bad), Err(RenderError::UnknownFormat { .. })),
            "{bad}"
        );
    }
}

#[test]
fn unknown_format_lists_every_supported_name() {
    let msg = render(&table(), "pdf", None).unwrap_err().to_string();
    assert!(msg.starts_with("unknown format \"pdf\""), "{msg}");
    for name in names() {
        assert!(msg.contains(name), "{msg}");
    }
}

#[test]
fn unknown_format_is_reported_before_validation() {
    let err = render(&invalid_table(), "pdf", None).unwrap_err();
    assert!(matches!(err, RenderError::UnknownFormat { .. }), "{err}");
    let err = render(&invalid_table(), "html", None).unwrap_err();
    assert!(matches!(err, RenderError::InvalidTable(_)), "{err}");
    assert!(err.to_string().contains("COL_COUNT"), "{err}");
}

#[test]
fn options_take_effect() {
    let t = table();
    let text = |format: &str, opts: Value| match render(&t, format, Some(&opts)).unwrap() {
        Output::Text(s) => s,
        Output::Binary(_) => unreachable!(),
    };
    assert!(text(
        "html",
        json!({ "inline_styles": true, "pretty_print": false })
    )
    .find('\n')
    .is_none());
    assert!(text("html", json!({ "class_prefix": "tbl" })).contains("class=\"tbl_table\""));
    assert!(text("html", json!({ "table_id": "sales" })).contains("<div id=\"sales\""));
    assert!(!text("latex", json!({ "booktabs": false })).contains("\\toprule"));
    assert!(text("latex", json!({ "longtable": true })).contains("\\begin{longtable}"));
    assert!(!text("typst", json!({ "repeat_header": false })).contains("table.header("));
    let ascii = text("ansi", json!({ "box_drawing": false }));
    assert!(ascii.contains("+--") && !ascii.contains('┌'), "{ascii}");
    // Every grid line fits in 20 columns once the (invisible) SGR escapes are removed.
    let narrow = text("ansi", json!({ "max_width": 20, "true_color": false }));
    let visible = |l: &str| {
        let mut n = 0;
        let mut in_escape = false;
        for c in l.chars() {
            match (in_escape, c) {
                (false, '\u{1b}') => in_escape = true,
                (true, 'm') => in_escape = false,
                (true, _) => {}
                (false, _) => n += 1,
            }
        }
        n
    };
    let grid: Vec<&str> = narrow
        .lines()
        .filter(|l| l.contains(['┌', '│', '├', '└']))
        .collect();
    assert!(!grid.is_empty());
    assert!(grid.iter().all(|l| visible(l) <= 20), "{narrow}");
    assert!(text("svg", json!({ "font_size": 20 })).contains("font-size: 20px"));
    assert!(text("quarto", json!({ "table_id": "sales" })).contains("\"tbl-sales\""));
}

#[test]
fn bad_options_are_invalid_options_errors() {
    let t = table();
    let cases: Vec<(&str, Value, &str)> = vec![
        ("html", json!({ "inline_style": true }), "unknown field"),
        ("html", json!({ "inline_styles": "yes" }), "invalid type"),
        ("html", json!([1, 2]), "expected a JSON object"),
        ("html", json!("inline"), "expected a JSON object"),
        (
            "html",
            json!({ "class_prefix": "x\"><script>" }),
            "class_prefix",
        ),
        ("html", json!({ "class_prefix": "" }), "class_prefix"),
        ("html", json!({ "class_prefix": "1abc" }), "class_prefix"),
        ("html", json!({ "table_id": "" }), "table_id"),
        ("html", json!({ "table_id": "a b" }), "table_id"),
        ("html", json!({ "table_id": 7 }), "invalid type"),
        (
            "latex",
            json!({ "longtable_threshold": -1 }),
            "invalid value",
        ),
        ("svg", json!({ "font_size": 0 }), "font_size"),
        ("svg", json!({ "font_size": -3 }), "font_size"),
        ("svg", json!({ "row_height": 1e9 }), "row_height"),
        ("svg", json!({ "cell_padding_x": -1 }), "cell_padding_x"),
        (
            "svg",
            json!({ "font_family": "x; } </style><script>" }),
            "font_family",
        ),
        ("svg", json!({ "font_family": "  " }), "font_family"),
        ("ansi", json!({ "max_width": -1 }), "invalid value"),
        ("quarto", json!({ "extra_attrs": "nope" }), "invalid type"),
        ("rtf", json!({ "anything": 1 }), "unknown field"),
        ("docx", json!({ "anything": 1 }), "unknown field"),
    ];
    for (format, opts, needle) in cases {
        let err = render(&t, format, Some(&opts)).unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(&err, RenderError::InvalidOptions { format: f, .. } if f == format),
            "{format} {opts}: {err:?}"
        );
        assert!(msg.contains(needle), "{format} {opts}: {msg}");
    }
    // Formats without options accept an empty object.
    for format in ["rtf", "pandoc", "docx", "xlsx", "pptx"] {
        assert!(render(&t, format, Some(&json!({}))).is_ok(), "{format}");
    }
    // Padding may be zero.
    assert!(render(&t, "svg", Some(&json!({ "cell_padding_x": 0 }))).is_ok());
}

#[test]
fn json_string_options() {
    let t = table();
    let a = render_with_json_options(&t, "html", Some(r#"{"inline_styles": true}"#)).unwrap();
    let b = render(&t, "html", Some(&json!({ "inline_styles": true }))).unwrap();
    assert!(a == b);
    // Empty / whitespace / absent all mean defaults.
    let d = render(&t, "html", None).unwrap();
    for empty in [None, Some(""), Some("  "), Some("null")] {
        assert!(
            render_with_json_options(&t, "html", empty).unwrap() == d,
            "{empty:?}"
        );
    }
    let err = render_with_json_options(&t, "html", Some("{inline_styles: true")).unwrap_err();
    assert!(err.to_string().contains("not valid JSON"), "{err}");
}

#[test]
fn text_and_binary_entry_points_check_the_kind() {
    let t = table();
    assert!(render_text(&t, "html", None).unwrap().contains("<table"));
    assert!(render_binary(&t, "xlsx", None).unwrap().starts_with(b"PK"));
    let err = render_text(&t, "docx", None).unwrap_err();
    assert_eq!(err.to_string(), "\"docx\" is a binary format, not text");
    let err = render_binary(&t, "html", None).unwrap_err();
    assert_eq!(err.to_string(), "\"html\" is a text format, not binary");
    assert!(matches!(
        render_text(&t, "nope", None),
        Err(RenderError::UnknownFormat { .. })
    ));
}

/// Whether `value` is of the documented `kind`, as the options tables write it.
fn of_kind(kind: &str, value: &Value) -> Option<bool> {
    Some(match kind {
        "boolean" => value.is_boolean(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_u64() || value.is_i64(),
        "string or `null`" => value.is_string() || value.is_null(),
        "integer or `null`" => value.is_u64() || value.is_i64() || value.is_null(),
        "list of `[key, value]` pairs" => value.as_array().is_some_and(|pairs| {
            pairs.iter().all(|p| {
                p.as_array()
                    .is_some_and(|kv| kv.len() == 2 && kv.iter().all(Value::is_string))
            })
        }),
        _ => return None,
    })
}

/// The options page against the registry: a section per format with options whose
/// rows are exactly its options, each with its real default and a type that fits it,
/// and a sentence naming exactly the formats without options.
fn option_doc_problems(doc: &str) -> Vec<String> {
    let doc = doc.replace("\r\n", "\n");
    let mut problems = Vec::new();
    let mut without = Vec::new();
    for w in REGISTRY {
        let name = w.name();
        let defaults = w.default_options();
        let fields = defaults.as_object().unwrap();
        let heading = format!("### `{name}`");
        let section = doc
            .split(&heading)
            .nth(1)
            .map(|s| s.split("\n### ").next().unwrap());
        if fields.is_empty() {
            without.push(name);
            if section.is_some() {
                problems.push(format!("{name} has no options but has a section"));
            }
            continue;
        }
        let Some(section) = section else {
            problems.push(format!("no section for {name}"));
            continue;
        };
        let mut documented = Vec::new();
        for line in section.lines().filter(|l| l.starts_with("| `")) {
            let cells: Vec<_> = line.trim_matches('|').split(" | ").map(str::trim).collect();
            let [option, kind, default, _meaning] = cells[..] else {
                problems.push(format!("{name}: malformed row {line:?}"));
                continue;
            };
            let option = option.trim_matches('`');
            documented.push(option.to_string());
            let Some(actual) = fields.get(option) else {
                problems.push(format!("{name}: documents unknown option {option}"));
                continue;
            };
            match serde_json::from_str::<Value>(default.trim_matches('`')) {
                Ok(shown) if &shown == actual => {}
                // `16` for 16.0: JSON doesn't tell them apart.
                Ok(shown) if shown.is_number() && shown.as_f64() == actual.as_f64() => {}
                _ => problems.push(format!(
                    "{name}: {option}'s default is shown as {default} but is {actual}"
                )),
            }
            match of_kind(kind, actual) {
                Some(true) => {}
                Some(false) => problems.push(format!(
                    "{name}: {option} is documented as {kind} but its default is {actual}"
                )),
                None => problems.push(format!("{name}: {option} has unknown type {kind:?}")),
            }
            // The default, given explicitly, is accepted.
            let explicit = json!({ option: actual });
            if let Err(e) = render(&table(), name, Some(&explicit)) {
                problems.push(format!("{name}: {explicit} is rejected: {e}"));
            }
        }
        let mut expected: Vec<_> = fields.keys().cloned().collect();
        let mut sorted = documented.clone();
        sorted.sort();
        expected.sort();
        if sorted != expected {
            problems.push(format!(
                "{name}: documents options {documented:?}, but has {expected:?}"
            ));
        }
    }
    // "RTF, Pandoc, DOCX, XLSX and PPTX have no options"
    let named: Vec<String> = doc
        .lines()
        .find_map(|l| {
            l.split(" have no options")
                .next()
                .filter(|_| l.contains(" have no options"))
        })
        .map(|list| {
            list.replace(" and ", ", ")
                .split(", ")
                .map(|n| n.trim().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    if named != without {
        problems.push(format!(
            "the page says {named:?} have no options, but {without:?} don't"
        ));
    }
    problems
}

fn writer_options_page() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/guide/writer-options.qmd"
    );
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn every_option_is_documented() {
    let problems = option_doc_problems(&writer_options_page());
    assert!(
        problems.is_empty(),
        "writer-options.qmd:\n{}",
        problems.join("\n")
    );
}

/// Planted bugs: each wrong options page must be caught.
#[test]
fn the_options_check_catches_wrong_pages() {
    let doc = writer_options_page();
    let plants: &[(&str, &str, &str)] = &[
        // A wrong default.
        (
            "| `font_size` | number | `16` |",
            "| `font_size` | number | `14` |",
            "default is shown as `14`",
        ),
        // A default that isn't JSON.
        (
            "| `class_prefix` | string | `\"gw\"` |",
            "| `class_prefix` | string | `gw` |",
            "default is shown as `gw`",
        ),
        // A wrong type.
        (
            "| `max_width` | integer |",
            "| `max_width` | boolean |",
            "documented as boolean",
        ),
        // A type the check doesn't know.
        (
            "| `max_width` | integer |",
            "| `max_width` | int |",
            "unknown type \"int\"",
        ),
        // A missing option.
        (
            "| `color` | boolean |",
            "| `colour` | boolean |",
            "documents unknown option colour",
        ),
        // A dropped row.
        (
            "| `repeat_header` | boolean | `true` |",
            "| repeat_header | boolean | `true` |",
            "documents options []",
        ),
        // A missing section.
        ("### `typst`", "### Typst", "no section for typst"),
        // A wrong "no options" sentence.
        (
            "RTF, Pandoc, DOCX, XLSX and PPTX",
            "RTF, Pandoc, DOCX and XLSX",
            "have no options, but",
        ),
    ];
    for (from, to, expected) in plants {
        assert!(doc.contains(from), "plant {from:?} no longer applies");
        let problems = option_doc_problems(&doc.replacen(from, to, 1));
        assert!(
            problems.iter().any(|p| p.contains(expected)),
            "plant {to:?} not caught as {expected:?}: {problems:?}"
        );
    }
    // A section for a format without options.
    let extra = format!("{doc}\n### `rtf`\n\n| Option | Type | Default | Meaning |\n");
    assert!(option_doc_problems(&extra)
        .iter()
        .any(|p| p == "rtf has no options but has a section"));
}

#[test]
fn every_format_is_deterministic() {
    // Byte-identical across renders (OOXML zips included: fixed timestamps, fixed
    // part order; no hash-map iteration order leaks into output).
    for ex in examples() {
        let t = ex.table();
        // A re-parsed copy has fresh hash maps (new random seeds), so any output
        // that followed map iteration order would differ.
        let copy = Table::from_json(&t.to_json().unwrap()).unwrap();
        for name in names() {
            let a = render(&t, name, None).unwrap();
            let b = render(&copy, name, None).unwrap();
            assert!(a == b, "{name} not deterministic for {}", ex.name);
        }
    }
}
