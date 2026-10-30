//! Booktabs rules: never two rules in a row (a doubled `\midrule` draws two
//! lines), and every column spanner is underlined by a `\cmidrule` over exactly
//! the columns it spans.

use gridwell_testkit::{
    cell, column, examples, labeled_group, placeholder, row, CellExt, TableBuilder,
};
use gridwell_writer_latex::{render_latex, LatexWriter, LatexWriterConfig};

fn is_rule(line: &str) -> bool {
    [
        "\\toprule",
        "\\midrule",
        "\\bottomrule",
        "\\hline",
        "\\cmidrule",
    ]
    .iter()
    .any(|r| line.starts_with(r))
}

/// The tabular's lines, in the pieces drawn one after another: a longtable's
/// heads and feet (ended by `\endhead`…) are each drawn apart from the body.
fn grid_pieces(src: &str) -> Vec<Vec<&str>> {
    let mut pieces = vec![vec![]];
    for l in src
        .lines()
        .skip_while(|l| !l.starts_with("\\begin{"))
        .take_while(|l| !l.starts_with("\\end{"))
    {
        if ["\\endfirsthead", "\\endhead", "\\endfoot", "\\endlastfoot"].contains(&l) {
            pieces.push(vec![]);
        } else {
            pieces.last_mut().unwrap().push(l);
        }
    }
    pieces
}

#[test]
fn corpus_has_no_doubled_rules() {
    for (ex, longtable) in examples().iter().flat_map(|ex| [(ex, false), (ex, true)]) {
        let src = LatexWriter::with_config(LatexWriterConfig {
            longtable,
            ..Default::default()
        })
        .render(&ex.table())
        .unwrap();
        for pair in grid_pieces(&src).iter().flat_map(|p| p.windows(2)) {
            assert!(
                !(is_rule(pair[0]) && is_rule(pair[1]) && !pair[1].starts_with("\\cmidrule")),
                "{}: {:?} right after {:?}\n{src}",
                ex.name,
                pair[1],
                pair[0]
            );
        }
    }
}

/// A gt-style head: an empty stub label, "Placebo", and a "Xanomeline" spanner
/// over the last two columns; then two row groups with labels.
fn spanner_table() -> gridwell_ir::Table {
    TableBuilder::new(4)
        .columns(vec![
            column("stat", ""),
            column("pbo", "Placebo"),
            column("low", "Low Dose"),
            column("high", "High Dose"),
        ])
        .head(row(vec![
            cell(""),
            cell(""),
            cell("Xanomeline").colspan(2),
            placeholder(),
        ]))
        .head(row(vec![
            cell(""),
            cell("Placebo"),
            cell("Low Dose"),
            cell("High Dose"),
        ]))
        .group(labeled_group(
            "Age (years)",
            vec![row(vec![
                cell("Mean").stub(),
                cell("75.2"),
                cell("75.7"),
                cell("74.4"),
            ])],
        ))
        .group(labeled_group(
            "Sex, n (%)",
            vec![row(vec![
                cell("Female").stub(),
                cell("53"),
                cell("50"),
                cell("40"),
            ])],
        ))
        .build()
}

#[test]
fn spanners_are_underlined_and_group_labels_dont_double_the_head_rule() {
    let src = render_latex(&spanner_table()).unwrap();
    let lines = grid_pieces(&src).concat();
    let at = |needle: &str| {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no {needle:?} in\n{src}"))
    };
    let spanner = at("{Xanomeline}");
    assert_eq!(lines[spanner + 1], "\\cmidrule(lr){3-4}", "{src}");
    let labels = at("Placebo");
    assert_eq!(lines[labels + 1], "\\midrule", "{src}");
    assert!(lines[labels + 2].contains("Age (years)"), "{src}");
    // Between groups, one rule before the label and one after.
    let second = at("Sex, n");
    assert_eq!(lines[second - 1], "\\midrule", "{src}");
    assert!(!is_rule(lines[second - 2]), "{src}");
    assert_eq!(lines[second + 1], "\\midrule", "{src}");
}

#[test]
fn spanner_rules_follow_hidden_columns() {
    // Hiding the first column shifts the spanner to visible columns 2-3.
    let mut t = spanner_table();
    t.column_spec[0].hidden = true;
    let src = render_latex(&t).unwrap();
    assert!(src.contains("\\cmidrule(lr){2-3}\n"), "{src}");
}

#[test]
fn without_booktabs_spanners_are_underlined_with_cline() {
    use gridwell_writer_latex::{LatexWriter, LatexWriterConfig};
    let src = LatexWriter::with_config(LatexWriterConfig {
        booktabs: false,
        ..Default::default()
    })
    .render(&spanner_table())
    .unwrap();
    assert!(src.contains("{Xanomeline} \\\\\n\\cline{3-4}\n"), "{src}");
    assert!(!src.contains("cmidrule"), "{src}");
}
