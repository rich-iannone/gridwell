//! Hidden columns: mapping the IR's grid columns onto the columns a writer emits.
//!
//! A column with `column_spec[i].hidden == true` must not appear in any output: not
//! its width, not its cells. Writers lay out *visible* columns only, so every cell's
//! position and span has to be projected from grid columns to visible columns. A span
//! that crosses a hidden column shrinks; a cell that lies entirely in hidden columns
//! disappears; a cell whose origin is hidden but whose span reaches a visible column
//! starts at the first visible column it covers.

use crate::ColumnSpec;

/// Which grid columns are visible, and where each lands among the visible columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnVisibility {
    /// `visible_before[c]` = number of visible columns strictly left of grid column
    /// `c`; has `len + 1` entries so ranges can be counted by subtraction.
    visible_before: Vec<usize>,
    hidden: Vec<bool>,
}

impl ColumnVisibility {
    /// From a column spec (one entry per grid column).
    pub fn from_spec(spec: &[ColumnSpec]) -> Self {
        Self::from_hidden(spec.iter().map(|c| c.hidden))
    }

    /// From a sequence of `hidden` flags, one per grid column.
    pub fn from_hidden(hidden: impl IntoIterator<Item = bool>) -> Self {
        let hidden: Vec<bool> = hidden.into_iter().collect();
        let mut visible_before = Vec::with_capacity(hidden.len() + 1);
        let mut n = 0;
        visible_before.push(0);
        for &h in &hidden {
            if !h {
                n += 1;
            }
            visible_before.push(n);
        }
        Self {
            visible_before,
            hidden,
        }
    }

    /// Number of grid columns.
    pub fn grid_len(&self) -> usize {
        self.hidden.len()
    }

    /// Number of visible columns.
    pub fn visible_len(&self) -> usize {
        *self.visible_before.last().unwrap_or(&0)
    }

    /// True if any column is hidden.
    pub fn any_hidden(&self) -> bool {
        self.hidden.iter().any(|&h| h)
    }

    /// Whether grid column `col` is visible (out-of-range columns are not).
    pub fn is_visible(&self, col: usize) -> bool {
        self.hidden.get(col).is_some_and(|&h| !h)
    }

    /// Project a cell occupying grid columns `col .. col + colspan` onto visible
    /// columns: `Some((visible_col, visible_colspan))`, or `None` if every column it
    /// covers is hidden. Ranges are clamped to the grid.
    pub fn project(&self, col: usize, colspan: usize) -> Option<(usize, usize)> {
        let start = col.min(self.grid_len());
        let end = col.saturating_add(colspan.max(1)).min(self.grid_len());
        let first = self.visible_before[start];
        let span = self.visible_before[end] - first;
        (span > 0).then_some((first, span))
    }

    /// Iterate the grid indices of the visible columns, in order.
    pub fn visible_columns(&self) -> impl Iterator<Item = usize> + '_ {
        self.hidden
            .iter()
            .enumerate()
            .filter(|(_, &h)| !h)
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vis(mask: &str) -> ColumnVisibility {
        // "v" = visible, "h" = hidden
        ColumnVisibility::from_hidden(mask.chars().map(|c| c == 'h'))
    }

    #[test]
    fn all_visible_is_identity() {
        let v = vis("vvvv");
        assert_eq!(v.visible_len(), 4);
        assert!(!v.any_hidden());
        for c in 0..4 {
            for s in 1..=4 - c {
                assert_eq!(v.project(c, s), Some((c, s)));
            }
        }
    }

    #[test]
    fn hidden_column_shifts_later_cells_left() {
        let v = vis("vhv");
        assert_eq!(v.visible_len(), 2);
        assert_eq!(v.project(0, 1), Some((0, 1)));
        assert_eq!(v.project(1, 1), None);
        assert_eq!(v.project(2, 1), Some((1, 1)));
        assert_eq!(v.visible_columns().collect::<Vec<_>>(), vec![0, 2]);
    }

    #[test]
    fn spans_shrink_across_hidden_columns() {
        let v = vis("vhvv");
        assert_eq!(v.project(0, 4), Some((0, 3)));
        assert_eq!(v.project(0, 2), Some((0, 1)));
        // Origin hidden, span reaches visible columns: starts at the first visible.
        assert_eq!(v.project(1, 2), Some((1, 1)));
        assert_eq!(v.project(1, 3), Some((1, 2)));
    }

    #[test]
    fn everything_hidden() {
        let v = vis("hhh");
        assert_eq!(v.visible_len(), 0);
        assert_eq!(v.project(0, 3), None);
    }

    #[test]
    fn out_of_range_is_clamped_not_panicking() {
        let v = vis("vv");
        assert_eq!(v.project(1, usize::MAX), Some((1, 1)));
        assert_eq!(v.project(5, 1), None);
        assert_eq!(v.project(0, 0), Some((0, 1)));
        assert!(!v.is_visible(9));
    }

    #[test]
    fn exhaustive_against_naive_projection() {
        // Every mask of up to 6 columns, every (col, span).
        for n in 0..=6usize {
            for bits in 0..(1u32 << n) {
                let hidden: Vec<bool> = (0..n).map(|i| bits & (1 << i) != 0).collect();
                let v = ColumnVisibility::from_hidden(hidden.clone());
                for col in 0..n {
                    for span in 1..=n - col {
                        let covered: Vec<usize> =
                            (col..col + span).filter(|&c| !hidden[c]).collect();
                        let naive = covered.first().map(|&first| {
                            let vis_col = (0..first).filter(|&c| !hidden[c]).count();
                            (vis_col, covered.len())
                        });
                        assert_eq!(
                            v.project(col, span),
                            naive,
                            "mask {hidden:?} col {col} span {span}"
                        );
                    }
                }
            }
        }
    }
}
