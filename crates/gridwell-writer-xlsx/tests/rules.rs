//! The default look's rules (`gridwell_layout::look`), as HTML draws them, as
//! cell borders: the rules on row boundaries, right of the stub and under the
//! spanners, the outer rules above the heading and below the notes, and the
//! table's own borders winning. With them the sheet's gridlines are hidden.

use gridwell_ir::style::StyleDef;
use gridwell_ir::{Table, TextDirection};
use gridwell_testkit::*;
use gridwell_writer_xlsx::{XlsxConfig, XlsxWriter};
use serde_json::json;

fn xml_with(t: &Table, base_styles: bool) -> (String, String) {
    let w = XlsxWriter::with_config(XlsxConfig { base_styles });
    (
        w.render_sheet_xml(t).unwrap(),
        w.render_styles_xml(t).unwrap(),
    )
}

fn xml(t: &Table) -> (String, String) {
    xml_with(t, true)
}

/// The `<border>` (in `borders`) of the cell at `cell_ref`; the default one if
/// the sheet doesn't have the cell.
fn border_of<'a>(sheet: &str, styles: &'a str, cell_ref: &str) -> &'a str {
    let s: usize = sheet
        .find(&format!("<c r=\"{cell_ref}\""))
        .map(|i| {
            let c = &sheet[i..];
            let tag = &c[..c.find('>').unwrap()];
            tag.split(" s=\"")
                .nth(1)
                .map_or(0, |s| s[..s.find('"').unwrap()].parse().unwrap())
        })
        .unwrap_or(0);
    let xfs = &styles[styles.find("<cellXfs").unwrap()..];
    let xf = xfs.split("<xf ").nth(s + 1).unwrap();
    let id: usize = xf
        .split("borderId=\"")
        .nth(1)
        .and_then(|s| s[..s.find('"')?].parse().ok())
        .unwrap_or(0);
    let borders = &styles[styles.find("<borders").unwrap()..];
    let b = borders.split("<border>").nth(id + 1).unwrap();
    &b[..b.find("</border>").unwrap()]
}

type Side = Option<(String, String)>;

/// The border on `side` (`"left"`, `"right"`, `"top"`, `"bottom"`) of the cell
/// at `cell_ref`: its style and color.
fn side(sheet: &str, styles: &str, cell_ref: &str, side: &str) -> Side {
    let b = border_of(sheet, styles, cell_ref);
    let el = &b[b.find(&format!("<{side}"))?..];
    let el = &el[..el.find(&format!("</{side}>")).unwrap_or(el.find("/>")?)];
    let style = el.split("style=\"").nth(1)?.split('"').next()?;
    let color = el.split("rgb=\"FF").nth(1)?.split('"').next()?;
    Some((style.to_string(), color.to_string()))
}

fn edge(style: &str, color: &str) -> Side {
    Some((style.into(), color.into()))
}

/// Section rules (2px), row rules (1px), the grand summary's double rule
/// (6px), light gray; the outer rules (2px), dark gray.
fn section() -> Side {
    edge("medium", "D3D3D3")
}
fn row_rule() -> Side {
    edge("thin", "D3D3D3")
}
fn grand() -> Side {
    edge("double", "D3D3D3")
}
fn outer() -> Side {
    edge("medium", "A8A8A8")
}
fn red() -> Side {
    edge("medium", "FF0000")
}

/// A style giving the sides a 2px red border (`"solid"`), or removing it
/// (`"none"`).
fn sides(style: &str, sides: &[&str]) -> StyleDef {
    let border: serde_json::Map<_, _> = sides
        .iter()
        .map(|s| {
            (
                s.to_string(),
                json!({ "width": "2px", "style": style, "color": "#FF0000" }),
            )
        })
        .collect();
    serde_json::from_value(json!({ "border": border })).unwrap()
}

fn structured() -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("").stub(), cell("B")]).role("column_label"))
        .group(labeled_group(
            "G",
            vec![
                row(vec![cell("x").stub(), cell("1")]),
                row(vec![cell("y").stub(), cell("2")]),
            ],
        ))
        .group(group(vec![]).summary(vec![
            row(vec![cell("All").stub(), cell("3")]).role("grand_summary"),
        ]))
        .build()
}

/// The sheet's view hides the gridlines.
fn hides_gridlines(sheet: &str) -> bool {
    let view = &sheet[sheet.find("<sheetView ").unwrap()..];
    view[..view.find('>').unwrap()].contains(" showGridLines=\"0\"")
}

#[test]
fn the_rules_follow_the_table_structure() {
    let (sheet, styles) = xml(&structured());
    assert!(hides_gridlines(&sheet), "{sheet}");
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    // Head, label (merged across), x, y, the grand summary.
    assert!(sheet.contains("<mergeCell ref=\"A2:B2\"/>"), "{sheet}");
    for c in ['A', 'B'] {
        // The table's top and the head's bottom; the label's own top and bottom.
        assert_eq!(
            (at(1, c, "top"), at(1, c, "bottom")),
            (section(), section())
        );
        assert_eq!(
            (at(2, c, "top"), at(2, c, "bottom")),
            (section(), section())
        );
        // A row rule between data rows, drawn by the row below.
        assert_eq!((at(3, c, "top"), at(3, c, "bottom")), (section(), None));
        assert_eq!((at(4, c, "top"), at(4, c, "bottom")), (row_rule(), None));
        // A double rule above the grand summary, the section's bottom below it.
        assert_eq!((at(5, c, "top"), at(5, c, "bottom")), (grand(), section()));
    }
    // Right of the stub, outside the head and the label.
    let stub: Vec<_> = (1..=5).map(|r| at(r, 'A', "right").is_some()).collect();
    assert_eq!(stub, [false, false, true, true, true]);
    assert_eq!(at(3, 'A', "right"), section());
    assert!((1..=5).all(|r| at(r, 'B', "right").is_none() && at(r, 'A', "left").is_none()));
    // Nothing outside the table.
    assert!(!sheet.contains("<c r=\"C"), "{sheet}");
    assert!(!sheet.contains("<row r=\"6\""), "{sheet}");
}

#[test]
fn a_cell_spanning_rows_draws_the_rules_of_its_outline() {
    let t = TableBuilder::new(2)
        .head(row(vec![cell("A"), cell("B")]).role("column_label"))
        .body(vec![
            row(vec![cell("1"), cell("2")]),
            row(vec![cell("x").rowspan(2), cell("2")]),
            row(vec![placeholder(), cell("3")]),
        ])
        .build();
    let (sheet, styles) = xml(&t);
    assert!(sheet.contains("<mergeCell ref=\"A3:A4\"/>"), "{sheet}");
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    // The span's top is a row rule; its bottom, on its last row, the table's.
    // The first position has the whole outline, the last its own sides of it.
    assert_eq!(
        (at(3, 'A', "top"), at(3, 'A', "bottom")),
        (row_rule(), section())
    );
    assert_eq!((at(4, 'A', "top"), at(4, 'A', "bottom")), (None, section()));
    assert_eq!(
        (at(3, 'B', "top"), at(3, 'B', "bottom")),
        (row_rule(), None)
    );
    assert_eq!(
        (at(4, 'B', "top"), at(4, 'B', "bottom")),
        (row_rule(), section())
    );
}

#[test]
fn without_base_styles_only_the_tables_own_borders_are_drawn() {
    let t = TableBuilder::new(2)
        .title("T")
        .source_note("N")
        .head(
            row(vec![cell("S").colspan(2).scope("colgroup"), placeholder()]).role("spanner_label"),
        )
        .head(row(vec![cell("A"), cell("B")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2")])])
        .build();
    let (sheet, styles) = xml_with(&t, false);
    // Only the default border, and the gridlines shown.
    assert!(styles.contains("<borders count=\"1\">"), "{styles}");
    assert!(!sheet.contains("showGridLines"), "{sheet}");
    let (sheet, styles) = xml_with(&structured(), false);
    assert!(styles.contains("<borders count=\"1\">"), "{styles}");
    assert!(!hides_gridlines(&sheet));
    // With the look: the outer rules and the spanner's, among the others.
    let (sheet, styles) = xml(&t);
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    for c in ['A', 'B'] {
        assert_eq!(at(1, c, "top"), outer(), "{c}");
        assert_eq!(at(2, c, "bottom"), section(), "{c}");
    }
    // The table's own borders are drawn either way.
    let t = TableBuilder::new(1)
        .style_def("b", sides("solid", &["bottom"]))
        .body(vec![row(vec![cell("1").style("b")])])
        .build();
    let (sheet, styles) = xml_with(&t, false);
    assert_eq!(side(&sheet, &styles, "A1", "bottom"), red());
}

#[test]
fn the_tables_own_borders_win() {
    let t = TableBuilder::new(2)
        .stub_cols(1)
        .style_def("b", sides("solid", &["bottom"]))
        .style_def("t", sides("solid", &["top"]))
        .style_def("l", sides("solid", &["left"]))
        .style_def("xt", sides("none", &["top"]))
        .head(row(vec![cell("").stub(), cell("B")]).role("column_label"))
        .body(vec![
            row(vec![cell("x").stub().style("t"), cell("1").style("b")]),
            row(vec![cell("y").stub(), cell("2").style("l")]),
            row(vec![cell("z").stub(), cell("3").style("t")]),
            row(vec![cell("w").stub(), cell("4").style("xt")]),
        ])
        .build();
    let (sheet, styles) = xml(&t);
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    // Over a cell's own top border the head draws no rule of its bottom.
    assert_eq!((at(1, 'A', "bottom"), at(2, 'A', "top")), (None, red()));
    assert_eq!(at(1, 'B', "bottom"), section());
    // Under a cell's own bottom border the row below draws no rule; beside it
    // the rule stays.
    assert_eq!(at(2, 'B', "bottom"), red());
    assert_eq!(at(3, 'B', "top"), None);
    assert_eq!(at(3, 'A', "top"), row_rule());
    // A cell's own left border: the stub left of it draws none of its own.
    assert_eq!(at(3, 'B', "left"), red());
    assert_eq!(at(3, 'A', "right"), None);
    assert_eq!(at(2, 'A', "right"), section());
    // A cell's own top replaces the rule.
    assert_eq!(at(4, 'B', "top"), red());
    // A removed top border removes the row rule, which is the cell's own.
    assert_eq!(at(5, 'B', "top"), None);
    assert_eq!(at(5, 'A', "top"), row_rule());
    assert_eq!(at(4, 'B', "bottom"), None);
    // Under a group label, the same; the label is merged across.
    let t = TableBuilder::new(2)
        .style_def("t", sides("solid", &["top"]))
        .group(labeled_group(
            "G",
            vec![row(vec![cell("1").style("t"), cell("2")])],
        ))
        .build();
    let (sheet, styles) = xml(&t);
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    assert_eq!((at(1, 'A', "bottom"), at(2, 'A', "top")), (None, red()));
    assert_eq!(
        (at(1, 'B', "bottom"), at(2, 'B', "top")),
        (section(), section())
    );
    // A group label's own borders are drawn, on its outline.
    let t = TableBuilder::new(2)
        .style_def("box", sides("solid", &["left", "right", "top", "bottom"]))
        .group(labeled_group("G", vec![row(vec![cell("1"), cell("2")])]).label_style("box"))
        .build();
    let (sheet, styles) = xml(&t);
    let at = |c: char, s| side(&sheet, &styles, &format!("{c}1"), s);
    assert_eq!(
        [
            at('A', "left"),
            at('A', "right"),
            at('A', "top"),
            at('A', "bottom")
        ],
        [red(), red(), red(), red()]
    );
    assert_eq!(
        [
            at('B', "left"),
            at('B', "right"),
            at('B', "top"),
            at('B', "bottom")
        ],
        [None, red(), red(), red()]
    );
}

fn spanners(dir: Option<TextDirection>) -> Table {
    TableBuilder::new(5)
        .stub_cols(1)
        .config(|c| c.dir = dir)
        .head(
            row(vec![
                cell("").stub(),
                cell("Group A").colspan(2).scope("colgroup"),
                placeholder(),
                cell("Group B").colspan(2).scope("colgroup"),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(
            row(vec![
                cell("").stub(),
                cell("A1"),
                cell("A2"),
                cell("B1"),
                cell("B2"),
            ])
            .role("column_label"),
        )
        .body(vec![row(vec![
            cell("x").stub(),
            cell("1"),
            cell("2"),
            cell("3"),
            cell("4"),
        ])])
        .build()
}

#[test]
fn spanner_rules_are_the_spanners_bottom_borders() {
    let (sheet, styles) = xml(&spanners(None));
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    assert!(sheet.contains("<mergeCell ref=\"B1:C1\"/>"), "{sheet}");
    // Under each spanner, unbroken between neighbors (a border can't be inset);
    // none under the stub's empty cell.
    for c in ['B', 'C', 'D', 'E'] {
        assert_eq!(at(1, c, "bottom"), section(), "{c}");
        assert_eq!(at(2, c, "top"), None, "{c}");
        assert_eq!(at(2, c, "bottom"), section(), "{c}");
    }
    assert_eq!(at(1, 'A', "bottom"), None);
    // A spanner rule of its own style.
    let mut t = spanners(None);
    t.config.rules = Some(
        serde_json::from_value(json!({ "spanners": { "style": "dotted", "color": "#FF0000" } }))
            .unwrap(),
    );
    let (sheet, styles) = xml(&t);
    for c in ['B', 'C', 'D', 'E'] {
        let at = |r: usize, s| side(&sheet, &styles, &format!("{c}{r}"), s);
        assert_eq!(at(1, "bottom"), edge("dotted", "FF0000"), "{c}");
        assert_eq!(at(2, "bottom"), section(), "{c}");
    }
    // A spanner's own bottom border replaces it.
    let t = TableBuilder::new(3)
        .style_def("b", sides("solid", &["bottom"]))
        .head(
            row(vec![
                cell("A").colspan(2).scope("colgroup").style("b"),
                placeholder(),
                cell("B").scope("colgroup"),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("A1"), cell("A2"), cell("B1")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3")])])
        .build();
    let (sheet, styles) = xml(&t);
    let at = |c: char| side(&sheet, &styles, &format!("{c}1"), "bottom");
    assert_eq!([at('A'), at('B'), at('C')], [red(), red(), section()]);
}

#[test]
fn right_to_left_the_rules_are_the_sheets_mirrored_sides() {
    let (sheet, styles) = xml(&spanners(Some(TextDirection::Rtl)));
    let view = &sheet[sheet.find("<sheetView ").unwrap()..];
    assert!(view[..view.find('>').unwrap()].contains(" rightToLeft=\"1\""));
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    // The stub is column A, on the right; its rule is at its end, `right` in a
    // right-to-left sheet (drawn on its left).
    assert_eq!((at(3, 'A', "right"), at(3, 'A', "left")), (section(), None));
    assert_eq!(at(1, 'B', "bottom"), section());
    // A cell's own left border is on its end, too.
    let t = TableBuilder::new(2)
        .stub_cols(1)
        .config(|c| c.dir = Some(TextDirection::Rtl))
        .style_def("l", sides("solid", &["left"]))
        .body(vec![row(vec![cell("x").stub(), cell("1").style("l")])])
        .build();
    let (sheet, styles) = xml(&t);
    let at = |c: char, s| side(&sheet, &styles, &format!("{c}1"), s);
    assert_eq!((at('B', "right"), at('B', "left")), (red(), None));
    assert_eq!(at('A', "right"), section());
}

/// The row of the cell whose shared string is `text`.
fn row_of(sheet: &str, sst: &str, text: &str) -> usize {
    let items: Vec<&str> = sst.split("<si>").skip(1).collect();
    let i = items
        .iter()
        .position(|s| s.contains(&format!(">{text}</t>")))
        .unwrap();
    let at = sheet.find(&format!("<v>{i}</v>")).unwrap();
    let row = &sheet[..at];
    let row = &row[row.rfind("<row r=\"").unwrap() + 8..];
    row[..row.find('"').unwrap()].parse().unwrap()
}

#[test]
fn outer_rules_are_drawn_above_a_heading_and_below_the_notes() {
    let base = || {
        TableBuilder::new(2)
            .head(row(vec![cell("A"), cell("B")]).role("column_label"))
            .body(vec![row(vec![cell("1"), cell("2")])])
    };
    let outer_rules = |t: &Table| {
        let (_, styles) = xml(t);
        styles.matches("<color rgb=\"FFA8A8A8\"/>").count()
    };
    assert_eq!(outer_rules(&base().build()), 0);
    let t = base()
        .title("T")
        .subtitle("S")
        .footnote("f", "1", "F")
        .source_note("N1")
        .source_note("N2")
        .build();
    let w = XlsxWriter::new();
    let (sheet, styles) = xml(&t);
    let sst = w.render_shared_strings_xml(&t).unwrap();
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    // Above the title, across the table though the title is one cell.
    let title = row_of(&sheet, &sst, "T");
    let subtitle = row_of(&sheet, &sst, "S");
    for c in ['A', 'B'] {
        assert_eq!(at(title, c, "top"), outer(), "{c}");
        assert_eq!(
            (at(subtitle, c, "top"), at(subtitle, c, "bottom")),
            (None, None)
        );
    }
    // Below the last note only.
    let (f, n1, n2) = (
        row_of(&sheet, &sst, "F"),
        row_of(&sheet, &sst, "N1"),
        row_of(&sheet, &sst, "N2"),
    );
    assert!(f < n1 && n1 < n2);
    for c in ['A', 'B'] {
        assert_eq!(at(n2, c, "bottom"), outer(), "{c}");
        for r in [f, n1] {
            assert_eq!(
                (at(r, c, "top"), at(r, c, "bottom")),
                (None, None),
                "{r}{c}"
            );
        }
    }
    // Footnotes alone: below the last of them.
    let t = base()
        .footnote("f", "1", "F")
        .footnote("g", "2", "G")
        .build();
    let (sheet, styles) = xml(&t);
    let sst = w.render_shared_strings_xml(&t).unwrap();
    let (f, g) = (row_of(&sheet, &sst, "F"), row_of(&sheet, &sst, "G"));
    assert_eq!(side(&sheet, &styles, &format!("A{g}"), "bottom"), outer());
    assert_eq!(side(&sheet, &styles, &format!("A{f}"), "bottom"), None);
}

#[test]
fn the_tables_rules_replace_the_looks() {
    let rules = json!({
        "top": { "style": "none" },
        "rows": { "style": "dashed", "color": "#0000FF" },
        "stub": { "style": "dotted", "width": "3px", "color": "#00FF00" },
        "grand_summary": { "width": "0" },
        "bottom": { "style": "double", "width": "4px" },
        "title": { "width": "4px", "style": "dashed" },
    });
    let mut t = structured();
    t.config.rules = Some(serde_json::from_value(rules).unwrap());
    t.header = TableBuilder::new(1).title("T").build().header;
    let (sheet, styles) = xml(&t);
    // The title, then the head, label, x, y and the grand summary.
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    for c in ['A', 'B'] {
        assert_eq!(at(1, c, "top"), edge("mediumDashed", "A8A8A8"), "{c}");
        // No top rule; the row rule dashed and blue.
        assert_eq!(at(2, c, "top"), None);
        assert_eq!(at(5, c, "top"), edge("dashed", "0000FF"));
        // The grand summary's rule removed, there is none there (as in HTML).
        assert_eq!(at(6, c, "top"), None);
        assert_eq!(at(6, c, "bottom"), edge("double", "D3D3D3"));
    }
    assert_eq!(at(4, 'A', "right"), edge("dotted", "00FF00"));

    // Removed, the title's rule is gone.
    let mut none = t.clone();
    none.config.rules =
        Some(serde_json::from_value(json!({ "title": { "style": "none" } })).unwrap());
    let (sheet, styles) = xml(&none);
    assert_eq!(side(&sheet, &styles, "A1", "top"), None);
    assert_eq!(side(&sheet, &styles, "A2", "top"), section());
}

#[test]
fn summaries_at_the_top_come_first_with_the_summary_rule_below() {
    let t = TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("").stub(), cell("B")]).role("column_label"))
        .group(
            labeled_group("G", vec![row(vec![cell("x").stub(), cell("1")])])
                .summary(vec![
                    row(vec![cell("Sum").stub(), cell("1")]).role("summary")
                ])
                .summary_top(),
        )
        .build();
    let (sheet, styles) = xml(&t);
    let w = XlsxWriter::new();
    let sst = w.render_shared_strings_xml(&t).unwrap();
    assert_eq!(
        (row_of(&sheet, &sst, "Sum"), row_of(&sheet, &sst, "x")),
        (3, 4)
    );
    let at = |r: usize, c: char, s| side(&sheet, &styles, &format!("{c}{r}"), s);
    for c in ['A', 'B'] {
        assert_eq!(
            (at(3, c, "top"), at(3, c, "bottom")),
            (section(), section())
        );
        // Both sides of a boundary draw its rule.
        assert_eq!(
            (at(4, c, "top"), at(4, c, "bottom")),
            (section(), section())
        );
    }
}
