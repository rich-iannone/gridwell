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

#[test]
fn every_option_is_documented() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/guide/writer-options.qmd"
    );
    let doc = std::fs::read_to_string(path).unwrap();
    for w in REGISTRY {
        let defaults = w.default_options();
        let fields = defaults.as_object().unwrap();
        if fields.is_empty() {
            continue;
        }
        let section = doc
            .split(&format!("### `{}`", w.name()))
            .nth(1)
            .unwrap_or_else(|| panic!("no section for {}", w.name()));
        let section = section.split("\n### ").next().unwrap();
        for name in fields.keys() {
            assert!(
                section.contains(&format!("| `{name}` |")),
                "{}: option {name} is not documented",
                w.name()
            );
        }
        // And nothing documented that the format doesn't have.
        for line in section.lines().filter(|l| l.starts_with("| `")) {
            let name = line.split('`').nth(1).unwrap();
            assert!(
                fields.contains_key(name),
                "{}: documents unknown option {name}",
                w.name()
            );
        }
    }
}
