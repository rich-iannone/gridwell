//! Every position's borders, for formats that draw the rules as cell borders
//! (PPTX, XLSX): the table's own, and the base rules ([`BaseRules`]) where
//! neither the cell nor the position across the edge draws its own.

use gridwell_ir::{BorderStyle, RowRole};

use super::{stronger, BaseRules, EdgePart, LineStyle, OwnEdges, Rule};
use crate::{ResolvedCell, ResolvedTable, Section, SectionKind, Sides, Slot, Stroke};

/// A position's borders. In a right-to-left table the left is the start (drawn
/// on the right): see [`cell_strokes`].
pub type Strokes = Sides<Option<Stroke>>;

/// Which spanner rules are the spanner's bottom border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpannerRules {
    /// Every one: the format can't draw the solid one inset from the cell's ends.
    Borders,
    /// Only one that isn't solid, as in HTML: the solid one is an inset line the
    /// writer draws itself.
    UnlessSolid,
}

/// A rule as a cell border.
pub fn rule_stroke(rule: Rule) -> Stroke {
    Stroke {
        style: match rule.style {
            LineStyle::Solid => BorderStyle::Solid,
            LineStyle::Dashed => BorderStyle::Dashed,
            LineStyle::Dotted => BorderStyle::Dotted,
            LineStyle::Double => BorderStyle::Double,
        },
        width_pt: rule.width_px * 0.75,
        color: Some(rule.color),
    }
}

/// Whether `cell`, in row `r` of `section`, is a spanner label with a rule under
/// it: a spanner row's cell with content, outside the stub.
pub fn is_spanner(section: &Section, r: usize, cell: &ResolvedCell) -> bool {
    section.kind == SectionKind::Head
        && section.rows[r].role == Some(RowRole::SpannerLabel)
        && !cell.content.is_empty()
        && !cell.is_stub
}

/// Every position's borders, a row of `ncols` for each of the table's rows in
/// display order (the head's, then each group's label, data rows and summary
/// rows). A position draws the borders of the cell covering it on the sides that
/// lie on that cell's outline: the cell's own ([`crate::ResolvedStyle::strokes`]
/// of `base_pt`), and with `rules`, the base rules ([`BaseRules::cell`], the
/// stub's right, and the spanner rules `spanners` says) where neither the cell
/// nor the position across the edge draws its own. In a right-to-left table the
/// left and right are the start and end: the strokes are mirrored.
pub fn cell_strokes(
    rt: &ResolvedTable,
    rules: Option<&BaseRules>,
    base_pt: f64,
    spanners: SpannerRules,
) -> Vec<Vec<Strokes>> {
    let ncols = rt.columns.len();
    let rtl = rt.is_rtl();
    // Each position's own borders, and the base rules it would draw.
    let mut own: Vec<Vec<Strokes>> = Vec::new();
    let mut base: Vec<Vec<Strokes>> = Vec::new();
    let section =
        |part: EdgePart, s: &Section, own: &mut Vec<_>, base: &mut Vec<_>| {
            for r in 0..s.rows.len() {
                let (mut o, mut b) = (Vec::with_capacity(ncols), Vec::with_capacity(ncols));
                for col in 0..ncols {
                    let Some((cell, outline)) = s.covering(r, col) else {
                        o.push(Strokes::default());
                        b.push(Strokes::default());
                        continue;
                    };
                    // In a right-to-left table the left and right are the cell's start
                    // and end: the strokes are mirrored, before the mask (a span's start
                    // edge is its outline's left).
                    let strokes = cell.style.strokes(base_pt).mirrored(rtl);
                    let mut rule = Strokes::default();
                    if let Some(rules) = rules {
                        let origin = match s.rows[r].slots.get(col) {
                            Some(Slot::CoveredV { origin_row, .. }) => *origin_row,
                            _ => r,
                        };
                        let edges = OwnEdges {
                            top: strokes.top.is_some(),
                            bottom: strokes.bottom.is_some(),
                            clears_top: cell.style.border_cleared.top,
                        };
                        let (top, bottom) = rules.cell(part, origin, cell.col, cell.rowspan, edges);
                        let mut bottom = bottom;
                        // A spanner rule drawn as the spanner's bottom border, as HTML
                        // draws one that isn't solid.
                        let as_border = |sp: &Rule| match spanners {
                            SpannerRules::Borders => true,
                            SpannerRules::UnlessSolid => sp.style != LineStyle::Solid,
                        };
                        if let Some(sp) = rt.rules.spanners.filter(|sp| {
                            as_border(sp) && !edges.bottom && is_spanner(s, origin, cell)
                        }) {
                            if bottom.is_none_or(|b| stronger(&sp, &b) != Some(false)) {
                                bottom = Some(sp);
                            }
                        }
                        rule.top = top.map(rule_stroke);
                        rule.bottom = bottom.map(rule_stroke);
                        if cell.is_stub && part != EdgePart::Head {
                            rule.right = rt.rules.stub.map(rule_stroke);
                        }
                    }
                    o.push(strokes.masked(outline));
                    b.push(rule.masked(outline));
                }
                own.push(o);
                base.push(b);
            }
        };
    section(EdgePart::Head, &rt.head, &mut own, &mut base);
    for (g, group) in rt.groups.iter().enumerate() {
        if let Some(label) = &group.label {
            // The label spans every column: the first position takes the start
            // edge, the last the end, and every position the top and bottom.
            let strokes = label.style.strokes(base_pt).mirrored(rtl);
            let piece = |c: usize| Sides {
                left: c == 0,
                right: c + 1 == ncols,
                ..Sides::ALL
            };
            own.push((0..ncols).map(|c| strokes.masked(piece(c))).collect());
            let r = own.len() - 1;
            debug_assert!(rules.is_none_or(|rules| rules.label_row(g) == Some(r)));
            let at = |b: usize| rules.and_then(|rules| rules.boundary.get(b).copied().flatten());
            let rule = Strokes {
                top: at(r).map(rule_stroke),
                bottom: at(r + 1).map(rule_stroke),
                ..Strokes::default()
            };
            base.push(vec![rule; ncols]);
        }
        for s in group.sections() {
            section(EdgePart::of(g, s), s, &mut own, &mut base);
        }
    }
    // The table's own borders win: a base rule is drawn only where the position
    // across the edge has no border of its own facing it.
    let mut out = Vec::with_capacity(own.len());
    for (r, (o_row, b_row)) in own.iter().zip(base).enumerate() {
        let row = o_row
            .iter()
            .zip(b_row)
            .enumerate()
            .map(|(c, (o, b))| {
                let across = |side: Option<&Strokes>, get: fn(&Strokes) -> &Option<Stroke>| {
                    side.is_some_and(|s| get(s).is_some())
                };
                let above = r.checked_sub(1).and_then(|r| own[r].get(c));
                let below = own.get(r + 1).and_then(|row| row.get(c));
                let next = o_row.get(c + 1);
                Strokes {
                    top: o
                        .top
                        .clone()
                        .or(b.top.filter(|_| !across(above, |s| &s.bottom))),
                    bottom: o
                        .bottom
                        .clone()
                        .or(b.bottom.filter(|_| !across(below, |s| &s.top))),
                    right: o
                        .right
                        .clone()
                        .or(b.right.filter(|_| !across(next, |s| &s.left))),
                    left: o.left.clone(),
                }
            })
            .collect();
        out.push(row);
    }
    out
}
