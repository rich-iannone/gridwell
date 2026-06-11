use crate::cell::Row;
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

                // Check overflow right
                if c + colspan > table_cols {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanOverflowRight,
                        section: section.to_string(),
                        row_group,
                        row: Some(r),
                        col: Some(c),
                        message: format!(
                            "Cell at (row={r}, col={c}) has colspan={colspan} but table only has {table_cols} columns (would need col index up to {})",
                            c + colspan - 1
                        ),
                    });
                    continue;
                }

                // Check overflow bottom
                if r + rowspan > num_rows {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanOverflowBottom,
                        section: section.to_string(),
                        row_group,
                        row: Some(r),
                        col: Some(c),
                        message: format!(
                            "Cell at (row={r}, col={c}) has rowspan={rowspan} but section only has {num_rows} rows (would need row index up to {})",
                            r + rowspan - 1
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
