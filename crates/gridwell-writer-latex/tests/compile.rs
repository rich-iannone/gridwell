//! Format-native oracle: the LaTeX we emit must compile, every cell's text must
//! come out of the PDF, and tricky text must come out verbatim.
//!
//! The corpus (CJK, Arabic, emoji, combining marks) is compiled with LuaLaTeX:
//! pdfLaTeX rejects characters outside its font encodings, which says nothing about
//! the writer. Span layouts and escaping use pdfLaTeX, the common case.
//!
//! Skipped when the tools are missing, unless `GRIDWELL_REQUIRE_LATEX` is set.

use std::path::{Path, PathBuf};
use std::process::Command;

use gridwell_testkit::spans::tilings;
use gridwell_testkit::{cell, column, examples, group, row, ColumnExt, TableBuilder};
use gridwell_writer_latex::render_latex;

/// Whether `tool` can be launched (exit status ignored: see the Typst oracle).
fn tool_available(tool: &str) -> bool {
    Command::new(tool).arg("-v").output().is_ok()
}

fn require(tool: &str) -> bool {
    if tool_available(tool) {
        return true;
    }
    if std::env::var_os("GRIDWELL_REQUIRE_LATEX").is_some_and(|v| !v.is_empty()) {
        panic!("{tool} not found but GRIDWELL_REQUIRE_LATEX is set");
    }
    eprintln!("skipping: {tool} not found");
    false
}

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gridwell-latex-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The packages the writer's output needs, on a page wide enough that a row of
/// cells stays on one line for pdftotext.
fn document(engine: &str, body: &str) -> String {
    // LuaLaTeX uses Unicode fonts natively; T1 is for pdfLaTeX.
    let fontenc = if engine == "pdflatex" {
        "\\usepackage[T1]{fontenc}\n"
    } else {
        ""
    };
    format!(
        "\\documentclass{{article}}\n{fontenc}\
         \\usepackage[a2paper,landscape,margin=1cm]{{geometry}}\n\
         \\usepackage{{booktabs,multirow,longtable}}\n\
         \\usepackage[table]{{xcolor}}\n\
         \\setlength{{\\parindent}}{{0pt}}\n\
         \\begin{{document}}\n{body}\n\\end{{document}}\n"
    )
}

/// Compile a document; `Err` carries the first error lines of the log.
fn compile(dir: &Path, stem: &str, body: &str) -> Result<PathBuf, String> {
    compile_with("pdflatex", dir, stem, body)
}

fn compile_with(engine: &str, dir: &Path, stem: &str, body: &str) -> Result<PathBuf, String> {
    std::fs::write(dir.join(format!("{stem}.tex")), document(engine, body)).unwrap();
    let out = Command::new(engine)
        .args(["-interaction=nonstopmode", "-halt-on-error"])
        .arg(format!("{stem}.tex"))
        .current_dir(dir)
        .output()
        .unwrap();
    if out.status.success() {
        Ok(dir.join(format!("{stem}.pdf")))
    } else {
        let log = std::fs::read_to_string(dir.join(format!("{stem}.log"))).unwrap_or_default();
        let err: Vec<&str> = log
            .lines()
            .skip_while(|l| !l.starts_with('!'))
            .take(8)
            .collect();
        Err(err.join("\n"))
    }
}

fn pdf_text(pdf: &Path) -> String {
    let out = Command::new("pdftotext")
        .arg("-layout")
        .arg(pdf)
        .arg("-")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn every_example_compiles_with_each_column_hidden() {
    if !require("lualatex") {
        return;
    }
    let dir = work_dir("examples");
    let mut failures = Vec::new();
    for ex in examples() {
        let base = ex.table();
        let mut body = render_latex(&base).unwrap();
        for hidden in 0..base.column_spec.len() {
            let mut t = base.clone();
            t.column_spec[hidden].hidden = true;
            body.push_str("\n\\par\\medskip\n");
            body.push_str(&render_latex(&t).unwrap());
        }
        if let Err(e) = compile_with("lualatex", &dir, ex.name, &body) {
            failures.push(format!("{}:\n{e}", ex.name));
        }
    }
    assert!(
        failures.is_empty(),
        "{} example(s) failed to compile:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn span_layouts_compile_and_show_every_cell() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    // One document per shape keeps this to a handful of pdflatex runs.
    let dir = work_dir("spans");
    for (rows, cols) in [(1, 4), (2, 3), (3, 3), (2, 4)] {
        let mut body = String::new();
        let mut labels = Vec::new();
        for (i, t) in tilings(rows, cols).into_iter().enumerate() {
            // Unique text per table so a missing cell can't hide behind another
            // table's copy.
            let mut table_rows = t.to_rows();
            for r in &mut table_rows {
                for c in &mut r.cells {
                    if let [gridwell_ir::content::ContentNode::Text { value }] =
                        c.content.as_mut_slice()
                    {
                        *value = format!("t{i}{value}");
                        labels.push(value.clone());
                    }
                }
            }
            let table = TableBuilder::new(cols as u32)
                .group(group(table_rows))
                .build();
            body.push_str(&render_latex(&table).unwrap());
            body.push_str("\n\\par\\medskip\n");
        }
        let stem = format!("spans{rows}x{cols}");
        let pdf = compile(&dir, &stem, &body)
            .unwrap_or_else(|e| panic!("{rows}x{cols} span layouts failed to compile:\n{e}"));
        let text = pdf_text(&pdf);
        let words: std::collections::HashSet<&str> = text.split_whitespace().collect();
        let missing: Vec<&String> = labels
            .iter()
            .filter(|l| !words.contains(l.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "{rows}x{cols}: cells missing from the PDF: {missing:?}"
        );
    }
}

#[test]
fn hidden_columns_never_reach_the_pdf() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    let dir = work_dir("hidden");
    let mut body = String::new();
    for t in tilings(2, 3) {
        for hidden in 0..3 {
            let mut rows = t.to_rows();
            for r in &mut rows {
                let c = &mut r.cells[hidden];
                if !c.is_placeholder && c.colspan == 1 {
                    c.content = vec![gridwell_ir::content::ContentNode::Text {
                        value: "HIDDENLEAK".into(),
                    }];
                }
            }
            let columns = (0..3)
                .map(|i| {
                    let c = column(&format!("c{i}"), "");
                    if i == hidden {
                        c.hidden()
                    } else {
                        c
                    }
                })
                .collect();
            let table = TableBuilder::new(3)
                .columns(columns)
                .group(group(rows))
                .build();
            body.push_str(&render_latex(&table).unwrap());
            body.push_str("\n\\par\\medskip\n");
        }
    }
    let pdf = compile(&dir, "hidden", &body).unwrap_or_else(|e| panic!("{e}"));
    assert!(!pdf_text(&pdf).contains("HIDDENLEAK"));
}

#[test]
fn tricky_text_renders_verbatim() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    let texts = [
        "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        "50% of $10 & #1 at ~home_dir",
        "\\textbf{not bold} \\\\ not a row",
        "{braces} and ^carets^",
        "café naïve Ærø",
    ];
    let rows = texts.iter().map(|t| row(vec![cell(t)])).collect();
    let src = render_latex(
        &TableBuilder::new(1)
            .head(row(vec![cell("header")]))
            .body(rows)
            .build(),
    )
    .unwrap();
    let dir = work_dir("verbatim");
    let pdf = compile(&dir, "verbatim", &src).unwrap_or_else(|e| panic!("{e}\n\n{src}"));
    let text = pdf_text(&pdf);
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let lines: Vec<String> = text.lines().map(squash).collect();
    let missing: Vec<&&str> = texts
        .iter()
        .filter(|t| !lines.contains(&squash(t)))
        .collect();
    assert!(
        missing.is_empty(),
        "not verbatim: {missing:?}\n\npdftotext:\n{text}"
    );
}

#[test]
fn styled_titles_labels_and_notes_compile() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    let style = |color: &str, bg: Option<&str>| gridwell_ir::StyleDef {
        color: Some(color.into()),
        background_color: bg.map(Into::into),
        font_style: Some("italic".into()),
        ..Default::default()
    };
    let mut t = TableBuilder::new(2)
        .title("Styled title")
        .subtitle("Styled subtitle")
        .extra_line("Extra line")
        .style_def("t", style("#AA0000", None))
        .style_def("l", style("#00AA00", Some("#EEEEEE")))
        .style_def("n", style("#0000AA", None))
        .group(
            gridwell_testkit::labeled_group("Styled label", vec![row(vec![cell("a"), cell("b")])])
                .label_style("l"),
        )
        .footnote("f", "1", "Styled footnote")
        .source_note("Styled source")
        .build();
    let h = t.header.as_mut().unwrap();
    h.title.as_mut().unwrap().style_id = Some("t".into());
    h.subtitle.as_mut().unwrap().style_id = Some("t".into());
    let f = t.footer.as_mut().unwrap();
    f.footnotes[0].style_id = Some("n".into());
    f.source_notes[0].style_id = Some("n".into());
    let src = render_latex(&t).unwrap();
    for want in [
        "\\textcolor[HTML]{AA0000}{\\textit{Styled title}}",
        "\\textcolor[HTML]{AA0000}{\\textit{Styled subtitle}}",
        "{\\small Extra line}",
        "\\cellcolor[HTML]{EEEEEE}\\textcolor[HTML]{00AA00}{\\textit{Styled label}}",
        "\\textcolor[HTML]{0000AA}{\\textit{Styled footnote}}",
        "\\textcolor[HTML]{0000AA}{\\textit{Styled source}}",
    ] {
        assert!(src.contains(want), "missing {want:?}:\n{src}");
    }
    let dir = work_dir("styled-lines");
    let pdf = compile(&dir, "styled", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
    let text = pdf_text(&pdf);
    for line in [
        "Styled title",
        "Styled subtitle",
        "Extra line",
        "Styled label",
        "Styled footnote",
        "Styled source",
    ] {
        assert!(text.contains(line), "{line} missing from PDF:\n{text}");
    }
}
