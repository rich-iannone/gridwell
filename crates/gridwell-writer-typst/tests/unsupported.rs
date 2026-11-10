//! What the writer reports it can't express (`unsupported`): double borders,
//! on cells and group labels, and nothing it does express.

use gridwell_ir::style::{BorderSet, StyleDef};
use gridwell_layout::unsupported::Place;
use gridwell_layout::SectionKind;
use gridwell_testkit::{border, cell, labeled_group, row, CellExt, TableBuilder};
use gridwell_writer_typst::unsupported;

fn sides(top: Option<&str>, left: Option<&str>, width: &str) -> StyleDef {
    let b = |s: Option<&str>| s.map(|s| border(width, s, "#000000"));
    StyleDef {
        border: Some(BorderSet {
            top: b(top),
            right: None,
            bottom: None,
            left: b(left),
        }),
        ..Default::default()
    }
}

fn body(c: usize) -> Place {
    Place::Cell {
        section: SectionKind::Body,
        group: Some(0),
        row: 0,
        col: c,
    }
}

#[test]
fn double_borders_are_reported() {
    let t = TableBuilder::new(5)
        .style_def("top", sides(Some("double"), None, "3px"))
        .style_def("left", sides(None, Some("double"), "3px"))
        .style_def("dashed", sides(Some("dashed"), Some("dotted"), "3px"))
        // A zero-width double draws nothing.
        .style_def("zero", sides(Some("double"), None, "0px"))
        .group(
            labeled_group(
                "L",
                vec![row(vec![
                    cell("a").style("top"),
                    cell("b").style("left"),
                    cell("c").style("dashed"),
                    cell("d").style("zero"),
                    cell("e"),
                ])],
            )
            .label_style("left"),
        )
        .build();
    let found = unsupported(&t);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].feature, "border-double");
    assert_eq!(found[0].places, [Place::GroupLabel(0), body(0), body(1)]);

    // A title or note has no borders in any form, so none is reported.
    let mut t = TableBuilder::new(1)
        .style_def("top", sides(Some("double"), None, "3px"))
        .title("T")
        .body(vec![row(vec![cell("x")])])
        .source_note("N")
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("top".into());
    t.footer.as_mut().unwrap().source_notes[0].style_id = Some("top".into());
    assert_eq!(unsupported(&t), []);
}

#[test]
fn the_corpus_has_two_gaps() {
    // Every corpus table but two is expressed whole: in `borders-double` every
    // cell is outlined double, and `custom-rules` has rules of its own (Typst
    // draws its own).
    for ex in gridwell_testkit::examples() {
        let found = unsupported(&ex.table());
        match ex.name {
            "borders-double" => {
                assert_eq!(found.len(), 1);
                assert_eq!(found[0].feature, "border-double");
                assert_eq!(found[0].places.len(), 9);
            }
            "custom-rules" => {
                assert_eq!(found.len(), 1);
                assert_eq!(found[0].feature, "table-rules");
                assert_eq!(found[0].places, [Place::Table]);
            }
            _ => assert_eq!(found, [], "{}", ex.name),
        }
    }
}
