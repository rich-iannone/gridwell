//! Cell formats in `styles.xml`: borders map to SpreadsheetML line styles by width
//! and kind, carry their colour, and are shared between cells that use the same
//! edges.

use gridwell_ir::style::BorderSet;
use gridwell_ir::StyleDef;
use gridwell_testkit::{border, cell, row, CellExt, TableBuilder};
use gridwell_writer_xlsx::XlsxWriter;

fn styles_for(bottom: Option<(&str, &str, &str)>, left: Option<(&str, &str, &str)>) -> String {
    let edge = |e: Option<(&str, &str, &str)>| e.map(|(w, s, c)| border(w, s, c));
    let t = TableBuilder::new(2)
        .style_def(
            "b",
            StyleDef {
                border: Some(BorderSet {
                    top: None,
                    right: None,
                    bottom: edge(bottom),
                    left: edge(left),
                }),
                ..Default::default()
            },
        )
        .body(vec![row(vec![cell("x").style("b"), cell("y").style("b")])])
        .build();
    XlsxWriter::new().render_styles_xml(&t).unwrap()
}

#[test]
fn border_widths_and_kinds_map_to_line_styles() {
    for (width, kind, want) in [
        ("1px", "solid", "thin"),
        ("0.75pt", "solid", "thin"),
        ("2px", "solid", "medium"),
        ("3px", "solid", "thick"),
        ("1mm", "solid", "thick"),
        ("1px", "dashed", "dashed"),
        ("2px", "dashed", "mediumDashed"),
        ("1px", "dotted", "dotted"),
        ("3px", "double", "double"),
    ] {
        let xml = styles_for(Some((width, kind, "#336699")), None);
        assert!(
            xml.contains(&format!(
                "<bottom style=\"{want}\"><color rgb=\"FF336699\"/></bottom>"
            )),
            "{width} {kind} → {want}:\n{xml}"
        );
        assert!(xml.contains("<borders count=\"2\">"), "{xml}");
        assert!(
            xml.contains("borderId=\"1\" xfId=\"0\" applyBorder=\"1\""),
            "{xml}"
        );
    }
}

#[test]
fn edges_keep_their_side_and_none_draws_nothing() {
    let xml = styles_for(
        Some(("1px", "solid", "red")),
        Some(("2px", "dashed", "blue")),
    );
    assert!(
        xml.contains("<border><left style=\"mediumDashed\"><color rgb=\"FF0000FF\"/></left><right/><top/><bottom style=\"thin\"><color rgb=\"FFFF0000\"/></bottom><diagonal/></border>"),
        "{xml}"
    );
    let none = styles_for(Some(("1px", "none", "red")), None);
    assert!(none.contains("<borders count=\"1\">"), "{none}");
}

#[test]
fn cells_with_the_same_edges_share_a_border() {
    // Two cells, one style: one border entry besides the empty one.
    let xml = styles_for(Some(("1px", "solid", "red")), None);
    assert_eq!(xml.matches("<border>").count(), 2, "{xml}");
}
