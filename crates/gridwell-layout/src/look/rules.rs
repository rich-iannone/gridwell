//! The rules on the table's row boundaries, and which cells draw them.
//!
//! The table's rows are counted in display order: the head's, then each group's
//! label, data rows and summary rows. Boundary `b` is above row `b`; the last is
//! the table's bottom. Each boundary gets the widest rule placed on it:
//!
//! - the table's top and bottom, the head's bottom, and above and below each
//!   group label: [`Look::section_rule`];
//! - below every row outside the head: [`Look::row_rule`];
//! - above a group's first summary row: [`Look::section_rule`], or
//!   [`Look::grand_summary_rule`] for grand summary rows.
//!
//! Who owns each rule follows HTML. Row rules and summary rules are the lower
//! cells' own top border, so a cell style that removes its top border removes them.
//! The table's edges, the head's bottom and the group labels' rules belong to
//! other elements, so they win over a removed border (`none` loses every
//! collapsed-border conflict). An owned boundary is drawn by the cells below it;
//! the others by the cells on both sides. Where a cell sets its own border, that
//! border replaces the rule; on the edges [`EdgeRules`] covers, the edge's cells
//! draw it instead.

use std::collections::HashMap;

use gridwell_ir::RowRole;

use super::{EdgePart, EdgeRules, Look, Rule, LOOK};
use crate::{ResolvedTable, Section};

/// The rule on each row boundary, and how cells draw them. See the module docs.
#[derive(Debug, Clone)]
pub struct BaseRules {
    /// The rule on each boundary (`boundary[b]` is above row `b`), one more than
    /// the table has rows.
    pub boundary: Vec<Option<Rule>>,
    /// The boundary's rule is the cells' below it.
    owned: Vec<bool>,
    /// The boundary is an edge drawn by its cells.
    by_cells: Vec<bool>,
    edges: EdgeRules,
    /// The first row of each part.
    starts: HashMap<EdgePart, usize>,
    /// Each group's label row, if it has a label.
    labels: Vec<Option<usize>>,
}

/// A cell's own border on its top and bottom, for [`BaseRules::cell`].
#[derive(Debug, Clone, Copy, Default)]
pub struct OwnEdges {
    /// The cell's style draws a border on its top.
    pub top: bool,
    pub bottom: bool,
    /// The cell's style removes its top border.
    pub clears_top: bool,
}

/// The wider of a boundary's rule so far and `b`.
fn wider(a: Option<Rule>, b: Rule) -> Option<Rule> {
    match a {
        Some(a) if a.width_px >= b.width_px => Some(a),
        _ => Some(b),
    }
}

impl BaseRules {
    pub fn new(rt: &ResolvedTable) -> Self {
        let look: &Look = &LOOK;
        let mut boundary = vec![Some(look.section_rule)];
        // Boundaries whose summary rule is the summary cells' own.
        let mut summary_owned: Vec<usize> = Vec::new();
        let mut starts = HashMap::new();
        let mut labels = Vec::new();
        // A section's rows and the boundary below each: none inside the head, row
        // rules elsewhere. Section rules go on afterwards.
        let mut add = |part: EdgePart, s: &Section, boundary: &mut Vec<Option<Rule>>| {
            starts.insert(part, boundary.len() - 1);
            let below = (part != EdgePart::Head).then_some(look.row_rule);
            boundary.extend(std::iter::repeat_n(below, s.rows.len()));
        };
        if !rt.head.is_empty() {
            add(EdgePart::Head, &rt.head, &mut boundary);
            *boundary.last_mut().unwrap() = Some(look.section_rule);
        }
        let head_end = boundary.len() - 1;
        for (g, group) in rt.groups.iter().enumerate() {
            labels.push(group.label.as_ref().map(|_| {
                let r = boundary.len() - 1;
                boundary[r] = wider(boundary[r], look.section_rule);
                boundary.push(Some(look.section_rule));
                r
            }));
            add(EdgePart::Rows(g), &group.rows, &mut boundary);
            let first_summary = boundary.len() - 1;
            add(EdgePart::Summary(g), &group.summary_rows, &mut boundary);
            if let Some(row) = group.summary_rows.rows.first() {
                let rule = if row.role == Some(RowRole::GrandSummary) {
                    look.grand_summary_rule
                } else {
                    look.section_rule
                };
                let before = boundary[first_summary];
                boundary[first_summary] = wider(before, rule);
                // The summary cells' own top border, unless a rule from elsewhere won.
                if boundary[first_summary] == Some(rule)
                    && before.is_none_or(|b| b.width_px < rule.width_px)
                {
                    summary_owned.push(first_summary);
                }
            }
        }
        let nrows = boundary.len() - 1;
        // The table's bottom.
        boundary[nrows] = Some(look.section_rule);
        let owned = (0..=nrows)
            .map(|b| {
                (b > 0 && b < nrows && boundary[b] == Some(look.row_rule))
                    || summary_owned.contains(&b)
            })
            .collect();
        let edges = EdgeRules::new(rt);
        let mut by_cells = vec![false; nrows + 1];
        by_cells[0] |= edges.top;
        if head_end > 0 {
            by_cells[head_end] |= edges.head;
        }
        by_cells[nrows] |= edges.bottom;
        Self {
            boundary,
            owned,
            by_cells,
            edges,
            starts,
            labels,
        }
    }

    /// How many rows the table has.
    pub fn rows(&self) -> usize {
        self.boundary.len() - 1
    }

    /// The row `row` of `part` is.
    pub fn row(&self, part: EdgePart, row: usize) -> usize {
        self.starts.get(&part).copied().unwrap_or(0) + row
    }

    /// Group `g`'s label row, if it has a label.
    pub fn label_row(&self, g: usize) -> Option<usize> {
        self.labels.get(g).copied().flatten()
    }

    /// The rules the cell starting at `row` and `col` of `part` and `rowspan` rows
    /// tall draws on its top and bottom (per cell, so they never cross a rowspan),
    /// unless it sets its own border there. On an edge drawn by its cells, the
    /// rule if the cell carries it. Elsewhere, its top: unless the rule is its own
    /// and its style removes it; its bottom: only rules no cell owns (an owned one
    /// is the next cell's top).
    pub fn cell(
        &self,
        part: EdgePart,
        row: usize,
        col: usize,
        rowspan: usize,
        own: OwnEdges,
    ) -> (Option<Rule>, Option<Rule>) {
        let bt = self.row(part, row);
        let bb = bt + rowspan;
        let at = |v: &[bool], b: usize| v.get(b).copied().unwrap_or(false);
        let rule = |b: usize| self.boundary.get(b).copied().flatten();
        let top = if at(&self.by_cells, bt) {
            rule(bt).filter(|_| self.edges.carries_top(part, row, col))
        } else if own.top || (at(&self.owned, bt) && own.clears_top) {
            None
        } else {
            rule(bt)
        };
        let bottom = if at(&self.by_cells, bb) {
            rule(bb).filter(|_| self.edges.carries_bottom(part, row, col))
        } else if own.bottom || at(&self.owned, bb) {
            None
        } else {
            rule(bb)
        };
        (top, bottom)
    }
}

#[cfg(test)]
mod tests;
