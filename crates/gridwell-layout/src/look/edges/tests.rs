use gridwell_ir::style::StyleDef;
use gridwell_ir::Table;
use gridwell_testkit::{cell, labeled_group, placeholder, row, CellExt, RowExt, TableBuilder};
use serde_json::json;

use super::{EdgePart, EdgeRules};
use crate::resolve;

use EdgePart::{Head, Rows, Summary};

/// A style setting (`"solid"`) or removing (`"none"`) the given sides.
fn sides(style: &str, sides: &[&str]) -> StyleDef {
    let mut border = serde_json::Map::new();
    for s in sides {
        border.insert(
            s.to_string(),
            json!({ "width": "1px", "style": style, "color": "#333333" }),
        );
    }
    serde_json::from_value(json!({ "border": border })).unwrap()
}

/// A builder with styles `t`, `b`, `tb` (solid top, bottom, both) and `xt`, `xb`
/// (top, bottom removed).
fn styled(cols: u32) -> TableBuilder {
    TableBuilder::new(cols)
        .style_def("t", sides("solid", &["top"]))
        .style_def("b", sides("solid", &["bottom"]))
        .style_def("tb", sides("solid", &["top", "bottom"]))
        .style_def("xt", sides("none", &["top"]))
        .style_def("xb", sides("none", &["bottom"]))
}

fn edges(t: &Table) -> EdgeRules {
    EdgeRules::new(&resolve(t))
}

/// Whether the cells starting in row `r` of `part`, in `cols`, carry the rule on
/// their (top, bottom).
fn carries(e: &EdgeRules, part: EdgePart, r: usize, cols: &[usize]) -> Vec<(bool, bool)> {
    cols.iter()
        .map(|&c| (e.carries_top(part, r, c), e.carries_bottom(part, r, c)))
        .collect()
}

#[test]
fn without_cell_borders_no_edge_is_the_cells() {
    let t = styled(2)
        .head(row(vec![cell("A"), cell("B")]))
        .body(vec![row(vec![cell("1"), cell("2")])])
        .build();
    let e = edges(&t);
    assert_eq!(e, EdgeRules::default());
    // Removing a border alone leaves the rule: `none` loses to it in HTML.
    let t = styled(1)
        .head(row(vec![cell("A").style("xb")]))
        .body(vec![row(vec![cell("1").style("xt")]).style("xb")])
        .build();
    assert_eq!(edges(&t), EdgeRules::default());
}

#[test]
fn bordered_cells_draw_every_edge_themselves() {
    let t = styled(2)
        .head(row(vec![cell("A").style("tb"), cell("B").style("tb")]))
        .body(vec![
            row(vec![cell("1").style("tb"), cell("2").style("tb")]),
            row(vec![cell("3").style("tb"), cell("4").style("tb")]),
        ])
        .build();
    let e = edges(&t);
    assert!(e.top && e.head && e.bottom);
    assert!(e.carriers.is_empty(), "{e:?}");
}

#[test]
fn the_other_cells_on_an_edge_carry_its_rule() {
    // Top: one cell sets its top, one removes it, one has neither.
    let t = styled(3)
        .head(row(vec![
            cell("A").style("t"),
            cell("B").style("xt"),
            cell("C"),
        ]))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3").style("b")])])
        .build();
    let e = edges(&t);
    assert!(e.top && !e.head && e.bottom);
    assert_eq!(
        carries(&e, Head, 0, &[0, 1, 2]),
        [(false, false), (false, false), (true, false)]
    );
    assert_eq!(
        carries(&e, Rows(0), 0, &[0, 1, 2]),
        [(false, true), (false, true), (false, false)]
    );
}

#[test]
fn under_the_head_a_border_across_the_edge_wins() {
    // A spanner over a cell with its own top, and one without.
    let t = styled(3)
        .head(row(vec![
            cell("AB").colspan(2),
            placeholder(),
            cell("C").style("b"),
        ]))
        .body(vec![row(vec![cell("1").style("t"), cell("2"), cell("3")])])
        .build();
    let e = edges(&t);
    assert!(!e.top && e.head && !e.bottom);
    // The spanner faces a border; "C" draws its own.
    assert_eq!(
        carries(&e, Head, 0, &[0, 2]),
        [(false, false), (false, false)]
    );
    // "2" carries it; "3" faces "C"'s border.
    assert_eq!(
        carries(&e, Rows(0), 0, &[0, 1, 2]),
        [(false, false), (true, false), (false, false)]
    );
}

#[test]
fn the_bottom_is_every_cell_reaching_it() {
    let t = styled(2)
        .head(row(vec![cell("A"), cell("B")]))
        .body(vec![
            row(vec![cell("1").rowspan(2).style("b"), cell("2")]),
            row(vec![placeholder(), cell("3")]),
        ])
        .build();
    let e = edges(&t);
    assert!(!e.top && !e.head && e.bottom);
    assert_eq!(
        carries(&e, Rows(0), 0, &[0, 1]),
        [(false, false), (false, false)]
    );
    assert_eq!(carries(&e, Rows(0), 1, &[1]), [(false, true)]);
}

#[test]
fn with_no_body_the_head_bottom_is_the_table_bottom() {
    let t = styled(2)
        .head(row(vec![cell("A").style("b"), cell("B")]))
        .build();
    let e = edges(&t);
    assert!(!e.top && e.head && e.bottom);
    assert_eq!(
        carries(&e, Head, 0, &[0, 1]),
        [(false, false), (false, true)]
    );
}

#[test]
fn group_labels_and_summaries() {
    // A label first: neither the top nor the head's bottom are cells.
    let t = styled(2)
        .stub_cols(1)
        .group(
            labeled_group("G", vec![row(vec![cell("x").stub().style("t"), cell("1")])]).summary(
                vec![row(vec![cell("Total").stub(), cell("1").style("b")]).role("summary")],
            ),
        )
        .build();
    let e = edges(&t);
    assert!(!e.top && !e.head && e.bottom);
    // The bottom is the summary's.
    assert_eq!(
        carries(&e, Summary(0), 0, &[0, 1]),
        [(false, true), (false, false)]
    );
    assert_eq!(
        carries(&e, Rows(0), 0, &[0, 1]),
        [(false, false), (false, false)]
    );

    // Under the head, a label first: the head's bottom stays the rule.
    let t = styled(2)
        .stub_cols(1)
        .head(row(vec![cell(""), cell("A")]))
        .group(labeled_group(
            "G",
            vec![row(vec![cell("x").stub().style("t"), cell("1")])],
        ))
        .build();
    let e = edges(&t);
    assert!(!e.top && !e.head && !e.bottom);
    assert!(e.carriers.is_empty(), "{e:?}");

    // Under the head, a group with only summary rows.
    let t = styled(2)
        .stub_cols(1)
        .head(row(vec![cell(""), cell("A")]))
        .group(gridwell_testkit::group(vec![]).summary(vec![
            row(vec![cell("Total").stub(), cell("1").style("t")]).role("summary"),
        ]))
        .build();
    let e = edges(&t);
    assert!(e.head && !e.top && !e.bottom);
    assert_eq!(
        carries(&e, Head, 0, &[0, 1]),
        [(false, true), (false, false)]
    );
    assert_eq!(
        carries(&e, Summary(0), 0, &[0, 1]),
        [(true, false), (false, false)]
    );
}

#[test]
fn summaries_at_the_top_come_first_and_edges_say_which_they_are() {
    use super::Edge;
    // Under the head, the first group's summary rows at the top; the data rows
    // reach the bottom.
    let t = styled(2)
        .stub_cols(1)
        .head(row(vec![cell("").style("tb"), cell("A")]))
        .group(
            gridwell_testkit::group(vec![row(vec![cell("x").stub(), cell("1").style("b")])])
                .summary(vec![
                    row(vec![cell("Total").stub(), cell("1")]).role("summary")
                ])
                .summary_top(),
        )
        .build();
    let e = edges(&t);
    assert!(e.top && e.head && e.bottom);
    let at = |part, r, c| (e.carried_top(part, r, c), e.carried_bottom(part, r, c));
    assert_eq!(at(Head, 0, 0), (None, None));
    assert_eq!(at(Head, 0, 1), (Some(Edge::Top), Some(Edge::Head)));
    // The summary row, not the data row, is under the head.
    assert_eq!(at(Summary(0), 0, 0), (None, None));
    assert_eq!(at(Summary(0), 0, 1), (Some(Edge::Head), None));
    assert_eq!(at(Rows(0), 0, 0), (None, Some(Edge::Bottom)));
    assert_eq!(at(Rows(0), 0, 1), (None, None));
}
