use crate::cell::{Cell, Row};
use crate::validation::{ValidationError, ValidationRule};

/// A materialized 2D occupancy grid for a table section.
/// Each cell in the grid holds an optional identifier (row, col of the owning cell).
#[derive(Debug)]
pub struct OccupancyGrid {
    pub rows: u32,
    pub cols: u32,
    /// Grid data: `grid[row][col]` = Some((owner_row, owner_col)) if occupied.
    pub grid: Vec<Vec<Option<(u32, u32)>>>,
}

impl OccupancyGrid {
    pub fn new(rows: u32, cols: u32) -> Self {
        let grid = vec![vec![None; cols as usize]; rows as usize];
        Self { rows, cols, grid }
    }

    /// Materialize the grid from a list of rows, collecting errors.
    /// `section` and `row_group` are used for error reporting.
    /// Each row has exactly `table_cols` cells (placeholders fill spanned positions).
    pub fn materialize(
        rows: &[Row],
        table_cols: u32,
        section: &str,
        row_group: Option<u32>,
    ) -> (Self, Vec<ValidationError>) {
        let num_rows = rows.len() as u32;
        let mut grid = Self::new(num_rows, table_cols);
        let mut errors = Vec::new();

        for (r, row) in rows.iter().enumerate() {
            let r = r as u32;

            for (c, cell) in row.cells.iter().enumerate() {
                let c = c as u32;

                // Placeholder cells don't claim grid positions — they mark positions
                // that are already claimed by another cell's colspan/rowspan.
                if cell.is_placeholder {
                    continue;
                }

                if c >= table_cols {
                    break;
                }

                let colspan = cell.colspan;
                let rowspan = cell.rowspan;

                // Check for zero values
                if colspan == 0 || rowspan == 0 {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanZeroValue,
                        section: section.to_string(),
                        row_group,
                        row: Some(r),
                        col: Some(c),
                        message: format!(
                            "Cell at (row={r}, col={c}) has colspan={colspan}, rowspan={rowspan} — minimum is 1"
                        ),
                    });
                    continue;
                }

                // Span arithmetic is done in u64: colspan/rowspan come straight from
                // untrusted JSON and may be as large as u32::MAX.
                let col_end = c as u64 + colspan as u64;
                let row_end = r as u64 + rowspan as u64;

                // Check overflow right
                if col_end > table_cols as u64 {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanOverflowRight,
                        section: section.to_string(),
                        row_group,
                        row: Some(r),
                        col: Some(c),
                        message: format!(
                            "Cell at (row={r}, col={c}) has colspan={colspan} but table only has {table_cols} columns (would need col index up to {})",
                            col_end - 1
                        ),
                    });
                    continue;
                }

                // Check overflow bottom
                if row_end > num_rows as u64 {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanOverflowBottom,
                        section: section.to_string(),
                        row_group,
                        row: Some(r),
                        col: Some(c),
                        message: format!(
                            "Cell at (row={r}, col={c}) has rowspan={rowspan} but section only has {num_rows} rows (would need row index up to {})",
                            row_end - 1
                        ),
                    });
                    // Still claim what we can within bounds
                    let effective_rowspan = num_rows - r;
                    claim_cells(
                        &mut grid,
                        &mut errors,
                        r,
                        c,
                        effective_rowspan,
                        colspan,
                        section,
                        row_group,
                    );
                    continue;
                }

                // Claim all grid positions for this cell
                claim_cells(
                    &mut grid,
                    &mut errors,
                    r,
                    c,
                    rowspan,
                    colspan,
                    section,
                    row_group,
                );
            }
        }

        // Check for gaps (unclaimed positions)
        for r in 0..num_rows {
            for c in 0..table_cols {
                if grid.grid[r as usize][c as usize].is_none() {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanGap,
                        section: section.to_string(),
                        row_group,
                        row: Some(r),
                        col: Some(c),
                        message: format!(
                            "Grid position (row={r}, col={c}) is not owned by any cell (missing placeholder or cell)"
                        ),
                    });
                }
            }
        }

        (grid, errors)
    }
}

/// Claim cells in the grid, reporting overlaps.
///
/// Callers must have bounds-checked the span first (`start + span <= grid size`), so the
/// additions below cannot overflow.
#[allow(clippy::too_many_arguments)]
fn claim_cells(
    grid: &mut OccupancyGrid,
    errors: &mut Vec<ValidationError>,
    start_row: u32,
    start_col: u32,
    rowspan: u32,
    colspan: u32,
    section: &str,
    row_group: Option<u32>,
) {
    for dr in 0..rowspan {
        for dc in 0..colspan {
            let r = (start_row + dr) as usize;
            let c = (start_col + dc) as usize;
            if r < grid.rows as usize && c < grid.cols as usize {
                if let Some((owner_r, owner_c)) = grid.grid[r][c] {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanOverlap,
                        section: section.to_string(),
                        row_group,
                        row: Some(start_row + dr),
                        col: Some(start_col + dc),
                        message: format!(
                            "Grid position (row={}, col={}) is already claimed by cell at (row={owner_r}, col={owner_c})",
                            start_row + dr, start_col + dc
                        ),
                    });
                } else {
                    grid.grid[r][c] = Some((start_row, start_col));
                }
            }
        }
    }
}

/// How one position of a section's grid is occupied.
///
/// Produced by [`resolve_slots`]. Writers use it to tell apart the two kinds of
/// placeholder, which most formats encode differently: positions covered from the
/// left by a colspan (`CoveredH`; e.g. skipped in HTML, `hMerge` in PPTX) and
/// positions covered from above by a rowspan (`CoveredV`; e.g. `vMerge` continuation
/// in DOCX, `\clvmrg` in RTF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The origin of a cell. Spans are clamped to the section's bounds.
    Origin { colspan: usize, rowspan: usize },
    /// Covered by a cell that starts earlier in the same row.
    CoveredH { origin_col: usize },
    /// Covered by a cell that starts in an earlier row. For a 2D span `origin_col`
    /// may be left of this position.
    CoveredV {
        origin_row: usize,
        origin_col: usize,
    },
    /// A placeholder that no span covers. Only occurs in invalid IR.
    Orphan,
}

/// Resolve every position of a section (thead, a group's rows, or a group's summary
/// rows) to a [`Slot`].
///
/// The result has one entry per row and, within it, one entry per cell, so it can be
/// zipped with `row.cells`. Intended for validated IR, but never panics on invalid IR:
/// spans are clamped to the positions that exist, overlaps keep the first owner, and
/// uncovered placeholders become [`Slot::Orphan`].
pub fn resolve_slots(rows: &[Row]) -> Vec<Vec<Slot>> {
    // owner[r][c] = (origin_row, origin_col) for positions covered by a span.
    let mut owner: Vec<Vec<Option<(usize, usize)>>> =
        rows.iter().map(|r| vec![None; r.cells.len()]).collect();
    let mut slots: Vec<Vec<Slot>> = rows
        .iter()
        .map(|r| vec![Slot::Orphan; r.cells.len()])
        .collect();

    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.is_placeholder {
                slots[r][c] = match owner[r][c] {
                    Some((orow, ocol)) if orow == r => Slot::CoveredH { origin_col: ocol },
                    Some((orow, ocol)) => Slot::CoveredV {
                        origin_row: orow,
                        origin_col: ocol,
                    },
                    None => Slot::Orphan,
                };
                continue;
            }

            let colspan = (cell.colspan.max(1) as usize).min(row.cells.len() - c);
            let rowspan = (cell.rowspan.max(1) as usize).min(rows.len() - r);
            slots[r][c] = Slot::Origin { colspan, rowspan };

            for (dr, owner_row) in owner.iter_mut().skip(r).take(rowspan).enumerate() {
                for (dc, slot) in owner_row.iter_mut().skip(c).take(colspan).enumerate() {
                    if (dr, dc) != (0, 0) && slot.is_none() {
                        *slot = Some((r, c));
                    }
                }
            }
        }
    }

    slots
}

/// Vertical-merge role of a cell in a "continuation cell" layout (see [`vmerge_layout`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VMerge {
    /// Not vertically merged.
    None,
    /// First cell of a vertically merged range (DOCX `vMerge="restart"`, RTF `\clvmgf`).
    Start,
    /// Continuation of a vertically merged range (DOCX `vMerge`, RTF `\clvmrg`). Has
    /// no IR cell; formats emit it empty.
    Continue,
}

/// One cell as emitted by formats that model rowspans with explicit continuation
/// cells (DOCX, RTF).
#[derive(Debug, Clone, Copy)]
pub struct MergeCell<'a> {
    /// Grid column of the cell's left edge.
    pub col: usize,
    /// Width in grid columns.
    pub span: usize,
    /// The IR cell for origins; `None` for continuations (and orphans in invalid IR).
    pub cell: Option<&'a Cell>,
    pub vmerge: VMerge,
}

/// Lay out a section for formats with continuation cells: every row lists the cells to
/// emit, left to right.
///
/// - An origin emits one cell as wide as its colspan (horizontally covered positions
///   are absorbed into it), marked [`VMerge::Start`] if it spans rows.
/// - A vertically covered position at the origin's column emits a
///   [`VMerge::Continue`] cell as wide as the origin's colspan; the rest of a 2D
///   span's positions in that row are absorbed into it.
/// - Orphan placeholders (invalid IR) emit an empty one-column cell so the row still
///   covers the grid.
///
/// For each row, the emitted spans tile `0..row.cells.len()` exactly when the section
/// is valid.
pub fn vmerge_layout(rows: &[Row]) -> Vec<Vec<MergeCell<'_>>> {
    let slots = resolve_slots(rows);
    rows.iter()
        .zip(&slots)
        .map(|(row, row_slots)| {
            let mut out = Vec::new();
            for (col, (cell, slot)) in row.cells.iter().zip(row_slots).enumerate() {
                match *slot {
                    Slot::Origin { colspan, rowspan } => out.push(MergeCell {
                        col,
                        span: colspan,
                        cell: Some(cell),
                        vmerge: if rowspan > 1 {
                            VMerge::Start
                        } else {
                            VMerge::None
                        },
                    }),
                    Slot::CoveredV {
                        origin_row,
                        origin_col,
                    } if origin_col == col => {
                        let span = match slots[origin_row][origin_col] {
                            Slot::Origin { colspan, .. } => colspan,
                            _ => 1,
                        };
                        out.push(MergeCell {
                            col,
                            span,
                            cell: None,
                            vmerge: VMerge::Continue,
                        });
                    }
                    Slot::CoveredH { .. } | Slot::CoveredV { .. } => {}
                    Slot::Orphan => out.push(MergeCell {
                        col,
                        span: 1,
                        cell: None,
                        vmerge: VMerge::None,
                    }),
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(colspan: u32, rowspan: u32) -> Cell {
        serde_json::from_value(serde_json::json!({
            "content": [{ "type": "text", "value": "x" }],
            "colspan": colspan,
            "rowspan": rowspan
        }))
        .unwrap()
    }

    fn ph() -> Cell {
        serde_json::from_value(serde_json::json!({ "content": [], "is_placeholder": true }))
            .unwrap()
    }

    fn rows(cells: Vec<Vec<Cell>>) -> Vec<Row> {
        cells
            .into_iter()
            .map(|cells| Row {
                role: None,
                style_id: None,
                cells,
            })
            .collect()
    }

    use Slot::*;

    #[test]
    fn colspan_then_plain_cell() {
        let s = resolve_slots(&rows(vec![vec![origin(2, 1), ph(), origin(1, 1)]]));
        assert_eq!(
            s,
            vec![vec![
                Origin {
                    colspan: 2,
                    rowspan: 1
                },
                CoveredH { origin_col: 0 },
                Origin {
                    colspan: 1,
                    rowspan: 1
                },
            ]]
        );
    }

    #[test]
    fn rowspan_covers_position_below() {
        let s = resolve_slots(&rows(vec![
            vec![origin(1, 2), origin(1, 1)],
            vec![ph(), origin(1, 1)],
        ]));
        assert_eq!(
            s[1][0],
            CoveredV {
                origin_row: 0,
                origin_col: 0
            }
        );
    }

    #[test]
    fn block_span_classifies_each_position() {
        let s = resolve_slots(&rows(vec![
            vec![origin(1, 1), origin(2, 2), ph()],
            vec![origin(1, 1), ph(), ph()],
        ]));
        assert_eq!(
            s[0][1],
            Origin {
                colspan: 2,
                rowspan: 2
            }
        );
        assert_eq!(s[0][2], CoveredH { origin_col: 1 });
        assert_eq!(
            s[1][1],
            CoveredV {
                origin_row: 0,
                origin_col: 1
            }
        );
        assert_eq!(
            s[1][2],
            CoveredV {
                origin_row: 0,
                origin_col: 1
            }
        );
    }

    #[test]
    fn uncovered_placeholder_is_orphan() {
        let s = resolve_slots(&rows(vec![vec![origin(1, 1), ph()]]));
        assert_eq!(s[0][1], Orphan);
    }

    #[test]
    fn invalid_spans_are_clamped_not_panicking() {
        // Huge spans, zero spans, ragged rows, overlap.
        let s = resolve_slots(&rows(vec![
            vec![origin(u32::MAX, u32::MAX), ph()],
            vec![ph()],
            vec![origin(0, 0), origin(3, 1), ph(), ph()],
        ]));
        assert_eq!(
            s[0][0],
            Origin {
                colspan: 2,
                rowspan: 3
            }
        );
        assert_eq!(s[0][1], CoveredH { origin_col: 0 });
        assert_eq!(
            s[1][0],
            CoveredV {
                origin_row: 0,
                origin_col: 0
            }
        );
        // (2,0) is an origin despite being "covered" — overlap keeps it as written.
        assert_eq!(
            s[2][0],
            Origin {
                colspan: 1,
                rowspan: 1
            }
        );
        assert_eq!(
            s[2][1],
            Origin {
                colspan: 3,
                rowspan: 1
            }
        );
        assert_eq!(s[2][3], CoveredH { origin_col: 1 });
    }

    #[test]
    fn vmerge_layout_block_span() {
        // A B B
        // C B B
        let rs = rows(vec![
            vec![origin(1, 1), origin(2, 2), ph()],
            vec![origin(1, 1), ph(), ph()],
        ]);
        let l = vmerge_layout(&rs);
        let shape = |r: usize| {
            l[r].iter()
                .map(|m| (m.col, m.span, m.cell.is_some(), m.vmerge))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shape(0),
            vec![(0, 1, true, VMerge::None), (1, 2, true, VMerge::Start)]
        );
        assert_eq!(
            shape(1),
            vec![(0, 1, true, VMerge::None), (1, 2, false, VMerge::Continue)]
        );
    }

    #[test]
    fn empty_section() {
        assert!(resolve_slots(&[]).is_empty());
    }
}
