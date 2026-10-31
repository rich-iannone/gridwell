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
         \\usepackage{{booktabs,multirow,longtable,tabularx}}\n\
         \\usepackage[normalem]{{ulem}}\n\
         \\usepackage[table]{{xcolor}}\n\
         \\setlength{{\\parindent}}{{0pt}}\n\
         \\begin{{document}}\n{body}\n\\end{{document}}\n"
    )
}

/// Compile a document; `Err` carries the first error lines of the log.
fn compile(dir: &Path, stem: &str, body: &str) -> Result<PathBuf, String> {
    compile_with("pdflatex", dir, stem, body)
}

/// Again while longtable asks for it: its column widths settle over runs.
fn compile_with(engine: &str, dir: &Path, stem: &str, body: &str) -> Result<PathBuf, String> {
    std::fs::write(dir.join(format!("{stem}.tex")), document(engine, body)).unwrap();
    for _ in 0..3 {
        let out = Command::new(engine)
            .args(["-interaction=nonstopmode", "-halt-on-error"])
            .arg(format!("{stem}.tex"))
            .current_dir(dir)
            .output()
            .unwrap();
        let log = std::fs::read_to_string(dir.join(format!("{stem}.log"))).unwrap_or_default();
        if !out.status.success() {
            let err: Vec<&str> = log
                .lines()
                .skip_while(|l| !l.starts_with('!'))
                .take(8)
                .collect();
            return Err(err.join("\n"));
        }
        if !log.contains("Rerun LaTeX") {
            break;
        }
    }
    Ok(dir.join(format!("{stem}.pdf")))
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
    let dir = work_dir("styled-lines");
    for longtable in [false, true] {
        let src = gridwell_writer_latex::LatexWriter::with_config(
            gridwell_writer_latex::LatexWriterConfig {
                longtable,
                ..Default::default()
            },
        )
        .render(&t)
        .unwrap();
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
        let stem = format!("styled-{longtable}");
        let pdf = compile(&dir, &stem, &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
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
}

/// Underline and strike-through (`ulem`) must survive line breaks: `\uline`
/// can't wrap the nested tabular a multi-line cell becomes, so each line is
/// decorated instead. Found by the property test (LaTeX failed with "Extra }").
#[test]
fn decorated_multiline_cells_compile() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    use gridwell_testkit::{cell_content, line_break, styled, text, CellExt};
    let def = |f: fn(&mut gridwell_ir::StyleDef)| {
        let mut d = gridwell_ir::StyleDef::default();
        f(&mut d);
        d
    };
    let lines = |a: &str, b: &str| cell_content(vec![text(a), line_break(), text(b)]);
    let table = TableBuilder::new(2)
        .style_def("u", def(|d| d.text_decoration = Some("underline".into())))
        .style_def(
            "s",
            def(|d| d.text_decoration = Some("line-through".into())),
        )
        .style_def(
            "big",
            def(|d| {
                d.text_decoration = Some("underline".into());
                d.font_size = Some("1.25em".into());
            }),
        )
        .head(row(vec![cell("H1"), cell("H2")]))
        .body(vec![
            row(vec![
                lines("UNDERONE", "UNDERTWO").style("u"),
                lines("STRUCKONE", "STRUCKTWO").style("s"),
            ]),
            row(vec![
                lines("BIGONE", "BIGTWO").style("big"),
                cell_content(vec![
                    text("plain "),
                    styled("RUN", "u"),
                    line_break(),
                    styled("NEXT", "s"),
                ]),
            ]),
        ])
        .title("Underlined title")
        .build();
    let src = render_latex(&table).unwrap();
    assert!(!src.contains("\\uline{\\begin{tabular}"), "{src}");
    assert!(
        src.contains("\\uline{UNDERONE}\\\\\\uline{UNDERTWO}"),
        "{src}"
    );
    let dir = work_dir("decorated");
    let pdf = compile(&dir, "decorated", &src)
        .unwrap_or_else(|e| panic!("compile failed:\n{e}\n\nsource:\n{src}"));
    let text = pdf_text(&pdf);
    for word in [
        "UNDERONE",
        "UNDERTWO",
        "STRUCKONE",
        "STRUCKTWO",
        "BIGONE",
        "BIGTWO",
        "RUN",
        "NEXT",
    ] {
        assert!(text.contains(word), "{word} missing:\n{text}");
    }
}

/// Cell borders are lines and rules of their own (`\omit` lines between rows,
/// `!{\vrule}` in `\multicolumn` specs): every span layout, every cell boxed in
/// one of the four styles (some aligned right), with and without booktabs, in a
/// tabular and a longtable, with natural and with `fr` and `%` widths, must
/// compile and keep every cell's text.
#[test]
fn bordered_span_layouts_compile_in_every_mode() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    use gridwell_testkit::{border, border_all};
    use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};
    let dir = work_dir("borders");
    let styles = ["solid", "dashed", "dotted", "double"];
    let modes = [(true, false), (false, false), (true, true), (false, true)];
    for ((rows, cols), widths) in [(2, 3), (3, 3)]
        .into_iter()
        .flat_map(|shape| [(shape, false), (shape, true)])
    {
        for (booktabs, longtable) in modes {
            let writer = LatexWriter::with_config(LatexWriterConfig {
                booktabs,
                longtable,
                ..Default::default()
            });
            let mut body = String::new();
            let mut labels = Vec::new();
            for (i, t) in tilings(rows, cols).into_iter().enumerate() {
                let mut table_rows = t.to_rows();
                for (n, c) in table_rows.iter_mut().flat_map(|r| &mut r.cells).enumerate() {
                    if let [gridwell_ir::content::ContentNode::Text { value }] =
                        c.content.as_mut_slice()
                    {
                        *value = format!("t{i}{value}");
                        labels.push(value.clone());
                        c.style_id = Some(styles[(i + n) % styles.len()].into());
                    }
                }
                let mut tb = TableBuilder::new(cols as u32);
                // The last style has no color: drawn in the text's.
                for (k, s) in styles.iter().enumerate() {
                    let mut edge = border(&format!("{}px", k + 1), s, "#CC0000");
                    edge.color = ["#CC0000", "#00AA00", "#0000BB"]
                        .get(k)
                        .map(|c| c.to_string());
                    let def = gridwell_ir::StyleDef {
                        border: Some(border_all(edge)),
                        text_align: (k == 1).then(|| "right".into()),
                        ..Default::default()
                    };
                    tb = tb.style_def(s, def);
                }
                if widths {
                    tb = tb.columns(
                        ["1fr", "25%", "2fr"]
                            .iter()
                            .enumerate()
                            .map(|(c, w)| column(&format!("c{c}"), "H").width(w))
                            .collect(),
                    );
                }
                let table = tb
                    .head(row(vec![cell("H"); cols]))
                    .group(group(table_rows))
                    .build();
                body.push_str(&writer.render(&table).unwrap());
                body.push_str("\n\\par\\medskip\n");
            }
            let stem = format!("borders{rows}x{cols}-{booktabs}-{longtable}-{widths}");
            let pdf = compile(&dir, &stem, &body).unwrap_or_else(|e| {
                panic!("{stem}: bordered span layouts failed to compile:\n{e}")
            });
            let text = pdf_text(&pdf);
            let words: std::collections::HashSet<&str> = text.split_whitespace().collect();
            let missing: Vec<&String> = labels
                .iter()
                .filter(|l| !words.contains(l.as_str()))
                .collect();
            assert!(
                missing.is_empty(),
                "{stem}: cells missing from the PDF: {missing:?}"
            );
        }
    }
}

/// Each word of the PDF with its left and right edges (its advance, not its
/// ink), in TeX points.
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
        v.split('"').next().unwrap().parse::<f64>().unwrap() * 72.27 / 72.0
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

/// Column widths, measured in the PDF: a fixed column is its width, a `%`
/// column that share of the table's width less the columns' padding, and `fr`
/// columns divide the rest in proportion; a cell aligned apart from its column
/// keeps the column's width. In a tabular (`tabularx`), the table is exactly its
/// `table_width`; in a longtable, the `fr` columns' share leaves out
/// natural-width columns.
#[test]
fn column_widths_are_measured_in_the_pdf() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    use gridwell_testkit::CellExt;
    use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};
    const SEP: f64 = 6.0; // `\tabcolsep`
    const W: f64 = 500.0;
    let table = |tag: &str| {
        let w = |s: &str| format!("{tag}{s}");
        TableBuilder::new(5)
            .table_width("500pt")
            .style_def(
                "right",
                gridwell_ir::StyleDef {
                    text_align: Some("right".into()),
                    ..Default::default()
                },
            )
            .columns(vec![
                column("a", "A").width("90pt").align("left"),
                column("b", "B").width("25%").align("left"),
                column("c", "C").width("1fr").align("left"),
                column("d", "D").width("2fr").align("right"),
                column("e", "E").align("left"),
            ])
            .body(vec![
                row(vec![
                    cell(&w("QA")),
                    cell(&w("QB")),
                    cell(&w("QC")),
                    cell(&w("QD")),
                    cell(&w("QE")),
                ]),
                row(vec![
                    cell(&w("RA")).style("right"),
                    cell(&w("RB")).style("right"),
                    cell(&w("RC")).style("right"),
                    cell(&w("RD")),
                    cell(""),
                ]),
            ])
            .build()
    };
    let dir = work_dir("widths");
    for longtable in [false, true] {
        let tag = if longtable { "L" } else { "T" };
        let src = LatexWriter::with_config(LatexWriterConfig {
            longtable,
            ..Default::default()
        })
        .render(&table(tag))
        .unwrap();
        let env = if longtable { "longtable" } else { "tabularx" };
        assert!(src.contains(&format!("\\begin{{{env}}}")), "{src}");
        let pdf = compile(&dir, &format!("widths-{tag}"), &src)
            .unwrap_or_else(|e| panic!("compile failed:\n{e}\n\nsource:\n{src}"));
        let words = word_edges(&pdf);
        let at = |w: &str| words[&format!("{tag}{w}")];
        // Column edges from the left-aligned words, and the right-aligned ones.
        let left: Vec<f64> = ["QA", "QB", "QC", "QE"]
            .iter()
            .map(|w| at(w).0 - SEP)
            .collect();
        let (l0, l1, l2, l4) = (left[0], left[1], left[2], left[3]);
        let r0 = at("RA").1 + SEP;
        let r1 = at("RB").1 + SEP;
        let r2 = at("RC").1 + SEP;
        let r3 = at("QD").1 + SEP;
        let near = |got: f64, want: f64, what: &str| {
            assert!(
                (got - want).abs() < 0.2,
                "{tag} {what}: {got:.2}pt, want {want:.2}pt\n{src}"
            );
        };
        near(at("RD").1 + SEP, r3, "fr column's rows");
        let content = W - 10.0 * SEP;
        near(l1 - l0, 90.0 + 2.0 * SEP, "fixed column");
        near(r0, l1, "right-aligned cell in the fixed column");
        near(l2 - l1, 0.25 * content + 2.0 * SEP, "25% column");
        near(r1, l2, "right-aligned cell in the % column");
        near(r3, l4, "right-aligned fr column");
        let fr1 = r2 - l2 - 2.0 * SEP;
        let fr2 = r3 - r2 - 2.0 * SEP;
        if longtable {
            near(fr1, (0.75 * content - 90.0) / 3.0, "1fr column");
        } else {
            let table = at("QE").1 + SEP - l0;
            near(table, W, "table width");
        }
        near(fr2, 2.0 * fr1, "2fr column");
    }
}

/// A table with a width and no `fr` column is that wide: its columns without a
/// width share what the others leave, in a tabular (`tabularx`) and a longtable.
#[test]
fn columns_without_a_width_fill_the_table_width() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    use gridwell_testkit::CellExt;
    use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};
    const SEP: f64 = 6.0; // `\tabcolsep`
    let table = |tag: &str| {
        let w = |s: &str| format!("{tag}{s}");
        TableBuilder::new(3)
            .table_width("300pt")
            .style_def(
                "right",
                gridwell_ir::StyleDef {
                    text_align: Some("right".into()),
                    ..Default::default()
                },
            )
            .columns(vec![
                column("a", "A"),
                column("b", "B").width("60pt"),
                column("c", "C"),
            ])
            .body(vec![
                row(vec![cell(&w("QA")), cell(&w("QB")), cell(&w("QC"))]),
                row(vec![cell(""), cell(""), cell(&w("RC")).style("right")]),
            ])
            .build()
    };
    let dir = work_dir("fill");
    for longtable in [false, true] {
        let tag = if longtable { "L" } else { "T" };
        let src = LatexWriter::with_config(LatexWriterConfig {
            longtable,
            ..Default::default()
        })
        .render(&table(tag))
        .unwrap();
        let pdf = compile(&dir, &format!("fill-{tag}"), &src)
            .unwrap_or_else(|e| panic!("compile failed:\n{e}\n\nsource:\n{src}"));
        let words = word_edges(&pdf);
        let at = |w: &str| words[&format!("{tag}{w}")];
        let edges = [
            at("QA").0 - SEP,
            at("QB").0 - SEP,
            at("QC").0 - SEP,
            at("RC").1 + SEP,
        ];
        let got: Vec<f64> = edges.windows(2).map(|e| e[1] - e[0]).collect();
        let fill = (300.0 - 60.0 - 6.0 * SEP) / 2.0 + 2.0 * SEP;
        for (g, want) in got.iter().zip([fill, 60.0 + 2.0 * SEP, fill]) {
            assert!(
                (g - want).abs() < 0.2,
                "{tag}: {got:.2?}, want {fill:.2} / 72 / {fill:.2}\n{src}"
            );
        }
    }
}

/// A longtable over pages: the title and subtitle open the first page only, the
/// column labels head every page, and the notes close the last page only, after
/// every row. Each row is shown once. The title starts at the table's left edge
/// (both at the margin), a note wider than the table doesn't widen it, and the
/// columns are as wide on every page (a wide cell on the last page widens the
/// first page's column too, once LaTeX has rerun).
#[test]
fn a_longtable_over_pages_repeats_its_head_and_keeps_title_and_notes() {
    if !require("pdflatex") || !require("pdftotext") {
        return;
    }
    use gridwell_testkit::CellExt;
    use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};
    const SEP: f64 = 6.0; // `\tabcolsep`
    const ROWS: usize = 250;
    let long_note = format!("LTSOURCE {} LTEND", vec!["wordy"; 40].join(" "));
    let table = |notes: bool| {
        let mut tb = TableBuilder::new(2)
            .title("LTTITLE")
            .subtitle("LTSUB")
            .style_def(
                "right",
                gridwell_ir::StyleDef {
                    text_align: Some("right".into()),
                    ..Default::default()
                },
            )
            .columns(vec![column("a", "HEADA"), column("b", "HEADB")])
            .head(row(vec![cell("HEADA"), cell("HEADB").style("right")]))
            .body(
                (0..ROWS)
                    .map(|i| {
                        row(vec![
                            cell(&format!("R{i}")),
                            cell(&if i == ROWS - 10 {
                                "WIDEWIDEWIDEWIDE".to_string()
                            } else {
                                format!("V{i}")
                            })
                            .style("right"),
                        ])
                    })
                    .collect(),
            );
        if notes {
            tb = tb.footnote("f", "a", "LTNOTE").source_note(&long_note);
        }
        tb.build()
    };
    let render = |t: gridwell_ir::Table| {
        LatexWriter::with_config(LatexWriterConfig {
            longtable: true,
            ..Default::default()
        })
        .render(&t)
        .unwrap()
    };
    let dir = work_dir("longtable-pages");
    let src = render(table(true));
    let pdf = compile(&dir, "pages", &src).unwrap_or_else(|e| panic!("{e}\n{src}"));
    let text = pdf_text(&pdf);
    let pages: Vec<Vec<&str>> = text
        .split('\x0c')
        .map(|p| p.split_whitespace().collect::<Vec<_>>())
        .filter(|p| !p.is_empty())
        .collect();
    assert!(pages.len() >= 3, "{} pages:\n{text}", pages.len());
    let count = |p: &[&str], w: &str| p.iter().filter(|x| **x == w).count();
    let last = pages.len() - 1;
    for (n, p) in pages.iter().enumerate() {
        let first = usize::from(n == 0);
        assert_eq!(count(p, "LTTITLE"), first, "page {n}: {p:?}");
        assert_eq!(count(p, "LTSUB"), first, "page {n}: {p:?}");
        assert_eq!(count(p, "HEADA"), 1, "page {n}: {p:?}");
        assert_eq!(count(p, "HEADB"), 1, "page {n}: {p:?}");
        assert_eq!(
            count(p, "LTNOTE"),
            usize::from(n == last),
            "page {n}: {p:?}"
        );
        assert_eq!(count(p, "wordy") > 0, n == last, "page {n}: {p:?}");
        // The labels come first on the page, below the title.
        let head = p.iter().position(|w| *w == "HEADA").unwrap();
        assert!(
            p[..head].iter().all(|w| ["LTTITLE", "LTSUB"].contains(w)),
            "page {n}: {p:?}"
        );
    }
    let all: Vec<&str> = pages.concat();
    for i in 0..ROWS {
        assert_eq!(count(&all, &format!("R{i}")), 1, "R{i}");
    }
    let row_end = all.iter().rposition(|w| w.starts_with('V')).unwrap();
    let note = all.iter().position(|w| *w == "LTNOTE").unwrap();
    assert!(note > row_end, "notes after the last row: {all:?}");
    // Positions (first page): the title at the table's edge, and the table as
    // wide with the long note as without.
    let words = word_edges(&pdf);
    let near = |got: f64, want: f64, what: &str| {
        assert!(
            (got - want).abs() < 0.2,
            "{what}: {got:.2}pt, want {want:.2}pt"
        );
    };
    near(
        words["LTTITLE"].0,
        words["HEADA"].0 - SEP,
        "title's left edge",
    );
    near(
        words["LTSOURCE"].0,
        words["HEADA"].0 - SEP,
        "note's left edge",
    );
    let plain = compile(&dir, "plain", &render(table(false))).unwrap();
    let plain = word_edges(&plain);
    near(
        words["HEADB"].1,
        plain["HEADB"].1,
        "right edge with a wide note",
    );
    near(words["V0"].1, plain["V0"].1, "right edge with a wide note");
    near(
        words["V0"].1,
        words["V249"].1,
        "column width on the first and last page",
    );
    assert!(
        words["LTEND"].1 > words["HEADB"].1 + 500.0,
        "the note is wider than the table"
    );
}
