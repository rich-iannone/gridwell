//! Property-test generators: arbitrary *valid* table IR.
//!
//! [`arb_valid_ir`] covers what the writers have to get right together: span
//! layouts in every section (head, data rows, summary rows), hidden columns, stub
//! columns, hidden column labels, group labels, styles with the full cascade
//! (column defaults, striping, conditionals, row and cell styles), footnotes, and
//! text that is hard to escape. Every generated table passes validation.
//!
//! Span layouts are packed from a list of small choices rather than a random
//! seed, so proptest can shrink a failing layout towards a simple one.

use proptest::prelude::*;

use crate::{column, footnote_mark, group, labeled_group, placeholder, row, styled, text};
use crate::{CellExt, ColumnExt, RowExt, TableBuilder};
use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{ConditionalSelector, StyleDef};
use gridwell_ir::{Row, Table};

/// Text that stresses escaping and measurement in every format.
const TRICKY: &[&str] = &[
    "",
    "plain",
    "two words",
    "東京 大阪",
    "مرحبا",
    "שלום",
    "😀👍🏽",
    "e\u{301}",
    "&<>\"'",
    "\\{}$%#_^~|",
    "*_`#[]@=+-/",
    "a\tb",
    "line\nfeed",
    "  padded  ",
    "1,234.56",
    "-0.5",
    "\u{a0}nbsp\u{a0}",
    "ctl\u{1}\u{7}\u{1b}x",
    "}{\\par\\b",
    "</td><script>",
];

fn arb_text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[A-Za-z0-9]{1,8}( [a-z]{1,6})?",
        2 => prop::sample::select(TRICKY).prop_map(str::to_string),
    ]
}

/// The style ids the generator defines (see [`palette`]).
const STYLE_IDS: &[&str] = &["bold", "fill", "italic_red", "bordered", "comp"];

fn palette(builder: TableBuilder) -> TableBuilder {
    let def = |f: fn(&mut StyleDef)| {
        let mut d = StyleDef::default();
        f(&mut d);
        d
    };
    builder
        .style_def("bold", def(|d| d.font_weight = Some("bold".into())))
        .style_def("fill", def(|d| d.background_color = Some("#DDEEFF".into())))
        .style_def(
            "italic_red",
            def(|d| {
                d.font_style = Some("italic".into());
                d.color = Some("hsl(0 80% 40%)".into());
                d.text_align = Some("right".into());
            }),
        )
        .style_def(
            "bordered",
            def(|d| {
                d.border = Some(crate::border_all(crate::border("2px", "dashed", "navy")));
                d.padding = Some(crate::padding_all("3px"));
            }),
        )
        .composition("comp", "fill", def(|d| d.font_weight = Some("700".into())))
}

/// Content for one cell: some text, sometimes styled, sometimes with a footnote
/// mark or a line break.
fn arb_content() -> impl Strategy<Value = Vec<ContentNode>> {
    (arb_text(), 0u8..8, arb_text()).prop_map(|(a, extra, b)| {
        let mut nodes = vec![text(&a)];
        match extra {
            0 => nodes.push(styled(&b, STYLE_IDS[a.len() % STYLE_IDS.len()])),
            1 => nodes.push(footnote_mark("f1", "1")),
            2 => {
                nodes.push(crate::line_break());
                nodes.push(text(&b));
            }
            _ => {}
        }
        nodes
    })
}

/// A section of `rows` × `cols` tiled by rectangles. Each choice picks the span
/// of the next origin (row-major, first free position) as a fraction of the room
/// available; running out of choices means 1×1 cells.
fn pack(rows: usize, cols: usize, choices: &[(u8, u8)], contents: &[Vec<ContentNode>]) -> Vec<Row> {
    let mut taken = vec![vec![false; cols]; rows];
    let mut grid: Vec<Vec<Option<gridwell_ir::Cell>>> = vec![vec![None; cols]; rows];
    let mut k = 0;
    for r in 0..rows {
        for c in 0..cols {
            if taken[r][c] {
                continue;
            }
            let free_right = (c..cols).take_while(|&cc| !taken[r][cc]).count();
            let (a, b) = choices.get(k).copied().unwrap_or((0, 0));
            let colspan = 1 + a as usize % free_right;
            // The rectangle must be free in every row it covers.
            let max_down = (r..rows)
                .take_while(|&rr| (c..c + colspan).all(|cc| !taken[rr][cc]))
                .count();
            let rowspan = 1 + b as usize % max_down;
            for (rr, taken_row) in taken.iter_mut().enumerate().skip(r).take(rowspan) {
                for (cc, t) in taken_row.iter_mut().enumerate().skip(c).take(colspan) {
                    *t = true;
                    grid[rr][cc] = Some(placeholder());
                }
            }
            let content = contents
                .get(k % contents.len().max(1))
                .cloned()
                .unwrap_or_default();
            grid[r][c] = Some(
                crate::cell_content(content)
                    .colspan(colspan as u32)
                    .rowspan(rowspan as u32),
            );
            k += 1;
        }
    }
    grid.into_iter()
        .map(|cells| row(cells.into_iter().map(|c| c.expect("tiled")).collect()))
        .collect()
}

/// One section's shape: row count, span choices, contents, and per-row style ids
/// (as indices into `STYLE_IDS`, `None` for no style).
#[derive(Debug, Clone)]
struct SectionSpec {
    rows: usize,
    choices: Vec<(u8, u8)>,
    contents: Vec<Vec<ContentNode>>,
    row_styles: Vec<Option<usize>>,
    cell_styles: Vec<Option<usize>>,
}

fn arb_section(max_rows: usize) -> impl Strategy<Value = SectionSpec> {
    (
        0..=max_rows,
        prop::collection::vec((any::<u8>(), any::<u8>()), 0..12),
        prop::collection::vec(arb_content(), 1..12),
        prop::collection::vec(prop::option::weighted(0.2, 0..STYLE_IDS.len()), 4),
        prop::collection::vec(prop::option::weighted(0.3, 0..STYLE_IDS.len()), 12),
    )
        .prop_map(
            |(rows, choices, contents, row_styles, cell_styles)| SectionSpec {
                rows,
                choices,
                contents,
                row_styles,
                cell_styles,
            },
        )
}

fn build_section(spec: &SectionSpec, cols: usize) -> Vec<Row> {
    let mut rows = pack(spec.rows, cols, &spec.choices, &spec.contents);
    let mut k = 0;
    for (r, row) in rows.iter_mut().enumerate() {
        if let Some(Some(s)) = spec.row_styles.get(r % spec.row_styles.len()) {
            *row = row.clone().style(STYLE_IDS[*s]);
        }
        for cell in row.cells.iter_mut().filter(|c| !c.is_placeholder) {
            if let Some(Some(s)) = spec.cell_styles.get(k % spec.cell_styles.len()) {
                cell.style_id = Some(STYLE_IDS[*s].into());
            }
            k += 1;
        }
    }
    rows
}

#[derive(Debug, Clone)]
struct GroupSpec {
    label: Option<String>,
    rows: SectionSpec,
    summary: SectionSpec,
}

fn arb_group() -> impl Strategy<Value = GroupSpec> {
    (prop::option::of(arb_text()), arb_section(3), arb_section(2)).prop_map(
        |(label, rows, summary)| GroupSpec {
            label,
            rows,
            summary,
        },
    )
}

/// An arbitrary valid table.
pub fn arb_valid_ir() -> impl Strategy<Value = Table> {
    (
        1usize..=5,
        any::<u8>(),
        0usize..=2,
        any::<bool>(),
        arb_section(2),
        prop::collection::vec(arb_group(), 1..=3),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<u8>()),
        (prop::option::of(arb_text()), prop::option::of(arb_text())),
        prop::collection::vec(prop::option::weighted(0.3, 0..STYLE_IDS.len()), 5),
    )
        .prop_map(
            |(
                cols,
                hidden_bits,
                stub,
                labels_hidden,
                head,
                groups,
                flags,
                (title, note),
                col_styles,
            )| {
                let (striping, include_stub, conditional, parity) = flags;
                // At least one visible column.
                let mut hidden: Vec<bool> =
                    (0..cols).map(|i| hidden_bits & (1 << i) != 0).collect();
                if hidden.iter().all(|h| *h) {
                    hidden[0] = false;
                }
                let stub = stub.min(cols);
                let columns = (0..cols)
                    .map(|i| {
                        let mut c = column(&format!("c{i}"), &format!("C{i}"));
                        if hidden[i] {
                            c = c.hidden();
                        }
                        if let Some(Some(s)) = col_styles.get(i) {
                            c = c.style(STYLE_IDS[*s]);
                        }
                        if i % 2 == 1 {
                            c = c.align("right");
                        }
                        c
                    })
                    .collect();
                let mut b = palette(TableBuilder::new(cols as u32))
                    .columns(columns)
                    .stub_cols(stub as u32);
                if striping {
                    b = b.striping(include_stub, true);
                }
                if conditional {
                    b = b.conditional(
                        "cond",
                        ConditionalSelector {
                            row_parity: Some(if parity % 2 == 0 { "odd" } else { "even" }.into()),
                            scope: Some(["tbody", "thead", "table"][parity as usize % 3].into()),
                        },
                        StyleDef {
                            color: Some("rgb(0 100 0)".into()),
                            ..Default::default()
                        },
                    );
                }
                if labels_hidden {
                    b = b.hide_column_labels();
                }
                if let Some(t) = &title {
                    b = b.title(t);
                }
                for r in build_section(&head, cols) {
                    b = b.head(r);
                }
                for g in &groups {
                    let rows = build_section(&g.rows, cols);
                    let mut gb = match &g.label {
                        Some(l) => labeled_group(l, rows),
                        None => group(rows),
                    };
                    // Summary rows need a stub column.
                    if stub > 0 {
                        gb = gb.summary(build_section(&g.summary, cols));
                    }
                    b = b.group(gb);
                }
                // Cells may carry mark "1" referring to footnote f1: always define it.
                b = b.footnote("f1", "1", note.as_deref().unwrap_or("note"));
                if let Some(n) = &note {
                    b = b.source_note(n);
                }
                b.build()
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn generated_tables_are_valid(t in arb_valid_ir()) {
            let errors = t.validate();
            prop_assert!(errors.is_empty(), "{:?}\n{}", errors, t.to_json().unwrap());
        }
    }

    #[test]
    fn packing_tiles_the_section() {
        // Every choice list tiles exactly (no gaps, no overlaps): validated as a
        // table.
        for seed in 0u8..=255 {
            let choices: Vec<(u8, u8)> = (0..10)
                .map(|i| (seed.wrapping_mul(i + 7), seed ^ i))
                .collect();
            let rows = pack(3, 4, &choices, &[vec![text("x")]]);
            let t = TableBuilder::new(4).body(rows).build();
            assert!(t.validate().is_empty(), "seed {seed}: {:?}", t.validate());
        }
    }
}
