//! The style cascade: column default → striping → conditionals → row → cell, with
//! compositions, per-side padding/border merging, and invalid values dropped.

use gridwell_core::{Color, FontSize, Length};
use gridwell_ir::style::{ConditionalSelector, StyleDef};
use gridwell_ir::{BorderStyle, HAlign, Table};
use gridwell_layout::{resolve, ResolvedCell, ResolvedTable, STRIPE_COLOR};
use gridwell_testkit::{
    border, cell, column, group, padding_all, row, CellExt, ColumnExt, RowExt, TableBuilder,
};

fn c(s: &str) -> Color {
    s.parse().unwrap()
}

fn def() -> StyleDef {
    StyleDef::default()
}

/// The body cells of the resolved table, by (data row index across groups, column).
fn body<'r, 'a>(rt: &'r ResolvedTable<'a>) -> Vec<Vec<&'r ResolvedCell<'a>>> {
    rt.groups
        .iter()
        .flat_map(|g| g.rows.rows.iter().map(|r| r.cells().collect()))
        .collect()
}

#[test]
fn each_layer_overrides_only_what_it_sets() {
    // Each layer sets `color` plus one property of its own; the highest layer
    // setting `color` wins, and every layer's own property survives.
    let t = TableBuilder::new(1)
        .columns(vec![column("a", "A").style("col")])
        .style_def(
            "col",
            StyleDef {
                color: Some("#000001".into()),
                font_family: Some("ColFont".into()),
                ..def()
            },
        )
        .style_def(
            "row",
            StyleDef {
                color: Some("#000003".into()),
                font_size: Some("20px".into()),
                ..def()
            },
        )
        .style_def(
            "cell",
            StyleDef {
                color: Some("#000004".into()),
                font_weight: Some("bold".into()),
                ..def()
            },
        )
        .conditional(
            "cond",
            ConditionalSelector {
                row_parity: None,
                scope: None,
            },
            StyleDef {
                color: Some("#000002".into()),
                text_transform: Some("uppercase".into()),
                ..def()
            },
        )
        .body(vec![
            row(vec![cell("all").style("cell")]).style("row"),
            row(vec![cell("no cell")]).style("row"),
            row(vec![cell("no row/cell")]),
        ])
        .build();
    let rt = resolve(&t);
    let b = body(&rt);
    let all = &b[0][0].style;
    assert_eq!(all.color, Some(c("#000004")));
    assert_eq!(all.font_family.as_deref(), Some("ColFont"));
    assert_eq!(all.font_size, Some(FontSize::Length(Length::Px(20.0))));
    assert!(all.is_bold());
    assert!(all.text_transform.is_some());
    assert_eq!(b[1][0].style.color, Some(c("#000003")));
    assert!(!b[1][0].style.is_bold());
    assert_eq!(b[2][0].style.color, Some(c("#000002")));
    assert!(b[2][0].style.font_size.is_none());

    // Without the conditional, the column default shows through.
    let mut t2 = t.clone();
    t2.styles.conditionals.clear();
    assert_eq!(body(&resolve(&t2))[2][0].style.color, Some(c("#000001")));
}

#[test]
fn column_default_skips_column_labels_but_applies_to_summaries() {
    let t = TableBuilder::new(1)
        .stub_cols(1)
        .columns(vec![column("a", "A").style("col")])
        .style_def(
            "col",
            StyleDef {
                background_color: Some("red".into()),
                ..def()
            },
        )
        .head(row(vec![cell("label")]))
        .group(group(vec![row(vec![cell("x")])]).summary(vec![row(vec![cell("total")])]))
        .build();
    let rt = resolve(&t);
    let head = rt.head.rows[0].cells().next().unwrap();
    assert!(head.is_header);
    assert_eq!(head.style.background_color, None);
    assert_eq!(body(&rt)[0][0].style.background_color, Some(c("red")));
    let summary = rt.groups[0].summary_rows.rows[0].cells().next().unwrap();
    assert_eq!(summary.style.background_color, Some(c("red")));
}

#[test]
fn compositions_extend_their_base() {
    let t = TableBuilder::new(2)
        .style_def(
            "base",
            StyleDef {
                color: Some("navy".into()),
                font_weight: Some("bold".into()),
                padding: Some(padding_all("4px")),
                ..def()
            },
        )
        .composition(
            "comp",
            "base",
            StyleDef {
                color: Some("teal".into()),
                padding: Some(gridwell_ir::style::Padding {
                    top: Some("9px".into()),
                    right: None,
                    bottom: None,
                    left: None,
                }),
                ..def()
            },
        )
        .composition(
            "dangling",
            "missing",
            StyleDef {
                color: Some("red".into()),
                ..def()
            },
        )
        .body(vec![row(vec![
            cell("x").style("comp"),
            cell("y").style("dangling"),
        ])])
        .build();
    let rt = resolve(&t);
    let b = body(&rt);
    let s = &b[0][0].style;
    assert_eq!(s.color, Some(c("teal")));
    assert!(s.is_bold());
    // Padding merges per side.
    assert_eq!(s.padding.top, Some(Length::Px(9.0)));
    assert_eq!(s.padding.left, Some(Length::Px(4.0)));
    // A composition whose base is missing contributes nothing.
    assert!(b[0][1].style.is_empty());
}

#[test]
fn borders_merge_per_side_and_none_removes_an_edge() {
    let t = TableBuilder::new(1)
        .style_def(
            "row",
            StyleDef {
                border: Some(gridwell_ir::style::BorderSet {
                    top: Some(border("2px", "solid", "red")),
                    bottom: Some(border("1px", "dashed", "blue")),
                    left: None,
                    right: None,
                }),
                ..def()
            },
        )
        .style_def(
            "cell",
            StyleDef {
                border: Some(gridwell_ir::style::BorderSet {
                    top: Some(border("1px", "none", "red")),
                    left: Some(border("3pt", "double", "nonsense")),
                    bottom: None,
                    right: None,
                }),
                ..def()
            },
        )
        .body(vec![row(vec![cell("x").style("cell")]).style("row")])
        .build();
    let rt = resolve(&t);
    let s = &body(&rt)[0][0].style;
    assert_eq!(s.border.top, None, "border-style none removes the edge");
    let bottom = s.border.bottom.as_ref().unwrap();
    assert_eq!(
        (bottom.style.clone(), bottom.width.clone(), bottom.color),
        (BorderStyle::Dashed, Some(Length::Px(1.0)), Some(c("blue")))
    );
    let left = s.border.left.as_ref().unwrap();
    assert_eq!(
        (left.style.clone(), left.color),
        (BorderStyle::Double, None)
    );
    assert_eq!(s.border.right, None);
}

#[test]
fn conditionals_match_scope_and_css_parity() {
    let cond = |parity: Option<&str>, scope: Option<&str>, color: &str| {
        (
            ConditionalSelector {
                row_parity: parity.map(Into::into),
                scope: scope.map(Into::into),
            },
            StyleDef {
                color: Some(color.into()),
                ..def()
            },
        )
    };
    let cases = [
        // (selector, which data rows match (1-based), head matches, summary matches)
        (
            cond(Some("odd"), Some("tbody"), "red"),
            vec![1, 3],
            false,
            false,
        ),
        (
            cond(Some("even"), Some("body"), "red"),
            vec![2],
            false,
            false,
        ),
        (cond(None, Some("thead"), "red"), vec![], true, false),
        (cond(None, None, "red"), vec![1, 2, 3], true, true),
        (
            cond(Some("odd"), Some("table"), "red"),
            vec![1, 3],
            true,
            true,
        ),
        (cond(Some("sometimes"), None, "red"), vec![], false, false),
        (cond(None, Some("tfoot"), "red"), vec![], false, false),
    ];
    for ((sel, style), data, head, summary) in cases {
        let what = format!("{sel:?}");
        let t = TableBuilder::new(1)
            .stub_cols(1)
            .conditional("c", sel, style)
            .head(row(vec![cell("h")]))
            // Two groups: data rows are numbered 1, 2 | 3 across them.
            .group(group(vec![row(vec![cell("1")]), row(vec![cell("2")])]))
            .group(group(vec![row(vec![cell("3")])]).summary(vec![row(vec![cell("s")])]))
            .build();
        let rt = resolve(&t);
        let red = Some(c("red"));
        let matched: Vec<usize> = body(&rt)
            .iter()
            .enumerate()
            .filter(|(_, r)| r[0].style.color == red)
            .map(|(i, _)| i + 1)
            .collect();
        assert_eq!(matched, data, "{what}");
        let h = rt.head.rows[0].cells().next().unwrap();
        assert_eq!(h.style.color == red, head, "{what}: head");
        let s = rt.groups[1].summary_rows.rows[0].cells().next().unwrap();
        assert_eq!(s.style.color == red, summary, "{what}: summary");
    }
}

#[test]
fn later_conditionals_override_earlier_ones() {
    let sel = || ConditionalSelector {
        row_parity: None,
        scope: None,
    };
    let t = TableBuilder::new(1)
        .conditional(
            "a",
            sel(),
            StyleDef {
                color: Some("red".into()),
                ..def()
            },
        )
        .conditional(
            "b",
            sel(),
            StyleDef {
                color: Some("blue".into()),
                ..def()
            },
        )
        .body(vec![row(vec![cell("x")])])
        .build();
    assert_eq!(body(&resolve(&t))[0][0].style.color, Some(c("blue")));
}

fn striped(include_stub: bool, include_body: bool) -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .striping(include_stub, include_body)
        .head(row(vec![cell("h"), cell("h")]))
        .group(
            group(
                (1..=4)
                    .map(|i| row(vec![cell(&format!("s{i}")), cell(&format!("b{i}"))]))
                    .collect(),
            )
            .summary(vec![
                row(vec![cell("t"), cell("t")]),
                row(vec![cell("t"), cell("t")]),
            ]),
        )
        .build()
}

#[test]
fn striping_colours_even_data_rows_in_the_included_columns() {
    for (stub, bodyc) in [(false, false), (true, false), (false, true), (true, true)] {
        let t = striped(stub, bodyc);
        let rt = resolve(&t);
        let flags: Vec<bool> = rt.groups[0].rows.rows.iter().map(|r| r.striped).collect();
        assert_eq!(flags, vec![false, true, false, true]);
        for (i, r) in body(&rt).iter().enumerate() {
            let even = i % 2 == 1;
            assert_eq!(
                r[0].style.background_color == Some(STRIPE_COLOR),
                even && stub,
                "stub row {i}"
            );
            assert_eq!(
                r[1].style.background_color == Some(STRIPE_COLOR),
                even && bodyc,
                "body row {i}"
            );
        }
        // Never the head or summary rows.
        for s in [&rt.head, &rt.groups[0].summary_rows] {
            for r in &s.rows {
                assert!(!r.striped);
                assert!(r.cells().all(|c| c.style.background_color.is_none()));
            }
        }
    }
}

#[test]
fn striping_is_below_conditionals_rows_and_cells() {
    let mut t = striped(true, true);
    t.styles.defs.insert(
        "white".into(),
        StyleDef {
            background_color: Some("white".into()),
            ..def()
        },
    );
    t.table.tbody[0].rows[1].cells[1].style_id = Some("white".into());
    t.table.tbody[0].rows[3].style_id = Some("white".into());
    let rt = resolve(&t);
    let b = body(&rt);
    assert_eq!(b[1][0].style.background_color, Some(STRIPE_COLOR));
    assert_eq!(b[1][1].style.background_color, Some(c("white")));
    assert_eq!(b[3][0].style.background_color, Some(c("white")));
}

#[test]
fn stripe_colour_is_gt_default_and_flattens_to_near_white() {
    assert_eq!(STRIPE_COLOR, c("rgba(128, 128, 128, 0.05)"));
    assert_eq!(STRIPE_COLOR.flatten(), Color::rgb(249, 249, 249));
}

#[test]
fn invalid_values_and_unknown_keywords_resolve_to_none() {
    let t = Table::from_json(
        &serde_json::json!({
            "ir_version": "1.0",
            "config": { "table_cols": 1, "body_rows": 1 },
            "styles": { "defs": { "s": {
                "color": "nope", "background_color": "#ééé", "font_size": "huge",
                "font_weight": "heavy", "font_style": "slanty", "text_align": "diagonal",
                "font_family": "Arial; } </style>", "indent": "1 px", "min_width": "NaNpx",
                "padding": { "top": "x", "left": "2px" }
            } }, "compositions": {}, "conditionals": [] },
            "column_spec": [{ "id": "a", "align": "sideways" }],
            "table": { "thead": { "rows": [] }, "tbody": [{ "rows": [
                { "role": "mystery", "cells": [{ "content": [], "style_id": "s", "scope": "everything" }] }
            ] }] }
        })
        .to_string(),
    )
    .unwrap();
    let rt = resolve(&t);
    let row = &rt.groups[0].rows.rows[0];
    assert_eq!(row.role, None);
    let cell = row.cells().next().unwrap();
    let s = &cell.style;
    assert_eq!(
        (
            s.color,
            s.background_color,
            s.font_size.clone(),
            s.font_weight.clone()
        ),
        (None, None, None, None)
    );
    assert_eq!((s.font_style.clone(), s.text_align.clone()), (None, None));
    assert_eq!((s.indent.clone(), s.min_width.clone()), (None, None));
    assert_eq!(s.font_family.as_deref(), Some("Arial  style"));
    assert_eq!(
        (s.padding.top.clone(), s.padding.left.clone()),
        (None, Some(Length::Px(2.0)))
    );
    assert_eq!(cell.align, HAlign::Left);
    assert_eq!(cell.scope, None);
}

#[test]
fn alignment_prefers_the_style_then_the_column() {
    let t = TableBuilder::new(2)
        .columns(vec![
            column("a", "A").align("right"),
            column("b", "B").align("center"),
        ])
        .style_def(
            "left",
            StyleDef {
                text_align: Some("left".into()),
                ..def()
            },
        )
        .body(vec![row(vec![cell("x"), cell("y").style("left")])])
        .build();
    let rt = resolve(&t);
    let b = body(&rt);
    assert_eq!(b[0][0].align, HAlign::Right);
    assert_eq!(b[0][1].align, HAlign::Left);
}

#[test]
fn header_footer_and_labels_resolve_their_styles() {
    let mut t = TableBuilder::new(1)
        .title("T")
        .subtitle("S")
        .style_def(
            "big",
            StyleDef {
                font_size: Some("x-large".into()),
                ..def()
            },
        )
        .group(gridwell_testkit::labeled_group("G", vec![row(vec![cell("x")])]).label_style("big"))
        .footnote("f1", "a", "note")
        .source_note("src")
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("big".into());
    t.footer.as_mut().unwrap().footnotes[0].style_id = Some("big".into());
    let rt = resolve(&t);
    let big = Some(FontSize::Keyword("x-large", 24.0));
    assert_eq!(rt.header.title.as_ref().unwrap().style.font_size, big);
    assert!(rt.header.subtitle.as_ref().unwrap().style.is_empty());
    assert_eq!(rt.groups[0].label.as_ref().unwrap().style.font_size, big);
    let f = &rt.footer.footnotes[0];
    assert_eq!((f.id, f.mark, f.style.font_size.clone()), ("f1", "a", big));
    assert_eq!(rt.footer.source_notes.len(), 1);
}

#[test]
fn every_style_def_field_survives_the_cascade() {
    // A def with every field set, applied at each layer in turn, resolves to the
    // same style: no layer drops a property.
    let full = StyleDef {
        font_family: Some("Inter".into()),
        font_size: Some("12pt".into()),
        font_weight: Some("700".into()),
        font_style: Some("italic".into()),
        color: Some("red".into()),
        background_color: Some("blue".into()),
        text_align: Some("center".into()),
        vertical_align: Some("top".into()),
        text_transform: Some("uppercase".into()),
        text_decoration: Some("underline".into()),
        white_space: Some("nowrap".into()),
        padding: Some(padding_all("3px")),
        border: Some(gridwell_testkit::border_all(border(
            "1px", "solid", "black",
        ))),
        indent: Some("1em".into()),
        word_break: Some("break-all".into()),
        overflow: Some("hidden".into()),
        text_overflow: Some("ellipsis".into()),
        min_width: Some("10px".into()),
        max_width: Some("90%".into()),
    };
    let expected = gridwell_layout::ResolvedStyle::from_def(&full);
    assert!(expected.padding.iter().all(|(_, p)| p.is_some()));
    assert!(expected.border.iter().all(|(_, b)| b.is_some()));
    let layers: Vec<Box<dyn Fn(TableBuilder) -> TableBuilder>> = vec![
        Box::new(|b| {
            b.columns(vec![column("a", "A").style("full")])
                .body(vec![row(vec![cell("x")])])
        }),
        Box::new(|b| b.body(vec![row(vec![cell("x")]).style("full")])),
        Box::new(|b| b.body(vec![row(vec![cell("x").style("full")])])),
    ];
    for layer in layers {
        let t = layer(TableBuilder::new(1).style_def("full", full.clone())).build();
        assert_eq!(body(&resolve(&t))[0][0].style, expected);
    }
    // And through a composition with empty overrides.
    let t = TableBuilder::new(1)
        .style_def("full", full.clone())
        .composition("comp", "full", def())
        .body(vec![row(vec![cell("x").style("comp")])])
        .build();
    assert_eq!(body(&resolve(&t))[0][0].style, expected);
}

#[test]
fn footnotes_are_linked_to_their_marks() {
    let mut t = TableBuilder::new(1)
        .title("T")
        .body(vec![row(vec![gridwell_testkit::cell_content(vec![
            gridwell_testkit::text("x"),
            gridwell_testkit::footnote_mark("a", "1"),
        ])])])
        .footnote("a", "1", "used in a cell")
        .footnote("b", "2", "used in the title")
        .footnote("c", "3", "never used")
        .build();
    t.header
        .as_mut()
        .unwrap()
        .title
        .as_mut()
        .unwrap()
        .content
        .push(gridwell_testkit::footnote_mark("b", "2"));
    let rt = resolve(&t);
    let f = &rt.footer;
    assert_eq!(f.footnote("a").map(|n| n.mark), Some("1"));
    assert!(f.footnote("missing").is_none());
    let referenced: Vec<(&str, bool)> = f.footnotes.iter().map(|n| (n.id, n.referenced)).collect();
    assert_eq!(referenced, vec![("a", true), ("b", true), ("c", false)]);
}
