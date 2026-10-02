//! Exhaustive span layouts for stress-testing writers.
//!
//! A *tiling* is a way to cover an `rows × cols` grid with axis-aligned rectangles,
//! each of which becomes one origin cell with that `rowspan`/`colspan`; every other
//! position it covers becomes a placeholder. Enumerating all tilings of small grids
//! gives every span shape a writer can meet — colspans next to colspans, rowspans
//! beside colspans, full-width and full-height spans, 2D blocks — in a few thousand
//! tables.
//!
//! Each origin cell's text is its grid position, `r{row}c{col}`, so tests can check
//! that a writer emitted every cell (see [`Tiling::origin_labels`]).

use gridwell_ir::{Cell, Row};

use crate::builder::{cell, placeholder, row, CellExt};

/// One rectangle in a tiling: the origin cell at (`row`, `col`) and its spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
}

/// A complete tiling of a `rows × cols` grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tiling {
    pub rows: usize,
    pub cols: usize,
    /// Rectangles in row-major order of their origin.
    pub rects: Vec<Rect>,
}

/// The label for the origin cell at (`row`, `col`).
pub fn label(row: usize, col: usize) -> String {
    format!("r{row}c{col}")
}

impl Tiling {
    /// Materialize as IR rows: origins get `r{row}c{col}` text plus spans; covered
    /// positions get placeholders.
    pub fn to_rows(&self) -> Vec<Row> {
        let mut grid: Vec<Vec<Option<Cell>>> = vec![vec![None; self.cols]; self.rows];
        for rect in &self.rects {
            for grid_row in grid.iter_mut().skip(rect.row).take(rect.rowspan) {
                for slot in grid_row.iter_mut().skip(rect.col).take(rect.colspan) {
                    *slot = Some(placeholder());
                }
            }
            grid[rect.row][rect.col] = Some(
                cell(&label(rect.row, rect.col))
                    .colspan(rect.colspan as u32)
                    .rowspan(rect.rowspan as u32),
            );
        }
        grid.into_iter()
            .map(|cells| {
                row(cells
                    .into_iter()
                    .map(|c| c.expect("tiling covers grid"))
                    .collect())
            })
            .collect()
    }

    /// Text of every origin cell, in row-major order.
    pub fn origin_labels(&self) -> Vec<String> {
        self.rects.iter().map(|r| label(r.row, r.col)).collect()
    }

    /// True if any rectangle spans more than one row or column.
    pub fn has_spans(&self) -> bool {
        self.rects.iter().any(|r| r.rowspan > 1 || r.colspan > 1)
    }

    /// A compact ASCII picture for failure messages, e.g.
    /// `AAB / CDB` — one letter per rectangle, rows separated by ` / `.
    pub fn ascii(&self) -> String {
        let mut grid = vec![vec!['?'; self.cols]; self.rows];
        for (i, rect) in self.rects.iter().enumerate() {
            let ch = (b'A' + (i % 26) as u8) as char;
            for row in grid.iter_mut().skip(rect.row).take(rect.rowspan) {
                for slot in row.iter_mut().skip(rect.col).take(rect.colspan) {
                    *slot = ch;
                }
            }
        }
        grid.into_iter()
            .map(|r| r.into_iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

/// Every tiling of a `rows × cols` grid by rectangles.
///
/// Counts grow quickly: 1×5 → 16, 2×3 → 34, 2×4 → 148, 3×3 → 322, 2×5 → 650,
/// 3×4 → 3164.
pub fn tilings(rows: usize, cols: usize) -> Vec<Tiling> {
    let mut out = Vec::new();
    let mut occupied = vec![vec![false; cols]; rows];
    let mut rects = Vec::new();
    enumerate(rows, cols, &mut occupied, &mut rects, &mut out);
    out
}

fn enumerate(
    rows: usize,
    cols: usize,
    occupied: &mut [Vec<bool>],
    rects: &mut Vec<Rect>,
    out: &mut Vec<Tiling>,
) {
    // The first free position in row-major order must be the origin of the next
    // rectangle; this visits each tiling exactly once.
    let Some((r0, c0)) = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .find(|&(r, c)| !occupied[r][c])
    else {
        out.push(Tiling {
            rows,
            cols,
            rects: rects.clone(),
        });
        return;
    };

    for h in 1..=rows - r0 {
        for w in 1..=cols - c0 {
            let free = (r0..r0 + h).all(|r| (c0..c0 + w).all(|c| !occupied[r][c]));
            if !free {
                // Wider rectangles at this height are blocked too.
                break;
            }
            set(occupied, r0, c0, h, w, true);
            rects.push(Rect {
                row: r0,
                col: c0,
                rowspan: h,
                colspan: w,
            });
            enumerate(rows, cols, occupied, rects, out);
            rects.pop();
            set(occupied, r0, c0, h, w, false);
        }
    }
}

fn set(occupied: &mut [Vec<bool>], r0: usize, c0: usize, h: usize, w: usize, value: bool) {
    for row in occupied.iter_mut().skip(r0).take(h) {
        for slot in row.iter_mut().skip(c0).take(w) {
            *slot = value;
        }
    }
}

#[cfg(test)]
// Index loops mirror the grid geometry the oracles describe; iterator chains obscure it.
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;
    use crate::builder::{group, TableBuilder};

    #[test]
    fn tiling_counts_match_known_values() {
        let counts: Vec<usize> = [(1, 1), (1, 5), (2, 3), (2, 4), (3, 3), (2, 5), (3, 4)]
            .iter()
            .map(|&(r, c)| tilings(r, c).len())
            .collect();
        assert_eq!(counts, vec![1, 16, 34, 148, 322, 650, 3164]);
    }

    #[test]
    fn tilings_are_distinct() {
        let all = tilings(3, 3);
        let mut pictures: Vec<String> = all.iter().map(|t| t.ascii()).collect();
        pictures.sort();
        pictures.dedup();
        assert_eq!(pictures.len(), all.len());
    }

    #[test]
    fn every_tiling_is_valid_ir_as_body_and_as_head() {
        for t in tilings(3, 3).into_iter().chain(tilings(2, 4)) {
            let cols = t.cols as u32;
            let body = TableBuilder::new(cols).group(group(t.to_rows())).build();
            assert!(
                body.validate().is_empty(),
                "body {}: {:?}",
                t.ascii(),
                body.validate()
            );

            let filler = t.to_rows()[0]
                .cells
                .iter()
                .enumerate()
                .map(|(i, _)| cell(&format!("b{i}")))
                .collect();
            let head = {
                let mut b = TableBuilder::new(cols);
                for r in t.to_rows() {
                    b = b.head(r);
                }
                b.body(vec![row(filler)]).build()
            };
            assert!(
                head.validate().is_empty(),
                "head {}: {:?}",
                t.ascii(),
                head.validate()
            );
        }
    }

    #[test]
    fn resolve_slots_agrees_with_every_tiling() {
        use gridwell_ir::{resolve_slots, Slot};
        for (rows, cols) in [(1, 5), (2, 4), (3, 3), (3, 4)] {
            for t in tilings(rows, cols) {
                let slots = resolve_slots(&t.to_rows());
                let mut expected = vec![vec![Slot::Orphan; cols]; rows];
                for rect in &t.rects {
                    for r in rect.row..rect.row + rect.rowspan {
                        for c in rect.col..rect.col + rect.colspan {
                            expected[r][c] = if (r, c) == (rect.row, rect.col) {
                                Slot::Origin {
                                    colspan: rect.colspan,
                                    rowspan: rect.rowspan,
                                }
                            } else if r == rect.row {
                                Slot::CoveredH {
                                    origin_col: rect.col,
                                }
                            } else {
                                Slot::CoveredV {
                                    origin_row: rect.row,
                                    origin_col: rect.col,
                                }
                            };
                        }
                    }
                }
                assert_eq!(slots, expected, "tiling {}", t.ascii());
            }
        }
    }

    #[test]
    fn vmerge_layout_tiles_every_row_for_every_tiling() {
        use gridwell_ir::{vmerge_layout, VMerge};
        for (rows, cols) in [(1, 5), (2, 4), (3, 3), (3, 4)] {
            for t in tilings(rows, cols) {
                let ir_rows = t.to_rows();
                let layout = vmerge_layout(&ir_rows);
                for (r, cells) in layout.iter().enumerate() {
                    // Spans tile 0..cols in order with no gaps or overlaps.
                    let mut next = 0;
                    for m in cells {
                        assert_eq!(m.col, next, "tiling {} row {r}: gap/overlap", t.ascii());
                        next += m.span;
                    }
                    assert_eq!(next, cols, "tiling {} row {r}: short row", t.ascii());
                }
                // Every rectangle yields one origin plus (rowspan - 1) continuations,
                // each as wide as the rectangle.
                for rect in &t.rects {
                    let origin = layout[rect.row].iter().find(|m| m.col == rect.col).unwrap();
                    assert!(origin.cell.is_some());
                    assert_eq!(origin.span, rect.colspan);
                    let want = if rect.rowspan > 1 {
                        VMerge::Start
                    } else {
                        VMerge::None
                    };
                    assert_eq!(origin.vmerge, want, "tiling {}", t.ascii());
                    for r in rect.row + 1..rect.row + rect.rowspan {
                        let cont = layout[r].iter().find(|m| m.col == rect.col).unwrap();
                        assert!(cont.cell.is_none());
                        assert_eq!((cont.span, cont.vmerge), (rect.colspan, VMerge::Continue));
                    }
                }
            }
        }
    }

    #[test]
    fn ascii_and_labels_describe_the_tiling() {
        let t = Tiling {
            rows: 2,
            cols: 3,
            rects: vec![
                Rect {
                    row: 0,
                    col: 0,
                    rowspan: 1,
                    colspan: 2,
                },
                Rect {
                    row: 0,
                    col: 2,
                    rowspan: 2,
                    colspan: 1,
                },
                Rect {
                    row: 1,
                    col: 0,
                    rowspan: 1,
                    colspan: 1,
                },
                Rect {
                    row: 1,
                    col: 1,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        };
        assert_eq!(t.ascii(), "AAB / CDB");
        assert_eq!(t.origin_labels(), vec!["r0c0", "r0c2", "r1c0", "r1c1"]);
        assert!(t.has_spans());
        let rows = t.to_rows();
        assert_eq!(rows[0].cells[0].colspan, 2);
        assert!(rows[0].cells[1].is_placeholder);
        assert_eq!(rows[0].cells[2].rowspan, 2);
        assert!(rows[1].cells[2].is_placeholder);
    }
}
