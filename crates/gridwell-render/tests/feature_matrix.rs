//! The feature matrix (M3): which IR features each Tier-1 writer (and PPTX)
//! expresses, measured, not claimed.
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
use gridwell_ir::{Table, TextDirection};
use gridwell_render::{Place, SectionKind};
use gridwell_testkit::images::LOGO_PNG;
use gridwell_testkit::{
    border, cell, cell_content, column, examples, footnote_mark, labeled_group, row, sized_image,
    styled, text, CellExt, ColumnExt, RowExt, TableBuilder,
};
use support::look::{self, Align, Doc, Line, Look, Rgb, VAlign};
use support::typeset;

const FORMATS: &[&str] = &["html", "latex", "typst", "rtf", "docx", "xlsx", "pptx"];

/// Read one table rendered to `format`.
fn read_one(format: &str, t: &Table) -> Option<Doc> {
    Some(match format {
        "html" => look::html(&gridwell_writer_html::render_html(t).unwrap()),
        "latex" => look::latex(&gridwell_writer_latex::render_latex(t).unwrap()),
        "rtf" => look::rtf(&gridwell_writer_rtf::render_rtf(t).unwrap()),
        "docx" => {
            let w = gridwell_writer_docx::DocxWriter::new();
            let mut doc = look::docx(&w.render_document_xml(t).unwrap());
            doc.lang = look::docx_lang(&w.render_styles_xml(t));
            doc
        }
        "pptx" => look::pptx(
            &gridwell_writer_pptx::PptxWriter::new()
                .render_slide_xml(t)
                .unwrap(),
        ),
        "xlsx" => {
            let w = gridwell_writer_xlsx::XlsxWriter::new();
            look::xlsx(
                &w.render_sheet_xml(t).unwrap(),
                &w.render_styles_xml(t).unwrap(),
                &w.render_shared_strings_xml(t).unwrap(),
                &w.render_drawing_xml(t).unwrap(),
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
            check: |d| target_only(d, "color #9932CC", |l| l.all(|r| r.color == Some(PURPLE))),
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
            what: "`vertical_align: bottom` (beside a taller cell)",
            // Alignment shows only in a row of cells of different heights.
            table: || {
                let mut t = probe(def(|d| d.vertical_align = Some("bottom".into())));
                let big = def(|d| d.font_size = Some("30px".into()));
                t.styles.defs.insert("big".into(), big);
                t.table.tbody[0].rows[0].cells[1].style_id = Some("big".into());
                t
            },
            // Where bottom is the format's default (XLSX), TARGET states nothing:
            // so it counts, and a control that states bottom leaks it.
            check: |d| {
                let t = get(d, "TARGET")?;
                if t.valign.or(d.valign_default) != Some(VAlign::Bottom) {
                    return Err(format!("TARGET lacks bottom alignment: {t:?}"));
                }
                for c in ["CONTROLA", "CONTROLB", "CONTROLC"] {
                    let l = get(d, c)?;
                    if l.valign == Some(VAlign::Bottom) {
                        return Err(format!("bottom alignment leaks into {c}: {l:?}"));
                    }
                }
                Ok(())
            },
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
            what: "border styles dashed, dotted",
            table: || line_styles_table(&[("DASHED", "dashed"), ("DOTTED", "dotted")]),
            check: |d| check_line_styles(d, &[("DASHED", Line::Dashed), ("DOTTED", Line::Dotted)]),
        },
        Feature {
            id: "border-double",
            what: "border style double",
            table: || line_styles_table(&[("DOUBLE", "double")]),
            check: |d| check_line_styles(d, &[("DOUBLE", Line::Double)]),
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
        Feature {
            id: "line-styles",
            what: "a `style_id` on the title, subtitle, header lines, group labels (and their fill) and notes",
            table: styled_lines_table,
            check: |d| {
                let purple = |r: &look::Run| r.underline && r.color == Some(PURPLE);
                for t in STYLED_LINES {
                    let r = d.run_showing(t).ok_or_else(|| format!("{t} not shown"))?;
                    if !purple(r) {
                        return Err(format!("{t} not purple and underlined: {r:?}"));
                    }
                }
                let label = get(d, "LABELTEXT")?;
                if label.fill != Some(NAVY) {
                    return Err(format!("LABELTEXT not filled #123456: {label:?}"));
                }
                let r = d.run_showing("PLAINNOTE").ok_or("PLAINNOTE not shown")?;
                if r.underline || r.color == Some(PURPLE) {
                    return Err(format!("the style leaks into PLAINNOTE: {r:?}"));
                }
                Ok(())
            },
        },
        Feature {
            id: "normal-weight",
            what: "`font_weight: normal` on what a format draws bold by default (column labels, the title, a group label, a run in a bold cell)",
            table: normal_weight_table,
            check: |d| {
                for t in ["HEADTEXT", "TITLETEXT", "LABELTEXT", "RUNTEXT"] {
                    let r = d.run_showing(t).ok_or_else(|| format!("{t} not shown"))?;
                    if r.bold {
                        return Err(format!("{t} is bold: {r:?}"));
                    }
                }
                let r = d.run_showing("BOLDTEXT").ok_or("BOLDTEXT not shown")?;
                if !r.bold {
                    return Err(format!("BOLDTEXT is not bold: {r:?}"));
                }
                Ok(())
            },
        },
        Feature {
            id: "normal-style",
            what: "`font_style: normal` on a run in an italic cell or line",
            table: normal_style_table,
            check: |d| {
                for t in ["RUNTEXT", "LINERUN"] {
                    let r = d.run_showing(t).ok_or_else(|| format!("{t} not shown"))?;
                    if r.italic {
                        return Err(format!("{t} is italic: {r:?}"));
                    }
                }
                for t in ["ITALICTEXT", "LINETEXT"] {
                    let r = d.run_showing(t).ok_or_else(|| format!("{t} not shown"))?;
                    if !r.italic {
                        return Err(format!("{t} is not italic: {r:?}"));
                    }
                }
                Ok(())
            },
        },
        Feature {
            id: "image",
            what: "an image (a PNG `data:` URI, 32×16px = 24×12pt)",
            // Alone in its cell, as a spreadsheet draws only such an image; its
            // alt text names the cell.
            table: || {
                let mut t = probe(StyleDef::default());
                t.table.tbody[0].rows[0].cells[0].content =
                    vec![sized_image(LOGO_PNG, "TARGET", Some("32px"), Some("16px"))];
                t
            },
            check: |d| {
                target_only(d, "a 24×12pt picture", |l| {
                    matches!(l.pictures[..], [(w, h)] if near(Some(w), 24.0) && near(Some(h), 12.0))
                })
            },
        },
        Feature {
            id: "lang",
            what: "`config.lang`, the table's language",
            table: || {
                let mut t = probe(StyleDef::default());
                t.config.lang = Some("ar".into());
                t
            },
            check: |d| match d.lang.as_deref() {
                Some("ar") => Ok(()),
                other => Err(format!("the language is {other:?}")),
            },
        },
        Feature {
            id: "rtl",
            what: "`config.dir: rtl`, columns and text right to left",
            table: || {
                let mut t = probe(StyleDef::default());
                t.config.dir = Some(TextDirection::Rtl);
                t
            },
            check: |d| d.rtl.then_some(()).ok_or_else(|| "left to right".into()),
        },
    ]
}

/// Features of the table as a whole, which a writer warns of at the table.
const TABLE_WIDE: &[&str] = &["lang", "rtl"];

/// `font_style: normal` on a run inside an italic cell and an italic source
/// note (`ITALICTEXT` and `LINETEXT`, the rest of each, stay italic).
fn normal_style_table() -> Table {
    let mut t = TableBuilder::new(2)
        .style_def("s", def(|d| d.font_style = Some("normal".into())))
        .style_def("i", def(|d| d.font_style = Some("italic".into())))
        .head(row(vec![cell("H1"), cell("H2")]))
        .body(vec![row(vec![
            cell_content(vec![text("ITALICTEXT "), styled("RUNTEXT", "s")]).style("i"),
            cell("b"),
        ])])
        .source_note("N")
        .build();
    let note = &mut t.footer.as_mut().unwrap().source_notes[0];
    note.content = vec![text("LINETEXT "), styled("LINERUN", "s")];
    note.style_id = Some("i".into());
    t
}

/// `font_weight: normal` on a column label, the title, a group label and a run
/// inside a bold cell (`BOLDTEXT`, the rest of that cell, stays bold).
fn normal_weight_table() -> Table {
    let mut t = TableBuilder::new(2)
        .style_def("s", def(|d| d.font_weight = Some("normal".into())))
        .style_def("b", def(|d| d.font_weight = Some("bold".into())))
        .title("TITLETEXT")
        .head(row(vec![cell("HEADTEXT").style("s"), cell("H2")]).role("column_label"))
        .group(
            labeled_group(
                "LABELTEXT",
                vec![row(vec![
                    cell_content(vec![text("BOLDTEXT "), styled("RUNTEXT", "s")]).style("b"),
                    cell("b"),
                ])],
            )
            .label_style("s"),
        )
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("s".into());
    t
}

const STYLED_LINES: [&str; 6] = [
    "TITLETEXT",
    "SUBTITLETEXT",
    "EXTRATEXT",
    "LABELTEXT",
    "FOOTNOTETEXT",
    "SOURCETEXT",
];

/// Every kind of line outside the grid's cells, styled purple and underlined
/// (the group label filled too), and an unstyled source note.
fn styled_lines_table() -> Table {
    let purple = |d: &mut StyleDef| {
        d.color = Some("#9932CC".into());
        d.text_decoration = Some("underline".into());
    };
    let mut t = TableBuilder::new(2)
        .style_def("s", def(purple))
        .style_def(
            "label",
            def(|d| {
                purple(d);
                d.background_color = Some("#123456".into());
            }),
        )
        .title("TITLETEXT")
        .subtitle("SUBTITLETEXT")
        .extra_line("EXTRATEXT")
        .head(row(vec![cell("H1"), cell("H2")]))
        .group(
            labeled_group("LABELTEXT", vec![row(vec![cell("a"), cell("b")])]).label_style("label"),
        )
        .footnote("f", "1", "FOOTNOTETEXT")
        .source_note("SOURCETEXT")
        .source_note("PLAINNOTE")
        .build();
    let s = || Some("s".to_string());
    let header = t.header.as_mut().unwrap();
    for line in header
        .title
        .iter_mut()
        .chain(&mut header.subtitle)
        .chain(&mut header.extra_lines)
    {
        line.style_id = s();
    }
    let footer = t.footer.as_mut().unwrap();
    footer.footnotes[0].style_id = s();
    footer.source_notes[0].style_id = s();
    t
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

/// One column per `(text, style)`: the text's cell has a 3px black bottom border
/// in that style; the plain row below it holds controls.
fn line_styles_table(cells: &[(&str, &str)]) -> Table {
    let mut b = TableBuilder::new(cells.len() as u32);
    for (text, style) in cells {
        b = b.style_def(text, def(|d| d.border = Some(bottom(style))));
    }
    b.head(row((1..=cells.len())
        .map(|i| cell(&format!("H{i}")))
        .collect()))
        .body(vec![
            row(cells.iter().map(|(t, _)| cell(t).style(t)).collect()),
            row(CONTROLS[..cells.len()].iter().map(|c| cell(c)).collect()),
        ])
        .build()
}

const CONTROLS: [&str; 3] = ["CONTROLA", "CONTROLB", "CONTROLC"];

/// Each text's bottom border has its line style, and the control below it doesn't
/// (it may have the format's plain grid line).
fn check_line_styles(d: &Doc, cells: &[(&str, Line)]) -> Result<(), String> {
    for (i, (text, line)) in cells.iter().enumerate() {
        let l = get(d, text)?;
        if l.borders[2].map(|e| e.line) != Some(*line) {
            return Err(format!("{text}: bottom border {:?}", l.borders[2]));
        }
        let c = get(d, CONTROLS[i])?;
        if c.borders.iter().any(|e| e.is_some_and(|e| e.line == *line)) {
            return Err(format!(
                "{line:?} leaks into {}: {:?}",
                CONTROLS[i], c.borders
            ));
        }
    }
    Ok(())
}

#[test]
fn every_probe_is_valid() {
    for f in features() {
        let t = (f.table)();
        assert!(t.validate().is_empty(), "{}: {:?}", f.id, t.validate());
    }
}

/// Why a writer doesn't express a feature. Either way the writer warns of it
/// (`gridwell_render::warnings`), and its warning's reason is the note on the
/// docs page.
#[derive(Clone, Copy, PartialEq)]
enum Gap {
    /// The format can express it; the writer doesn't yet.
    Todo,
    /// The format can't (portably) express it.
    Limit,
}

/// The declared ✗ cells; every other cell is ✓.
fn gap(feature: &str, format: &str) -> Option<Gap> {
    use Gap::{Limit, Todo};
    Some(match (feature, format) {
        ("font-family", "latex") => Limit,
        ("vertical-align", "latex") => Limit,
        ("border-double", "typst") => Limit,
        ("image", "latex" | "pptx") => Limit,
        ("lang", "latex" | "xlsx") => Limit,
        ("lang", "rtf") => Todo,
        ("rtl", "latex") => Limit,
        _ => return None,
    })
}

/// The reason `format` gives for not expressing feature `f`: its warning for
/// the probe table.
fn reason(f: &Feature, format: &str) -> Option<&'static str> {
    gridwell_render::warnings(&(f.table)(), Some(format))
        .unwrap()
        .into_iter()
        .find(|w| w.feature == f.id)
        .map(|w| w.reason)
}

/// The docs page for the declared matrix.
fn matrix_page(features: &[Feature]) -> String {
    let mut notes: Vec<&'static str> = Vec::new();
    let mut out = String::from(
        "---\ntitle: \"Feature Matrix\"\n---\n\n\
         <!-- Generated by crates/gridwell-render/tests/feature_matrix.rs; \
         GRIDWELL_BLESS_MATRIX=1 cargo test -p gridwell-render --test feature_matrix \
         rewrites it. Do not edit by hand. -->\n\n\
         Which IR features each Tier-1 writer (plus PPTX) expresses. Every cell is \
         *measured*: \
         the test renders a probe table, reads the output back with a format-specific \
         reader (cell by cell: weight, slant, decoration, color, size, font, fill, \
         alignment, borders; for the table as a whole, its language and direction) and \
         checks the feature is there — and doesn't leak into neighboring cells. The test \
         fails whenever a measurement disagrees with this page.\n\n\
         ✓ supported · ✗ not yet (the format can express it) · ⊘ the format can't \
         (portably) express it. Numbers refer to the notes below. A table that uses a \
         feature its format lacks gets a warning saying where, with the note as its \
         reason: see [Validation](../schema/validation.qmd#warnings).\n\n",
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
                    let sym = match g {
                        Gap::Todo => "✗",
                        Gap::Limit => "⊘",
                    };
                    let why = reason(f, format)
                        .unwrap_or_else(|| panic!("{} / {format}: a gap, but no warning", f.id));
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
        "pptx" => "PPTX",
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
    // Line endings aside: Git may check the page out with CRLF on Windows.
    let on_disk = std::fs::read_to_string(path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        on_disk == page,
        "docs/guide/feature-matrix.qmd is out of date; rerun with GRIDWELL_BLESS_MATRIX=1"
    );
}

/// A writer warns of exactly the features it lacks: every ✗ cell's probe gets a
/// warning naming the feature, at the cell carrying it (each such probe's is
/// the first body cell) or at the table for a table-wide feature, and no ✓
/// cell's does.
#[test]
fn writers_warn_of_their_gaps() {
    let mut wrong = Vec::new();
    let mut warned = 0;
    for f in features() {
        let table = (f.table)();
        for format in FORMATS {
            let warnings = gridwell_render::warnings(&table, Some(format)).unwrap();
            let named: Vec<_> = warnings.iter().filter(|w| w.feature == f.id).collect();
            match (gap(f.id, format), named.as_slice()) {
                (Some(_), [w]) => {
                    warned += 1;
                    assert_eq!(w.format, *format);
                    let first = if TABLE_WIDE.contains(&f.id) {
                        Place::Table
                    } else {
                        Place::Cell {
                            section: SectionKind::Body,
                            group: Some(0),
                            row: 0,
                            col: 0,
                        }
                    };
                    assert_eq!(w.places, [first], "{w}");
                }
                (Some(_), _) => wrong.push(format!("{} / {format}: {named:?}", f.id)),
                (None, []) => {}
                (None, _) => wrong.push(format!("{} / {format}: warns {named:?}", f.id)),
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert_eq!(warned, 9);
}

/// The probes' columns have no width; a column with one (fixed, `%`, `fr`) must
/// keep its alignment and a cell's own (LaTeX writes those as paragraph
/// columns, aligned by `\raggedleft`…, not by a column letter).
#[test]
fn alignment_holds_in_sized_columns() {
    let t = TableBuilder::new(3)
        .style_def("left", def(|d| d.text_align = Some("left".into())))
        .columns(vec![
            column("a", "A").width("90pt").align("right"),
            column("b", "B").width("25%").align("center"),
            column("c", "C").width("1fr").align("right"),
        ])
        .body(vec![
            row(vec![cell("FIXED"), cell("PERCENT"), cell("FRACTION")]),
            row(vec![
                cell("OWNA").style("left"),
                cell("OWNB").style("left"),
                cell("OWNC").style("left"),
            ]),
        ])
        .build();
    for format in FORMATS {
        let Some(docs) = read_all(format, std::slice::from_ref(&t)) else {
            continue;
        };
        let doc = docs.into_iter().next().unwrap().unwrap();
        for (text, want) in [
            ("FIXED", Align::Right),
            ("PERCENT", Align::Center),
            ("FRACTION", Align::Right),
            ("OWNA", Align::Left),
            ("OWNB", Align::Left),
            ("OWNC", Align::Left),
        ] {
            let l = get(&doc, text).unwrap();
            // Office paragraphs are left-aligned unless they say otherwise.
            let office = ["rtf", "docx", "xlsx", "pptx"].contains(format);
            let got = l.align.or(office.then_some(Align::Left));
            assert_eq!(got, Some(want), "{format} {text}: {l:?}");
        }
    }
}

/// A longtable shows what its tabular does: the same cells, looking the same,
/// and the same title and notes, in the same formatting (the matrix reads the
/// tabular). Every feature probe and every corpus table.
#[test]
fn latex_longtable_reads_like_its_tabular() {
    use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};
    let read = |t: &Table, longtable| {
        look::latex(
            &LatexWriter::with_config(LatexWriterConfig {
                longtable,
                ..Default::default()
            })
            .render(t)
            .unwrap(),
        )
    };
    // Runs with their text's spacing evened out (a title row's line breaks are
    // not a tabular's), and blank runs dropped.
    let runs = |d: &Doc| -> Vec<look::Run> {
        d.outside_runs
            .iter()
            .filter(|r| !r.text.trim().is_empty())
            .map(|r| look::Run {
                text: r.text.split_whitespace().collect::<Vec<_>>().join(" "),
                ..r.clone()
            })
            .collect()
    };
    let (features, examples) = (features(), examples());
    let tables = features
        .iter()
        .map(|f| (f.id, (f.table)()))
        .chain(examples.iter().map(|ex| (ex.name, ex.table())));
    let mut titled = 0;
    for (name, t) in tables {
        let (tabular, longtable) = (read(&t, false), read(&t, true));
        assert_eq!(longtable.cells, tabular.cells, "{name}");
        assert_eq!(runs(&longtable), runs(&tabular), "{name}");
        titled += usize::from(!runs(&tabular).is_empty());
    }
    assert!(titled > 10, "only {titled} tables with a title or notes");
}

/// Typst ≤ 0.12 (pinned in CI) renders `#super[7]` as the character "⁷"; newer
/// versions keep "7" and use superscript glyphs. The reader must see a
/// superscript "7" either way.
#[test]
fn typst_reader_reads_unicode_superscripts() {
    if !typeset::require("typst", "GRIDWELL_REQUIRE_TYPST") {
        return;
    }
    let src = "#table(columns: 2, [TARGET⁷], [CONTROLA], [CONTROLB], [CONTROLC])".to_string();
    let doc = typeset::typst_looks(&[src]).pop().unwrap().unwrap();
    let cell = doc
        .cells
        .iter()
        .find(|c| c.text.starts_with("TARGET"))
        .expect("TARGET cell");
    let mark = cell.run_with("7").expect("a run with 7");
    assert!(mark.superscript, "{:?}", cell.runs);
    assert!(
        !cell.run_with("TARGET").unwrap().superscript,
        "{:?}",
        cell.runs
    );
}

/// The Typst writer writes a border that is the same on every side as one
/// stroke, and the reader must see it on all four sides; the probes above only
/// set one side.
#[test]
fn typst_reader_reads_uniform_strokes() {
    if !typeset::require("typst", "GRIDWELL_REQUIRE_TYPST") {
        return;
    }
    let t = TableBuilder::new(2)
        .style_def(
            "box",
            def(|d| {
                d.border = Some(gridwell_testkit::border_all(border(
                    "2px", "dotted", "#FF0000",
                )))
            }),
        )
        .head(row(vec![cell("H1"), cell("H2")]))
        .body(vec![row(vec![
            cell("TARGET").style("box"),
            cell("CONTROLA"),
        ])])
        .build();
    let src = gridwell_writer_typst::render_typst(&t).unwrap();
    assert!(src.contains("stroke: (thickness: 1.5pt,"), "{src}");
    let doc = typeset::typst_looks(&[src]).pop().unwrap().unwrap();
    let target = get(&doc, "TARGET").unwrap();
    for e in target.borders {
        let e = e.unwrap_or_else(|| panic!("{:?}", target.borders));
        assert!(e.line == Line::Dotted && e.color == Some(RED) && near(e.width_pt, 1.5));
    }
    assert_eq!(get(&doc, "CONTROLA").unwrap().borders, [None; 4]);
}

/// Beyond the probe's one cell: a styled run gets its own font inside a plain
/// cell and keeps the cell's when it names none, a generic family maps to a font (`monospace` after `'Fira Mono'`), and
/// a list naming no font (`system-ui, cursive`) leaves the format's default;
/// header lines are in the line's font. Every format but LaTeX, which has no
/// named fonts (see `gap()`).
#[test]
fn fonts_reach_runs_and_lines() {
    let t = gridwell_testkit::examples::font_families();
    let mut title_only = t.clone();
    for r in &mut title_only.table.tbody[0].rows {
        for c in &mut r.cells {
            c.style_id = None;
            c.content = vec![text("x")];
        }
    }
    let family = |l: &Look, text: &str| {
        l.run_with(text)
            .unwrap_or_else(|| panic!("no run {text:?} in {l:?}"))
            .family
            .clone()
            .map(|f| f.to_ascii_lowercase())
    };
    for format in FORMATS.iter().filter(|f| **f != "latex") {
        let Some(docs) = read_all(format, std::slice::from_ref(&t)) else {
            continue;
        };
        let doc = docs.into_iter().next().unwrap().unwrap();
        let default = family(get(&doc, "H1").unwrap(), "H1");
        let a = get(&doc, "a e").unwrap();
        let bc = get(&doc, "b c").unwrap();
        let d = get(&doc, "d").unwrap();
        let has = |f: &Option<String>, name: &str| f.as_deref().is_some_and(|f| f.contains(name));
        // A run with no font of its own is in the cell's.
        assert!(has(&family(a, "a"), "georgia"), "{format}: {a:?}");
        assert!(has(&family(a, "e"), "georgia"), "{format}: {a:?}");
        assert!(has(&family(bc, "c"), "fira mono"), "{format}: {bc:?}");
        assert_eq!(family(bc, "b"), default, "{format}: {bc:?}");
        if *format == "html" {
            // CSS keeps the list as written.
            assert!(has(&family(d, "d"), "system-ui"), "{format}: {d:?}");
            continue;
        }
        assert_eq!(family(d, "d"), default, "{format}: {d:?}");
        // The fallback for `monospace` is written where the format takes a list.
        if *format == "typst" {
            let src = gridwell_writer_typst::render_typst(&t).unwrap();
            assert!(
                src.contains("font: (\"Fira Mono\", \"Courier New\")"),
                "{src}"
            );
            // Typst's title takes no style (yet), so no font either.
            continue;
        }
        let out = match *format {
            "rtf" => gridwell_writer_rtf::render_rtf(&title_only).unwrap(),
            "docx" => gridwell_writer_docx::DocxWriter::new()
                .render_document_xml(&title_only)
                .unwrap(),
            "pptx" => gridwell_writer_pptx::PptxWriter::new()
                .render_slide_xml(&title_only)
                .unwrap(),
            "xlsx" => gridwell_writer_xlsx::XlsxWriter::new()
                .render_styles_xml(&title_only)
                .unwrap(),
            _ => unreachable!(),
        };
        assert!(out.contains("Fira Mono"), "{format}: title font\n{out}");
    }
}
