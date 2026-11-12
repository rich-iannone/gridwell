//! The table's rules (the default look's, [`gridwell_layout::look`], as
//! `config.rules` changes them), as HTML draws them.
//! The rules on row boundaries and right of the stub become cell borders, so they
//! move with the table. Lines a cell border can't draw are line shapes: the outer
//! rules above the heading and below the notes (which are text boxes of their
//! own), and the solid spanner rules (inset from their cell's ends, so neighboring
//! spanners' rules stay apart; any other spanner rule is the spanner's bottom
//! border, as in HTML).

use std::fmt::Write;

use gridwell_layout::look::{is_spanner, BaseRules, LineStyle, Rule, SpannerRules, LOOK};
use gridwell_layout::ResolvedTable;

use super::{RenderError, BASE_PT};

// A position's borders: `lnL`, `lnR`, `lnT`, `lnB`. In a right-to-left table
// the left is the start (drawn on the right): the writer mirrors them as it
// writes the columns as they are seen.
pub(super) use gridwell_layout::look::Strokes;

/// EMU per px: 0.75pt, 12700 EMU each.
const EMU_PER_PX: f64 = 9525.0;

/// Every position's borders ([`gridwell_layout::look::cell_strokes`]); a solid
/// spanner rule is an inset line ([`spanner_lines`]).
pub(super) fn cell_strokes(rt: &ResolvedTable, rules: Option<&BaseRules>) -> Vec<Vec<Strokes>> {
    gridwell_layout::look::cell_strokes(rt, rules, BASE_PT, SpannerRules::UnlessSolid)
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

/// The solid rule under each spanner label ([`is_spanner`]), at the cell's
/// bottom (above the rule on the boundary), inset
/// [`LOOK.spanner_rule_inset`](gridwell_layout::look::Look::spanner_rule_inset)
/// at each end but the table's edges. The table's top left is at `origin`.
pub(super) fn spanner_lines(
    rt: &ResolvedTable,
    rules: &BaseRules,
    col_widths: &[u32],
    row_heights: &[u32],
    (table_x, table_y): (u32, u32),
) -> Vec<Line> {
    let Some(rule) = rt.rules.spanners.filter(|r| r.style == LineStyle::Solid) else {
        return Vec::new();
    };
    let w = rule_emu(rule);
    let inset = (LOOK.spanner_rule_inset * EMU_PER_PX).round() as u32;
    let ncols = col_widths.len();
    let table_w: u32 = col_widths.iter().sum();
    let edge = |n: usize| col_widths.iter().take(n).sum::<u32>();
    let mut out = Vec::new();
    for (r, row) in rt.head.rows.iter().enumerate() {
        for cell in row.cells() {
            if !is_spanner(&rt.head, r, cell) {
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
    let cmpd = if rule.is_double() {
        " cmpd=\"dbl\""
    } else {
        ""
    };
    let dash = match rule.style {
        LineStyle::Dashed => "<a:prstDash val=\"dash\"/>",
        LineStyle::Dotted => "<a:prstDash val=\"sysDot\"/>",
        LineStyle::Solid | LineStyle::Double => "",
    };
    writeln!(
        buf,
        "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"Rule {id}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>\
         <p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{width}\" cy=\"0\"/></a:xfrm>\
         <a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom>\
         <a:ln w=\"{w}\" cap=\"flat\"{cmpd}><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>{dash}</a:ln>\
         </p:spPr></p:cxnSp>",
        rule.color.to_rrggbb()
    )?;
    Ok(())
}
