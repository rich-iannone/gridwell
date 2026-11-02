use gridwell_ir::style::StyleDef;
use gridwell_ir::{HAlign, Table};
use gridwell_testkit::{
    cell, column, footnote_mark, group, line_break, placeholder, row, text, CellExt, ColumnExt,
    RowExt, TableBuilder,
};

use crate::{resolve, ResolvedTable};

/// One column aligned on `mark` (none: the default), body cells `body`.
fn table(mark: Option<&str>, body: &[&str]) -> Table {
    let mut amount = column("v", "Amount").align("char");
    if let Some(m) = mark {
        amount = amount.align_char(m);
    }
    TableBuilder::new(1)
        .columns(vec![amount])
        .head(row(vec![cell("Amount")]).role("column_label"))
        .body(body.iter().map(|v| row(vec![cell(v)])).collect())
        .build()
}

/// Each body cell's padding as text, `|` where there is none.
fn pads(rt: &ResolvedTable) -> Vec<String> {
    rt.groups
        .iter()
        .flat_map(|g| g.rows.rows.iter().chain(&g.summary_rows.rows))
        .flat_map(|r| r.cells())
        .map(|c| c.align_pad.map_or("|".into(), |p| p.text()))
        .collect()
}

#[test]
fn cells_pad_to_the_longest_fraction() {
    let t = table(None, &["1234.5", "6.789", "42.0", "7", "n/a", ""]);
    let rt = resolve(&t);
    assert_eq!(pads(&rt), ["00", "|", "00", ".000", ".000", "|"]);
    // Right-aligned, the column and its label too.
    assert_eq!(rt.columns[0].align, HAlign::Right);
    assert_eq!(rt.columns[0].align_char, Some("."));
    let label = rt.head.rows[0].cells().next().unwrap();
    assert_eq!(
        (label.align.clone(), label.align_char),
        (HAlign::Right, None)
    );
    assert_eq!(label.align_pad, None);
    for c in rt.groups[0].rows.rows.iter().flat_map(|r| r.cells()) {
        assert_eq!((c.align.clone(), c.align_char), (HAlign::Right, Some(".")));
    }
}

#[test]
fn padding_as_text_and_as_spaces() {
    let t = table(Some(","), &["1,5", "22", "3,125"]);
    let rt = resolve(&t);
    assert_eq!(pads(&rt), ["00", ",000", "|"]);
    let p = rt.groups[0].rows.rows[1].cells().next().unwrap().align_pad;
    assert_eq!(p.unwrap().spaces(), "\u{2008}\u{2007}\u{2007}\u{2007}");
    // A character of several: a punctuation space each.
    let t = table(Some("±"), &["4 ± 1", "5"]);
    let p = resolve(&t).groups[0].rows.rows[1]
        .cells()
        .next()
        .unwrap()
        .align_pad;
    assert_eq!(p.unwrap().text(), "±00");
    assert_eq!(p.unwrap().spaces(), "\u{2008}\u{2007}\u{2007}");
}

#[test]
fn nothing_is_padded_without_the_character() {
    let rt_of = |body: &[&str]| pads(&resolve(&table(None, body)));
    assert_eq!(rt_of(&["12", "3", "n/a"]), ["|", "|", "|"]);
    // Already lined up.
    assert_eq!(rt_of(&["1.25", "10.50"]), ["|", "|"]);
    // An empty align_char is the default.
    assert_eq!(pads(&resolve(&table(Some(""), &["1.5", "2"]))), ["|", ".0"]);
}

#[test]
fn the_last_line_and_trailing_space_are_measured() {
    let mut t = table(None, &["1.5", "x"]);
    // `1.25` on the cell's last line; the others are ignored.
    t.table.tbody[0].rows[1].cells[0].content = vec![
        text("10.12345"),
        line_break(),
        text("7.1234"),
        line_break(),
        text("1.25  "),
    ];
    assert_eq!(pads(&resolve(&t)), ["0", "|"]);
    // The first of several characters on the line.
    let t = table(None, &["1.5 (2.25)", "3.5"]);
    assert_eq!(pads(&resolve(&t)), ["|", "0000000"]);
    // A footnote mark counts as what follows the character.
    let mut t = table(None, &["1.5", "x"]);
    t.table.tbody[0].rows[1].cells[0].content = vec![text("2.5"), footnote_mark("n", "1")];
    assert_eq!(pads(&resolve(&t)), ["0", "|"]);
}

#[test]
fn only_cells_aligned_on_the_character_take_part() {
    let mut t = table(None, &["1.5", "2.25", "3.125"]);
    // A style aligning a cell left takes it out; it isn't measured either.
    t.styles.defs.insert(
        "l".into(),
        StyleDef {
            text_align: Some("left".into()),
            ..StyleDef::default()
        },
    );
    t.table.tbody[0].rows[2].cells[0].style_id = Some("l".into());
    let rt = resolve(&t);
    assert_eq!(pads(&rt), ["0", "|", "|"]);
    let left = rt.groups[0].rows.rows[2].cells().next().unwrap();
    assert_eq!((left.align.clone(), left.align_char), (HAlign::Left, None));

    // A style's `text_align: char` in a column that isn't: the column's
    // character, else a period; only those cells are measured.
    let mut t = TableBuilder::new(1)
        .columns(vec![column("v", "V")])
        .style_def(
            "c",
            StyleDef {
                text_align: Some("char".into()),
                ..StyleDef::default()
            },
        )
        .body(vec![
            row(vec![cell("1.5").style("c")]),
            row(vec![cell("2.125")]),
            row(vec![cell("3").style("c")]),
        ])
        .build();
    assert_eq!(pads(&resolve(&t)), ["|", "|", ".0"]);
    t.column_spec[0].align_char = Some(",".into());
    t.table.tbody[0].rows[0].cells[0].content = vec![text("1,5")];
    assert_eq!(pads(&resolve(&t)), ["|", "|", ",0"]);
}

#[test]
fn summary_rows_line_up_with_the_body_and_spanning_cells_dont() {
    let t = TableBuilder::new(2)
        .columns(vec![column("a", "A"), column("v", "V").align("char")])
        .head(row(vec![cell("A"), cell("V")]))
        .stub_cols(1)
        .group(
            group(vec![
                row(vec![cell("x"), cell("1.5")]),
                row(vec![cell("wide 9.99").colspan(2), placeholder()]),
            ])
            .summary(vec![row(vec![cell("sum"), cell("10.125")])]),
        )
        .build();
    let rt = resolve(&t);
    let cells: Vec<_> = rt
        .groups
        .iter()
        .flat_map(|g| g.rows.rows.iter().chain(&g.summary_rows.rows))
        .flat_map(|r| r.cells())
        .map(|c| (c.align_char, c.align_pad.map(|p| p.text())))
        .collect();
    assert_eq!(
        cells,
        [
            (None, None),
            (Some("."), Some("00".into())),
            (None, None),
            (None, None),
            (Some("."), None),
        ]
    );
    let spanning = rt.groups[0].rows.rows[1].cells().next().unwrap();
    assert_eq!(spanning.align, HAlign::Left);
}

#[test]
fn hidden_columns_and_other_columns_are_apart() {
    let t = TableBuilder::new(3)
        .columns(vec![
            column("a", "A").align("char"),
            column("h", "H").align("char").hidden(),
            column("b", "B").align("char"),
        ])
        .body(vec![
            row(vec![cell("1.5"), cell("1.55555"), cell("1.25")]),
            row(vec![cell("2.125"), cell("2"), cell("2")]),
        ])
        .build();
    // Row by row: the hidden column's long fraction pads nothing.
    let rt = resolve(&t);
    assert_eq!(pads(&rt), ["00", "|", "|", ".00"]);
}
