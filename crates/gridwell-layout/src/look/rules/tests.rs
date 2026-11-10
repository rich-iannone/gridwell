use gridwell_ir::style::StyleDef;
use gridwell_testkit::{cell, group, labeled_group, row, CellExt, RowExt, TableBuilder};
use serde_json::json;

use super::{BaseRules, OwnEdges};
use crate::look::{EdgePart, LineStyle, Rule, LOOK};
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

/// `t` with `config.rules` set from JSON.
fn with_rules(t: TableBuilder, rules: serde_json::Value) -> TableBuilder {
    t.config(|c| c.rules = Some(serde_json::from_value(rules).unwrap()))
}

fn rule(width_px: f64, color: &str, style: LineStyle) -> Option<Rule> {
    Some(Rule {
        width_px,
        color: color.parse().unwrap(),
        style,
    })
}

#[test]
fn summaries_at_the_top_have_a_rule_above_and_below() {
    let sum = |label: &str, role: &str| row(vec![cell(label).stub(), cell("9")]).role(role);
    let data = |label: &str| row(vec![cell(label).stub(), cell("1")]);
    let t = TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("A"), cell("B")]))
        .group(
            group(vec![])
                .summary(vec![
                    sum("All", "grand_summary"),
                    sum("Mean", "grand_summary"),
                ])
                .summary_top(),
        )
        .group(
            labeled_group("G", vec![data("x"), data("y")])
                .summary(vec![sum("Sum", "summary"), sum("Max", "summary")])
                .summary_top(),
        )
        .group(
            group(vec![data("z")])
                .summary(vec![sum("Sum", "summary")])
                .summary_top(),
        )
        .build();
    let rt = resolve(&t);
    let rules = BaseRules::new(&rt);
    // Rows: head, All, Mean, G label, Sum, Max, x, y, Sum, z.
    assert_eq!(rules.rows(), 10);
    assert_eq!(rules.boundary, [S, S, R, G, S, R, S, R, S, S, S]);
    assert_eq!(
        [
            rules.row(Summary(1), 0),
            rules.row(Rows(1), 0),
            rules.row(Summary(2), 0),
            rules.row(Rows(2), 0)
        ],
        [4, 6, 8, 9]
    );
    assert_eq!(rules.label_row(1), Some(3));
    // Under a label, the rule above the summary rows is the label's; the rule
    // below them isn't the next row's, so both sides draw it.
    assert_eq!(rules.cell(Summary(1), 0, 1, 1, plain()), (S, None));
    assert_eq!(rules.cell(Summary(1), 1, 1, 1, plain()), (R, S));
    assert_eq!(rules.cell(Rows(1), 0, 1, 1, plain()), (S, None));
    let clears = OwnEdges {
        clears_top: true,
        ..plain()
    };
    assert_eq!(rules.cell(Rows(1), 0, 1, 1, clears), (S, None));
    assert_eq!(rules.cell(Summary(1), 0, 1, 1, clears), (S, None));
    // Without one, it is the summary rows' own.
    assert_eq!(rules.cell(Summary(2), 0, 1, 1, plain()), (S, S));
    assert_eq!(rules.cell(Summary(2), 0, 1, 1, clears), (None, S));
    // The grand summary's rule is below it.
    assert_eq!(rules.cell(Summary(0), 1, 1, 1, plain()), (R, G));
    assert_eq!(rules.cell(Summary(0), 0, 1, 1, plain()), (S, None));
}

#[test]
fn config_rules_change_the_defaults() {
    use crate::look::{TableRules, DEFAULT_RULES};
    let t = || TableBuilder::new(1).body(vec![row(vec![cell("1")])]);
    assert_eq!(TableRules::new(&t().build()), DEFAULT_RULES);
    let t = with_rules(
        t(),
        json!({
            "title": { "style": "none" },
            "top": { "width": "3px", "style": "dashed", "color": "#FF0000" },
            "column_labels": { "color": "blue" },
            "spanners": { "width": "0" },
            "group": { "width": "0.25em" },
            "rows": { "width": "1pt", "style": "hidden" },
            "stub": { "width": "-2px", "color": "nonsense", "style": "wavy" },
            "summary": { "style": "double" },
            "grand_summary": { "width": "1px" },
            "bottom": { "width": "4px", "style": "double" },
        }),
    )
    .build();
    let r = TableRules::new(&t);
    let light = "#D3D3D3";
    assert_eq!(r.title, None);
    assert_eq!(r.top, rule(3.0, "#FF0000", LineStyle::Dashed));
    assert_eq!(r.column_labels, rule(2.0, "blue", LineStyle::Solid));
    assert_eq!(r.spanners, None);
    // 0.25em of the look's 16px.
    assert_eq!(r.group, rule(4.0, light, LineStyle::Solid));
    assert_eq!(r.rows, None);
    // Invalid values change nothing.
    assert_eq!(r.stub, DEFAULT_RULES.stub);
    // A double rule under 3px is solid, as in CSS.
    assert_eq!(r.summary, rule(2.0, light, LineStyle::Solid));
    assert_eq!(r.grand_summary, rule(1.0, light, LineStyle::Solid));
    assert_eq!(r.bottom, rule(4.0, light, LineStyle::Double));
    assert_eq!(r.notes, DEFAULT_RULES.notes);
}

#[test]
fn where_rules_meet_html_s_collapsed_borders_decide() {
    let t = || {
        TableBuilder::new(1)
            .head(row(vec![cell("A")]))
            .group(labeled_group(
                "G",
                vec![row(vec![cell("1")]), row(vec![cell("2")])],
            ))
    };
    let boundary = |rules| BaseRules::new(&resolve(&with_rules(t(), rules).build())).boundary;
    let light = "#D3D3D3";
    // Rows: head, G, 1, 2. Equally wide, the stronger style wins: below the label,
    // the row rule.
    let dashed = rule(1.0, light, LineStyle::Dashed);
    assert_eq!(
        boundary(json!({ "group": { "width": "1px", "style": "dashed" } })),
        [S, S, R, R, S]
    );
    // Differing only in color, the upper cell's: below the label, the label's.
    let red = rule(1.0, "red", LineStyle::Solid);
    assert_eq!(
        boundary(json!({ "group": { "width": "1px", "color": "red" } })),
        [S, S, red, R, S]
    );
    // Over the head's, the cell's.
    assert_eq!(
        boundary(json!({ "group": { "color": "red" } }))[1],
        rule(2.0, "red", LineStyle::Solid)
    );
    let one_group = |rules| {
        let t = TableBuilder::new(1).group(group(vec![row(vec![cell("1")])]));
        BaseRules::new(&resolve(&with_rules(t, rules).build())).boundary
    };
    assert_eq!(
        one_group(json!({ "top": { "width": "1px", "style": "dashed" } })),
        [dashed, S]
    );
    // A removed rule leaves the others; the first row has no row rule.
    assert_eq!(
        boundary(json!({ "group": { "style": "none" } })),
        [S, S, R, R, S]
    );
    assert_eq!(
        boundary(json!({ "column_labels": { "style": "none" }, "group": { "style": "none" } })),
        [S, None, R, R, S]
    );
    assert_eq!(
        one_group(json!({ "top": { "style": "none" }, "rows": { "width": "5px" } })),
        [None, S]
    );
    // The table's top loses to a wider row rule only where there is one.
    let thick = rule(5.0, light, LineStyle::Solid);
    assert_eq!(
        boundary(json!({ "rows": { "width": "5px" } })),
        [S, S, thick, thick, S]
    );
}

#[test]
fn with_no_groups_the_head_bottom_is_the_table_bottom() {
    let t = with_rules(
        TableBuilder::new(1).head(row(vec![cell("A")])),
        json!({ "column_labels": { "width": "4px" } }),
    )
    .build();
    let rt = resolve(&t);
    assert!(rt.groups.is_empty(), "{:?}", rt.groups.len());
    assert_eq!(BaseRules::new(&rt).boundary, [S, S]);
}
