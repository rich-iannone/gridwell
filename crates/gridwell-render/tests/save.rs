//! Saving to a file: the format comes from the extension (or an explicit name), the
//! bytes on disk are exactly the rendered output, and every way the path can be
//! wrong is reported before anything is written.

use std::path::{Path, PathBuf};

use gridwell_ir::Table;
use gridwell_render::{
    format_for_path, render, render_with_json_options, save, RenderError, REGISTRY,
};
use gridwell_testkit::{cell, examples, row, TableBuilder};

fn table() -> Table {
    TableBuilder::new(2)
        .title("Title")
        .head(row(vec![cell("A"), cell("B")]))
        .body(vec![row(vec![cell("1"), cell("2")])])
        .footnote("f", "1", "note")
        .build()
}

/// A fresh, empty directory for one test.
fn dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("save")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn extensions_map_to_formats() {
    for (file, format) in [
        ("t.html", "html"),
        ("t.tex", "latex"),
        ("t.typ", "typst"),
        ("t.rtf", "rtf"),
        ("t.svg", "svg"),
        ("t.txt", "ansi"),
        ("t.docx", "docx"),
        ("t.xlsx", "xlsx"),
        ("t.pptx", "pptx"),
        ("dir.v2/T.RTF", "rtf"),
        ("t.Docx", "docx"),
        ("archive.tar.xlsx", "xlsx"),
    ] {
        assert_eq!(
            format_for_path(Path::new(file)).unwrap().name(),
            format,
            "{file}"
        );
    }
}

#[test]
fn every_unambiguous_extension_resolves_to_its_writer() {
    for w in REGISTRY {
        let sharing = REGISTRY
            .iter()
            .filter(|o| o.extension() == w.extension())
            .count();
        let path = format!("table.{}", w.extension());
        match format_for_path(Path::new(&path)) {
            Ok(found) => {
                assert_eq!(sharing, 1);
                assert_eq!(found.name(), w.name());
            }
            Err(RenderError::AmbiguousExtension { formats, .. }) => {
                assert!(sharing > 1);
                assert!(formats.contains(&w.name()));
            }
            Err(e) => panic!("{path}: {e}"),
        }
    }
}

#[test]
fn json_is_ambiguous() {
    let err = format_for_path(Path::new("out/table.JSON")).err().unwrap();
    let RenderError::AmbiguousExtension {
        extension, formats, ..
    } = &err
    else {
        panic!("{err}");
    };
    assert_eq!(extension, "JSON");
    assert_eq!(formats, &["pandoc", "quarto"]);
    let msg = err.to_string();
    assert!(msg.contains("out/table.JSON"), "{msg}");
    assert!(msg.contains("pandoc or quarto"), "{msg}");
    assert!(msg.contains("pass a format"), "{msg}");
}

#[test]
fn missing_and_unknown_extensions_are_errors() {
    for path in ["table", "table.", ".hidden", "dir.d/table"] {
        let err = format_for_path(Path::new(path)).err().unwrap();
        assert!(
            matches!(err, RenderError::NoExtension { .. }),
            "{path}: {err}"
        );
        assert!(err.to_string().contains("no file extension"), "{err}");
    }
    let err = format_for_path(Path::new("table.pdf")).err().unwrap();
    assert!(
        matches!(&err, RenderError::UnknownExtension { extension, .. } if extension == "pdf"),
        "{err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("\".pdf\""), "{msg}");
    // Lists every known extension once.
    assert!(
        msg.contains("html, tex, typ, rtf, svg, txt, json, docx, xlsx, pptx"),
        "{msg}"
    );
}

#[test]
fn saved_bytes_equal_the_rendered_output() {
    let d = dir("bytes");
    for ex in examples() {
        let t = ex.table();
        for w in REGISTRY {
            if w.name() == "ansi" {
                continue; // file defaults differ; see below
            }
            let path = d.join(format!("{}.{}", ex.name, w.extension()));
            let used = save(&t, &path, Some(w.name()), None).unwrap();
            assert_eq!(used.name(), w.name());
            let expected = render(&t, w.name(), None).unwrap().into_bytes();
            assert!(
                std::fs::read(&path).unwrap() == expected,
                "{} / {}",
                ex.name,
                w.name()
            );
        }
    }
}

#[test]
fn format_is_inferred_from_the_extension() {
    let d = dir("infer");
    let t = table();
    for (file, format) in [("t.rtf", "rtf"), ("t.HTML", "html"), ("t.docx", "docx")] {
        let path = d.join(file);
        assert_eq!(save(&t, &path, None, None).unwrap().name(), format);
        let expected = render(&t, format, None).unwrap().into_bytes();
        assert_eq!(std::fs::read(&path).unwrap(), expected, "{file}");
    }
}

#[test]
fn explicit_format_overrides_the_extension() {
    let d = dir("override");
    let t = table();
    for format in ["pandoc", "quarto"] {
        let path = d.join(format!("{format}.json"));
        assert_eq!(save(&t, &path, Some(format), None).unwrap().name(), format);
        let expected = render(&t, format, None).unwrap().into_bytes();
        assert_eq!(std::fs::read(&path).unwrap(), expected);
    }
    // Any extension (or none) with an explicit format.
    let path = d.join("table");
    save(&t, &path, Some("LaTeX"), None).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\\begin{"), "{text}");
}

#[test]
fn ansi_files_are_plain_text_unless_color_is_set() {
    let d = dir("ansi");
    for ex in examples() {
        let t = ex.table();
        let path = d.join(format!("{}.txt", ex.name));
        save(&t, &path, None, None).unwrap();
        let plain = std::fs::read_to_string(&path).unwrap();
        assert!(!plain.contains('\x1b'), "{}", ex.name);
        let expected = render_with_json_options(&t, "ansi", Some(r#"{"color": false}"#)).unwrap();
        assert_eq!(plain, expected.as_text().unwrap(), "{}", ex.name);

        // Other options keep the file default; an explicit color wins.
        save(&t, &path, None, Some(r#"{"box_drawing": false}"#)).unwrap();
        let ascii = std::fs::read_to_string(&path).unwrap();
        assert!(!ascii.contains('\x1b'), "{}", ex.name);
        assert!(!ascii.contains('│'), "{}", ex.name);

        save(&t, &path, Some("ansi"), Some(r#"{"color": true}"#)).unwrap();
        let colored = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            colored,
            render(&t, "ansi", None).unwrap().as_text().unwrap()
        );
    }
}

#[test]
fn options_pass_through() {
    let d = dir("options");
    let path = d.join("t.html");
    save(&table(), &path, None, Some(r#"{"class_prefix": "zz"}"#)).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("zz"));
}

#[test]
fn errors_write_nothing() {
    let d = dir("errors");
    let t = table();
    let mut invalid = table();
    invalid.config.table_cols = 3;
    let cases: Vec<(&str, Option<&str>, Option<&str>, &Table)> = vec![
        ("a.json", None, None, &t),
        ("a.pdf", None, None, &t),
        ("a", None, None, &t),
        ("a.html", Some("nope"), None, &t),
        ("a.html", None, Some(r#"{"nope": 1}"#), &t),
        ("a.html", None, Some("{not json"), &t),
        ("a.txt", None, Some("[1]"), &t),
        ("a.html", None, None, &invalid),
    ];
    for (file, format, options, table) in cases {
        let path = d.join(file);
        let err = save(table, &path, format, options).err().unwrap();
        assert!(!path.exists(), "{file} written despite {err}");
    }
    assert!(matches!(
        save(&t, &d.join("a.html"), Some("nope"), None),
        Err(RenderError::UnknownFormat { .. })
    ));
    assert!(matches!(
        save(&invalid, &d.join("a.html"), None, None),
        Err(RenderError::InvalidTable(_))
    ));
}

#[test]
fn unwritable_paths_are_io_errors_naming_the_path() {
    let d = dir("io");
    let path = d.join("missing-dir").join("t.html");
    let err = save(&table(), &path, None, None).err().unwrap();
    assert!(matches!(err, RenderError::Io { .. }), "{err}");
    let msg = err.to_string();
    assert!(msg.contains("missing-dir"), "{msg}");
    assert!(msg.starts_with("cannot write"), "{msg}");
}

#[test]
fn text_files_have_no_trailing_newline_added() {
    let d = dir("newline");
    let t = table();
    for format in ["html", "rtf", "latex"] {
        let path = d.join(format!("t.{format}"));
        save(&t, &path, Some(format), None).unwrap();
        let rendered = render(&t, format, None).unwrap();
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk.len(), rendered.as_text().unwrap().len(), "{format}");
    }
}
