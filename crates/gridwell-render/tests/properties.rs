//! Property tests over arbitrary valid IR (`gridwell_testkit::arb::arb_valid_ir`):
//! every format renders without error, and every format that can be read back
//! shows exactly the IR's grid.
//!
//! Typst and LaTeX are read back by typesetting (`support::typeset`), which is
//! too slow per case; their property test draws the same number of tables and
//! typesets them in batches. It can't shrink: a failure prints the IR.

mod support;

use gridwell_ir::Table;
use gridwell_render::{names, render};
use gridwell_testkit::arb::arb_valid_ir;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use support::readers::{html_grid, pandoc_grid, rtf_grid, xlsx_grid};
use support::typeset::{latex_compare, latex_observable, latex_tables, latex_tabular, typst_grids};
use support::{compare, expected, ooxml, typeset};

const CASES: u32 = 256;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: CASES,
        ..ProptestConfig::default()
    })]

    #[test]
    fn every_format_renders_and_readable_formats_show_the_grid(t in arb_valid_ir()) {
        for name in names() {
            prop_assert!(render(&t, name, None).is_ok(), "{name} failed");
        }
        let want = expected(&t);
        let text = |f: &str| render(&t, f, None).unwrap().as_text().unwrap().to_string();
        let mut checks = vec![
            ("html", Some(html_grid(&text("html"))), false),
            ("pandoc", Some(pandoc_grid(&text("pandoc"))), false),
            ("quarto", Some(pandoc_grid(&text("quarto"))), false),
            ("rtf", rtf_grid(&text("rtf"), want.width()), false),
        ];
        let docx = gridwell_writer_docx::DocxWriter::new().render_document_xml(&t).unwrap();
        checks.push(("docx", Some(ooxml::docx_grid(&docx)), false));
        let pptx = gridwell_writer_pptx::PptxWriter::new().render_slide_xml(&t).unwrap();
        checks.push(("pptx", Some(ooxml::pptx_grid(&pptx)), false));
        let xlsx = render(&t, "xlsx", None).unwrap().into_bytes();
        checks.push(("xlsx", Some(xlsx_grid(&xlsx, want.header_lines, want.rows.len(), want.width())), true));
        for (format, got, numbers) in checks {
            // RTF can't always tell columns apart (see `rtf_grid`).
            let Some(got) = got else { continue };
            if let Err(e) = compare(&got, &want, numbers) {
                prop_assert!(false, "{format}: {e}\nIR: {}", t.to_json().unwrap());
            }
        }
    }
}

/// `CASES` random valid tables (from a fresh seed each run, like `proptest!`).
fn random_tables() -> Vec<Table> {
    let mut runner = TestRunner::new(ProptestConfig::default());
    (0..CASES)
        .map(|_| arb_valid_ir().new_tree(&mut runner).unwrap().current())
        .collect()
}

fn report(format: &str, failures: &[(String, &Table)]) {
    if let Some((e, t)) = failures.first() {
        panic!(
            "{} of {CASES} random tables fail the {format} grid; the first:\n{e}\nIR: {}",
            failures.len(),
            t.to_json().unwrap()
        );
    }
}

#[test]
fn typst_shows_the_grid_of_random_tables() {
    if !typeset::require("typst", "GRIDWELL_REQUIRE_TYPST") {
        return;
    }
    let tables = random_tables();
    let sources: Vec<String> = tables
        .iter()
        .map(|t| gridwell_writer_typst::render_typst(t).unwrap())
        .collect();
    let failures: Vec<_> = tables
        .iter()
        .zip(typst_grids(&sources))
        .filter_map(|(t, got)| {
            got.and_then(|g| compare(&g, &expected(t), false))
                .err()
                .map(|e| (e, t))
        })
        .collect();
    report("Typst", &failures);
}

#[test]
fn latex_shows_the_grid_of_random_tables() {
    if !typeset::require("lualatex", "GRIDWELL_REQUIRE_LATEX")
        || !typeset::require("pdftotext", "GRIDWELL_REQUIRE_LATEX")
    {
        return;
    }
    let tables = random_tables();
    let mut checked = Vec::new();
    let mut tabulars = Vec::new();
    for t in &tables {
        let want = expected(t);
        let src = gridwell_writer_latex::render_latex(t).unwrap();
        match latex_tabular(&src) {
            Some(tab) if latex_observable(&want) => {
                tabulars.push(tab.to_string());
                checked.push((t, want));
            }
            Some(_) => {}
            None => assert!(want.rows.is_empty(), "no tabular:\n{src}"),
        }
    }
    // Random spans can cover a column boundary in every row (see
    // `latex_observable`), but most tables must be checked.
    assert!(
        checked.len() * 4 > tables.len() * 3,
        "only {} of {} checked",
        checked.len(),
        tables.len()
    );
    let failures: Vec<_> = checked
        .iter()
        .zip(latex_tables(&tabulars))
        .filter_map(|((t, want), got)| {
            got.and_then(|g| latex_compare(&g, want))
                .err()
                .map(|e| (e, *t))
        })
        .collect();
    report("LaTeX", &failures);
}
