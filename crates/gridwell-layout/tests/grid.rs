//! The visible grid, checked against a naive model.
//!
//! For every tiling of small grids by rectangles (every possible span layout) and
//! every set of hidden columns, the naive model projects each rectangle onto the
//! visible columns directly: it covers the visible columns it touched, starting at
//! the first. The layout must place exactly those origins with exactly those spans,
//! and every covered slot must point back at the origin that covers it.

use gridwell_ir::{Table, VMerge};
use gridwell_layout::{resolve, ResolvedTable, Section, Slot};
use gridwell_testkit::spans::{label, tilings, Tiling};
use gridwell_testkit::{
    cell, column, examples, group, placeholder, row, CellExt, ColumnExt, TableBuilder,
};

/// Text of a cell's single text node.
fn text(content: &[gridwell_ir::content::ContentNode]) -> String {
    match content {
        [gridwell_ir::content::ContentNode::Text { value }] => value.clone(),
        _ => String::new(),
    }
}

/// Structural invariants that hold for any IR, valid or not:
/// - every row has one slot per visible column;
/// - every covered slot points at an origin whose rectangle contains it, with
///   `last` set exactly on the rectangle's bottom row;
/// - every origin's rectangle lies inside the section and its other positions are
///   covered by it.
///
/// Returns the number of `Empty` slots.
fn check_section(s: &Section, width: usize, what: &str) -> usize {
    let mut empty = 0;
    for (r, row) in s.rows.iter().enumerate() {
        assert_eq!(row.slots.len(), width, "{what}: row {r} width");
        for (c, slot) in row.slots.iter().enumerate() {
            match slot {
                Slot::Origin(cell) => {
                    assert_eq!((cell.col, r), (c, r), "{what}: origin position");
                    assert!(cell.colspan >= 1 && cell.rowspan >= 1, "{what}");
                    assert!(c + cell.colspan <= width, "{what}: colspan overflows");
                    assert!(
                        r + cell.rowspan <= s.rows.len(),
                        "{what}: rowspan overflows"
                    );
                    for dr in 0..cell.rowspan {
                        for dc in 0..cell.colspan {
                            let covered = &s.rows[r + dr].slots[c + dc];
                            let ok = match (dr, dc, covered) {
                                (0, 0, _) => true,
                                (0, _, Slot::CoveredH { origin_col }) => *origin_col == c,
                                (
                                    _,
                                    _,
                                    Slot::CoveredV {
                                        origin_row,
                                        origin_col,
                                        last,
                                    },
                                ) => {
                                    (*origin_row, *origin_col) == (r, c)
                                        && *last == (dr == cell.rowspan - 1)
                                }
                                _ => false,
                            };
                            assert!(
                                ok,
                                "{what}: ({},{}) not covered by origin ({r},{c}): {covered:?}",
                                r + dr,
                                c + dc
                            );
                        }
                    }
                }
                Slot::CoveredH { origin_col } => {
                    let o = row.slots[*origin_col]
                        .origin()
                        .unwrap_or_else(|| panic!("{what}: CoveredH at ({r},{c}) → no origin"));
                    assert!(*origin_col < c && c < origin_col + o.colspan, "{what}");
                }
                Slot::CoveredV {
                    origin_row,
                    origin_col,
                    ..
                } => {
                    let o = s.rows[*origin_row].slots[*origin_col]
                        .origin()
                        .unwrap_or_else(|| panic!("{what}: CoveredV at ({r},{c}) → no origin"));
                    assert!(*origin_row < r && r < origin_row + o.rowspan, "{what}");
                    assert!(*origin_col <= c && c < origin_col + o.colspan, "{what}");
                }
                Slot::Empty => empty += 1,
            }
        }
    }
    empty
}

fn check_table(t: &ResolvedTable, what: &str) -> usize {
    let width = t.columns.len();
    t.sections().map(|s| check_section(s, width, what)).sum()
}

/// Continuation layout: each row's spans tile the visible columns left to right.
fn check_continuation(s: &Section, width: usize, what: &str) {
    for (r, cells) in s.continuation_rows().iter().enumerate() {
        let mut next = 0;
        for m in cells {
            assert_eq!(m.col, next, "{what}: row {r} gap/overlap at {}", m.col);
            assert!(m.span >= 1);
            // Continuations have no cell; starts always do.
            match m.vmerge {
                VMerge::Continue => assert!(m.cell.is_none(), "{what}"),
                VMerge::Start => assert!(m.cell.is_some_and(|c| c.rowspan > 1), "{what}"),
                VMerge::None => assert!(m.cell.is_none_or(|c| c.rowspan == 1), "{what}"),
            }
            next += m.span;
        }
        assert_eq!(next, width, "{what}: row {r} covers {next} of {width}");
    }
}

fn table_for(t: &Tiling, hidden: &[bool]) -> Table {
    let cols = (0..t.cols)
        .map(|i| {
            let c = column(&format!("c{i}"), "");
            if hidden[i] {
                c.hidden()
            } else {
                c
            }
        })
        .collect();
    TableBuilder::new(t.cols as u32)
        .columns(cols)
        .group(group(t.to_rows()))
        .build()
}

/// The naive projection: `(origin label, visible row, first visible col, colspan,
/// rowspan)` for each rectangle that touches a visible column, sorted.
fn naive(t: &Tiling, hidden: &[bool]) -> Vec<(String, usize, usize, usize, usize)> {
    let vis = |c: usize| (0..c).filter(|&i| !hidden[i]).count();
    let mut out: Vec<_> = t
        .rects
        .iter()
        .filter_map(|rect| {
            let visible: Vec<usize> = (rect.col..rect.col + rect.colspan)
                .filter(|&c| !hidden[c])
                .collect();
            let first = *visible.first()?;
            Some((
                label(rect.row, rect.col),
                rect.row,
                vis(first),
                visible.len(),
                rect.rowspan,
            ))
        })
        .collect();
    out.sort();
    out
}

fn actual(rt: &ResolvedTable) -> Vec<(String, usize, usize, usize, usize)> {
    let mut out: Vec<_> = rt.groups[0]
        .rows
        .rows
        .iter()
        .enumerate()
        .flat_map(|(r, row)| {
            row.cells()
                .map(move |c| (text(c.content), r, c.col, c.colspan, c.rowspan))
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_tiling_under_every_hidden_mask_matches_the_naive_projection() {
    let mut checked = 0;
    for (rows, cols) in [(1, 5), (2, 4), (3, 3), (3, 4), (2, 5)] {
        for t in tilings(rows, cols) {
            for mask in 0u32..(1 << cols) {
                let hidden: Vec<bool> = (0..cols).map(|i| mask & (1 << i) != 0).collect();
                let table = table_for(&t, &hidden);
                let rt = resolve(&table);
                let what = format!("{} hidden {hidden:?}", t.ascii());
                assert_eq!(rt.columns.len(), hidden.iter().filter(|h| !**h).count());
                assert_eq!(
                    check_table(&rt, &what),
                    0,
                    "{what}: empty slots in valid IR"
                );
                assert_eq!(actual(&rt), naive(&t, &hidden), "{what}");
                check_continuation(&rt.groups[0].rows, rt.columns.len(), &what);
                checked += 1;
            }
        }
    }
    // 1×5: 16×32, 2×4: 148×16, 3×3: 322×8, 3×4: 3164×16, 2×5: 650×32
    assert_eq!(checked, 16 * 32 + 148 * 16 + 322 * 8 + 3164 * 16 + 650 * 32);
}

#[test]
fn continuation_rows_match_the_ir_helper_without_hidden_columns() {
    for t in tilings(3, 3) {
        let table = table_for(&t, &[false; 3]);
        let rt = resolve(&table);
        let ours: Vec<Vec<_>> = rt.groups[0]
            .rows
            .continuation_rows()
            .iter()
            .map(|r| {
                r.iter()
                    .map(|m| (m.col, m.span, m.cell.map(|c| text(c.content)), m.vmerge))
                    .collect()
            })
            .collect();
        let ir_rows = &table.table.tbody[0].rows;
        let theirs: Vec<Vec<_>> = gridwell_ir::vmerge_layout(ir_rows)
            .iter()
            .map(|r| {
                r.iter()
                    .map(|m| (m.col, m.span, m.cell.map(|c| text(&c.content)), m.vmerge))
                    .collect()
            })
            .collect();
        assert_eq!(ours, theirs, "{}", t.ascii());
    }
}

#[test]
fn corpus_with_each_column_hidden_holds_invariants() {
    for ex in examples() {
        let base = ex.table();
        assert!(base.validate().is_empty(), "{}", ex.name);
        assert_eq!(check_table(&resolve(&base), ex.name), 0, "{}", ex.name);
        for hidden in 0..base.column_spec.len() {
            let mut t = base.clone();
            t.column_spec[hidden].hidden = true;
            let what = format!("{} hide {hidden}", ex.name);
            let rt = resolve(&t);
            assert_eq!(check_table(&rt, &what), 0, "{what}");
            for s in rt.sections() {
                check_continuation(s, rt.columns.len(), &what);
            }
        }
    }
}

#[test]
fn origin_in_hidden_column_moves_right() {
    // A spans columns 0–1; column 0 is hidden: A starts at visible column 0 with
    // colspan 1 and takes column 1's style and alignment.
    let t = TableBuilder::new(3)
        .columns(vec![
            column("a", "A").hidden().align("right"),
            column("b", "B").align("center").style("bstyle"),
            column("c", "C"),
        ])
        .style_def(
            "bstyle",
            gridwell_ir::StyleDef {
                color: Some("red".into()),
                ..Default::default()
            },
        )
        .body(vec![row(vec![
            cell("A").colspan(2),
            placeholder(),
            cell("C"),
        ])])
        .build();
    let rt = resolve(&t);
    let cells: Vec<_> = rt.groups[0].rows.rows[0].cells().collect();
    assert_eq!(cells.len(), 2);
    assert_eq!(
        (
            text(cells[0].content).as_str(),
            cells[0].col,
            cells[0].colspan
        ),
        ("A", 0, 1)
    );
    assert_eq!(cells[0].grid_col, 1);
    assert_eq!(cells[0].align, gridwell_ir::HAlign::Center);
    assert_eq!(cells[0].style.color, Some("red".parse().unwrap()));
}

#[test]
fn every_column_hidden_leaves_empty_rows() {
    let t = TableBuilder::new(2)
        .columns(vec![column("a", "A").hidden(), column("b", "B").hidden()])
        .head(row(vec![cell("h1"), cell("h2")]))
        .body(vec![row(vec![cell("x"), cell("y")])])
        .build();
    let rt = resolve(&t);
    assert!(rt.is_empty());
    assert_eq!(rt.head.rows.len(), 1);
    assert!(rt.head.rows[0].slots.is_empty());
    assert_eq!(check_table(&rt, "all hidden"), 0);
}

#[test]
fn hidden_column_labels_empty_the_head() {
    let t = TableBuilder::new(1)
        .hide_column_labels()
        .head(row(vec![cell("h")]))
        .body(vec![row(vec![cell("x")])])
        .build();
    let rt = resolve(&t);
    assert!(rt.head.is_empty());
    assert_eq!(rt.groups[0].rows.rows.len(), 1);
}

#[test]
fn stub_columns_and_flags() {
    let t = TableBuilder::new(3)
        .stub_cols(2)
        .columns(vec![
            column("a", "A").hidden(),
            column("b", "B"),
            column("c", "C"),
        ])
        .body(vec![row(vec![cell("a"), cell("b"), cell("c").stub()])])
        .build();
    let rt = resolve(&t);
    assert_eq!(rt.stub_cols, 1);
    assert!(rt.columns[0].is_stub && !rt.columns[1].is_stub);
    let stub: Vec<bool> = rt.groups[0].rows.rows[0]
        .cells()
        .map(|c| c.is_stub)
        .collect();
    // b is in a stub column; c is flagged.
    assert_eq!(stub, vec![true, true]);
}

#[test]
fn row_numbers_count_data_rows_across_groups() {
    let t = TableBuilder::new(1)
        .stub_cols(1)
        .head(row(vec![cell("h")]))
        .group(
            group(vec![row(vec![cell("1")]), row(vec![cell("2")])])
                .summary(vec![row(vec![cell("s1")])]),
        )
        .group(
            group(vec![row(vec![cell("3")])])
                .summary(vec![row(vec![cell("s1")]), row(vec![cell("s2")])]),
        )
        .build();
    let rt = resolve(&t);
    assert_eq!(rt.head.rows[0].number, 1);
    let data: Vec<usize> = rt
        .groups
        .iter()
        .flat_map(|g| g.rows.rows.iter().map(|r| r.number))
        .collect();
    assert_eq!(data, vec![1, 2, 3]);
    let summary: Vec<Vec<usize>> = rt
        .groups
        .iter()
        .map(|g| g.summary_rows.rows.iter().map(|r| r.number).collect())
        .collect();
    assert_eq!(summary, vec![vec![1], vec![1, 2]]);
}

#[test]
fn columns_parse_widths_and_alignment() {
    let t = TableBuilder::new(4)
        .columns(vec![
            column("a", "A")
                .width("120px")
                .min_width("1in")
                .max_width("50%"),
            column("b", "B").width("auto").align("right"),
            column("c", "C").width("nonsense").align("diagonal"),
            column("d", "D").width("2fr"),
        ])
        .body(vec![row(vec![cell("1"), cell("2"), cell("3"), cell("4")])])
        .build();
    let rt = resolve(&t);
    use gridwell_core::Length::*;
    use gridwell_ir::HAlign;
    let c = &rt.columns;
    assert_eq!(
        (
            c[0].width.clone(),
            c[0].min_width.clone(),
            c[0].max_width.clone()
        ),
        (Some(Px(120.0)), Some(In(1.0)), Some(Percent(50.0)))
    );
    assert_eq!(
        (c[1].width.clone(), c[1].align.clone()),
        (None, HAlign::Right)
    );
    assert_eq!(
        (c[2].width.clone(), c[2].align.clone()),
        (None, HAlign::Left)
    );
    assert_eq!(c[3].width, Some(Fr(2.0)));
}

/// A tiny deterministic generator (xorshift) for the robustness test.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn invalid_ir_never_panics_and_keeps_structural_invariants() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let tiles = tilings(3, 4);
    for i in 0..20_000 {
        let t = &tiles[rng.below(tiles.len() as u64) as usize];
        let mut rows = t.to_rows();
        // Corrupt the grid: random spans (incl. 0 and huge), toggled placeholders,
        // ragged rows.
        for _ in 0..rng.below(4) + 1 {
            let r = rng.below(rows.len() as u64) as usize;
            if rows[r].cells.is_empty() {
                continue;
            }
            let c = rng.below(rows[r].cells.len() as u64) as usize;
            let spans = [0, 1, 2, 5, u32::MAX];
            let cells = &mut rows[r].cells;
            match rng.below(5) {
                0 => cells[c].colspan = spans[rng.below(5) as usize],
                1 => cells[c].rowspan = spans[rng.below(5) as usize],
                2 => cells[c].is_placeholder = !cells[c].is_placeholder,
                3 => {
                    cells.pop();
                }
                _ => cells.push(cells[c].clone()),
            }
        }
        let hidden: Vec<bool> = (0..5).map(|_| rng.below(3) == 0).collect();
        let mut table = TableBuilder::new(4).group(group(rows)).build();
        // Column spec may disagree with table_cols, too.
        table.column_spec = (0..rng.below(6) as usize)
            .map(|k| {
                let c = column(&format!("c{k}"), "");
                if hidden[k] {
                    c.hidden()
                } else {
                    c
                }
            })
            .collect();
        let rt = resolve(&table);
        check_table(&rt, &format!("case {i}"));
        for s in rt.sections() {
            check_continuation(s, rt.columns.len(), &format!("case {i}"));
        }
    }
}

#[test]
fn invalid_fixtures_resolve_without_panicking() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/invalid");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let json = std::fs::read_to_string(&path).unwrap();
        let Ok(table) = Table::from_json(&json) else {
            continue;
        };
        let rt = resolve(&table);
        check_table(&rt, &path.display().to_string());
        n += 1;
    }
    assert!(n >= 8, "only {n} invalid fixtures parsed");
}
