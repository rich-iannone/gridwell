//! A cell's weight replaces the default: column labels are bold unless their
//! style sets another weight, and a body cell is bold only if its style says so.

use gridwell_ir::style::StyleDef;
use gridwell_testkit::{cell, group, row, CellExt, RowExt, TableBuilder};
use gridwell_writer_ansi::render_ansi;

const BOLD: &str = "\x1b[1m";

fn weight(w: &str) -> StyleDef {
    StyleDef {
        font_weight: Some(w.into()),
        ..StyleDef::default()
    }
}

/// Whether the text `t` is shown bold: a bold escape opens the run it is in.
fn bold(out: &str, t: &str) -> bool {
    let at = out.find(t).unwrap_or_else(|| panic!("no {t} in {out:?}"));
    let open = &out[..at];
    let run = &open[open.rfind("\x1b[0m").map_or(0, |i| i + 4)..];
    run.contains(BOLD)
}

#[test]
fn a_style_weight_replaces_the_default() {
    let table = TableBuilder::new(3)
        .style_def("n", weight("normal"))
        .style_def("l", weight("lighter"))
        .style_def("b", weight("bold"))
        .head(
            row(vec![
                cell("HNORMAL").style("n"),
                cell("HLIGHT").style("l"),
                cell("HPLAIN"),
            ])
            .role("column_label"),
        )
        .group(group(vec![row(vec![
            cell("BBOLD").style("b"),
            cell("BNORMAL").style("n"),
            cell("BPLAIN"),
        ])]))
        .build();
    let out = render_ansi(&table).unwrap();
    for (t, want) in [
        ("HNORMAL", false),
        ("HLIGHT", false),
        ("HPLAIN", true),
        ("BBOLD", true),
        ("BNORMAL", false),
        ("BPLAIN", false),
    ] {
        assert_eq!(bold(&out, t), want, "{t} in {out:?}");
    }
}
