use gridwell_ir::style::StyleDef;
use gridwell_testkit::{cell, group, labeled_group, row, CellExt, RowExt, TableBuilder};
use serde_json::json;

use super::{BaseRules, OwnEdges};
use crate::look::{EdgePart, Rule, LOOK};
use crate::resolve;

use EdgePart::{Head, Rows, Summary};

const S: Option<Rule> = Some(LOOK.section_rule);
const R: Option<Rule> = Some(LOOK.row_rule);
const G: Option<Rule> = Some(LOOK.grand_summary_rule);

fn plain() -> OwnEdges {
    OwnEdges::default()
}

#[test]
fn boundaries_follow_the_table_structure() {
    let t = TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("A"), cell("B")]))
        .group(labeled_group(
            "G1",
            vec![
                row(vec![cell("x").stub(), cell("1")]),
                row(vec![cell("y").stub(), cell("2")]),
            ],
        ))
        .group(
            labeled_group("G2", vec![row(vec![cell("z").stub(), cell("3")])])
                .summary(vec![
                    row(vec![cell("Sum").stub(), cell("3")]).role("summary")
                ]),
        )
        .group(group(vec![]).summary(vec![
            row(vec![cell("All").stub(), cell("6")]).role("grand_summary"),
        ]))
        .build();
    let rules = BaseRules::new(&resolve(&t));
    // Rows: head, G1 label, x, y, G2 label, z, Sum, All.
    assert_eq!(rules.rows(), 8);
    assert_eq!(rules.boundary, [S, S, S, R, S, S, S, G, S]);
    assert_eq!((rules.label_row(0), rules.label_row(1)), (Some(1), Some(4)));
    assert_eq!(rules.label_row(2), None);
    assert_eq!(
        [
            rules.row(Head, 0),
            rules.row(Rows(0), 1),
            rules.row(Rows(1), 0)
        ],
        [0, 3, 5]
    );
    assert_eq!([rules.row(Summary(1), 0), rules.row(Summary(2), 0)], [6, 7]);
    // A row rule is drawn by the cell below, the head's bottom by both sides.
    assert_eq!(rules.cell(Rows(0), 0, 1, 1, plain()), (S, None));
    assert_eq!(rules.cell(Rows(0), 1, 1, 1, plain()), (R, S));
    assert_eq!(rules.cell(Head, 0, 0, 1, plain()), (S, S));
    // The summary rules are the summary cells' own.
    assert_eq!(rules.cell(Rows(1), 0, 1, 1, plain()), (S, None));
    assert_eq!(rules.cell(Summary(2), 0, 1, 1, plain()), (G, S));
}

/// A style setting (`"solid"`) or removing (`"none"`) the given sides.
fn sides(style: &str, sides: &[&str]) -> StyleDef {
    let border: serde_json::Map<_, _> = sides
        .iter()
        .map(|s| (s.to_string(), json!({ "width": "1px", "style": style })))
        .collect();
    serde_json::from_value(json!({ "border": border })).unwrap()
}

#[test]
fn own_borders_and_removed_borders() {
    let t = TableBuilder::new(1)
        .style_def("t", sides("solid", &["top"]))
        .style_def("xt", sides("none", &["top"]))
        .head(row(vec![cell("A")]))
        .body(vec![
            row(vec![cell("1")]),
            row(vec![cell("2").style("xt")]),
            row(vec![cell("3").style("t")]),
        ])
        .build();
    let rt = resolve(&t);
    let rules = BaseRules::new(&rt);
    let own = |top, clears_top| OwnEdges {
        top,
        bottom: false,
        clears_top,
    };
    // Removing the top border removes the row rule, which is the cell's own...
    assert_eq!(rules.cell(Rows(0), 1, 0, 1, own(false, true)), (None, None));
    // ...but not the head's rule, which isn't.
    assert_eq!(rules.cell(Rows(0), 0, 0, 1, own(false, true)), (S, None));
    // A border of the cell's own replaces the rule.
    assert_eq!(rules.cell(Rows(0), 2, 0, 1, own(true, false)), (None, S));
    let bottom = OwnEdges {
        bottom: true,
        ..plain()
    };
    assert_eq!(rules.cell(Rows(0), 2, 0, 1, bottom), (R, None));
}

#[test]
fn edges_drawn_by_cells_draw_the_rule_where_cells_carry_it() {
    let t = TableBuilder::new(2)
        .style_def("t", sides("solid", &["top"]))
        .style_def("xt", sides("none", &["top"]))
        .body(vec![row(vec![cell("1").style("t"), cell("2")])])
        .build();
    let rules = BaseRules::new(&resolve(&t));
    // The top is the cells': the second carries the rule, the first draws its own.
    let own = OwnEdges {
        top: true,
        ..plain()
    };
    assert_eq!(rules.cell(Rows(0), 0, 0, 1, own).0, None);
    assert_eq!(rules.cell(Rows(0), 0, 1, 1, plain()).0, S);
    // A rowspan reaches the bottom boundary of its last row.
    let t = TableBuilder::new(2)
        .body(vec![
            row(vec![cell("1").rowspan(2), cell("2")]),
            row(vec![cell("3")]),
        ])
        .build();
    let rules = BaseRules::new(&resolve(&t));
    assert_eq!(rules.cell(Rows(0), 0, 0, 2, plain()), (S, S));
    assert_eq!(rules.cell(Rows(0), 0, 1, 1, plain()), (S, None));
}

#[test]
fn a_table_without_rows_has_its_top_and_bottom() {
    let t = TableBuilder::new(1).body(vec![]).build();
    let rules = BaseRules::new(&resolve(&t));
    assert_eq!(rules.rows(), 0);
    assert_eq!(rules.boundary, [S]);
    // Out of range asks for nothing.
    assert_eq!(rules.cell(Rows(3), 9, 0, 4, plain()), (None, None));
}
