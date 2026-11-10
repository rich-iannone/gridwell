//! Which cells draw the section rules on the table's edges.
//!
//! The base look puts a section rule on three edges no cell owns: the table's
//! top, the bottom of the column labels and the table's bottom (in HTML, the
//! borders of the `<table>` and its `<thead>`). A cell's own border on an edge
//! replaces the rule over that cell; in HTML's collapsed-border model the wider
//! border would win instead, so a thin cell border would lose to the rule and the
//! edge would mix the two. Once any cell on an edge sets its own border there, the
//! edge is drawn by its cells: each draws its own border, or *carries* the rule.
//!
//! A cell carries the rule unless its style sets or removes that side, or (under
//! the column labels) a cell across the edge from it sets its own border there.
//! The head's edge is between the head cells reaching its bottom and the row
//! under the head, unless that is a group label: a group label draws rules of its
//! own, so it is never part of an edge. With no body, the head's bottom is the
//! table's.

use std::collections::HashMap;

use crate::{ResolvedCell, ResolvedTable, Section, SectionKind};

/// The part of the table a row is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgePart {
    Head,
    /// Group `g`'s data rows.
    Rows(usize),
    /// Group `g`'s summary rows.
    Summary(usize),
}

impl EdgePart {
    /// The part section `s` of group `g` is (the head, whatever `g`).
    pub fn of(g: usize, s: &Section) -> Self {
        match s.kind {
            SectionKind::Head => EdgePart::Head,
            SectionKind::Body => EdgePart::Rows(g),
            SectionKind::Summary => EdgePart::Summary(g),
        }
    }
}

/// One of the table's edges that its cells may draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    /// The table's top.
    Top,
    /// The bottom of the column labels.
    Head,
    /// The table's bottom (also the head's, when the table has no body).
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Top,
    Bottom,
}

/// The table's edges that its cells draw, and which cells carry the rule there.
/// See the module docs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeRules {
    /// The table's top is drawn by its first row's cells.
    pub top: bool,
    /// The bottom of the column labels is drawn by the cells on either side.
    pub head: bool,
    /// The table's bottom is drawn by its last cells.
    pub bottom: bool,
    /// (part, row in the part, column, side) of each cell carrying the rule, and
    /// the edge it is on.
    carriers: HashMap<(EdgePart, usize, usize, Side), Edge>,
}

/// A cell and where it starts.
struct At<'r, 'a> {
    part: EdgePart,
    row: usize,
    cell: &'r ResolvedCell<'a>,
}

impl At<'_, '_> {
    fn sets(&self, side: Side) -> bool {
        let s = &self.cell.style;
        match side {
            Side::Top => s.border.top.is_some(),
            Side::Bottom => s.border.bottom.is_some(),
        }
    }

    fn clears(&self, side: Side) -> bool {
        let s = &self.cell.style;
        match side {
            Side::Top => s.border_cleared.top,
            Side::Bottom => s.border_cleared.bottom,
        }
    }

    fn overlaps(&self, other: &At) -> bool {
        let (a, b) = (self.cell, other.cell);
        a.col < b.col + b.colspan && b.col < a.col + a.colspan
    }
}

/// The cells starting in a section's first row.
fn first_row<'r, 'a>(part: EdgePart, s: &'r Section<'a>) -> Vec<At<'r, 'a>> {
    s.rows.first().map_or_else(Vec::new, |row| {
        row.cells().map(|cell| At { part, row: 0, cell }).collect()
    })
}

/// The cells reaching a section's bottom.
fn last_row<'r, 'a>(part: EdgePart, s: &'r Section<'a>) -> Vec<At<'r, 'a>> {
    let n = s.rows.len();
    s.rows
        .iter()
        .enumerate()
        .flat_map(|(row, r)| {
            r.cells()
                .filter(move |c| row + c.rowspan >= n)
                .map(move |cell| At { part, row, cell })
        })
        .collect()
}

impl EdgeRules {
    pub fn new(rt: &ResolvedTable) -> Self {
        let mut out = Self::default();
        // The table's rows in order: a section's, or a group label (`None`).
        let mut parts: Vec<Option<(EdgePart, &Section)>> = Vec::new();
        if !rt.head.is_empty() {
            parts.push(Some((EdgePart::Head, &rt.head)));
        }
        for (g, group) in rt.groups.iter().enumerate() {
            if group.label.is_some() {
                parts.push(None);
            }
            for s in group.sections() {
                if !s.is_empty() {
                    parts.push(Some((EdgePart::of(g, s), s)));
                }
            }
        }
        if let Some(&Some((part, s))) = parts.first() {
            out.top = out.outer(&first_row(part, s), Side::Top, Edge::Top);
        }
        if let Some(&Some((part, s))) = parts.last() {
            out.bottom = out.outer(&last_row(part, s), Side::Bottom, Edge::Bottom);
        }
        if parts.len() == 1 && !rt.head.is_empty() {
            // Only column labels: their bottom is the table's.
            out.head = out.bottom;
        } else if !rt.head.is_empty() {
            let upper = last_row(EdgePart::Head, &rt.head);
            let lower = match parts.get(1) {
                Some(&Some((part, s))) => first_row(part, s),
                _ => Vec::new(),
            };
            out.head = out.inner(&upper, &lower);
        }
        out
    }

    /// The table's top or bottom: whether its cells draw it.
    fn outer(&mut self, cells: &[At], side: Side, edge: Edge) -> bool {
        if !cells.iter().any(|c| c.sets(side)) {
            return false;
        }
        for c in cells.iter().filter(|c| !c.sets(side) && !c.clears(side)) {
            self.carriers
                .insert((c.part, c.row, c.cell.col, side), edge);
        }
        true
    }

    /// The head's bottom, between `upper` and `lower` cells.
    fn inner(&mut self, upper: &[At], lower: &[At]) -> bool {
        if !upper.iter().any(|c| c.sets(Side::Bottom)) && !lower.iter().any(|c| c.sets(Side::Top)) {
            return false;
        }
        for (cells, side, across, facing) in [
            (upper, Side::Bottom, lower, Side::Top),
            (lower, Side::Top, upper, Side::Bottom),
        ] {
            for c in cells {
                if !c.sets(side)
                    && !c.clears(side)
                    && !across.iter().any(|a| a.overlaps(c) && a.sets(facing))
                {
                    self.carriers
                        .insert((c.part, c.row, c.cell.col, side), Edge::Head);
                }
            }
        }
        true
    }

    /// Whether the cell starting at `row` and `col` of `part` draws the rule on
    /// its top edge (which is then one its cells draw).
    pub fn carries_top(&self, part: EdgePart, row: usize, col: usize) -> bool {
        self.carried_top(part, row, col).is_some()
    }

    /// Whether the cell starting at `row` and `col` of `part` draws the rule on
    /// its bottom edge.
    pub fn carries_bottom(&self, part: EdgePart, row: usize, col: usize) -> bool {
        self.carried_bottom(part, row, col).is_some()
    }

    /// The edge whose rule the cell starting at `row` and `col` of `part` draws on
    /// its top, if it carries one.
    pub fn carried_top(&self, part: EdgePart, row: usize, col: usize) -> Option<Edge> {
        self.carriers.get(&(part, row, col, Side::Top)).copied()
    }

    /// The edge whose rule the cell draws on its bottom, if it carries one.
    pub fn carried_bottom(&self, part: EdgePart, row: usize, col: usize) -> Option<Edge> {
        self.carriers.get(&(part, row, col, Side::Bottom)).copied()
    }
}

#[cfg(test)]
mod tests;
