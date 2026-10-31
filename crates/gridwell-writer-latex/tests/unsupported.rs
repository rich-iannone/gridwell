//! What the writer reports it can't express (`unsupported`): named fonts,
//! cells aligned to the middle or bottom, and dashed or dotted vertical
//! borders, each where the table uses it, and nothing it does express.

use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{BorderSet, StyleDef};
use gridwell_ir::{Cell, Table};
use gridwell_layout::unsupported::Place;
use gridwell_layout::SectionKind;
use gridwell_testkit::{
    border, cell, cell_content, column, labeled_group, line_break, placeholder, raw, row, styled,
    text, CellExt, ColumnExt, RowExt, TableBuilder,
};
use gridwell_writer_latex::unsupported;

fn def(f: impl FnOnce(&mut StyleDef)) -> StyleDef {
    let mut d = StyleDef::default();
    f(&mut d);
    d
}

fn family(f: &str) -> StyleDef {
    def(|d| d.font_family = Some(f.into()))
}

fn sides(left: Option<&str>, right: Option<&str>, bottom: Option<&str>) -> StyleDef {
    let b = |s: Option<&str>| s.map(|s| border("2px", s, "#000000"));
    def(|d| {
        d.border = Some(BorderSet {
            top: None,
            right: b(right),
            bottom: b(bottom),
            left: b(left),
        })
    })
}

fn body(r: usize, c: usize) -> Place {
    Place::Cell {
        section: SectionKind::Body,
        group: Some(0),
        row: r,
        col: c,
    }
}

/// A one-row table whose cells take the styles `styles` (`None`: unstyled).
fn cells(styles: &[(&str, Option<StyleDef>)]) -> Table {
    let mut b = TableBuilder::new(styles.len() as u32);
    let mut cs = Vec::new();
    for (i, (id, d)) in styles.iter().enumerate() {
        let c = cell(&format!("C{i}"));
        cs.push(match d {
            Some(d) => {
                b = b.style_def(id, d.clone());
                c.style(id)
            }
            None => c,
        });
    }
    b.body(vec![row(cs)]).build()
}

/// The places `table` uses `feature`, as the writer reports them.
fn places(table: &Table, feature: &str) -> Vec<Place> {
    unsupported(table)
        .into_iter()
        .find(|u| u.feature == feature)
        .map(|u| u.places)
        .unwrap_or_default()
}

#[test]
fn a_named_font_is_reported_and_a_monospace_cell_is_not() {
    let t = cells(&[
        ("georgia", Some(family("Georgia, serif"))),
        ("mono", Some(family("monospace"))),
        ("courier", Some(family("\"Courier New\", monospace"))),
        // No font at all: no format draws it.
        ("system", Some(family("system-ui, -apple-system"))),
        ("serif", Some(family("serif"))),
        ("plain", None),
    ]);
    assert_eq!(places(&t, "font-family"), [body(0, 0), body(0, 4)]);
}

#[test]
fn a_font_outside_a_cell_or_in_a_run_is_reported() {
    // Only a cell or group label is ever `\texttt`: a monospace title, note or
    // run is reported too.
    let mut t = TableBuilder::new(2)
        .style_def("mono", family("monospace"))
        .style_def("georgia", family("Georgia"))
        .title("T")
        .subtitle("S")
        .group(
            labeled_group(
                "L",
                vec![row(vec![
                    cell_content(vec![text("a "), styled("b", "georgia")]),
                    cell_content(vec![styled("c", "mono")]),
                ])],
            )
            .label_style("mono"),
        )
        .footnote("f", "1", "F")
        .source_note("N")
        .build();
    let h = t.header.as_mut().unwrap();
    h.title.as_mut().unwrap().style_id = Some("mono".into());
    h.subtitle.as_mut().unwrap().style_id = Some("georgia".into());
    let f = t.footer.as_mut().unwrap();
    f.footnotes[0].style_id = Some("mono".into());
    f.source_notes[0].style_id = Some("georgia".into());
    assert_eq!(
        places(&t, "font-family"),
        [
            Place::Title,
            Place::Subtitle,
            body(0, 0),
            body(0, 1),
            Place::Footnote(0),
            Place::SourceNote(0),
        ]
    );

    // A group label in a named font is reported.
    let t = TableBuilder::new(1)
        .style_def("georgia", family("Georgia"))
        .group(labeled_group("L", vec![row(vec![cell("x")])]).label_style("georgia"))
        .build();
    assert_eq!(places(&t, "font-family"), [Place::GroupLabel(0)]);
}

#[test]
fn middle_and_bottom_cells_are_reported() {
    let valign = |v: &str| Some(def(|d| d.vertical_align = Some(v.into())));
    let mut t = cells(&[
        ("top", valign("top")),
        ("middle", valign("middle")),
        ("bottom", valign("bottom")),
        ("baseline", valign("baseline")),
        ("plain", None),
        ("tall", None),
    ]);
    t.table.tbody[0].rows[0].cells[5].content = vec![text("a"), line_break(), text("b")];
    assert_eq!(places(&t, "vertical-align"), [body(0, 1), body(0, 2)]);

    // A group label is alone in its row.
    let t = TableBuilder::new(1)
        .style_def("bottom", def(|d| d.vertical_align = Some("bottom".into())))
        .group(labeled_group("L", vec![row(vec![cell("x")])]).label_style("bottom"))
        .build();
    assert_eq!(places(&t, "vertical-align"), []);
}

/// A bottom-aligned cell `B`, beside a cell `content` in a style of its own
/// (`neighbor`) in a column `width` wide (`None`: unsized).
fn beside(neighbor: StyleDef, content: Vec<ContentNode>, width: Option<&str>) -> Table {
    let mut b = column("b", "B");
    if let Some(w) = width {
        b = b.width(w);
    }
    TableBuilder::new(2)
        .columns(vec![column("a", "A"), b])
        .style_def("bottom", def(|d| d.vertical_align = Some("bottom".into())))
        .style_def("neighbor", neighbor)
        .style_def("big", def(|d| d.font_size = Some("20px".into())))
        .body(vec![row(vec![
            cell("B").style("bottom"),
            cell_content(content).style("neighbor"),
        ])])
        .build()
}

#[test]
fn a_row_of_single_lines_in_one_size_is_not_reported() {
    // Every cell at the row's top shows as aligned anywhere: so a middle or
    // bottom cell beside cells no taller is as the IR has it.
    let x = || vec![text("x")];
    let none = StyleDef::default;
    let t = beside(none(), x(), None);
    assert_eq!(places(&t, "vertical-align"), []);
    // Raw content for another format, and a run with no size, are one line.
    let t = beside(none(), vec![raw("html", "<br><br>")], None);
    assert_eq!(places(&t, "vertical-align"), []);
    let t = beside(none(), vec![styled("x", "neighbor")], None);
    assert_eq!(places(&t, "vertical-align"), []);
    // A neighbor in the same size, or a smaller one.
    let sized = |s: &str| def(|d| d.font_size = Some(s.into()));
    let t = beside(sized("10pt"), x(), None);
    assert_eq!(places(&t, "vertical-align"), []);
    let t = beside(sized("8pt"), x(), None);
    assert_eq!(places(&t, "vertical-align"), []);
    // A cell taller itself fills its row.
    let mut t = beside(none(), x(), None);
    t.table.tbody[0].rows[0].cells[0].content = vec![text("a"), line_break(), text("b")];
    assert_eq!(places(&t, "vertical-align"), []);
    // Every cell bottom-aligned, in one size, is reported for none of them.
    let t = beside(def(|d| d.vertical_align = Some("bottom".into())), x(), None);
    assert_eq!(places(&t, "vertical-align"), []);
}

#[test]
fn a_cell_beside_a_taller_one_is_reported() {
    let x = || vec![text("x")];
    let none = StyleDef::default;
    let cases: Vec<(&str, Table)> = vec![
        (
            "a line break",
            beside(none(), vec![text("a"), line_break(), text("b")], None),
        ),
        ("raw LaTeX", beside(none(), vec![raw("latex", "x")], None)),
        ("a sized column", beside(none(), x(), Some("2in"))),
        (
            "a bigger font",
            beside(def(|d| d.font_size = Some("14pt".into())), x(), None),
        ),
        (
            "a run in a size of its own",
            beside(none(), vec![text("a "), styled("b", "big")], None),
        ),
    ];
    for (why, t) in cases {
        assert_eq!(places(&t, "vertical-align"), [body(0, 0)], "{why}");
    }

    // A cell spanning rows is a `\multirow`, centered on them: bottom is
    // reported even beside cells a line high, and middle isn't, even beside a
    // taller one.
    let spanning = |v: &str, beside: Cell| {
        TableBuilder::new(2)
            .style_def("v", def(|d| d.vertical_align = Some(v.into())))
            .body(vec![
                row(vec![cell("S").style("v").rowspan(2), beside]),
                row(vec![placeholder(), cell("y")]),
            ])
            .build()
    };
    let two_lines = || cell_content(vec![text("a"), line_break(), text("b")]);
    assert_eq!(
        places(&spanning("middle", two_lines()), "vertical-align"),
        []
    );
    assert_eq!(places(&spanning("top", two_lines()), "vertical-align"), []);
    let t = spanning("bottom", cell("x"));
    assert_eq!(places(&t, "vertical-align"), [body(0, 0)]);
}

#[test]
fn dashed_and_dotted_vertical_borders_are_reported() {
    let t = cells(&[
        ("ldash", Some(sides(Some("dashed"), None, None))),
        ("rdot", Some(sides(None, Some("dotted"), None))),
        // Horizontal edges are drawn in their style, and vertical rules
        // can be solid or double.
        ("bdash", Some(sides(None, None, Some("dashed")))),
        ("solid", Some(sides(Some("solid"), Some("double"), None))),
        ("plain", None),
    ]);
    assert_eq!(places(&t, "border-styles"), [body(0, 0), body(0, 1)]);

    let t = TableBuilder::new(1)
        .style_def("dash", sides(Some("dashed"), None, None))
        .group(labeled_group("L", vec![row(vec![cell("x")])]).label_style("dash"))
        .build();
    assert_eq!(places(&t, "border-styles"), [Place::GroupLabel(0)]);

    // A title or note has no borders in any form, so none is reported.
    let mut t = TableBuilder::new(1)
        .style_def("dash", sides(Some("dashed"), Some("dotted"), None))
        .title("T")
        .body(vec![row(vec![cell("x")])])
        .footnote("f", "1", "F")
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("dash".into());
    t.footer.as_mut().unwrap().footnotes[0].style_id = Some("dash".into());
    assert_eq!(places(&t, "border-styles"), []);
}

#[test]
fn places_are_where_the_ir_has_them() {
    // The head, a later group, a summary row; a cell past a hidden column is
    // in its IR column.
    let bottom = def(|d| d.vertical_align = Some("bottom".into()));
    let two_lines = || cell_content(vec![text("x"), line_break(), text("x")]);
    let t = TableBuilder::new(3)
        .columns(vec![
            column("a", "A"),
            column("b", "B").hidden(),
            column("c", "C"),
        ])
        .style_def("b", bottom)
        .style_def("big", def(|d| d.font_size = Some("20px".into())))
        .head(row(vec![
            cell("h").style("big"),
            cell("h"),
            cell("h").style("b"),
        ]))
        .group(labeled_group(
            "G1",
            vec![row(vec![cell("x").style("big"), cell("x"), cell("x")])],
        ))
        .group(
            labeled_group(
                "G2",
                vec![
                    row(vec![cell("x"), cell("x"), cell("x")]),
                    // Each cell is bottom-aligned, and each drawn one two
                    // lines high.
                    row(vec![two_lines(), cell("x"), two_lines()]).style("b"),
                ],
            )
            .summary(vec![row(vec![
                cell("s").style("b"),
                cell("s"),
                two_lines(),
            ])]),
        )
        .build();
    let found = places(&t, "vertical-align");
    let shown: Vec<String> = found.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        [
            "head row 1, column 3",
            "group 2 row 2, column 1",
            "group 2 row 2, column 3",
            "group 2 summary row 1, column 1",
        ]
    );

    // A styled cell in a hidden column isn't drawn, so it isn't reported.
    let t = TableBuilder::new(2)
        .columns(vec![column("a", "A"), column("b", "B").hidden()])
        .style_def("b", def(|d| d.vertical_align = Some("bottom".into())))
        .body(vec![row(vec![cell("x"), cell("y").style("b")])])
        .build();
    assert!(unsupported(&t).is_empty());
    // Nor does it make a row taller.
    let t = TableBuilder::new(2)
        .columns(vec![column("a", "A"), column("b", "B").hidden()])
        .style_def("b", def(|d| d.vertical_align = Some("bottom".into())))
        .body(vec![row(vec![cell("x").style("b"), two_lines()])])
        .build();
    assert!(unsupported(&t).is_empty());
}

#[test]
fn the_corpus_has_one_gap() {
    // Every corpus table but one is expressed whole: `borders-dashed` has
    // dashed and dotted sides.
    for ex in gridwell_testkit::examples() {
        let found: Vec<_> = unsupported(&ex.table())
            .into_iter()
            .map(|u| u.feature)
            .collect();
        let want: &[&str] = match ex.name {
            "borders-dashed" => &["border-styles"],
            _ => &[],
        };
        assert_eq!(found, want, "{}", ex.name);
    }
    // The fonts example: a monospace title, a cell in Georgia, a monospace run.
    let t = gridwell_testkit::examples::font_families();
    assert_eq!(
        places(&t, "font-family"),
        [Place::Title, body(0, 0), body(0, 1)]
    );
}
