//! The default look's rules (`gridwell_layout::look`), as HTML draws them: the
//! rules on row boundaries and right of the stub as cell borders, the table's
//! own borders winning, and the spanner and outer rules as line shapes.

use gridwell_ir::style::StyleDef;
use gridwell_ir::{Table, TextDirection};
use gridwell_testkit::*;
use gridwell_writer_pptx::{PptxConfig, PptxWriter};
use serde_json::json;

fn slide(t: &Table) -> String {
    PptxWriter::new().render_slide_xml(t).unwrap()
}

/// The `<a:tc>`s of each `<a:tr>`.
fn cells(xml: &str) -> Vec<Vec<&str>> {
    xml.split("<a:tr ")
        .skip(1)
        .map(|r| {
            let mut tcs: Vec<&str> = r.split("</a:tc>").collect();
            tcs.pop();
            tcs
        })
        .collect()
}

/// The cell's border on `side` (`"T"`, `"B"`, `"L"`, `"R"`): its width in EMU,
/// whether it is double, and its color.
fn line(tc: &str, side: &str) -> Option<(u32, bool, String)> {
    let start = tc.find(&format!("<a:ln{side} "))?;
    let el = &tc[start..start + tc[start..].find(&format!("</a:ln{side}>")).unwrap()];
    let width = el.split("w=\"").nth(1)?.split('"').next()?.parse().ok()?;
    let color = el.split("srgbClr val=\"").nth(1)?.split('"').next()?;
    Some((width, el.contains("cmpd=\"dbl\""), color.to_string()))
}

/// Section rules (2px), row rules (1px) and the grand summary's double rule
/// (6px), in EMU, light gray.
fn section() -> Option<(u32, bool, String)> {
    Some((19050, false, "D3D3D3".into()))
}
fn row_rule() -> Option<(u32, bool, String)> {
    Some((9525, false, "D3D3D3".into()))
}
fn grand() -> Option<(u32, bool, String)> {
    Some((57150, true, "D3D3D3".into()))
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

fn red() -> Option<(u32, bool, String)> {
    Some((19050, false, "FF0000".into()))
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

#[test]
fn the_rules_follow_the_table_structure() {
    let xml = slide(&structured());
    let rows = cells(&xml);
    // Head, label, x, y, the grand summary.
    assert_eq!(rows.len(), 5);
    let at = |r: usize, c: usize, side| line(rows[r][c], side);
    for c in 0..2 {
        // The table's top and the head's bottom; the label's own top and bottom.
        assert_eq!((at(0, c, "T"), at(0, c, "B")), (section(), section()));
        assert_eq!((at(1, c, "T"), at(1, c, "B")), (section(), section()));
        // A row rule between data rows, drawn by the row below.
        assert_eq!((at(2, c, "T"), at(2, c, "B")), (section(), None));
        assert_eq!((at(3, c, "T"), at(3, c, "B")), (row_rule(), None));
        // A double rule above the grand summary, the section's bottom below it.
        assert_eq!((at(4, c, "T"), at(4, c, "B")), (grand(), section()));
    }
    // Right of the stub, outside the head and the label.
    let stub: Vec<_> = (0..5).map(|r| at(r, 0, "R").is_some()).collect();
    assert_eq!(stub, [false, false, true, true, true]);
    assert!((0..5).all(|r| at(r, 1, "R").is_none() && at(r, 0, "L").is_none()));
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
    let xml = slide(&t);
    let rows = cells(&xml);
    let at = |r: usize, c: usize, side| line(rows[r][c], side);
    // The span's top is a row rule; its bottom, on its last row, the table's.
    assert_eq!((at(2, 0, "T"), at(2, 0, "B")), (row_rule(), None));
    assert_eq!((at(3, 0, "T"), at(3, 0, "B")), (None, section()));
    assert_eq!((at(3, 1, "T"), at(3, 1, "B")), (row_rule(), section()));
}

#[test]
fn without_base_styles_only_the_tables_own_borders_are_drawn() {
    let bare = |t: &Table| {
        PptxWriter::with_config(PptxConfig { base_styles: false })
            .render_slide_xml(t)
            .unwrap()
    };
    let t = TableBuilder::new(2)
        .title("T")
        .source_note("N")
        .head(
            row(vec![cell("S").colspan(2).scope("colgroup"), placeholder()]).role("spanner_label"),
        )
        .head(row(vec![cell("A"), cell("B")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2")])])
        .build();
    let xml = bare(&t);
    assert!(
        !xml.contains("<a:ln") && !xml.contains("<p:cxnSp>"),
        "{xml}"
    );
    // With the look: the outer rules and the spanner's.
    assert_eq!(slide(&t).matches("<p:cxnSp>").count(), 3);
    let xml = bare(&structured());
    assert!(!xml.contains("<a:ln"), "{xml}");
    // The table's own borders are drawn either way.
    let t = TableBuilder::new(1)
        .style_def("b", sides("solid", &["bottom"]))
        .body(vec![row(vec![cell("1").style("b")])])
        .build();
    assert_eq!(line(cells(&bare(&t))[0][0], "B"), red());
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
    let xml = slide(&t);
    let rows = cells(&xml);
    let at = |r: usize, c: usize, side| line(rows[r][c], side);
    // Over a cell's own top border the head draws no rule of its bottom.
    assert_eq!((at(0, 0, "B"), at(1, 0, "T")), (None, red()));
    assert_eq!(at(0, 1, "B"), section());
    // Under a cell's own bottom border the row below draws no rule; beside it
    // the rule stays.
    assert_eq!(at(1, 1, "B"), red());
    assert_eq!(at(2, 1, "T"), None);
    assert_eq!(at(2, 0, "T"), row_rule());
    // A cell's own left border: the stub left of it draws none of its own.
    assert_eq!(at(2, 1, "L"), red());
    assert_eq!(at(2, 0, "R"), None);
    assert_eq!(at(1, 0, "R"), section());
    // A cell's own top replaces the rule.
    assert_eq!(at(3, 1, "T"), red());
    // A removed top border removes the row rule, which is the cell's own.
    assert_eq!(at(4, 1, "T"), None);
    assert_eq!(at(4, 0, "T"), row_rule());
    assert_eq!(at(3, 1, "B"), None);
    // Under a group label, the same.
    let t = TableBuilder::new(2)
        .style_def("t", sides("solid", &["top"]))
        .group(labeled_group(
            "G",
            vec![row(vec![cell("1").style("t"), cell("2")])],
        ))
        .build();
    let xml = slide(&t);
    let rows = cells(&xml);
    let at = |r: usize, c: usize, side| line(rows[r][c], side);
    assert_eq!((at(0, 0, "B"), at(1, 0, "T")), (None, red()));
    assert_eq!((at(0, 1, "B"), at(1, 1, "T")), (section(), section()));
}

/// Each line shape's (x, y, length, line width): `y` its center.
fn lines(xml: &str) -> Vec<(u32, u32, u32, u32)> {
    let num = |s: &str, key: &str| -> u32 {
        s.split(key)
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    xml.split("<p:cxnSp>")
        .skip(1)
        .map(|s| {
            (
                num(s, "<a:off x=\""),
                num(s, " y=\""),
                num(s, "<a:ext cx=\""),
                num(s, "<a:ln w=\""),
            )
        })
        .collect()
}

/// The table's (x, y) and its column widths and row heights.
fn grid(xml: &str) -> ((u32, u32), Vec<u32>, Vec<u32>) {
    let frame = xml.split("<p:graphicFrame>").nth(1).unwrap();
    let num = |key: &str| -> u32 {
        frame
            .split(key)
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let all = |key: &str| -> Vec<u32> {
        xml.split(key)
            .skip(1)
            .map(|s| s.split('"').next().unwrap().parse().unwrap())
            .collect()
    };
    (
        (num("<a:off x=\""), num(" y=\"")),
        all("<a:gridCol w=\""),
        all("<a:tr h=\""),
    )
}

fn spanners(dir: Option<TextDirection>) -> Table {
    TableBuilder::new(4)
        .config(|c| c.dir = dir)
        // Unequal, so a mirror image is not the same widths.
        .columns(vec![
            column("a1", "A1").width("40pt"),
            column("a2", "A2").width("80pt"),
            column("b1", "B1").width("60pt"),
            column("b2", "B2").width("100pt"),
        ])
        .head(
            row(vec![
                cell("Group A").colspan(2).scope("colgroup"),
                placeholder(),
                cell("Group B").colspan(2).scope("colgroup"),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("A1"), cell("A2"), cell("B1"), cell("B2")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3"), cell("4")])])
        .build()
}

#[test]
fn spanner_rules_are_inset_but_at_the_tables_edges() {
    let xml = slide(&spanners(None));
    let ((x, y), cols, rows) = grid(&xml);
    let inset = 38100; // 4px
    let mid = x + cols[0] + cols[1];
    let end = x + cols.iter().sum::<u32>();
    let l = lines(&xml);
    assert_eq!(l.len(), 2, "{l:?}");
    // 2px lines, ending at the bottom of the spanner row.
    for &(_, ly, _, w) in &l {
        assert_eq!((w, ly + w / 2), (19050, y + rows[0]));
    }
    assert_eq!((l[0].0, l[0].0 + l[0].2), (x, mid - inset));
    assert_eq!((l[1].0, l[1].0 + l[1].2), (mid + inset, end));
}

#[test]
fn right_to_left_the_spanner_rules_are_mirrored() {
    let xml = slide(&spanners(Some(TextDirection::Rtl)));
    let ((x, _), cols, _) = grid(&xml);
    let inset = 38100;
    let end = x + cols.iter().sum::<u32>();
    // The columns are written as they are seen: the first spanner's on the right.
    assert_eq!(cols, [1_270_000, 762_000, 1_016_000, 508_000]);
    let mid = x + cols[0] + cols[1];
    let l = lines(&xml);
    assert_eq!((l[0].0, l[0].0 + l[0].2), (mid + inset, end));
    assert_eq!((l[1].0, l[1].0 + l[1].2), (x, mid - inset));
}

#[test]
fn outer_rules_are_drawn_above_a_heading_and_below_notes() {
    let base = || {
        TableBuilder::new(1)
            .head(row(vec![cell("A")]).role("column_label"))
            .body(vec![row(vec![cell("1")])])
    };
    let outer = |t: Table| lines(&slide(&t));
    assert!(outer(base().build()).is_empty());
    assert_eq!(outer(base().title("T").build()).len(), 1);
    assert_eq!(outer(base().source_note("N").build()).len(), 1);
    let xml = slide(&base().title("T").source_note("N").build());
    let ((_, y), _, rows) = grid(&xml);
    let l = lines(&xml);
    assert_eq!(l.len(), 2);
    assert_eq!(xml.matches("srgbClr val=\"A8A8A8\"").count(), 2);
    // Above the title, which is above the table; below the notes.
    assert!(l[0].1 < y);
    assert!(l[1].1 > y + rows.iter().sum::<u32>());
}
