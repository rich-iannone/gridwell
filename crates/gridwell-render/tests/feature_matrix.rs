//! The feature matrix (M3): which IR features each Tier-1 writer expresses,
//! measured, not claimed.
//!
//! Every feature has a probe table: a `TARGET` cell carrying the feature and
//! `CONTROL` cells that don't — one beside it, and two in the next row, so
//! formatting that leaks into later cells fails too. Each format's output is read
//! back with a style reader (`support::look`) and the probe's check decides ✓.
//! Every (feature, format) pair declares ✓ or ✗ with a reason; the test fails
//! when the measurement disagrees in either direction, so implementing a feature
//! means flipping its ✗ — and a regression can't go unnoticed.
//!
//! The declared matrix is published as `docs/guide/feature-matrix.qmd`, which
//! this test keeps in sync (`GRIDWELL_BLESS_MATRIX=1` rewrites it).
//! `GRIDWELL_MATRIX_DISCOVER=1` prints the measured matrix (with each failing
//! check's reason) instead of asserting.

mod support;

use gridwell_ir::style::{BorderSet, ConditionalSelector, StyleDef};
use gridwell_ir::Table;
use gridwell_testkit::{
    border, cell, cell_content, column, footnote_mark, row, styled, text, CellExt, ColumnExt,
    RowExt, TableBuilder,
};
use support::look::{self, Align, Doc, Line, Look, Rgb, VAlign};
use support::typeset;

const FORMATS: &[&str] = &["html", "latex", "typst", "rtf", "docx", "xlsx"];

/// Read one table rendered to `format`.
fn read_one(format: &str, t: &Table) -> Option<Doc> {
    Some(match format {
        "html" => look::html(&gridwell_writer_html::render_html(t).unwrap()),
        "latex" => look::latex(&gridwell_writer_latex::render_latex(t).unwrap()),
        "rtf" => look::rtf(&gridwell_writer_rtf::render_rtf(t).unwrap()),
        "docx" => look::docx(
            &gridwell_writer_docx::DocxWriter::new()
                .render_document_xml(t)
                .unwrap(),
        ),
        "xlsx" => {
            let w = gridwell_writer_xlsx::XlsxWriter::new();
            look::xlsx(
                &w.render_sheet_xml(t).unwrap(),
                &w.render_styles_xml(t).unwrap(),
            )
        }
        _ => return None,
    })
}

/// Read every probe table rendered to `format` (`None`: no reader, or its tool
/// is missing and not required).
fn read_all(format: &str, tables: &[Table]) -> Option<Vec<Result<Doc, String>>> {
    match format {
        "typst" => {
            if !typeset::require("typst", "GRIDWELL_REQUIRE_TYPST") {
                return None;
            }
            let sources: Vec<String> = tables
                .iter()
                .map(|t| gridwell_writer_typst::render_typst(t).unwrap())
                .collect();
            Some(typeset::typst_looks(&sources))
        }
        f => tables.iter().map(|t| read_one(f, t).map(Ok)).collect(),
    }
}

// ─── Probes ───

const PURPLE: Rgb = (0x99, 0x32, 0xCC);
const NAVY: Rgb = (0x12, 0x34, 0x56);
const RED: Rgb = (0xFF, 0x00, 0x00);

/// The probe table: `TARGET` (styled with `s`) and three controls.
fn probe(def: StyleDef) -> Table {
    TableBuilder::new(2)
        .style_def("s", def)
        .head(row(vec![cell("H1"), cell("H2")]))
        .body(vec![
            row(vec![cell("TARGET").style("s"), cell("CONTROLA")]),
            row(vec![cell("CONTROLB"), cell("CONTROLC")]),
        ])
        .build()
}

fn def(f: impl FnOnce(&mut StyleDef)) -> StyleDef {
    let mut d = StyleDef::default();
    f(&mut d);
    d
}

type Check = fn(&Doc) -> Result<(), String>;

fn get<'d>(doc: &'d Doc, text: &str) -> Result<&'d Look, String> {
    doc.cell(text).ok_or_else(|| {
        format!(
            "no cell {text:?} in {:?}",
            doc.cells.iter().map(|c| &c.text).collect::<Vec<_>>()
        )
    })
}

/// `TARGET` has the property; no control does.
fn target_only(doc: &Doc, what: &str, has: impl Fn(&Look) -> bool) -> Result<(), String> {
    let target = get(doc, "TARGET")?;
    if !has(target) {
        return Err(format!("TARGET lacks {what}: {target:?}"));
    }
    for c in ["CONTROLA", "CONTROLB", "CONTROLC"] {
        let l = get(doc, c)?;
        if has(l) {
            return Err(format!("{what} leaks into {c}: {l:?}"));
        }
    }
    Ok(())
}

fn near(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 0.26)
}

struct Feature {
    id: &'static str,
    /// What the feature is, for the docs page.
    what: &'static str,
    table: fn() -> Table,
    check: Check,
}

fn features() -> Vec<Feature> {
    vec![
        Feature {
            id: "bold",
            what: "`font_weight: bold` (or ≥ 600)",
            table: || probe(def(|d| d.font_weight = Some("bold".into()))),
            check: |d| target_only(d, "bold", |l| l.all(|r| r.bold)),
        },
        Feature {
            id: "italic",
            what: "`font_style: italic`",
            table: || probe(def(|d| d.font_style = Some("italic".into()))),
            check: |d| target_only(d, "italic", |l| l.all(|r| r.italic)),
        },
        Feature {
            id: "underline",
            what: "`text_decoration: underline`",
            table: || probe(def(|d| d.text_decoration = Some("underline".into()))),
            check: |d| target_only(d, "underline", |l| l.all(|r| r.underline)),
        },
        Feature {
            id: "strikethrough",
            what: "`text_decoration: line-through`",
            table: || probe(def(|d| d.text_decoration = Some("line-through".into()))),
            check: |d| target_only(d, "strikethrough", |l| l.all(|r| r.strike)),
        },
        Feature {
            id: "text-color",
            what: "`color`",
            table: || probe(def(|d| d.color = Some("#9932CC".into()))),
            check: |d| target_only(d, "colour #9932CC", |l| l.all(|r| r.color == Some(PURPLE))),
        },
        Feature {
            id: "font-size",
            what: "`font_size` (20px = 15pt)",
            table: || probe(def(|d| d.font_size = Some("20px".into()))),
            check: |d| target_only(d, "15pt text", |l| l.all(|r| near(r.size_pt, 15.0))),
        },
        Feature {
            id: "font-family",
            what: "`font_family` (a named family, not just monospace)",
            table: || probe(def(|d| d.font_family = Some("Georgia, serif".into()))),
            check: |d| {
                target_only(d, "Georgia", |l| {
                    l.all(|r| {
                        r.family
                            .as_deref()
                            .is_some_and(|f| f.to_ascii_lowercase().contains("georgia"))
                    })
                })
            },
        },
        Feature {
            id: "text-transform",
            what: "`text_transform: uppercase`",
            table: || {
                let mut t = probe(def(|d| d.text_transform = Some("uppercase".into())));
                t.table.tbody[0].rows[0].cells[0].content = vec![text("target")];
                t
            },
            check: |d| get(d, "TARGET").map(|_| ()),
        },
        Feature {
            id: "fill",
            what: "`background_color`",
            table: || probe(def(|d| d.background_color = Some("#123456".into()))),
            check: |d| target_only(d, "fill #123456", |l| l.fill == Some(NAVY)),
        },
        Feature {
            id: "cell-align",
            what: "`text_align` on a cell",
            table: || probe(def(|d| d.text_align = Some("right".into()))),
            check: |d| target_only(d, "right alignment", |l| l.align == Some(Align::Right)),
        },
        Feature {
            id: "column-align",
            what: "a column's `align`",
            table: || {
                let mut t = probe(StyleDef::default());
                t.column_spec[0].align = "right".into();
                t
            },
            check: |d| {
                for c in ["TARGET", "CONTROLB"] {
                    let l = get(d, c)?;
                    if l.align != Some(Align::Right) {
                        return Err(format!("{c} not right-aligned: {l:?}"));
                    }
                }
                let l = get(d, "CONTROLA")?;
                match l.align {
                    Some(Align::Right) => Err(format!("column 2 right-aligned too: {l:?}")),
                    _ => Ok(()),
                }
            },
        },
        Feature {
            id: "vertical-align",
            what: "`vertical_align: bottom`",
            table: || probe(def(|d| d.vertical_align = Some("bottom".into()))),
            check: |d| target_only(d, "bottom alignment", |l| l.valign == Some(VAlign::Bottom)),
        },
        Feature {
            id: "border",
            what: "`border` (2px solid red bottom)",
            table: || {
                probe(def(|d| {
                    d.border = Some(BorderSet {
                        top: None,
                        right: None,
                        bottom: Some(border("2px", "solid", "#FF0000")),
                        left: None,
                    })
                }))
            },
            check: |d| {
                target_only(d, "a 1.5pt solid red bottom border", |l| {
                    l.borders[2].is_some_and(|e| {
                        e.line == Line::Solid && e.color == Some(RED) && near(e.width_pt, 1.5)
                    })
                })
            },
        },
        Feature {
            id: "border-styles",
            what: "border styles dashed, dotted, double",
            table: || {
                TableBuilder::new(3)
                    .style_def("dash", def(|d| d.border = Some(bottom("dashed"))))
                    .style_def("dot", def(|d| d.border = Some(bottom("dotted"))))
                    .style_def("dbl", def(|d| d.border = Some(bottom("double"))))
                    .head(row(vec![cell("H1"), cell("H2"), cell("H3")]))
                    .body(vec![
                        row(vec![
                            cell("DASHED").style("dash"),
                            cell("DOTTED").style("dot"),
                            cell("DOUBLE").style("dbl"),
                        ]),
                        row(vec![cell("x"), cell("y"), cell("z")]),
                    ])
                    .build()
            },
            check: |d| {
                for (text, line) in [
                    ("DASHED", Line::Dashed),
                    ("DOTTED", Line::Dotted),
                    ("DOUBLE", Line::Double),
                ] {
                    let l = get(d, text)?;
                    if l.borders[2].map(|e| e.line) != Some(line) {
                        return Err(format!("{text}: bottom border {:?}", l.borders[2]));
                    }
                }
                Ok(())
            },
        },
        Feature {
            id: "column-style",
            what: "a column's `style_id` (cascades to its cells)",
            table: || {
                TableBuilder::new(2)
                    .style_def("s", def(|d| d.background_color = Some("#123456".into())))
                    .columns(vec![column("a", "A").style("s"), column("b", "B")])
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![cell("TARGET"), cell("CONTROLA")]),
                        row(vec![cell("TARGETB"), cell("CONTROLC")]),
                    ])
                    .build()
            },
            check: |d| {
                for c in ["TARGET", "TARGETB"] {
                    if get(d, c)?.fill != Some(NAVY) {
                        return Err(format!("{c} not filled: {:?}", get(d, c)?));
                    }
                }
                match get(d, "CONTROLA")?.fill {
                    Some(NAVY) => Err("column 2 filled too".into()),
                    _ => Ok(()),
                }
            },
        },
        Feature {
            id: "row-style",
            what: "a row's `style_id`",
            table: || {
                TableBuilder::new(2)
                    .style_def("s", def(|d| d.background_color = Some("#123456".into())))
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![cell("TARGET"), cell("TARGETB")]).style("s"),
                        row(vec![cell("CONTROLB"), cell("CONTROLC")]),
                    ])
                    .build()
            },
            check: |d| {
                for c in ["TARGET", "TARGETB"] {
                    if get(d, c)?.fill != Some(NAVY) {
                        return Err(format!("{c} not filled: {:?}", get(d, c)?));
                    }
                }
                for c in ["CONTROLB", "CONTROLC"] {
                    if get(d, c)?.fill == Some(NAVY) {
                        return Err(format!("{c} filled too"));
                    }
                }
                Ok(())
            },
        },
        Feature {
            id: "striping",
            what: "`row_striping` (every second body row filled)",
            table: || {
                TableBuilder::new(2)
                    .striping(true, true)
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![cell("ODD1"), cell("ODD2")]),
                        row(vec![cell("EVEN1"), cell("EVEN2")]),
                        row(vec![cell("ODD3"), cell("ODD4")]),
                    ])
                    .build()
            },
            check: |d| {
                let fill = |t| get(d, t).map(|l| l.fill);
                let even = fill("EVEN1")?;
                if even.is_none() || fill("EVEN2")? != even {
                    return Err(format!("even row not filled: {:?}", get(d, "EVEN1")?));
                }
                for t in ["ODD1", "ODD2", "ODD3", "ODD4"] {
                    if fill(t)? == even {
                        return Err(format!("{t} striped too"));
                    }
                }
                Ok(())
            },
        },
        Feature {
            id: "conditional-style",
            what: "`styles.conditionals` (row parity)",
            table: || {
                TableBuilder::new(2)
                    .conditional(
                        "c",
                        ConditionalSelector {
                            row_parity: Some("even".into()),
                            scope: Some("tbody".into()),
                        },
                        def(|d| d.background_color = Some("#123456".into())),
                    )
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![cell("ODD1"), cell("ODD2")]),
                        row(vec![cell("EVEN1"), cell("EVEN2")]),
                    ])
                    .build()
            },
            check: |d| {
                if get(d, "EVEN1")?.fill != Some(NAVY) || get(d, "EVEN2")?.fill != Some(NAVY) {
                    return Err(format!("even row not filled: {:?}", get(d, "EVEN1")?));
                }
                if get(d, "ODD1")?.fill == Some(NAVY) {
                    return Err("odd row filled".into());
                }
                Ok(())
            },
        },
        Feature {
            id: "style-composition",
            what: "`styles.compositions` (extends + overrides)",
            table: || {
                TableBuilder::new(2)
                    .style_def("base", def(|d| d.background_color = Some("#123456".into())))
                    .composition("s", "base", def(|d| d.font_weight = Some("bold".into())))
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![cell("TARGET").style("s"), cell("CONTROLA")]),
                        row(vec![cell("CONTROLB"), cell("CONTROLC")]),
                    ])
                    .build()
            },
            check: |d| {
                target_only(d, "fill and bold", |l| {
                    l.fill == Some(NAVY) && l.all(|r| r.bold)
                })
            },
        },
        Feature {
            id: "styled-run",
            what: "inline `styled_text` (part of a cell bold)",
            table: || {
                TableBuilder::new(2)
                    .style_def("b", def(|d| d.font_weight = Some("bold".into())))
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![
                            cell_content(vec![
                                text("plain "),
                                styled("STRONG", "b"),
                                text(" tail"),
                            ]),
                            cell("CONTROLA"),
                        ]),
                        row(vec![cell("CONTROLB"), cell("CONTROLC")]),
                    ])
                    .build()
            },
            check: |d| {
                let l = d
                    .cells
                    .iter()
                    .find(|c| c.text.contains("STRONG"))
                    .ok_or("no styled cell")?;
                let strong = l.run_with("STRONG").ok_or("no STRONG run")?;
                let plain = l.run_with("plain").ok_or("no plain run")?;
                let tail = l.run_with("tail").ok_or("no tail run")?;
                if !strong.bold || plain.bold || tail.bold {
                    return Err(format!("runs: {:?}", l.runs));
                }
                if get(d, "CONTROLA")?.any(|r| r.bold) {
                    return Err("bold leaks into CONTROLA".into());
                }
                Ok(())
            },
        },
        Feature {
            id: "footnote-mark",
            what: "footnote marks as superscript",
            table: || {
                TableBuilder::new(2)
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![
                        row(vec![
                            cell_content(vec![text("TARGET"), footnote_mark("f", "7")]),
                            cell("CONTROLA"),
                        ]),
                        row(vec![cell("CONTROLB"), cell("CONTROLC")]),
                    ])
                    .footnote("f", "7", "The note.")
                    .build()
            },
            check: |d| {
                let l = d
                    .cells
                    .iter()
                    .find(|c| c.text.contains("TARGET"))
                    .ok_or("no TARGET cell")?;
                let mark = l.run_with("7").ok_or("no mark run")?;
                if !mark.superscript || l.run_with("TARGET").is_some_and(|r| r.superscript) {
                    return Err(format!("runs: {:?}", l.runs));
                }
                if get(d, "CONTROLA")?.any(|r| r.superscript) {
                    return Err("superscript leaks into CONTROLA".into());
                }
                Ok(())
            },
        },
        Feature {
            id: "title",
            what: "`header.title` and `header.subtitle`",
            table: || {
                TableBuilder::new(2)
                    .title("TITLETEXT")
                    .subtitle("SUBTITLETEXT")
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![row(vec![cell("a"), cell("b")])])
                    .build()
            },
            check: |d| shown(d, &["TITLETEXT", "SUBTITLETEXT"]),
        },
        Feature {
            id: "notes",
            what: "footnotes and source notes",
            table: || {
                TableBuilder::new(2)
                    .head(row(vec![cell("H1"), cell("H2")]))
                    .body(vec![row(vec![
                        cell_content(vec![text("a"), footnote_mark("f", "1")]),
                        cell("b"),
                    ])])
                    .footnote("f", "1", "FOOTNOTETEXT")
                    .source_note("SOURCETEXT")
                    .build()
            },
            check: |d| shown(d, &["FOOTNOTETEXT", "SOURCETEXT"]),
        },
    ]
}

/// Each text is shown outside the table's data cells (formats with no "outside",
/// such as a spreadsheet, show it in a cell of its own).
fn shown(d: &Doc, texts: &[&str]) -> Result<(), String> {
    for t in texts {
        let in_cell = d.cells.iter().any(|c| c.text.contains(t));
        if !d.outside.contains(t) && !in_cell {
            return Err(format!("{t} not shown (outside: {:?})", d.outside));
        }
    }
    Ok(())
}

fn bottom(style: &str) -> BorderSet {
    BorderSet {
        top: None,
        right: None,
        bottom: Some(border("3px", style, "#000000")),
        left: None,
    }
}

#[test]
fn every_probe_is_valid() {
    for f in features() {
        let t = (f.table)();
        assert!(t.validate().is_empty(), "{}: {:?}", f.id, t.validate());
    }
}

/// Why a writer doesn't express a feature.
#[derive(Clone, Copy, PartialEq)]
enum Gap {
    /// The format can express it; the writer doesn't yet (M3 work).
    Todo(&'static str),
    /// The format can't (portably) express it.
    Limit(&'static str),
}

/// The declared ✗ cells; every other cell is ✓.
fn gap(feature: &str, format: &str) -> Option<Gap> {
    use Gap::{Limit, Todo};
    const NOT_YET: Gap = Todo("not implemented");
    Some(match (feature, format) {
        ("underline" | "strikethrough" | "text-transform", "latex" | "typst" | "rtf" | "docx" | "xlsx") => NOT_YET,
        ("font-size", "latex" | "rtf" | "docx" | "xlsx") => NOT_YET,
        ("font-family", "latex") => Limit(
            "a named system font needs XeLaTeX/LuaLaTeX with fontspec; the output also targets pdfLaTeX (monospace families map to `\\texttt`)",
        ),
        ("font-family", "typst") => Todo("only monospace families are mapped (to Courier New)"),
        ("font-family", "rtf" | "docx" | "xlsx") => NOT_YET,
        ("vertical-align", "latex" | "typst" | "rtf" | "docx" | "xlsx") => NOT_YET,
        ("border" | "border-styles", "latex" | "typst" | "rtf" | "docx") => NOT_YET,
        ("styled-run" | "footnote-mark", "xlsx") => Todo("cell text is written as one plain run (no rich text)"),
        _ => return None,
    })
}

/// The docs page for the declared matrix.
fn matrix_page(features: &[Feature]) -> String {
    let mut notes: Vec<&'static str> = Vec::new();
    let mut out = String::from(
        "---\ntitle: \"Feature Matrix\"\n---\n\n\
         <!-- Generated by crates/gridwell-render/tests/feature_matrix.rs; \
         GRIDWELL_BLESS_MATRIX=1 cargo test -p gridwell-render --test feature_matrix \
         rewrites it. Do not edit by hand. -->\n\n\
         Which IR features each Tier-1 writer expresses. Every cell is *measured*: \
         the test renders a probe table, reads the output back with a format-specific \
         reader (cell by cell: weight, slant, decoration, colour, size, font, fill, \
         alignment, borders) and checks the feature is there — and doesn't leak into \
         neighbouring cells. The test fails whenever a measurement disagrees with this \
         page.\n\n\
         ✓ supported · ✗ not yet (the format can express it) · ⊘ the format can't \
         (portably) express it. Numbers refer to the notes below.\n\n",
    );
    out.push_str("| Feature | IR | ");
    out.push_str(
        &FORMATS
            .iter()
            .map(|f| format!("{} |", label(f)))
            .collect::<Vec<_>>()
            .join(" "),
    );
    out.push_str("\n|---|---|");
    out.push_str(&"---|".repeat(FORMATS.len()));
    out.push('\n');
    for f in features {
        out.push_str(&format!("| {} | {} |", f.id, f.what));
        for format in FORMATS {
            let mark = match gap(f.id, format) {
                None => "✓".to_string(),
                Some(g) => {
                    let (sym, why) = match g {
                        Gap::Todo(w) => ("✗", w),
                        Gap::Limit(w) => ("⊘", w),
                    };
                    let n = match notes.iter().position(|x| *x == why) {
                        Some(i) => i + 1,
                        None => {
                            notes.push(why);
                            notes.len()
                        }
                    };
                    format!("{sym}<sup>{n}</sup>")
                }
            };
            out.push_str(&format!(" {mark} |"));
        }
        out.push('\n');
    }
    out.push_str("\n**Notes**\n\n");
    for (i, n) in notes.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, n.replace("\\\\", "\\")));
    }
    out.push_str(
        "\nStructure — spans, hidden columns, stub columns, hidden column labels, group \
         labels and summary rows — is verified separately for every writer by the grid \
         invariant: each output is read back as a grid and compared with the IR's.\n",
    );
    out
}

fn label(format: &str) -> &'static str {
    match format {
        "html" => "HTML",
        "latex" => "LaTeX",
        "typst" => "Typst",
        "rtf" => "RTF",
        "docx" => "DOCX",
        "xlsx" => "XLSX",
        _ => unreachable!(),
    }
}

#[test]
fn feature_matrix() {
    let discover = std::env::var_os("GRIDWELL_MATRIX_DISCOVER").is_some();
    let features = features();
    let tables: Vec<Table> = features.iter().map(|f| (f.table)()).collect();
    let docs: Vec<Option<Vec<Result<Doc, String>>>> =
        FORMATS.iter().map(|f| read_all(f, &tables)).collect();
    let mut report = String::new();
    let mut wrong = Vec::new();
    println!(
        "{:<20}{}",
        "",
        FORMATS
            .iter()
            .map(|f| format!("{f:^6}"))
            .collect::<String>()
    );
    for (i, f) in features.iter().enumerate() {
        let mut line = format!("{:<20}", f.id);
        for (format, docs) in FORMATS.iter().zip(&docs) {
            let cell = match docs {
                None => "?",
                Some(docs) => {
                    let measured = docs[i]
                        .as_ref()
                        .map_err(Clone::clone)
                        .and_then(|d| (f.check)(d));
                    let declared = gap(f.id, format);
                    match (&measured, declared) {
                        (Ok(()), Some(_)) => wrong.push(format!(
                            "{} / {format}: works now — remove its gap in `gap()` and rebless the docs",
                            f.id
                        )),
                        (Err(e), None) => wrong.push(format!("{} / {format}: {e}", f.id)),
                        _ => {}
                    }
                    match measured {
                        Ok(()) => "✓",
                        Err(e) => {
                            report.push_str(&format!("{} / {format}: {e}\n", f.id));
                            "✗"
                        }
                    }
                }
            };
            line.push_str(&format!("{cell:^6}"));
        }
        println!("{line}");
    }
    if discover {
        println!("\n{report}");
        return;
    }
    assert!(
        wrong.is_empty(),
        "the matrix disagrees with its declaration:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn docs_page_matches_the_declared_matrix() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/guide/feature-matrix.qmd"
    );
    let page = matrix_page(&features());
    if std::env::var_os("GRIDWELL_BLESS_MATRIX").is_some() {
        std::fs::write(path, &page).unwrap();
    }
    let on_disk = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        on_disk == page,
        "docs/guide/feature-matrix.qmd is out of date; rerun with GRIDWELL_BLESS_MATRIX=1"
    );
}
