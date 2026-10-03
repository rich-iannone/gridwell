//! Format-native oracle: the Typst we emit must compile, and text must come out
//! verbatim.
//!
//! - Every corpus example and every span layout from `gridwell_testkit::spans` is
//!   compiled with the `typst` CLI.
//! - Tricky text (all ASCII punctuation, markup look-alikes, line-start markers) is
//!   compiled to PDF and read back with `pdftotext`; it must appear unchanged.
//!
//! Skipped when the tools are missing, unless `GRIDWELL_REQUIRE_TYPST` is set (as in
//! the pinned harness image / CI).

use std::path::{Path, PathBuf};
use std::process::Command;

use gridwell_testkit::spans::tilings;
use gridwell_testkit::{cell, column, examples, group, row, CellExt, ColumnExt, TableBuilder};
use gridwell_writer_typst::render_typst;

/// Whether `tool` can be launched. Exit status is deliberately ignored: tools disagree
/// on version flags (`pdftotext --version` exits non-zero; Poppler uses `-v`), and a
/// false "missing" silently turns the test into a skip.
fn tool_available(tool: &str) -> bool {
    Command::new(tool).arg("-v").output().is_ok()
}

/// `true` if the test should run; panics if the tool is required but missing.
fn require(tool: &str) -> bool {
    if tool_available(tool) {
        return true;
    }
    if std::env::var_os("GRIDWELL_REQUIRE_TYPST").is_some_and(|v| !v.is_empty()) {
        panic!("{tool} not found but GRIDWELL_REQUIRE_TYPST is set");
    }
    eprintln!("skipping: {tool} not found");
    false
}

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gridwell-typst-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compile `source`; `Err` carries typst's diagnostics.
fn compile(dir: &Path, stem: &str, source: &str) -> Result<PathBuf, String> {
    let typ = dir.join(format!("{stem}.typ"));
    let pdf = dir.join(format!("{stem}.pdf"));
    // A wide page keeps each cell's text on one line for pdftotext.
    let doc = format!("#set page(width: 60cm, height: auto, margin: 1cm)\n{source}");
    std::fs::write(&typ, doc).unwrap();
    let out = Command::new("typst")
        .arg("compile")
        .arg(&typ)
        .arg(&pdf)
        .output()
        .unwrap();
    if out.status.success() {
        Ok(pdf)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[test]
fn every_example_compiles() {
    if !require("typst") {
        return;
    }
    let dir = work_dir("examples");
    let failures: Vec<String> = examples()
        .into_iter()
        .filter_map(|ex| {
            let src = render_typst(&ex.table()).unwrap();
            compile(&dir, ex.name, &src).err().map(|e| {
                format!(
                    "{}:\n{}",
                    ex.name,
                    e.lines().take(6).collect::<Vec<_>>().join("\n")
                )
            })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} example(s) failed to compile:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn span_layouts_compile() {
    if !require("typst") {
        return;
    }
    // One document holding many tables keeps this to a single typst invocation.
    let dir = work_dir("spans");
    let mut doc = String::new();
    for (rows, cols) in [(1, 4), (2, 3), (3, 3)] {
        for t in tilings(rows, cols) {
            let table = TableBuilder::new(cols as u32)
                .group(group(t.to_rows()))
                .build();
            doc.push_str(&render_typst(&table).unwrap());
            doc.push('\n');
        }
    }
    if let Err(e) = compile(&dir, "spans", &doc) {
        panic!("span layouts failed to compile:\n{e}");
    }
}

#[test]
fn tricky_text_renders_verbatim() {
    if !require("typst") || !require("pdftotext") {
        return;
    }
    let texts = [
        "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        "a*b* _c_ `d` $e$",
        "// not a comment /* nor this */",
        "= not a heading",
        "- not a list",
        "+ not an enum",
        "/ not: a term",
        "-5 --x ... ~",
        "<label> @ref #code [content]",
        "C:\\path\\to\\file",
        "https://example.com/a_b*c",
        // Accented Latin is covered by Typst's bundled fonts. CJK and emoji are
        // deliberately absent: without system fonts for them (as on CI runners)
        // every missing glyph maps to the same box and pdftotext can't tell them
        // apart, which tests the environment rather than our escaping.
        // escape_typst's unit test covers that non-ASCII passes through untouched.
        "café naïve Ærø",
    ];
    let mut b = TableBuilder::new(1).head(row(vec![cell("header")]));
    let rows = texts.iter().map(|t| row(vec![cell(t)])).collect();
    b = b.body(rows);
    let src = render_typst(&b.build()).unwrap();

    let dir = work_dir("verbatim");
    let pdf = compile(&dir, "verbatim", &src)
        .unwrap_or_else(|e| panic!("compile failed:\n{e}\n\nsource:\n{src}"));
    let out = Command::new("pdftotext")
        .arg("-layout")
        .arg(&pdf)
        .arg("-")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    // `pdftotext -layout` pads around glyphs from fallback fonts (e.g. emoji), so
    // compare with whitespace runs collapsed.
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let lines: Vec<String> = text.lines().map(squash).collect();
    let missing: Vec<&&str> = texts
        .iter()
        .filter(|t| !lines.contains(&squash(t)))
        .collect();
    assert!(
        missing.is_empty(),
        "not rendered verbatim: {missing:?}\n\npdftotext output:\n{text}"
    );
}

/// Compile `src` and return its text, one trimmed, whitespace-collapsed line per
/// entry.
fn pdf_lines(stem: &str, src: &str) -> Vec<String> {
    let dir = work_dir(stem);
    let pdf = compile(&dir, stem, src)
        .unwrap_or_else(|e| panic!("compile failed:\n{e}\n\nsource:\n{src}"));
    let out = Command::new("pdftotext")
        .arg("-layout")
        .arg(&pdf)
        .arg("-")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn hidden_columns_are_dropped_and_cells_stay_aligned() {
    if !require("typst") || !require("pdftotext") {
        return;
    }
    // Columns: visible, hidden, visible, hidden(spanned into). The spanner covers
    // grid columns 1..4 (hidden, visible, hidden) and must shrink to one column.
    let table = TableBuilder::new(4)
        .columns(vec![
            column("a", "A"),
            column("h1", "H1").hidden(),
            column("b", "B"),
            column("h2", "H2").hidden(),
        ])
        .head(row(vec![
            cell("left"),
            cell("span").colspan(3),
            gridwell_testkit::placeholder(),
            gridwell_testkit::placeholder(),
        ]))
        .body(vec![
            row(vec![
                cell("A1"),
                cell("SECRET1"),
                cell("B1"),
                cell("SECRET2"),
            ]),
            row(vec![
                cell("A2"),
                cell("SECRET3"),
                cell("B2"),
                cell("SECRET4"),
            ]),
        ])
        .build();
    assert!(table.validate().is_empty(), "{:?}", table.validate());
    let src = render_typst(&table).unwrap();
    assert!(!src.contains("SECRET"), "hidden content emitted:\n{src}");
    let lines = pdf_lines("hidden", &src);
    assert_eq!(lines, vec!["left span", "A1 B1", "A2 B2"], "source:\n{src}");
}

#[test]
fn every_column_hidden_compiles() {
    if !require("typst") {
        return;
    }
    let table = TableBuilder::new(2)
        .columns(vec![column("a", "A").hidden(), column("b", "B").hidden()])
        .body(vec![row(vec![cell("x"), cell("y")])])
        .footnote("f1", "1", "still shown")
        .build();
    let src = render_typst(&table).unwrap();
    assert!(!src.contains("#table("), "{src}");
    compile(&work_dir("allhidden"), "allhidden", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
}

#[test]
fn each_footnote_and_source_note_is_its_own_line() {
    if !require("typst") || !require("pdftotext") {
        return;
    }
    let table = TableBuilder::new(1)
        .head(row(vec![cell("H")]))
        .body(vec![row(vec![cell("x")])])
        .footnote("f1", "a", "First note.")
        .footnote("f2", "b", "Second note.")
        .source_note("Source one.")
        .source_note("Source two.")
        .build();
    let lines = pdf_lines("notes", &render_typst(&table).unwrap());
    // The regression: consecutive notes were joined into one paragraph. Check that no
    // extracted line holds two notes. (Whether pdftotext keeps a raised mark on its
    // note's line varies by Poppler version, so marks are only checked for presence.)
    let notes = ["First note.", "Second note.", "Source one.", "Source two."];
    for line in &lines {
        let on_line: Vec<_> = notes.iter().filter(|n| line.contains(*n)).collect();
        assert!(
            on_line.len() <= 1,
            "notes joined on one line: {line:?} (all: {lines:?})"
        );
    }
    for want in notes {
        assert!(
            lines.iter().any(|l| l.contains(want)),
            "missing {want:?} in {lines:?}"
        );
    }
    for mark in ["a", "b"] {
        assert!(
            lines
                .iter()
                .any(|l| l == mark || l.starts_with(&format!("{mark} "))),
            "missing mark {mark:?} in {lines:?}"
        );
    }
}

#[test]
fn no_header_rule_without_header_rows() {
    let table = TableBuilder::new(2)
        .body(vec![row(vec![cell("a"), cell("b")])])
        .build();
    let src = render_typst(&table).unwrap();
    assert!(!src.contains("table.hline"), "{src}");
}

#[test]
fn every_colour_form_compiles() {
    if !require("typst") {
        return;
    }
    let forms = [
        "darkorchid",
        "#abc",
        "#abcd",
        "#12345678",
        "rgb(1 2 3 / 40%)",
        "hsl(200deg 50% 50%)",
        "transparent",
    ];
    let mut builder = TableBuilder::new(forms.len() as u32);
    let mut cells = Vec::new();
    for (i, c) in forms.iter().enumerate() {
        let id = format!("c{i}");
        builder = builder.style_def(
            &id,
            gridwell_ir::StyleDef {
                color: Some(c.to_string()),
                background_color: Some(c.to_string()),
                ..Default::default()
            },
        );
        cells.push(cell("x").style(&id));
    }
    let table = builder.body(vec![row(cells)]).build();
    let src = render_typst(&table).unwrap();
    assert!(src.contains("rgb(\"#12345678\")"), "{src}");
    compile(&work_dir("colours"), "colours", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
}
