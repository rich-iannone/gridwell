//! The rules of the default look ([`gridwell_layout::look`]), as HTML draws them.
//! The rules on row boundaries and right of the stub become cell borders, so they
//! move with the table. Lines a cell border can't draw are line shapes: the outer
//! rules above the heading and below the notes (which are text boxes of their
//! own), and the spanner rules (inset from their cell's ends, so neighboring
//! spanners' rules stay apart).

use std::fmt::Write;

use gridwell_ir::{BorderStyle, RowRole};
use gridwell_layout::look::{BaseRules, EdgePart, OwnEdges, Rule, LOOK};
use gridwell_layout::{ResolvedTable, Section, Sides, Slot, Stroke};

use super::{RenderError, BASE_PT};

/// A position's borders: `lnL`, `lnR`, `lnT`, `lnB`. In a right-to-left table
/// the left is the start (drawn on the right): the writer mirrors them as it
/// writes the columns as they are seen.
pub(super) type Strokes = Sides<Option<Stroke>>;

/// EMU per px: 0.75pt, 12700 EMU each.
const EMU_PER_PX: f64 = 9525.0;

/// A rule as a cell border.
fn stroke(rule: Rule) -> Stroke {
    Stroke {
        style: if rule.double {
            BorderStyle::Double
        } else {
            BorderStyle::Solid
        },
        width_pt: rule.width_px * 0.75,
        color: Some(rule.color),
    }
}

/// Every position's borders, a row of `ncols` for each of the table's rows in
/// display order (the head's, then each group's label, data rows and summary
/// rows). A position draws the borders of the cell covering it on the sides that
/// lie on that cell's outline: the cell's own, and with `rules`, the base rules
/// ([`BaseRules::cell`], and the stub's right) where neither the cell nor the
/// position across the edge draws its own.
pub(super) fn cell_strokes(rt: &ResolvedTable, rules: Option<&BaseRules>) -> Vec<Vec<Strokes>> {
    let ncols = rt.columns.len();
    let rtl = rt.is_rtl();
    // Each position's own borders, and the base rules it would draw.
    let mut own: Vec<Vec<Strokes>> = Vec::new();
    let mut base: Vec<Vec<Strokes>> = Vec::new();
    let section = |part: EdgePart, s: &Section, own: &mut Vec<_>, base: &mut Vec<_>| {
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
                let strokes = cell.style.strokes(BASE_PT).mirrored(rtl);
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
                    rule.top = top.map(stroke);
                    rule.bottom = bottom.map(stroke);
                    if cell.is_stub && part != EdgePart::Head {
                        rule.right = Some(stroke(LOOK.section_rule));
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
            let strokes = label.style.strokes(BASE_PT).mirrored(rtl);
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
                top: at(r).map(stroke),
                bottom: at(r + 1).map(stroke),
                ..Strokes::default()
            };
            base.push(vec![rule; ncols]);
        }
        section(EdgePart::Rows(g), &group.rows, &mut own, &mut base);
        section(
            EdgePart::Summary(g),
            &group.summary_rows,
            &mut own,
            &mut base,
        );
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

/// A straight line: from `x` to `x + width` at `y` (its center), in EMU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Line {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub rule: Rule,
}

/// A rule's width in EMU.
pub(super) fn rule_emu(rule: Rule) -> u32 {
    (rule.width_px * EMU_PER_PX).round() as u32
}

/// The rule under each spanner label: a spanner row's cells with content, outside
/// the stub, at the cell's bottom (above the rule on the boundary), inset
/// [`LOOK.spanner_rule_inset`](gridwell_layout::look::Look::spanner_rule_inset)
/// at each end but the table's edges. The table's top left is at `origin`.
pub(super) fn spanner_lines(
    rt: &ResolvedTable,
    rules: &BaseRules,
    col_widths: &[u32],
    row_heights: &[u32],
    (table_x, table_y): (u32, u32),
) -> Vec<Line> {
    let rule = LOOK.spanner_rule;
    let w = rule_emu(rule);
    let inset = (LOOK.spanner_rule_inset * EMU_PER_PX).round() as u32;
    let ncols = col_widths.len();
    let table_w: u32 = col_widths.iter().sum();
    let edge = |n: usize| col_widths.iter().take(n).sum::<u32>();
    let mut out = Vec::new();
    for (r, row) in rt.head.rows.iter().enumerate() {
        if row.role != Some(RowRole::SpannerLabel) {
            continue;
        }
        for cell in row.cells() {
            if cell.content.is_empty() || cell.is_stub {
                continue;
            }
            let end = cell.col + cell.colspan;
            let mut x0 = edge(cell.col);
            let mut x1 = edge(end);
            if cell.col > 0 {
                x0 += inset;
            }
            if end < ncols {
                x1 = x1.saturating_sub(inset);
            }
            if x1 <= x0 {
                continue;
            }
            // Right to left, the first column is on the right.
            let x = if rt.is_rtl() { table_w - x1 } else { x0 };
            let b = r + cell.rowspan;
            let bottom = row_heights
                .iter()
                .take(b)
                .fold(table_y, |a, h| a.saturating_add(*h));
            // Clear of the line on the boundary: the base rule, or the cell's own.
            let below = rules.boundary.get(b).copied().flatten().map_or(0, rule_emu);
            let own = cell
                .style
                .strokes(BASE_PT)
                .bottom
                .map_or(0, |k| (k.width_pt * 12700.0).round() as u32);
            let y = bottom.saturating_sub(below.max(own) / 2 + w / 2);
            out.push(Line {
                x: table_x + x,
                y,
                width: x1 - x0,
                rule,
            });
        }
    }
    out
}

/// A line shape (a connector, as PowerPoint draws lines) with flat ends, so it is
/// exactly as long as `line` says.
pub(super) fn write_line(buf: &mut String, id: u32, line: &Line) -> Result<(), RenderError> {
    let Line { x, y, width, rule } = *line;
    let w = rule_emu(rule);
    let cmpd = if rule.double { " cmpd=\"dbl\"" } else { "" };
    writeln!(
        buf,
        "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"Rule {id}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>\
         <p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{width}\" cy=\"0\"/></a:xfrm>\
         <a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom>\
         <a:ln w=\"{w}\" cap=\"flat\"{cmpd}><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>\
         </p:spPr></p:cxnSp>",
        rule.color.to_rrggbb()
    )?;
    Ok(())
}
