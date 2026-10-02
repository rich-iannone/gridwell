//! Span stress test across every writer.
//!
//! Every tiling of small grids (see `gridwell_testkit::spans`) is placed in the table
//! head, the body, and (for one-row tilings) a summary row. Each resulting table is
//! valid IR, so every writer must:
//!
//! 1. render it without panicking, and
//! 2. emit the text of every origin cell (`r{row}c{col}`) somewhere in its output, and
//! 3. satisfy format-level structural invariants where cheap to check: XLSX cells in
//!    the right column with matching `<mergeCell>`s; DOCX `gridSpan`s and PPTX `<a:tc>`
//!    counts summing to `table_cols` per row; RTF `\\cellx`/`\\cell` agreement.
//!
//! (1) and (3) are hard requirements. (2) is a cheap proxy for "spans didn't push cells off
//! the grid"; writers that still fail it are listed in `KNOWN_DROPPING_CELLS` and the
//! test fails if that list goes stale in either direction. The full "logical grid
//! equals IR grid" oracle is roadmap item M2.
//!
//! This lives in the FFI crate only because it is the one crate that depends on all
//! writers; it moves to `gridwell-render` in M1.

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use gridwell_ir::Table;
use gridwell_testkit::spans::{tilings, Tiling};
use gridwell_testkit::{cell, group, labeled_group, row, TableBuilder};

type RenderFn = fn(&Table) -> Result<String, String>;

/// Every writer, rendering to something searchable. Binary writers render their main
/// XML part (and the full zip, to exercise packaging) instead of zip bytes.
fn writers() -> Vec<(&'static str, RenderFn)> {
    fn e<E: std::fmt::Display>(r: Result<String, E>) -> Result<String, String> {
        r.map_err(|e| e.to_string())
    }
    vec![
        ("html", |t| e(gridwell_writer_html::render_html(t))),
        ("latex", |t| e(gridwell_writer_latex::render_latex(t))),
        ("typst", |t| e(gridwell_writer_typst::render_typst(t))),
        ("rtf", |t| e(gridwell_writer_rtf::render_rtf(t))),
        ("svg", |t| e(gridwell_writer_svg::render_svg(t))),
        ("ansi", |t| e(gridwell_writer_ansi::render_ansi(t))),
        ("pandoc", |t| e(gridwell_writer_pandoc::render_pandoc(t))),
        ("quarto", |t| e(gridwell_writer_quarto::render_quarto(t))),
        ("docx", |t| {
            gridwell_writer_docx::render_docx(t).map_err(|e| e.to_string())?;
            e(gridwell_writer_docx::DocxWriter::new().render_document_xml(t))
        }),
        ("xlsx", |t| {
            gridwell_writer_xlsx::render_xlsx(t).map_err(|e| e.to_string())?;
            e(gridwell_writer_xlsx::XlsxWriter::new().render_sheet_xml(t))
        }),
        ("pptx", |t| {
            gridwell_writer_pptx::render_pptx(t).map_err(|e| e.to_string())?;
            e(gridwell_writer_pptx::PptxWriter::new().render_slide_xml(t))
        }),
    ]
}

/// Writers known to drop origin cells for some span layouts, by placement. Remove
/// entries as writers are fixed (the test fails if an entry no longer drops cells).
///
/// Pandoc/Quarto: summary rows are not emitted at all yet (a missing feature, not a span
/// bug; roadmap M3, Tier 2).
const KNOWN_DROPPING_CELLS: &[(&str, Placement)] = &[
    ("pandoc", Placement::Summary),
    ("quarto", Placement::Summary),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Placement {
    Head,
    Body,
    Summary,
}

struct Scenario {
    placement: Placement,
    tiling: Tiling,
    table: Table,
}

fn plain_row(prefix: &str, cols: usize) -> gridwell_ir::Row {
    row((0..cols).map(|i| cell(&format!("{prefix}{i}"))).collect())
}

fn scenarios() -> Vec<Scenario> {
    let shapes = [
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (1, 5),
        (2, 1),
        (2, 2),
        (2, 3),
        (2, 4),
        (2, 5),
        (3, 1),
        (3, 2),
        (3, 3),
        (3, 4),
    ];
    let mut out = Vec::new();
    for (rows, cols) in shapes {
        for tiling in tilings(rows, cols) {
            let n = cols as u32;

            let body = TableBuilder::new(n)
                .title("Span stress")
                .head(plain_row("h", cols))
                .group(labeled_group("Group", tiling.to_rows()))
                .group(group(vec![plain_row("z", cols)]))
                .build();
            out.push(Scenario {
                placement: Placement::Body,
                tiling: tiling.clone(),
                table: body,
            });

            let mut head = TableBuilder::new(n);
            for r in tiling.to_rows() {
                head = head.head(r);
            }
            let head = head.body(vec![plain_row("b", cols)]).build();
            out.push(Scenario {
                placement: Placement::Head,
                tiling: tiling.clone(),
                table: head,
            });

            if rows == 1 {
                let summary = TableBuilder::new(n)
                    .stub_cols(1)
                    .head(plain_row("h", cols))
                    .group(group(vec![plain_row("b", cols)]).summary(tiling.to_rows()))
                    .build();
                out.push(Scenario {
                    placement: Placement::Summary,
                    tiling,
                    table: summary,
                });
            }
        }
    }
    out
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic>".to_string())
}

#[test]
fn every_writer_handles_every_span_layout() {
    let scenarios = scenarios();
    assert!(
        scenarios.len() > 8000,
        "expected thousands of scenarios, got {}",
        scenarios.len()
    );

    // Silence the default hook: thousands of identical panic messages are noise; the
    // failures are collected and reported below.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut panics: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut errors: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut dropped: BTreeMap<(&str, Placement), Vec<String>> = BTreeMap::new();
    let mut structure: BTreeMap<&str, Vec<String>> = BTreeMap::new();

    for s in &scenarios {
        let errs = s.table.validate();
        assert!(
            errs.is_empty(),
            "{:?} {} invalid: {errs:?}",
            s.placement,
            s.tiling.ascii()
        );

        for (name, render) in writers() {
            let ctx = format!("{:?} {}", s.placement, s.tiling.ascii());
            match panic::catch_unwind(AssertUnwindSafe(|| render(&s.table))) {
                Err(payload) => panics
                    .entry(name)
                    .or_default()
                    .push(format!("{ctx}: {}", panic_message(payload))),
                Ok(Err(e)) => errors.entry(name).or_default().push(format!("{ctx}: {e}")),
                Ok(Ok(out)) => {
                    if let Some(v) = structural_violation(name, &s.tiling, &out) {
                        structure
                            .entry(name)
                            .or_default()
                            .push(format!("{ctx}: {v}"));
                    }
                    let missing: Vec<String> = s
                        .tiling
                        .origin_labels()
                        .into_iter()
                        .filter(|l| !out.contains(l.as_str()))
                        .collect();
                    if !missing.is_empty() {
                        dropped
                            .entry((name, s.placement))
                            .or_default()
                            .push(format!("{ctx}: missing {missing:?}"));
                    }
                }
            }
        }
    }

    panic::set_hook(default_hook);

    let summarize = |label: &str, map: Vec<(String, &Vec<String>)>| -> String {
        let mut msg = String::new();
        for (key, items) in map {
            msg += &format!(
                "\n  {label} {key}: {} scenario(s), e.g.\n    {}",
                items.len(),
                items
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n    ")
            );
        }
        msg
    };

    let mut failures = String::new();
    failures += &summarize(
        "PANIC",
        panics.iter().map(|(k, v)| (k.to_string(), v)).collect(),
    );
    failures += &summarize(
        "ERROR",
        errors.iter().map(|(k, v)| (k.to_string(), v)).collect(),
    );
    failures += &summarize(
        "STRUCTURE",
        structure.iter().map(|(k, v)| (k.to_string(), v)).collect(),
    );

    let unexpected: Vec<_> = dropped
        .iter()
        .filter(|(k, _)| !KNOWN_DROPPING_CELLS.contains(k))
        .map(|((w, p), v)| (format!("{w} {p:?}"), v))
        .collect();
    failures += &summarize("DROPPED CELLS", unexpected);

    for known in KNOWN_DROPPING_CELLS {
        if !dropped.contains_key(known) {
            failures += &format!(
                "\n  STALE: {known:?} no longer drops cells — remove it from KNOWN_DROPPING_CELLS"
            );
        }
    }

    assert!(
        failures.is_empty(),
        "span stress failures over {} scenarios:{failures}",
        scenarios.len()
    );
}

// ─── Format-level structural checks ───
//
// Cheap, format-specific invariants that catch cells landing in the wrong column (which
// the "label is present" check cannot see). Each returns a description of the first
// violation, if any.

fn structural_violation(writer: &str, tiling: &Tiling, out: &str) -> Option<String> {
    match writer {
        "xlsx" => xlsx_violation(tiling, out),
        "docx" => rows_violation(out, "<w:tr>", "</w:tr>", tiling.cols, docx_row_width),
        "pptx" => rows_violation(out, "<a:tr ", "</a:tr>", tiling.cols, pptx_row_width),
        "rtf" => rtf_violation(out),
        _ => None,
    }
}

/// Split `out` into the bodies between `open` and `close`, and check `width(body)`.
fn rows_violation(
    out: &str,
    open: &str,
    close: &str,
    cols: usize,
    width: fn(&str) -> usize,
) -> Option<String> {
    for (i, chunk) in out.split(open).skip(1).enumerate() {
        let body = chunk.split(close).next().unwrap_or("");
        let w = width(body);
        if w != cols {
            return Some(format!("row {i} spans {w} grid columns, expected {cols}"));
        }
    }
    None
}

/// Sum of `gridSpan` (default 1) over the `<w:tc>` elements of a `<w:tr>`.
fn docx_row_width(row: &str) -> usize {
    row.split("<w:tc>")
        .skip(1)
        .map(|tc| {
            tc.split("<w:gridSpan w:val=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .and_then(|n| n.parse().ok())
                .unwrap_or(1)
        })
        .sum()
}

/// Number of `<a:tc>` elements in an `<a:tr>` (DrawingML needs one per grid column).
fn pptx_row_width(row: &str) -> usize {
    row.matches("<a:tc>").count() + row.matches("<a:tc ").count()
}

/// Every origin label sits in the XLSX column of its grid column, all labels share one
/// row offset, and every span has a matching `<mergeCell>`.
fn xlsx_violation(tiling: &Tiling, out: &str) -> Option<String> {
    fn parse_ref(r: &str) -> (usize, usize) {
        let letters: String = r.chars().take_while(|c| c.is_ascii_uppercase()).collect();
        let col = letters
            .bytes()
            .fold(0, |acc, b| acc * 26 + (b - b'A' + 1) as usize)
            - 1;
        let row = r[letters.len()..].parse().unwrap();
        (col, row)
    }
    fn col_letters(mut c: usize) -> String {
        let mut s = Vec::new();
        loop {
            s.push(b'A' + (c % 26) as u8);
            if c < 26 {
                break;
            }
            c = c / 26 - 1;
        }
        s.reverse();
        String::from_utf8(s).unwrap()
    }

    let mut row_offset = None;
    for rect in &tiling.rects {
        let label = gridwell_testkit::spans::label(rect.row, rect.col);
        let needle = format!("<t>{label}</t>");
        let pos = out.find(&needle)?;
        let cell_start = out[..pos].rfind("<c r=\"")? + 6;
        let cell_ref = out[cell_start..].split('"').next()?;
        let (col, row) = parse_ref(cell_ref);
        if col != rect.col {
            return Some(format!(
                "{label} written to {cell_ref} (column {col}), expected column {}",
                rect.col
            ));
        }
        let offset = row - rect.row;
        if *row_offset.get_or_insert(offset) != offset {
            return Some(format!(
                "{label} written to row {row}: inconsistent row offset"
            ));
        }
        if rect.rowspan > 1 || rect.colspan > 1 {
            let merge = format!(
                "<mergeCell ref=\"{}{}:{}{}\"/>",
                col_letters(rect.col),
                row,
                col_letters(rect.col + rect.colspan - 1),
                row + rect.rowspan - 1
            );
            if !out.contains(&merge) {
                return Some(format!("missing {merge}"));
            }
        }
    }
    None
}

/// In every `\trowd … \row` block: one `\cell` per `\cellx`, strictly increasing right
/// edges, and every row of the table ending at the same right edge.
fn rtf_violation(out: &str) -> Option<String> {
    let mut table_right = None;
    for (i, block) in out.split("\\trowd").skip(1).enumerate() {
        let block = block.split("\\row").next().unwrap_or("");
        let edges: Vec<u32> = block
            .split("\\cellx")
            .skip(1)
            .map(|s| {
                s.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .map(|n| n.parse().unwrap_or(0))
            .collect();
        let cells = block.matches("\\cell").count() - edges.len();
        if cells != edges.len() {
            return Some(format!(
                "row {i}: {} \\cellx but {cells} \\cell",
                edges.len()
            ));
        }
        if edges.windows(2).any(|w| w[0] >= w[1]) {
            return Some(format!("row {i}: right edges not increasing: {edges:?}"));
        }
        let right = *edges.last()?;
        if *table_right.get_or_insert(right) != right {
            return Some(format!(
                "row {i} ends at {right}, other rows at {}",
                table_right.unwrap()
            ));
        }
    }
    None
}
