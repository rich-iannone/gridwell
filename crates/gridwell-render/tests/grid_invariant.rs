//! The cross-writer grid invariant (M2): for every corpus example and every span
//! layout, under every set of hidden columns, each format that can be read back
//! shows exactly the IR's grid. Readers use the formats' own structure, and HTML
//! and XLSX are read by independent parsers (html5ever, calamine).
//!
//! | Format | Reader |
//! |---|---|
//! | HTML | html5ever (`scraper`), HTML table algorithm |
//! | XLSX | calamine (cell values + merged regions) |
//! | DOCX | quick-xml: `w:tr`/`w:tc`, `gridSpan`, `vMerge` |
//! | PPTX | quick-xml: `a:tr`/`a:tc`, `gridSpan`/`rowSpan`/`hMerge`/`vMerge` |
//! | RTF | tokenizer: `\trowd`, `\cellx` boundaries, `\clvmrg`, `\cell`, `\row` |
//! | Pandoc, Quarto | JSON AST: `Table` head, bodies (intermediate head + rows) |
//! | Typst | `typst query`: each cell's resolved position, spans and text |
//! | LaTeX | LuaLaTeX → PDF; cell edges and words by position (`pdftotext -bbox`) |
//!
//! The typeset formats need their tools (see `support::typeset`); they skip
//! without them unless `GRIDWELL_REQUIRE_TYPST` / `GRIDWELL_REQUIRE_LATEX` is set.
//! SVG and ANSI have their own structural oracles in their crates.

mod support;

use gridwell_ir::Table;
use gridwell_testkit::spans::tilings;
use gridwell_testkit::{
    cell, column, examples, group, labeled_group, placeholder, row, CellExt, TableBuilder,
};
use support::readers::{html_grid, pandoc_grid, rtf_grid, xlsx_grid};
use support::typeset::{latex_compare, latex_observable, latex_tables, latex_tabular, typst_grids};
use support::{compare, expected, ooxml, typeset, Grid};

type Reader = fn(&Table, &support::Expected) -> Option<Grid>;

fn readers() -> Vec<(&'static str, Reader, bool)> {
    vec![
        (
            "html",
            |t, _| Some(html_grid(&gridwell_writer_html::render_html(t).unwrap())),
            false,
        ),
        (
            "xlsx",
            |t, e| {
                let bytes = gridwell_writer_xlsx::render_xlsx(t).unwrap();
                Some(xlsx_grid(&bytes, e.header_lines, e.rows.len(), e.width()))
            },
            true,
        ),
        (
            "docx",
            |t, _| {
                let xml = gridwell_writer_docx::DocxWriter::new()
                    .render_document_xml(t)
                    .unwrap();
                Some(ooxml::docx_grid(&xml))
            },
            false,
        ),
        (
            "pptx",
            |t, _| {
                let xml = gridwell_writer_pptx::PptxWriter::new()
                    .render_slide_xml(t)
                    .unwrap();
                Some(ooxml::pptx_grid(&xml))
            },
            false,
        ),
        (
            "rtf",
            |t, e| rtf_grid(&gridwell_writer_rtf::render_rtf(t).unwrap(), e.width()),
            false,
        ),
        (
            "pandoc",
            |t, _| {
                Some(pandoc_grid(
                    &gridwell_writer_pandoc::render_pandoc(t).unwrap(),
                ))
            },
            false,
        ),
        (
            "quarto",
            |t, _| {
                Some(pandoc_grid(
                    &gridwell_writer_quarto::render_quarto(t).unwrap(),
                ))
            },
            false,
        ),
    ]
}

fn hide(mut t: Table, mask: &[bool]) -> Table {
    for (spec, &h) in t.column_spec.iter_mut().zip(mask) {
        spec.hidden = spec.hidden || h;
    }
    t
}

/// Every corpus example as is, with its column labels hidden, and with each column
/// hidden; every span layout in
/// the head, a labelled body group and (one-row layouts) a summary row, under
/// every hidden-column mask that leaves a column visible.
fn scenarios() -> Vec<(String, Table)> {
    let mut out = Vec::new();
    for ex in examples() {
        let t = ex.table();
        let n = t.column_spec.len();
        out.push((ex.name.to_string(), t.clone()));
        if !t.table.thead.rows.is_empty() {
            let mut no_labels = t.clone();
            no_labels.config.column_labels_hidden = true;
            out.push((format!("{} labels hidden", ex.name), no_labels));
        }
        for c in 0..n {
            let mut mask = vec![false; n];
            mask[c] = true;
            if mask
                .iter()
                .zip(&t.column_spec)
                .filter(|(m, s)| !**m && !s.hidden)
                .count()
                > 0
            {
                out.push((format!("{} hide {c}", ex.name), hide(t.clone(), &mask)));
            }
        }
    }
    let plain = |p: &str, n: usize| row((0..n).map(|i| cell(&format!("{p}{i}"))).collect());
    for (rows, cols) in [(1, 4), (2, 3), (3, 3), (2, 4)] {
        for t in tilings(rows, cols) {
            for m in 0u32..(1 << cols) - 1 {
                let mask: Vec<bool> = (0..cols).map(|i| m & (1 << i) != 0).collect();
                let columns: Vec<_> = (0..cols).map(|i| column(&format!("c{i}"), "")).collect();
                let n = cols as u32;
                let name = format!("{} mask {m:b}", t.ascii());
                let body = TableBuilder::new(n)
                    .columns(columns.clone())
                    .head(plain("h", cols))
                    .group(labeled_group("G", t.to_rows()))
                    .group(group(vec![plain("z", cols)]))
                    .build();
                out.push((format!("body {name}"), hide(body, &mask)));
                let mut head = TableBuilder::new(n).columns(columns.clone());
                for r in t.to_rows() {
                    head = head.head(r);
                }
                out.push((
                    format!("head {name}"),
                    hide(head.body(vec![plain("b", cols)]).build(), &mask),
                ));
                if rows == 1 {
                    let summary = TableBuilder::new(n)
                        .columns(columns)
                        .stub_cols(1)
                        .head(plain("h", cols))
                        .group(group(vec![plain("b", cols)]).summary(t.to_rows()))
                        .build();
                    out.push((format!("summary {name}"), hide(summary, &mask)));
                }
            }
        }
    }
    out
}

#[test]
fn every_readable_format_shows_the_ir_grid() {
    let readers = readers();
    let mut failures: Vec<String> = Vec::new();
    let mut checked = vec![0usize; readers.len()];
    let mut rtf_ambiguous = 0;
    let scenarios = scenarios();
    for (name, table) in &scenarios {
        assert!(
            table.validate().is_empty(),
            "{name}: {:?}",
            table.validate()
        );
        let want = expected(table);
        for (i, (format, read, numbers)) in readers.iter().enumerate() {
            let Some(got) = read(table, &want) else {
                rtf_ambiguous += 1;
                continue;
            };
            checked[i] += 1;
            if let Err(e) = compare(&got, &want, *numbers) {
                if failures.len() < 15 {
                    failures.push(format!("{format} / {name}: {e}"));
                } else {
                    failures.push(String::new());
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} grid mismatch(es):\n\n{}",
        failures.len(),
        failures
            .iter()
            .filter(|f| !f.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
    // RTF can't place cells when some column boundary never appears in any row;
    // that must stay rare (every scenario has a plain row).
    assert!(
        rtf_ambiguous * 20 < scenarios.len(),
        "{rtf_ambiguous} ambiguous RTF grids"
    );
    for ((format, ..), n) in readers.iter().zip(&checked) {
        assert!(*n > scenarios.len() / 2, "{format}: only {n} checked");
    }
}

/// Collect failures (the first 15 in full) and fail with all of them.
fn report(what: &str, failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} {what} mismatch(es):\n\n{}",
        failures.len(),
        failures
            .iter()
            .take(15)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

#[test]
fn typst_shows_the_ir_grid() {
    if !typeset::require("typst", "GRIDWELL_REQUIRE_TYPST") {
        return;
    }
    let scenarios = scenarios();
    let sources: Vec<String> = scenarios
        .iter()
        .map(|(_, t)| gridwell_writer_typst::render_typst(t).unwrap())
        .collect();
    let mut failures = Vec::new();
    for ((name, table), got) in scenarios.iter().zip(typst_grids(&sources)) {
        if let Err(e) = got.and_then(|g| compare(&g, &expected(table), false)) {
            failures.push(format!("typst / {name}: {e}"));
        }
    }
    report("Typst grid", &failures);
}

#[test]
fn latex_shows_the_ir_grid() {
    if !typeset::require("lualatex", "GRIDWELL_REQUIRE_LATEX")
        || !typeset::require("pdftotext", "GRIDWELL_REQUIRE_LATEX")
    {
        return;
    }
    let scenarios = scenarios();
    let mut checked = Vec::new();
    let mut tabulars = Vec::new();
    let mut unobservable = 0;
    for (name, table) in &scenarios {
        let want = expected(table);
        if !latex_observable(&want) {
            unobservable += 1;
            continue;
        }
        let src = gridwell_writer_latex::render_latex(table).unwrap();
        match latex_tabular(&src) {
            Some(t) => {
                tabulars.push(t.to_string());
                checked.push((name, want));
            }
            None => assert!(want.rows.is_empty(), "{name}: no tabular\n{src}"),
        }
    }
    let mut failures = Vec::new();
    for ((name, want), got) in checked.iter().zip(latex_tables(&tabulars)) {
        if let Err(e) = got.and_then(|g| latex_compare(&g, want)) {
            failures.push(format!("latex / {name}: {e}"));
        }
    }
    report("LaTeX grid", &failures);
    assert!(
        checked.len() * 2 > scenarios.len(),
        "only {} checked",
        checked.len()
    );
    // As for RTF: every scenario has a plain row, so this must stay rare.
    assert!(
        unobservable * 20 < scenarios.len(),
        "{unobservable} unobservable LaTeX grids"
    );
}

/// TeX gives a column that only spanned cells cover no width of its own: here
/// zero (two column boundaries at one position) and negative (column 1 ends left
/// of where it starts). The LaTeX reader must not assume edges are ordered or
/// distinct by position. Both came from the property test.
#[test]
fn latex_columns_only_spans_cover() {
    if !typeset::require("lualatex", "GRIDWELL_REQUIRE_LATEX")
        || !typeset::require("pdftotext", "GRIDWELL_REQUIRE_LATEX")
    {
        return;
    }
    let p = placeholder;
    // Column 1: the span over columns 0–1 is exactly as wide as column 0.
    let zero = TableBuilder::new(4)
        .body(vec![
            row(vec![cell("same"), cell("b").colspan(2), p(), cell("d")]),
            row(vec![
                cell("same").colspan(2),
                p(),
                cell("cd").colspan(2),
                p(),
            ]),
            row(vec![cell("abc").colspan(3), p(), p(), cell("d")]),
        ])
        .build();
    // Column 1: the span over columns 0–1 is narrower than column 0.
    let negative = TableBuilder::new(3)
        .body(vec![
            row(vec![cell("ab").colspan(2), p(), cell("c")]),
            row(vec![
                cell("a rather long first cell"),
                cell("bc").colspan(2),
                p(),
            ]),
        ])
        .build();
    let tables = [&zero, &negative];
    let tabulars: Vec<String> = tables
        .iter()
        .map(|t| {
            latex_tabular(&gridwell_writer_latex::render_latex(t).unwrap())
                .unwrap()
                .to_string()
        })
        .collect();
    let got: Vec<_> = latex_tables(&tabulars)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    for (t, g) in tables.iter().zip(&got) {
        assert!(t.validate().is_empty(), "{:?}", t.validate());
        latex_compare(g, &expected(t)).unwrap();
    }
    // The layouts really are degenerate: 4 columns with 4 distinct boundary
    // positions, and column 1's right boundary left of its left one.
    assert_eq!(got[0].boundaries.len(), 4, "{:?}", got[0]);
    let x = |row: usize, cell: usize, right: bool| {
        let c = &got[1].rows[row][cell];
        got[1].boundaries[if right { c.to } else { c.from }]
    };
    assert!(x(0, 0, true) < x(1, 0, true), "{:?}", got[1]);
}
