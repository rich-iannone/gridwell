//! The rules on the table's row boundaries, and which cells draw them.
//!
//! The table's rows are counted in display order: the head's, then each group's
//! label, data rows and summary rows (the summary rows first when they are placed
//! at the top). Boundary `b` is above row `b`; the last is the table's bottom.
//! The rules ([`TableRules`]) placed on a boundary are those of HTML's base
//! stylesheet:
//!
//! - the table's top and bottom (`top`, `bottom`), the head's bottom
//!   (`column_labels`, unless the table has no groups: then its bottom is the
//!   table's), and above and below each group label (`group`);
//! - above every row outside the head (`rows`) but the first group's first, which
//!   is at the table's top or under the head;
//! - above the first of a run of summary rows (`summary`), or of grand summary
//!   rows (`grand_summary`), instead of the row rule; except for grand summary
//!   rows placed at the top, which open the table;
//! - below the last summary row placed at the top (`summary`, or
//!   `grand_summary` for a grand summary row).
//!
//! Where several meet, the one drawn is the one HTML's collapsed borders draw
//! ([`stronger`]; an equal pair is the upper cell's, then the lower cell's, then
//! the head's, then the table's).
//!
//! Who owns each rule follows HTML. Row rules and the summary rules above summary
//! rows are the lower cells' own top border, so a cell style that removes its top
//! border removes them. The table's edges, the head's bottom, the group labels'
//! rules and those below the summary rows at the top belong to other elements, so
//! they win over a removed border (`none` loses every collapsed-border conflict).
//! An owned boundary is drawn by the cells below it; the others by the cells on
//! both sides. Where a cell sets its own border, that border replaces the rule;
//! on the edges [`EdgeRules`] covers, the edge's cells draw it instead.

use std::collections::HashMap;

use gridwell_ir::RowRole;

use super::{stronger, Edge, EdgePart, EdgeRules, Rule, TableRules};
use crate::{ResolvedTable, SectionKind};

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
    /// The boundary under the head, if the table has one.
    head_end: Option<usize>,
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

/// Whose border a rule on a boundary is, weakest first: of two equal rules, the
/// later's is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Table,
    Head,
    LowerCell,
    UpperCell,
}

/// A row's class in HTML, for where runs of summary rows start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Other,
    Summary,
    Grand,
}

impl BaseRules {
    pub fn new(rt: &ResolvedTable) -> Self {
        let rules: &TableRules = &rt.rules;
        // The rules placed on each boundary, and whose they are.
        let mut placed: Vec<Vec<(Rule, Owner)>> = vec![Vec::new()];
        let mut starts = HashMap::new();
        let mut labels = Vec::new();
        let place = |placed: &mut Vec<Vec<(Rule, Owner)>>, b: usize, rule, owner| {
            if let Some(rule) = rule {
                placed[b].push((rule, owner));
            }
        };
        if !rt.head.is_empty() {
            starts.insert(EdgePart::Head, 0);
            placed.extend(rt.head.rows.iter().map(|_| Vec::new()));
        }
        let head_end = placed.len() - 1;
        for (g, group) in rt.groups.iter().enumerate() {
            // The class of the row above, within the group's tbody.
            let mut above = None;
            if group.label.is_some() {
                let r = placed.len() - 1;
                labels.push(Some(r));
                placed.push(Vec::new());
                place(&mut placed, r, rules.group, Owner::LowerCell);
                place(&mut placed, r + 1, rules.group, Owner::UpperCell);
                above = Some(Class::Other);
            } else {
                labels.push(None);
            }
            for s in group.sections() {
                let part = EdgePart::of(g, s);
                let top = s.kind == SectionKind::Summary && group.summary_top;
                starts.insert(part, placed.len() - 1);
                for (i, row) in s.rows.iter().enumerate() {
                    let r = placed.len() - 1;
                    placed.push(Vec::new());
                    let class = match (s.kind, &row.role) {
                        (SectionKind::Summary, Some(RowRole::GrandSummary)) => Class::Grand,
                        (SectionKind::Summary, _) => Class::Summary,
                        _ => Class::Other,
                    };
                    let starts_run = above != Some(class);
                    let rule = match class {
                        Class::Summary if starts_run => Some(rules.summary),
                        Class::Grand if starts_run && !top => Some(rules.grand_summary),
                        _ if g == 0 && above.is_none() => Some(None),
                        _ => None,
                    };
                    place(&mut placed, r, rule.unwrap_or(rules.rows), Owner::LowerCell);
                    if top && i + 1 == s.rows.len() {
                        let below = match class {
                            Class::Grand => rules.grand_summary,
                            _ => rules.summary,
                        };
                        place(&mut placed, r + 1, below, Owner::UpperCell);
                    }
                    above = Some(class);
                }
            }
        }
        let nrows = placed.len() - 1;
        if !rt.head.is_empty() && !rt.groups.is_empty() {
            place(&mut placed, head_end, rules.column_labels, Owner::Head);
        }
        place(&mut placed, 0, rules.top, Owner::Table);
        place(&mut placed, nrows, rules.bottom, Owner::Table);
        let winners: Vec<Option<(Rule, Owner)>> = placed
            .iter()
            .map(|rules| {
                rules.iter().copied().reduce(|best, next| {
                    let wins = stronger(&next.0, &best.0).unwrap_or(next.1 > best.1);
                    if wins {
                        next
                    } else {
                        best
                    }
                })
            })
            .collect();
        let boundary = winners.iter().map(|w| w.map(|(r, _)| r)).collect();
        // A group label isn't a cell: its rules are drawn by the cells on both sides.
        let owned = winners
            .iter()
            .enumerate()
            .map(|(b, w)| {
                w.is_some_and(|(_, o)| o == Owner::LowerCell) && !labels.contains(&Some(b))
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
            head_end: (!rt.head.is_empty()).then_some(head_end),
        }
    }

    /// The rule on one of the edges its cells may draw ([`EdgeRules`]).
    pub fn edge(&self, edge: Edge) -> Option<Rule> {
        let b = match edge {
            Edge::Top => 0,
            Edge::Head => self.head_end?,
            Edge::Bottom => self.rows(),
        };
        self.boundary[b]
    }

    /// Which edges the cells draw, and which cells carry the rule there.
    pub fn edges(&self) -> &EdgeRules {
        &self.edges
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
