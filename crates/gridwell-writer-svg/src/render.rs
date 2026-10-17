//! IR → SVG.
//!
//! SVG has no layout engine, so this module is one: it measures text (real font
//! metrics, see [`crate::measure`]), sizes columns and rows to fit, wraps text, and
//! positions every glyph run and rule. Rendering is two passes: [`layout`] computes
//! boxes, lines and rules; [`emit`] writes them out. Keeping the geometry in a plain
//! struct lets tests check layout invariants (no text outside its cell, cells tile
//! the table, everything inside the canvas) directly.
//!
//! The look is gridwell's default ([`gridwell_layout::look`], shared with the HTML
//! base stylesheet): rules where HTML's collapsed borders put them, the same
//! padding, sizes and colors. Rules sit on row and column boundaries; a boundary
//! takes the widest rule meeting there, and each cell's box includes half of each
//! inner boundary's rule (all of an outer one), so cells still tile the table.

use std::fmt::Write;

use gridwell_core::xml::escape as escape_xml;
use gridwell_core::Length;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, RowRole, Table};
use gridwell_layout::look::{Pad, Rule as LookRule, LOOK};
use gridwell_layout::{resolve, ResolvedBorder, ResolvedStyle, ResolvedTable, SectionKind, Sides};
use thiserror::Error;

use crate::measure::{max_width, wrap, Line, Run, RunStyle, SUP_SCALE};
use crate::SvgConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Line advance as a multiple of the font size: Roboto's normal line height
/// ((ascender − descender) / units per em = 2400 / 2048), what HTML's
/// `line-height: normal` gives in Roboto.
pub const LINE_HEIGHT: f64 = 2400.0 / 2048.0;
/// Roboto's ascender in em: the baseline's distance below the top of a line box.
const ASCENT: f64 = 1900.0 / 2048.0;
/// Heading and note lines wrap only past this many `default_col_width`s (or the
/// table's width, if wider): about a page, where HTML would wrap them.
const WRAP_CAP_COLUMNS: f64 = 8.0;

pub fn render(table: &Table, config: &SvgConfig) -> Result<String, RenderError> {
    emit(&layout(table, config), config)
}

// ─────────────────────────────── geometry ────────────────────────────────

/// Horizontal alignment of text within its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Vertical alignment of text within its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Middle,
    /// Column labels and spanners, as in HTML's base stylesheet.
    Bottom,
}

/// A block of wrapped text placed in a box.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    pub lines: Vec<Line>,
    pub font_size: f64,
    pub align: Align,
    pub valign: VAlign,
    /// The box the text must stay inside (already inset by padding and rules).
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One table cell (or full-width group label) after layout.
#[derive(Debug, Clone, PartialEq)]
pub struct CellBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: Option<String>,
    pub text: TextBlock,
}

/// A straight rule: horizontal (`y1 == y2`) or vertical (`x1 == x2`), centered on
/// the line between its end points.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleLine {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    /// Thickness in px.
    pub width: f64,
    pub color: String,
    /// Two lines a third of `width` thick, a third apart.
    pub double: bool,
    /// Dash and gap length in px (dashed and dotted borders); `None`: continuous.
    pub dash: Option<f64>,
}

/// The whole table after layout.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    /// Title, subtitle, extra lines (above the table).
    pub header: Vec<TextBlock>,
    pub cells: Vec<CellBox>,
    /// The table's own rectangle.
    pub table_x: f64,
    pub table_y: f64,
    pub table_width: f64,
    pub table_height: f64,
    /// Footnotes and source notes (below the table).
    pub footer: Vec<TextBlock>,
    pub rules: Vec<RuleLine>,
}

// ──────────────────────────────── layout ─────────────────────────────────

/// What a cell is, for its padding and rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// A column label (or an empty cell in a spanner row).
    Label,
    /// A spanner label.
    Spanner,
    /// A data, stub or summary cell.
    Body,
}

/// A cell before column widths are known.
struct PendingCell {
    row: usize,
    col: usize,
    colspan: usize,
    rowspan: usize,
    runs: Vec<Vec<Run>>,
    natural: f64,
    align: Align,
    fill: Option<String>,
    part: Part,
    pad: Pad,
    /// A stub cell outside the head: draws the stub's right border.
    stub: bool,
    /// The cell's own style removes its top border.
    clears_top: bool,
    /// The cell's own borders (from its style), which replace the base rules.
    borders: Sides<Option<RuleLine>>,
}

/// A style's border as a rule template (position filled in when drawn): `None` for
/// unset, removed or unknown sides.
fn ir_border(b: &Option<ResolvedBorder>, text_color: &str) -> Option<RuleLine> {
    let b = b.as_ref()?;
    let width = b.width.as_ref().and_then(px_or_zero).unwrap_or(1.0);
    if width <= 0.0 {
        return None;
    }
    let color = b
        .color
        .map_or_else(|| text_color.to_string(), |c| c.to_css());
    let (double, dash) = match b.style {
        BorderStyle::Solid => (false, None),
        BorderStyle::Double => (true, None),
        BorderStyle::Dashed => (false, Some(width * 3.0)),
        BorderStyle::Dotted => (false, Some(width)),
        _ => return None,
    };
    Some(RuleLine {
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        width,
        color,
        double,
        dash,
    })
}

/// A cell's padding: the part's, with any side the cell's style sets.
fn styled_pad(mut pad: Pad, style: &ResolvedStyle) -> Pad {
    let p = &style.padding;
    for (side, v) in [
        (&mut pad.top, &p.top),
        (&mut pad.right, &p.right),
        (&mut pad.bottom, &p.bottom),
        (&mut pad.left, &p.left),
    ] {
        if let Some(px) = v.as_ref().and_then(px_or_zero) {
            *side = px;
        }
    }
    pad
}

/// A full-width row (group label) before layout.
struct PendingBand {
    row: usize,
    runs: Vec<Vec<Run>>,
    fill: Option<String>,
}

/// The rule on a boundary between rows (or at the table's top or bottom).
fn wider(a: Option<LookRule>, b: LookRule) -> Option<LookRule> {
    match a {
        Some(a) if a.width_px >= b.width_px => Some(a),
        _ => Some(b),
    }
}

/// Compute the geometry of every box, line and rule. See the module docs.
pub fn layout(table: &Table, config: &SvgConfig) -> Layout {
    let fs = config.font_size;
    let look = &LOOK;
    let base = config.base_styles;
    let body_pad = Pad {
        top: config.cell_padding_y,
        right: config.cell_padding_x,
        bottom: config.cell_padding_y,
        left: config.cell_padding_x,
    };
    let rt = resolve(table);
    let rt = &rt;
    let ncols = rt.columns.len();

    // 1. Collect cells and bands in display order, assigning global row indices, and
    //    the rule on every row boundary (`hrule[r]` is above row `r`).
    let mut cells: Vec<PendingCell> = Vec::new();
    let mut bands: Vec<PendingBand> = Vec::new();
    let mut hrule: Vec<Option<LookRule>> = vec![None];
    // Boundaries whose summary rule is the summary cells' own (see `owned` below).
    let mut summary_owned: Vec<usize> = Vec::new();
    let mut nrows = 0usize;
    // The rule a boundary gets by default: hairlines between body rows.
    let next_rule = |hrule: &mut Vec<Option<LookRule>>, r: Option<LookRule>| hrule.push(r);

    let add_section = |section: &gridwell_layout::Section,
                       nrows: &mut usize,
                       cells: &mut Vec<PendingCell>,
                       hrule: &mut Vec<Option<LookRule>>| {
        let head = section.kind == SectionKind::Head;
        for (r, row) in section.rows.iter().enumerate() {
            let spanner_row = head && row.role == Some(RowRole::SpannerLabel);
            for cell in row.cells() {
                let mut style = RunStyle::default();
                apply_style(&mut style, &cell.style, fs);
                let runs = content_runs(cell.content, &style, rt, fs, base);
                let natural = paragraphs_width(&runs, fs);
                let part = if spanner_row && !cell.content.is_empty() && !cell.is_stub {
                    Part::Spanner
                } else if head {
                    Part::Label
                } else {
                    Part::Body
                };
                let mut pad = match part {
                    Part::Spanner => look.spanner_pad,
                    Part::Label => look.head_cell_pad,
                    Part::Body => body_pad,
                };
                if part == Part::Spanner {
                    // No outer padding at the table's edges.
                    if cell.col == 0 {
                        pad.left = 0.0;
                    }
                    if cell.col + cell.colspan == ncols {
                        pad.right = 0.0;
                    }
                }
                let pad = styled_pad(pad, &cell.style);
                let text_color = cell
                    .style
                    .paint()
                    .map_or_else(|| LOOK.text_color.to_css(), |c| c.to_css());
                let b = &cell.style.border;
                let borders = Sides {
                    top: ir_border(&b.top, &text_color),
                    right: ir_border(&b.right, &text_color),
                    bottom: ir_border(&b.bottom, &text_color),
                    left: ir_border(&b.left, &text_color),
                };
                cells.push(PendingCell {
                    row: *nrows + r,
                    col: cell.col,
                    colspan: cell.colspan,
                    rowspan: cell.rowspan,
                    runs,
                    natural,
                    align: svg_align(&cell.align),
                    fill: cell.style.fill().map(|c| c.to_css()),
                    part,
                    pad,
                    stub: cell.is_stub && !head,
                    clears_top: cell.style.border_cleared.top,
                    borders,
                });
            }
            // The boundary below this row: none inside the head, hairlines between
            // body rows; section rules are applied afterwards.
            let below = if head { None } else { Some(look.row_rule) };
            hrule.push(below);
            let _ = &row;
        }
        *nrows += section.rows.len();
    };

    hrule[0] = Some(look.section_rule);
    if !rt.head.is_empty() {
        add_section(&rt.head, &mut nrows, &mut cells, &mut hrule);
        hrule[nrows] = wider(None, look.section_rule);
    }
    for group in &rt.groups {
        if let Some(label) = &group.label {
            let mut style = RunStyle::default();
            apply_style(&mut style, &label.style, fs);
            let r = nrows;
            hrule[r] = wider(hrule[r], look.section_rule);
            bands.push(PendingBand {
                row: r,
                runs: content_runs(label.content, &style, rt, fs, base),
                fill: label.style.fill().map(|c| c.to_css()),
            });
            nrows += 1;
            next_rule(&mut hrule, Some(look.section_rule));
        }
        add_section(&group.rows, &mut nrows, &mut cells, &mut hrule);
        let first_summary = nrows;
        add_section(&group.summary_rows, &mut nrows, &mut cells, &mut hrule);
        if let Some(row) = group.summary_rows.rows.first() {
            let rule = if row.role == Some(RowRole::GrandSummary) {
                look.grand_summary_rule
            } else {
                look.section_rule
            };
            let before = hrule[first_summary];
            hrule[first_summary] = wider(before, rule);
            // The summary cells' own top border, unless a rule from elsewhere won.
            if hrule[first_summary] == Some(rule)
                && before.is_none_or(|b| b.width_px < rule.width_px)
            {
                summary_owned.push(first_summary);
            }
        }
    }
    // The table's bottom.
    if let Some(last) = hrule.last_mut() {
        *last = Some(look.section_rule);
    }
    debug_assert_eq!(hrule.len(), nrows + 1);
    // Who owns each boundary's rule, as in HTML: hairlines and summary rules are the
    // lower cells' own top border, so a cell style removing its top border removes
    // them; the table's edges, the head's bottom and group labels' rules belong to
    // other elements and win over a removed border (`none` loses every collapsed-
    // border conflict). Owned boundaries are drawn by the cells below; others by the
    // cells on both sides.
    let owned: Vec<bool> = (0..=nrows)
        .map(|b| {
            (b > 0 && b < nrows && hrule[b] == Some(look.row_rule)) || summary_owned.contains(&b)
        })
        .collect();
    if !base {
        hrule.iter_mut().for_each(|r| *r = None);
    }
    let rule_w = |r: &Option<LookRule>| r.map_or(0.0, |r| r.width_px);
    // Each boundary is as thick as the widest rule or cell border on it.
    let mut hthick: Vec<f64> = hrule.iter().map(rule_w).collect();
    // Vertical boundaries (`vthick[v]` is left of visible column `v`): the stub's
    // right border and cell borders.
    let mut vthick = vec![0.0f64; ncols + 1];
    for c in &cells {
        if base && c.stub {
            let v = c.col + c.colspan;
            vthick[v] = vthick[v].max(look.section_rule.width_px);
        }
        let w = |b: &Option<RuleLine>| b.as_ref().map_or(0.0, |b| b.width);
        hthick[c.row] = hthick[c.row].max(w(&c.borders.top));
        let below = c.row + c.rowspan;
        hthick[below] = hthick[below].max(w(&c.borders.bottom));
        vthick[c.col] = vthick[c.col].max(w(&c.borders.left));
        let right = c.col + c.colspan;
        vthick[right] = vthick[right].max(w(&c.borders.right));
    }
    // A boundary's share of a row or column: all of an outer rule, half of an inner.
    let share = |b: usize| {
        if b == 0 || b == nrows {
            hthick[b]
        } else {
            hthick[b] / 2.0
        }
    };
    let vshare = |v: usize| {
        if v == 0 || v == ncols {
            vthick[v]
        } else {
            vthick[v] / 2.0
        }
    };

    // 2. Column widths: fixed where the spec gives an absolute width, otherwise fit
    //    the widest single-column cell; clamp to min/max; then grow for spanners.
    // No minimum beyond each cell's own content and padding (as in HTML).
    let min_col = 0.0;
    let max_of = |v: usize| rt.columns[v].max_width.as_ref().and_then(px);
    let mut widths = vec![0.0f64; ncols];
    let mut fixed = vec![false; ncols];
    for (v, col) in rt.columns.iter().enumerate() {
        if let Some(w) = col.width.as_ref().and_then(px) {
            widths[v] = w;
            fixed[v] = true;
        }
    }
    let need_w = |c: &PendingCell| {
        c.natural + c.pad.left + c.pad.right + vshare(c.col) + vshare(c.col + c.colspan)
    };
    for cell in cells.iter().filter(|c| c.colspan == 1) {
        if !fixed[cell.col] {
            widths[cell.col] = widths[cell.col].max(need_w(cell));
        }
    }
    for (v, col) in rt.columns.iter().enumerate() {
        let lo = col.min_width.as_ref().and_then(px).unwrap_or(min_col);
        let hi = max_of(v).unwrap_or(f64::INFINITY).max(lo);
        widths[v] = widths[v].max(lo).min(hi);
    }
    let grow = |widths: &mut [f64], range: std::ops::Range<usize>, need: f64| {
        let have: f64 = widths[range.clone()].iter().sum();
        if need <= have {
            return;
        }
        let growable: Vec<usize> = range
            .clone()
            .filter(|&v| !fixed[v] && max_of(v).is_none_or(|hi| widths[v] < hi))
            .collect();
        let targets = if growable.is_empty() {
            return;
        } else {
            growable
        };
        let share = (need - have) / targets.len() as f64;
        for v in targets {
            widths[v] += share;
        }
    };
    for cell in cells.iter().filter(|c| c.colspan > 1) {
        grow(&mut widths, cell.col..cell.col + cell.colspan, need_w(cell));
    }
    let band_need = bands
        .iter()
        .map(|b| paragraphs_width(&b.runs, fs) + body_pad.left + body_pad.right)
        .fold(0.0, f64::max);
    grow(&mut widths, 0..ncols, band_need);

    // 3. Heading and footer text, measured before the table width is final: the
    //    table is as wide as its widest heading or note line (as in HTML, where they
    //    share the table's box). Lines wrap only past a cap, so an image doesn't
    //    grow without bound.
    let h = &rt.header;
    let heading_parts = h
        .title
        .iter()
        .map(|t| (t, look.title_scale, look.title_pad))
        .chain(
            h.subtitle
                .iter()
                .map(|t| (t, look.subtitle_scale, look.subtitle_pad)),
        )
        .chain(
            h.extra_lines
                .iter()
                .map(|t| (t, look.header_line_scale, look.header_line_pad)),
        );
    let heading: Vec<(Vec<Vec<Run>>, f64, Pad)> = heading_parts
        .map(|(line, scale, pad)| {
            let size = fs * scale;
            let mut style = RunStyle::default();
            apply_style(&mut style, &line.style, size);
            (
                content_runs(line.content, &style, rt, size, base),
                size,
                pad,
            )
        })
        .collect();
    let note_size = fs * look.note_scale;
    let mark_style = RunStyle {
        sup: true,
        italic: base,
        ..Default::default()
    };
    let notes: Vec<Vec<Vec<Run>>> = {
        let footer = &rt.footer;
        let footnotes = footer.footnotes.iter().map(|n| {
            let mut paragraphs = content_runs(n.content, &RunStyle::default(), rt, note_size, base);
            let mut first = vec![
                Run {
                    text: n.mark.to_string(),
                    style: mark_style.clone(),
                },
                Run {
                    text: " ".into(),
                    style: RunStyle::default(),
                },
            ];
            first.append(&mut paragraphs[0]);
            paragraphs[0] = first;
            paragraphs
        });
        let sources = footer
            .source_notes
            .iter()
            .map(|n| content_runs(n.content, &RunStyle::default(), rt, note_size, base));
        footnotes.chain(sources).collect()
    };
    let columns_width: f64 = widths.iter().sum();
    let cap = columns_width.max(config.default_col_width * WRAP_CAP_COLUMNS);
    let text_need = |paragraphs: &[Vec<Run>], size: f64, pad: &Pad| {
        let lines = wrap_paragraphs(paragraphs, size, cap - pad.left - pad.right);
        max_width(&lines) + pad.left + pad.right
    };
    // An absolute `config.table_width` is a minimum, as CSS `width` on a table.
    let table_min = table
        .config
        .table_width
        .as_deref()
        .and_then(|w| w.parse::<Length>().ok())
        .as_ref()
        .and_then(px)
        .unwrap_or(0.0);
    let outer_need = heading
        .iter()
        .map(|(p, size, pad)| text_need(p, *size, pad))
        .chain(
            notes
                .iter()
                .map(|p| text_need(p, note_size, &look.note_pad)),
        )
        .fold(table_min, f64::max);
    if outer_need > columns_width + 1e-9 && ncols > 0 {
        // Spread the extra width over the auto columns in proportion to their width
        // (as a browser does for a `width: 100%` table); all columns if none is auto.
        let auto: Vec<usize> = (0..ncols).filter(|&v| !fixed[v]).collect();
        let targets = if auto.is_empty() {
            (0..ncols).collect()
        } else {
            auto
        };
        let total: f64 = targets.iter().map(|&v| widths[v]).sum();
        let extra = outer_need - columns_width;
        for &v in &targets {
            widths[v] += if total > 0.0 {
                extra * widths[v] / total
            } else {
                extra / targets.len() as f64
            };
        }
    }
    let col_x = prefix_sums(0.0, &widths);
    // With no visible columns there's nothing to widen: the (empty) table takes the
    // heading's and notes' width.
    let table_width = col_x[ncols].max(outer_need);

    // 4. Wrap cell text to the final widths, then size rows to fit.
    let cell_height = |lines: &[Line]| block_height(lines, fs);
    let inner_w = |c: &PendingCell| {
        col_x[c.col + c.colspan]
            - col_x[c.col]
            - c.pad.left
            - c.pad.right
            - vshare(c.col)
            - vshare(c.col + c.colspan)
    };
    let wrapped: Vec<Vec<Line>> = cells
        .iter()
        .map(|c| wrap_paragraphs(&c.runs, fs, inner_w(c)))
        .collect();
    let band_lines: Vec<Vec<Line>> = bands
        .iter()
        .map(|b| {
            wrap_paragraphs(
                &b.runs,
                fs,
                (table_width - body_pad.left - body_pad.right).max(fs),
            )
        })
        .collect();

    let need_h = |row: usize, rowspan: usize, pad: &Pad, lines: &[Line]| {
        cell_height(lines) + pad.top + pad.bottom + share(row) + share(row + rowspan)
    };
    let mut heights = vec![config.row_height; nrows];
    for (cell, lines) in cells.iter().zip(&wrapped) {
        if cell.rowspan == 1 {
            heights[cell.row] = heights[cell.row].max(need_h(cell.row, 1, &cell.pad, lines));
        }
    }
    for (band, lines) in bands.iter().zip(&band_lines) {
        heights[band.row] = heights[band.row].max(need_h(band.row, 1, &body_pad, lines));
    }
    for (cell, lines) in cells.iter().zip(&wrapped) {
        if cell.rowspan > 1 {
            let last = cell.row + cell.rowspan - 1;
            let have: f64 = heights[cell.row..=last].iter().sum();
            let need = need_h(cell.row, cell.rowspan, &cell.pad, lines);
            if need > have {
                heights[last] += need - have;
            }
        }
    }

    // 5. The heading, centered over the table, under the outer rule.
    let mut out = Layout {
        table_width,
        ..Layout::default()
    };
    let outer = look.outer_rule;
    let mut y = 0.0;
    if base && !heading.is_empty() {
        out.rules
            .push(hline(0.0, table_width, outer.width_px / 2.0, &outer));
        y = outer.width_px;
    }
    for (paragraphs, size, pad) in &heading {
        let lines = wrap_paragraphs(paragraphs, *size, table_width - pad.left - pad.right);
        let h = block_height(&lines, *size);
        out.header.push(TextBlock {
            lines,
            font_size: *size,
            align: Align::Center,
            valign: VAlign::Middle,
            x: pad.left,
            y: y + pad.top,
            width: table_width - pad.left - pad.right,
            height: h,
        });
        y += pad.top + h + pad.bottom;
    }

    // 6. Place cells and bands, and their rules.
    let row_y = prefix_sums(y, &heights);
    out.table_y = y;
    out.table_height = row_y[nrows] - y;
    // The center of a boundary's rule.
    let rule_center = |b: usize| {
        let w = hthick[b];
        if b == 0 {
            row_y[0] + w / 2.0
        } else if b == nrows {
            row_y[nrows] - w / 2.0
        } else {
            row_y[b]
        }
    };
    for (cell, lines) in cells.iter().zip(wrapped) {
        let (x, w) = (
            col_x[cell.col],
            col_x[cell.col + cell.colspan] - col_x[cell.col],
        );
        let (cy, h) = (
            row_y[cell.row],
            row_y[cell.row + cell.rowspan] - row_y[cell.row],
        );
        let (top, bottom) = (share(cell.row), share(cell.row + cell.rowspan));
        let (left, right) = (vshare(cell.col), vshare(cell.col + cell.colspan));
        let pad = cell.pad;
        out.cells.push(CellBox {
            x,
            y: cy,
            width: w,
            height: h,
            fill: cell.fill.clone(),
            text: TextBlock {
                lines,
                font_size: fs,
                align: cell.align,
                valign: if cell.part == Part::Body {
                    VAlign::Middle
                } else {
                    VAlign::Bottom
                },
                x: x + left + pad.left,
                y: cy + top + pad.top,
                width: w - left - right - pad.left - pad.right,
                height: h - top - bottom - pad.top - pad.bottom,
            },
        });
        // The cell's own borders, which replace the base rules on their edges.
        let vcenter = |v: usize| {
            if v == 0 {
                col_x[0] + vthick[0] / 2.0
            } else if v == ncols {
                col_x[ncols] - vthick[ncols] / 2.0
            } else {
                col_x[v]
            }
        };
        let place = |t: &RuleLine, x1: f64, y1: f64, x2: f64, y2: f64| RuleLine {
            x1,
            y1,
            x2,
            y2,
            ..t.clone()
        };
        let (bt, bb) = (cell.row, cell.row + cell.rowspan);
        let (vl, vr) = (cell.col, cell.col + cell.colspan);
        if let Some(t) = &cell.borders.top {
            out.rules
                .push(place(t, x, rule_center(bt), x + w, rule_center(bt)));
        }
        if let Some(t) = &cell.borders.bottom {
            out.rules
                .push(place(t, x, rule_center(bb), x + w, rule_center(bb)));
        }
        if let Some(t) = &cell.borders.left {
            out.rules
                .push(place(t, vcenter(vl), cy, vcenter(vl), cy + h));
        }
        if let Some(t) = &cell.borders.right {
            out.rules
                .push(place(t, vcenter(vr), cy, vcenter(vr), cy + h));
        }
        if !base {
            continue;
        }
        // The base rules on the cell's edges (drawn per cell, so they never cross a
        // rowspan), unless it sets its own border there. Its top: unless the rule is
        // its own and its style removes it. Its bottom: only rules no cell owns (an
        // owned one is the next cell's top).
        if let (Some(rule), false) = (hrule[bt], cell.borders.top.is_some()) {
            if !(owned[bt] && cell.clears_top) {
                out.rules.push(hline(x, x + w, rule_center(bt), &rule));
            }
        }
        if let (Some(rule), false, false) = (hrule[bb], cell.borders.bottom.is_some(), owned[bb]) {
            out.rules.push(hline(x, x + w, rule_center(bb), &rule));
        }
        if cell.stub && cell.borders.right.is_none() {
            let rx = vcenter(vr);
            let rule = look.section_rule;
            out.rules.push(RuleLine {
                x1: rx,
                y1: cy,
                x2: rx,
                y2: cy + h,
                width: rule.width_px,
                color: rule.color.to_css(),
                double: false,
                dash: None,
            });
        }
        if cell.part == Part::Spanner {
            let rule = look.spanner_rule;
            let inset = look.spanner_rule_inset;
            let x1 = if cell.col == 0 { x } else { x + inset };
            let x2 = if cell.col + cell.colspan == ncols {
                x + w
            } else {
                x + w - inset
            };
            let ry = cy + h - share(cell.row + cell.rowspan) - rule.width_px / 2.0;
            out.rules.push(hline(x1, x2, ry, &rule));
        }
    }
    for (band, lines) in bands.iter().zip(band_lines) {
        let (cy, h) = (row_y[band.row], heights[band.row]);
        let (top, bottom) = (share(band.row), share(band.row + 1));
        out.cells.push(CellBox {
            x: 0.0,
            y: cy,
            width: table_width,
            height: h,
            fill: band.fill.clone(),
            text: TextBlock {
                lines,
                font_size: fs,
                align: Align::Left,
                valign: VAlign::Middle,
                x: body_pad.left,
                y: cy + top + body_pad.top,
                width: table_width - body_pad.left - body_pad.right,
                height: h - top - bottom - body_pad.top - body_pad.bottom,
            },
        });
        if base {
            for b in [band.row, band.row + 1] {
                if let Some(rule) = hrule[b] {
                    out.rules
                        .push(hline(0.0, table_width, rule_center(b), &rule));
                }
            }
        }
    }

    // 7. Footnotes and source notes below the table, over the outer rule.
    let mut y = row_y[nrows];
    let pad = look.note_pad;
    for paragraphs in &notes {
        let lines = wrap_paragraphs(paragraphs, note_size, table_width - pad.left - pad.right);
        let h = block_height(&lines, note_size);
        out.footer.push(TextBlock {
            lines,
            font_size: note_size,
            align: Align::Left,
            valign: VAlign::Middle,
            x: pad.left,
            y: y + pad.top,
            width: table_width - pad.left - pad.right,
            height: h,
        });
        y += pad.top + h + pad.bottom;
    }
    if base && !notes.is_empty() {
        out.rules
            .push(hline(0.0, table_width, y + outer.width_px / 2.0, &outer));
        y += outer.width_px;
    }

    // A boundary between two cells is drawn by both (the bottom edge of one, the
    // top of the other): keep one of each identical rule.
    let mut seen = std::collections::HashSet::new();
    out.rules.retain(|r| {
        let bits = |v: f64| v.to_bits();
        seen.insert((
            [
                bits(r.x1),
                bits(r.y1),
                bits(r.x2),
                bits(r.y2),
                bits(r.width),
            ],
            r.color.clone(),
            r.double,
            r.dash.map(bits),
        ))
    });

    // 8. Canvas: the table's width (everything else is laid out within it).
    out.width = table_width.ceil();
    out.height = y.ceil();
    out
}

fn hline(x1: f64, x2: f64, y: f64, rule: &LookRule) -> RuleLine {
    RuleLine {
        x1,
        y1: y,
        x2,
        y2: y,
        width: rule.width_px,
        color: rule.color.to_css(),
        double: rule.double,
        dash: None,
    }
}

/// A length in px, zero allowed (padding, border widths).
fn px_or_zero(l: &Length) -> Option<f64> {
    match l {
        &Length::Px(0.0) => Some(0.0),
        l => px(l),
    }
}

/// `[start, start + v0, start + v0 + v1, …]` (one more entry than `values`).
fn prefix_sums(start: f64, values: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(values.len() + 1);
    let mut acc = start;
    out.push(acc);
    for v in values {
        acc += v;
        out.push(acc);
    }
    out
}

/// Lay out each explicit paragraph (split at `line_break`) and concatenate.
fn wrap_paragraphs(paragraphs: &[Vec<Run>], font_size: f64, width: f64) -> Vec<Line> {
    paragraphs
        .iter()
        .flat_map(|p| wrap(p, font_size, width))
        .collect()
}

fn paragraphs_width(paragraphs: &[Vec<Run>], font_size: f64) -> f64 {
    max_width(&wrap_paragraphs(paragraphs, font_size, f64::INFINITY))
}

/// Convert content nodes to paragraphs of styled runs (`line_break` starts a new
/// paragraph).
fn content_runs(
    nodes: &[ContentNode],
    base: &RunStyle,
    rt: &ResolvedTable,
    block_px: f64,
    italic_marks: bool,
) -> Vec<Vec<Run>> {
    let mut paragraphs: Vec<Vec<Run>> = vec![Vec::new()];
    for node in nodes {
        let current = paragraphs.last_mut().unwrap();
        match node {
            ContentNode::Text { value } => current.push(Run {
                text: value.clone(),
                style: base.clone(),
            }),
            ContentNode::StyledText { value, style_id } => {
                let mut style = base.clone();
                if let Some(id) = style_id.as_deref() {
                    apply_style(&mut style, &rt.style(id), block_px);
                }
                current.push(Run {
                    text: value.clone(),
                    style,
                });
            }
            ContentNode::LineBreak {} => paragraphs.push(Vec::new()),
            ContentNode::FootnoteMark { mark_text, .. } => current.push(Run {
                text: mark_text.clone(),
                style: RunStyle {
                    sup: true,
                    italic: base.italic || italic_marks,
                    ..base.clone()
                },
            }),
            ContentNode::Image { alt, .. } => {
                if let Some(alt) = alt {
                    current.push(Run {
                        text: alt.clone(),
                        style: base.clone(),
                    });
                }
            }
            ContentNode::Raw { .. } | ContentNode::Unknown => {}
        }
    }
    // Newlines inside text are not breaks in SVG (that's `line_break`): spaces.
    for run in paragraphs.iter_mut().flatten() {
        if run.text.contains(['\n', '\r']) {
            run.text = run.text.replace(['\r', '\n'], " ");
        }
    }
    paragraphs
}

/// Weight, slant, color, decoration and size from a style; unset properties keep
/// `style`'s value. A relative size resolves against the run's size so far, or
/// `block_px`, the size of the text block it is in.
fn apply_style(style: &mut RunStyle, s: &ResolvedStyle, block_px: f64) {
    if let Some(w) = &s.font_weight {
        style.bold = w.is_bold();
    }
    if let Some(f) = &s.font_style {
        style.italic = f.is_italic();
    }
    if let Some(c) = s.paint() {
        style.color = Some(c.to_css());
    }
    style.underline |= s.is_underline();
    style.strike |= s.is_strike();
    let parent_pt = style.size.unwrap_or(block_px) * 0.75;
    if let Some(pt) = s.size_pt(parent_pt) {
        style.size = Some(pt / 0.75);
    }
}

fn svg_align(a: &HAlign) -> Align {
    match a {
        HAlign::Center => Align::Center,
        // Decimal ("char") alignment is approximated by right alignment.
        HAlign::Right | HAlign::Char => Align::Right,
        _ => Align::Left,
    }
}

/// A length in px, for widths with an absolute meaning. `%`, `fr`, `em` and `auto`
/// have no container to resolve against in a standalone SVG: `None` (auto).
fn px(l: &Length) -> Option<f64> {
    match l {
        &Length::Px(v) => Some(v),
        l @ (Length::Pt(_) | Length::In(_) | Length::Cm(_) | Length::Mm(_)) => {
            l.to_pt(0.0, 0.0).map(|pt| pt / 0.75)
        }
        _ => None,
    }
    .filter(|v| v.is_finite() && *v > 0.0)
}

// ───────────────────────────────── emit ──────────────────────────────────

fn emit(l: &Layout, config: &SvgConfig) -> Result<String, RenderError> {
    let mut buf = String::with_capacity(8192);
    writeln!(
        buf,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">",
        w = num(l.width),
        h = num(l.height)
    )?;
    let fill = if config.base_styles {
        format!(" fill: {};", LOOK.text_color.to_css())
    } else {
        String::new()
    };
    // Defaults on the root, for text to inherit: an element's own presentation
    // attributes (a heading's `font-size`) beat inherited values, but would lose to
    // a rule matching `text` itself. Fills and rules are drawn with crisp edges, so
    // adjacent cells meet without antialiased seams.
    writeln!(
        buf,
        "<style>svg {{ font-family: {}; font-size: {}px;{fill} }} \
         rect {{ shape-rendering: crispEdges; }}</style>",
        escape_xml(&config.font_family),
        num(config.font_size)
    )?;

    // Fills first, then rules, then text on top.
    for cell in &l.cells {
        if let Some(fill) = &cell.fill {
            writeln!(
                buf,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\"/>",
                num(cell.x),
                num(cell.y),
                num(cell.width),
                num(cell.height)
            )?;
        }
    }
    for rule in &l.rules {
        rule_line(&mut buf, rule)?;
    }
    for block in &l.header {
        text_block(&mut buf, block)?;
    }
    for cell in &l.cells {
        text_block(&mut buf, &cell.text)?;
    }
    for block in &l.footer {
        text_block(&mut buf, block)?;
    }

    buf.push_str("</svg>\n");
    Ok(buf)
}

/// A rule as filled rectangles (crisp at any zoom; a double rule is two).
fn rule_line(buf: &mut String, r: &RuleLine) -> Result<(), RenderError> {
    if let Some(dash) = r.dash {
        writeln!(
            buf,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" \
             stroke-width=\"{}\" stroke-dasharray=\"{} {}\"/>",
            num(r.x1),
            num(r.y1),
            num(r.x2),
            num(r.y2),
            r.color,
            num(r.width),
            num(dash),
            num(dash)
        )?;
        return Ok(());
    }
    let horizontal = r.y1 == r.y2;
    let parts: Vec<(f64, f64)> = if r.double {
        let t = r.width / 3.0;
        vec![(-r.width / 2.0, t), (r.width / 2.0 - t, t)]
    } else {
        vec![(-r.width / 2.0, r.width)]
    };
    for (offset, t) in parts {
        let (x, y, w, h) = if horizontal {
            (r.x1.min(r.x2), r.y1 + offset, (r.x2 - r.x1).abs(), t)
        } else {
            (r.x1 + offset, r.y1.min(r.y2), t, (r.y2 - r.y1).abs())
        };
        writeln!(
            buf,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            num(x),
            num(y),
            num(w),
            num(h),
            r.color
        )?;
    }
    Ok(())
}

/// The height of laid-out lines: each is as tall as its largest text.
fn block_height(lines: &[Line], font_size: f64) -> f64 {
    lines.iter().map(|l| l.size(font_size) * LINE_HEIGHT).sum()
}

/// y of the top of a block's first line.
fn first_line_top(b: &TextBlock) -> f64 {
    let spare = (b.height - block_height(&b.lines, b.font_size)).max(0.0);
    match b.valign {
        VAlign::Middle => b.y + spare / 2.0,
        VAlign::Bottom => b.y + spare,
    }
}

/// Write a block's lines.
fn text_block(buf: &mut String, b: &TextBlock) -> Result<(), RenderError> {
    let mut top = first_line_top(b);
    let (x, anchor) = match b.align {
        Align::Left => (b.x, ""),
        Align::Center => (b.x + b.width / 2.0, " text-anchor=\"middle\""),
        Align::Right => (b.x + b.width, " text-anchor=\"end\""),
    };
    for line in &b.lines {
        let size = line.size(b.font_size);
        let line_h = size * LINE_HEIGHT;
        let line_top = top;
        top += line_h;
        if line.runs.is_empty() {
            continue;
        }
        // Baseline: the line box holds the font's ascent and descent exactly.
        let baseline = line_top + (line_h - size * LINE_HEIGHT) / 2.0 + size * ASCENT;
        // `textLength`: draw the line exactly as wide as the layout measured it,
        // whatever font the viewer substitutes, by adjusting the letter spacing (the
        // default `lengthAdjust`), which leaves the glyph shapes undistorted.
        write!(
            buf,
            "<text x=\"{}\" y=\"{}\" font-size=\"{}px\"{anchor} textLength=\"{}\" \
             xml:space=\"preserve\">",
            num(x),
            num(baseline),
            num(b.font_size),
            num(line.draw_width)
        )?;
        for run in &line.runs {
            let s = &run.style;
            let mut attrs = String::new();
            if s.bold {
                attrs.push_str(" font-weight=\"bold\"");
            }
            if s.italic {
                attrs.push_str(" font-style=\"italic\"");
            }
            if let Some(c) = &s.color {
                write!(attrs, " fill=\"{c}\"")?;
            }
            match (s.underline, s.strike) {
                (true, true) => attrs.push_str(" text-decoration=\"underline line-through\""),
                (true, false) => attrs.push_str(" text-decoration=\"underline\""),
                (false, true) => attrs.push_str(" text-decoration=\"line-through\""),
                (false, false) => {}
            }
            if s.sup {
                // Small and raised by a fraction of the surrounding text's size (as
                // HTML's `vertical-align`), not a full superscript.
                let around = run.size(b.font_size);
                write!(
                    attrs,
                    " font-size=\"{}px\" baseline-shift=\"{}px\"",
                    num(around * SUP_SCALE),
                    num(around * LOOK.mark_raise_em * SUP_SCALE)
                )?;
            } else if let Some(size) = s.size {
                write!(attrs, " font-size=\"{}px\"", num(size))?;
            }
            if attrs.is_empty() {
                buf.push_str(&escape_xml(&run.text));
            } else {
                write!(buf, "<tspan{attrs}>{}</tspan>", escape_xml(&run.text))?;
            }
        }
        buf.push_str("</text>\n");
    }
    Ok(())
}

/// Compact, deterministic number formatting (at most 2 decimals, never `-0`).
fn num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

#[cfg(test)]
mod tests;
