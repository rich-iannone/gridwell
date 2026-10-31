//! Format-native oracle: the Typst we emit must compile, and text must come out
//! verbatim.
//!
//! - Every corpus example (plus a styled merge) and every span layout from `gridwell_testkit::spans` is
//!   compiled with the `typst` CLI.
//! - Tricky text (all ASCII punctuation, markup look-alikes, line-start markers) is
//!   compiled to PDF and read back with `pdftotext`; it must appear unchanged.
//!
//! Skipped when the tools are missing, unless `GRIDWELL_REQUIRE_TYPST` is set (as in
//! the pinned harness image / CI).

use std::path::{Path, PathBuf};
use std::process::Command;

use gridwell_ir::style::StyleDef;
use gridwell_testkit::examples::{font_families, styled_merge};
use gridwell_testkit::spans::tilings;
use gridwell_testkit::{
    cell, column, examples, group, labeled_group, row, CellExt, ColumnExt, TableBuilder,
};
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
    // Plus a styled merged cell under a bordered group label, font lists, and a
    // font name needing escapes.
    let mut quoted = font_families();
    quoted.styles.defs.get_mut("serif").unwrap().font_family =
        Some(r#"'Odd "Name\', serif"#.into());
    let failures: Vec<String> = examples()
        .into_iter()
        .map(|ex| (ex.name, ex.table()))
        .chain([
            ("styled-merge", styled_merge()),
            ("font-families", font_families()),
            ("font-quoted", quoted),
        ])
        .filter_map(|(name, table)| {
            let src = render_typst(&table).unwrap();
            compile(&dir, name, &src).err().map(|e| {
                format!(
                    "{}:\n{}",
                    name,
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
fn line_styles_replace_their_defaults() {
    // Typst rejects a repeated `#text` argument: a style setting the size, color
    // or weight of a line must replace the line's default, not add to it.
    let mut table = TableBuilder::new(1)
        .style_def(
            "s",
            StyleDef {
                font_weight: Some("normal".into()),
                font_size: Some("20px".into()),
                color: Some("#9932CC".into()),
                background_color: Some("#123456".into()),
                ..StyleDef::default()
            },
        )
        .title("Title")
        .subtitle("Subtitle")
        .extra_line("Extra")
        .head(row(vec![cell("H")]))
        .group(labeled_group("Label", vec![row(vec![cell("x")])]).label_style("s"))
        .footnote("f1", "a", "Footnote")
        .source_note("Source")
        .build();
    let s = || Some("s".to_string());
    let header = table.header.as_mut().unwrap();
    header.title.as_mut().unwrap().style_id = s();
    header.subtitle.as_mut().unwrap().style_id = s();
    header.extra_lines[0].style_id = s();
    let footer = table.footer.as_mut().unwrap();
    footer.footnotes[0].style_id = s();
    footer.source_notes[0].style_id = s();
    let src = render_typst(&table).unwrap();

    // The weight the style sets is written, not left to the default.
    let text = "#text(weight: \"regular\", size: 15pt, fill: rgb(\"#9932CC\"))";
    let lines: Vec<&str> = src.lines().map(str::trim).collect();
    for want in [
        format!("#align(left)[{text}[Title]]"),
        format!("#align(left)[{text}[Subtitle]]"),
        format!("#align(left)[{text}[Extra]]"),
        format!("{text}[#super[a] Footnote]"),
        format!("{text}[Source]"),
        format!("table.cell(colspan: 1, fill: rgb(\"#123456\"))[{text}[Label]],"),
    ] {
        assert!(lines.contains(&want.as_str()), "no {want:?} in\n{src}");
    }
    // Only the header cell is bold; no line keeps its default gray.
    assert_eq!(src.matches("weight: \"bold\"").count(), 1, "{src}");
    assert!(!src.contains("luma"), "{src}");
    if require("typst") {
        compile(&work_dir("lines"), "lines", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
    }
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
    compile(&work_dir("colors"), "colors", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
}

#[test]
fn every_font_size_form_alignment_and_striping_compiles() {
    if !require("typst") {
        return;
    }
    let sizes = [
        "12px", "9pt", "1.2em", "1.5rem", "90%", "0", "small", "xx-large", "smaller", "larger",
    ];
    let mut builder = TableBuilder::new(2)
        .stub_cols(1)
        .striping(true, true)
        .columns(vec![
            column("a", "A").align("right"),
            column("b", "B").align("center"),
        ]);
    let mut rows = Vec::new();
    for (i, size) in sizes.iter().enumerate() {
        let id = format!("s{i}");
        builder = builder.style_def(
            &id,
            gridwell_ir::StyleDef {
                font_size: Some(size.to_string()),
                text_align: Some(["left", "right", "center", "justify"][i % 4].into()),
                background_color: Some("rgba(10, 20, 30, 0.3)".into()),
                ..Default::default()
            },
        );
        rows.push(row(vec![cell(size).style(&id), cell("x")]));
    }
    let src = render_typst(&builder.body(rows).build()).unwrap();
    for want in [
        "size: 0.9em",
        "size: 24pt",
        "size: 0.8333em",
        // Column `a` is right-aligned: only differing cell alignments are emitted.
        "align: left",
        "align: center",
        "fill: rgb(\"#0A141E4D\")",
    ] {
        assert!(src.contains(want), "missing {want:?}:\n{src}");
    }
    compile(&work_dir("styles"), "styles", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
}

/// Each word's left and right edge in the PDF, in points.
fn word_edges(pdf: &Path) -> std::collections::HashMap<String, (f64, f64)> {
    let out = Command::new("pdftotext")
        .arg("-bbox")
        .arg(pdf)
        .arg("-")
        .output()
        .unwrap();
    let xml = String::from_utf8_lossy(&out.stdout).into_owned();
    let attr = |tag: &str, name: &str| -> f64 {
        let v = tag.split(&format!("{name}=\"")).nth(1).unwrap();
        v.split('"').next().unwrap().parse::<f64>().unwrap()
    };
    xml.split("<word ")
        .skip(1)
        .map(|w| {
            let (tag, rest) = w.split_once('>').unwrap();
            let text = rest.split('<').next().unwrap().to_string();
            (text, (attr(tag, "xMin"), attr(tag, "xMax")))
        })
        .collect()
}

/// Column and table widths, measured in the PDF: a fixed column is its width, a
/// `%` column that share of the table's width, and `fr` columns divide what the
/// others leave, in proportion. A table with a width is that wide (a `%` width is
/// a share of the line); its columns without a width share it equally when no
/// column is `fr`. Without a width, `fr` columns fill the line.
#[test]
fn column_and_table_widths_are_measured_in_the_pdf() {
    if !require("typst") || !require("pdftotext") {
        return;
    }
    const INSET: f64 = 5.0; // Typst's default cell inset
    const LINE: f64 = (60.0 - 2.0) / 2.54 * 72.0; // the test page's line, in pt

    // A row of left-aligned words named `{tag}{i}`, then one right-aligned word in
    // the last column, `{tag}R`.
    let table = |tag: &str, table_width: Option<&str>, widths: &[&str]| {
        let n = widths.len();
        let cols = widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let c = column(&format!("c{i}"), "C");
                if w.is_empty() {
                    c
                } else {
                    c.width(w)
                }
            })
            .collect();
        let mut b = TableBuilder::new(n as u32).columns(cols).style_def(
            "right",
            gridwell_ir::StyleDef {
                text_align: Some("right".into()),
                ..Default::default()
            },
        );
        if let Some(w) = table_width {
            b = b.table_width(w);
        }
        let last = (0..n)
            .map(|i| {
                if i + 1 == n {
                    cell(&format!("{tag}R")).style("right")
                } else {
                    cell("")
                }
            })
            .collect();
        b.body(vec![
            row((0..n).map(|i| cell(&format!("{tag}{i}"))).collect()),
            row(last),
        ])
        .build()
    };
    let dir = work_dir("widths");
    // Each column's width, from the left edges and the table's right edge.
    let measure = |tag: &str, table_width: Option<&str>, widths: &[&str]| -> Vec<f64> {
        let src = render_typst(&table(tag, table_width, widths)).unwrap();
        let pdf = compile(&dir, tag, &src)
            .unwrap_or_else(|e| panic!("compile failed:\n{e}\n\nsource:\n{src}"));
        let words = word_edges(&pdf);
        let at = |i: usize| words[&format!("{tag}{i}")];
        let n = widths.len();
        let mut edges: Vec<f64> = (0..n).map(|i| at(i).0 - INSET).collect();
        edges.push(words[&format!("{tag}R")].1 + INSET);
        assert!(
            (edges[0] - 1.0 / 2.54 * 72.0).abs() < 0.5,
            "{tag} starts at {}\n{src}",
            edges[0]
        );
        edges.windows(2).map(|e| e[1] - e[0]).collect()
    };
    let near = |got: &[f64], want: &[f64], what: &str| {
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 0.5, "{what}: {got:.2?}, want {want:.2?}");
        }
    };
    // A 400pt table: 1fr and 3fr share what the fixed, 25% and `auto` columns leave.
    let got = measure("WA", Some("400pt"), &["100pt", "25%", "1fr", "3fr", ""]);
    let fr = (400.0 - 200.0 - got[4]) / 4.0;
    near(&got, &[100.0, 100.0, fr, 3.0 * fr, got[4]], "400pt table");
    assert!(got[4] > 2.0 * INSET && fr > 20.0, "{got:?}");
    // Columns without a width share the table's width.
    near(
        &measure("WB", Some("300pt"), &["", "", ""]),
        &[100.0; 3],
        "300pt table",
    );
    near(
        &measure("WC", Some("300pt"), &["60pt", ""]),
        &[60.0, 240.0],
        "300pt, fixed + auto",
    );
    // A `%` table width is a share of the line, which its `%` columns divide.
    let half = LINE / 2.0;
    near(
        &measure("WD", Some("50%"), &["25%", ""]),
        &[half / 4.0, half * 3.0 / 4.0],
        "50% table",
    );
    // Without a table width, `fr` fills the line.
    near(
        &measure("WE", None, &["100pt", "1fr"]),
        &[100.0, LINE - 100.0],
        "no table width",
    );
}
