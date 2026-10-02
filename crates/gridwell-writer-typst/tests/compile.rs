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
use gridwell_testkit::{cell, examples, group, row, TableBuilder};
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
    if std::env::var_os("GRIDWELL_REQUIRE_TYPST").is_some() {
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
        "café 東京 😀",
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
