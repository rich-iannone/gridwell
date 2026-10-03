//! IR → SVG.
//!
//! SVG has no layout engine, so this module is one: it measures text (conservatively,
//! see [`crate::measure`]), sizes columns and rows to fit, wraps text, and positions
//! every glyph run. Rendering is two passes: [`layout`] computes boxes and lines;
//! [`emit`] writes them out. Keeping the geometry in a plain struct lets tests check
//! layout invariants (no text outside its cell, cells inside the canvas) directly.

use std::fmt::Write;

use gridwell_core::Length;
use gridwell_ir::content::ContentNode;
use gridwell_ir::style::StyleDef;
use gridwell_ir::{resolve_slots, ColumnVisibility, HAlign, Row, Slot, Table};
use thiserror::Error;

use crate::measure::{max_width, wrap, Line, Run, RunStyle, SUP_SCALE};
use crate::SvgConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Line advance as a multiple of the font size.
const LINE_HEIGHT: f64 = 1.4;
/// Header/footer text sizes relative to the base font size.
const TITLE_SCALE: f64 = 1.4;
const SUBTITLE_SCALE: f64 = 1.1;
const NOTE_SCALE: f64 = 0.85;

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

/// A block of wrapped text placed in a box.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    pub lines: Vec<Line>,
    pub font_size: f64,
    pub align: Align,
    /// The box the text must stay inside (already inset by padding).
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
    /// Draw the thin rule along the bottom edge.
    pub bottom_rule: bool,
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
    /// y of the thick rule under the column labels, if any.
    pub header_rule: Option<f64>,
    /// Footnotes and source notes (below the table).
    pub footer: Vec<TextBlock>,
}

// ──────────────────────────────── layout ─────────────────────────────────

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
}

/// A full-width row (group label) before layout.
struct PendingBand {
    row: usize,
    runs: Vec<Vec<Run>>,
}

/// Compute the geometry of every box and line. See the module docs.
pub fn layout(table: &Table, config: &SvgConfig) -> Layout {
    let fs = config.font_size;
    let pad_x = config.cell_padding_x;
    let pad_y = config.cell_padding_y;
    let styles = Styles(table);
    let vis = ColumnVisibility::from_spec(&table.column_spec);
    let ncols = vis.visible_len();
    let visible_spec: Vec<_> = vis
        .visible_columns()
        .map(|c| &table.column_spec[c])
        .collect();

    // 1. Collect cells and bands in display order, assigning global row indices.
    let mut cells: Vec<PendingCell> = Vec::new();
    let mut bands: Vec<PendingBand> = Vec::new();
    let mut rule_after_row: Option<usize> = None;
    let mut nrows = 0usize;

    let add_section =
        |rows: &[Row], is_header: bool, nrows: &mut usize, cells: &mut Vec<PendingCell>| {
            let slots = resolve_slots(rows);
            for (r, (row, row_slots)) in rows.iter().zip(&slots).enumerate() {
                for (c, (cell, slot)) in row.cells.iter().zip(row_slots).enumerate() {
                    let Slot::Origin { colspan, rowspan } = *slot else {
                        continue;
                    };
                    // Cells entirely in hidden columns are dropped; spans shrink.
                    let Some((vcol, vspan)) = vis.project(c, colspan) else {
                        continue;
                    };
                    let mut base = RunStyle {
                        bold: is_header,
                        ..Default::default()
                    };
                    let cell_def = cell.style_id.as_deref().and_then(|id| styles.resolve(id));
                    let row_def = row.style_id.as_deref().and_then(|id| styles.resolve(id));
                    for def in [row_def.as_ref(), cell_def.as_ref()].into_iter().flatten() {
                        apply_style(&mut base, def);
                    }
                    let fill = [cell_def.as_ref(), row_def.as_ref()]
                        .into_iter()
                        .flatten()
                        .find_map(|d| d.background_color.as_deref().and_then(valid_color));
                    let align = cell_def
                        .as_ref()
                        .and_then(|d| d.text_align.as_ref())
                        .map(svg_align)
                        .unwrap_or_else(|| svg_align(&visible_spec[vcol].align));
                    let runs = content_runs(&cell.content, &base, &styles);
                    let natural = paragraphs_width(&runs, fs);
                    cells.push(PendingCell {
                        row: *nrows + r,
                        col: vcol,
                        colspan: vspan,
                        // A rowspan never crosses its section (validated).
                        rowspan: rowspan.min(rows.len() - r),
                        runs,
                        natural,
                        align,
                        fill,
                    });
                }
            }
            *nrows += rows.len();
        };

    if !table.config.column_labels_hidden && !table.table.thead.rows.is_empty() {
        add_section(&table.table.thead.rows, true, &mut nrows, &mut cells);
        rule_after_row = Some(nrows);
    }
    for group in &table.table.tbody {
        if let Some(label) = &group.label {
            let mut base = RunStyle {
                bold: true,
                ..Default::default()
            };
            if let Some(def) = label.style_id.as_deref().and_then(|id| styles.resolve(id)) {
                apply_style(&mut base, &def);
            }
            bands.push(PendingBand {
                row: nrows,
                runs: content_runs(&label.content, &base, &styles),
            });
            nrows += 1;
        }
        add_section(&group.rows, false, &mut nrows, &mut cells);
        add_section(&group.summary_rows, false, &mut nrows, &mut cells);
    }

    // 2. Column widths: fixed where the spec gives an absolute width, otherwise fit
    //    the widest single-column cell; clamp to min/max; then grow for spanners.
    let min_col = 2.0 * pad_x + fs;
    let max_of = |v: usize| visible_spec[v].max_width.as_deref().and_then(px);
    let mut widths = vec![0.0f64; ncols];
    let mut fixed = vec![false; ncols];
    for (v, spec) in visible_spec.iter().enumerate() {
        if let Some(w) = px(&spec.width) {
            widths[v] = w;
            fixed[v] = true;
        }
    }
    for cell in cells.iter().filter(|c| c.colspan == 1) {
        if !fixed[cell.col] {
            widths[cell.col] = widths[cell.col].max(cell.natural + 2.0 * pad_x);
        }
    }
    for (v, spec) in visible_spec.iter().enumerate() {
        let lo = spec.min_width.as_deref().and_then(px).unwrap_or(min_col);
        let hi = max_of(v).unwrap_or(f64::INFINITY).max(lo);
        widths[v] = widths[v].max(lo).min(hi);
    }
    for cell in cells.iter().filter(|c| c.colspan > 1) {
        let range = cell.col..cell.col + cell.colspan;
        let have: f64 = widths[range.clone()].iter().sum();
        let need = cell.natural + 2.0 * pad_x;
        if need > have {
            let growable: Vec<usize> = range
                .filter(|&v| !fixed[v] && max_of(v).is_none_or(|hi| widths[v] < hi))
                .collect();
            if !growable.is_empty() {
                let share = (need - have) / growable.len() as f64;
                for v in growable {
                    widths[v] += share;
                }
            }
        }
    }
    let col_x = prefix_sums(0.0, &widths);
    let table_width = col_x[ncols];

    // 3. Wrap cell text to the final widths, then size rows to fit.
    let line_h = fs * LINE_HEIGHT;
    let block_height = |lines: &[Line]| lines.len() as f64 * line_h;
    let wrapped: Vec<Vec<Line>> = cells
        .iter()
        .map(|c| {
            let inner = col_x[c.col + c.colspan] - col_x[c.col] - 2.0 * pad_x;
            wrap_paragraphs(&c.runs, fs, inner)
        })
        .collect();
    let band_lines: Vec<Vec<Line>> = bands
        .iter()
        .map(|b| wrap_paragraphs(&b.runs, fs, (table_width - 2.0 * pad_x).max(fs)))
        .collect();

    let mut heights = vec![config.row_height; nrows];
    for (cell, lines) in cells.iter().zip(&wrapped) {
        if cell.rowspan == 1 {
            heights[cell.row] = heights[cell.row].max(block_height(lines) + 2.0 * pad_y);
        }
    }
    for (band, lines) in bands.iter().zip(&band_lines) {
        heights[band.row] = heights[band.row].max(block_height(lines) + 2.0 * pad_y);
    }
    for (cell, lines) in cells.iter().zip(&wrapped) {
        if cell.rowspan > 1 {
            let last = cell.row + cell.rowspan - 1;
            let have: f64 = heights[cell.row..=last].iter().sum();
            let need = block_height(lines) + 2.0 * pad_y;
            if need > have {
                heights[last] += need - have;
            }
        }
    }

    // 4. Header text above the table, wrapped to the table (or a minimum) width.
    let text_area = table_width.max(config.default_col_width * 3.0) - 2.0 * pad_x;
    let mut out = Layout::default();
    let mut y = pad_y;
    let mut header_lines: Vec<(&[ContentNode], f64, bool)> = Vec::new();
    if let Some(h) = &table.header {
        header_lines.extend(
            h.title
                .iter()
                .map(|t| (t.content.as_slice(), TITLE_SCALE, true)),
        );
        header_lines.extend(
            h.subtitle
                .iter()
                .map(|t| (t.content.as_slice(), SUBTITLE_SCALE, false)),
        );
        header_lines.extend(
            h.extra_lines
                .iter()
                .map(|t| (t.content.as_slice(), 1.0, false)),
        );
    }
    for (content, scale, bold) in header_lines {
        let size = fs * scale;
        let base = RunStyle {
            bold,
            ..Default::default()
        };
        let lines = wrap_paragraphs(&content_runs(content, &base, &styles), size, text_area);
        let h = lines.len() as f64 * size * LINE_HEIGHT;
        out.header.push(TextBlock {
            width: max_width(&lines).max(text_area),
            lines,
            font_size: size,
            align: Align::Left,
            x: pad_x,
            y,
            height: h,
        });
        y += h + pad_y;
    }

    // 5. Place cells and bands.
    let row_y = prefix_sums(y, &heights);
    out.table_y = y;
    out.table_width = table_width;
    out.table_height = row_y[nrows] - y;
    out.header_rule = rule_after_row.map(|r| row_y[r]);

    for (cell, lines) in cells.iter().zip(wrapped) {
        let (x, w) = (
            col_x[cell.col],
            col_x[cell.col + cell.colspan] - col_x[cell.col],
        );
        let (cy, h) = (
            row_y[cell.row],
            row_y[cell.row + cell.rowspan] - row_y[cell.row],
        );
        out.cells.push(CellBox {
            x,
            y: cy,
            width: w,
            height: h,
            fill: cell.fill.clone(),
            bottom_rule: true,
            text: TextBlock {
                lines,
                font_size: fs,
                align: cell.align,
                x: x + pad_x,
                y: cy + pad_y,
                width: w - 2.0 * pad_x,
                height: h - 2.0 * pad_y,
            },
        });
    }
    for (band, lines) in bands.iter().zip(band_lines) {
        let (cy, h) = (row_y[band.row], heights[band.row]);
        out.cells.push(CellBox {
            x: 0.0,
            y: cy,
            width: table_width,
            height: h,
            fill: Some("#f0f0f0".into()),
            bottom_rule: false,
            text: TextBlock {
                lines,
                font_size: fs,
                align: Align::Left,
                x: pad_x,
                y: cy + pad_y,
                width: table_width - 2.0 * pad_x,
                height: h - 2.0 * pad_y,
            },
        });
    }

    // 6. Footnotes and source notes below the table.
    let mut y = row_y[nrows] + pad_y;
    if let Some(footer) = &table.footer {
        let size = fs * NOTE_SCALE;
        let footnotes = footer.footnotes.iter().map(|n| {
            let mut paragraphs = content_runs(&n.content, &RunStyle::default(), &styles);
            let mut first = vec![
                Run {
                    text: n.mark.clone(),
                    style: RunStyle {
                        sup: true,
                        ..Default::default()
                    },
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
            .map(|n| content_runs(&n.content, &RunStyle::default(), &styles));
        for paragraphs in footnotes.chain(sources) {
            let lines = wrap_paragraphs(&paragraphs, size, text_area);
            let h = lines.len() as f64 * size * LINE_HEIGHT;
            out.footer.push(TextBlock {
                width: max_width(&lines).max(text_area),
                lines,
                font_size: size,
                align: Align::Left,
                x: pad_x,
                y,
                height: h,
            });
            y += h + pad_y * 0.5;
        }
    }

    // 7. Canvas: everything drawn must fit.
    let text_right = out
        .header
        .iter()
        .chain(&out.footer)
        .map(|b| b.x + max_width(&b.lines) + pad_x)
        .fold(0.0, f64::max);
    out.width = table_width.max(text_right).ceil();
    out.height = (y + pad_y).ceil();
    out
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
fn content_runs(nodes: &[ContentNode], base: &RunStyle, styles: &Styles) -> Vec<Vec<Run>> {
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
                if let Some(def) = style_id.as_deref().and_then(|id| styles.resolve(id)) {
                    apply_style(&mut style, &def);
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

fn apply_style(style: &mut RunStyle, def: &StyleDef) {
    if let Some(w) = &def.font_weight {
        style.bold = w.is_bold();
    }
    if let Some(s) = &def.font_style {
        style.italic = s.is_italic();
    }
    if let Some(c) = def.color.as_deref().and_then(valid_color) {
        style.color = Some(c);
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
fn px(s: &str) -> Option<f64> {
    match s.parse::<Length>().ok()? {
        Length::Px(v) => Some(v),
        l @ (Length::Pt(_) | Length::In(_) | Length::Cm(_) | Length::Mm(_)) => {
            l.to_pt(0.0, 0.0).map(|pt| pt / 0.75)
        }
        _ => None,
    }
    .filter(|v| v.is_finite() && *v > 0.0)
}

/// `#rgb` / `#rrggbb` only: the forms safe to drop into an attribute verbatim.
fn valid_color(c: &str) -> Option<String> {
    let hex = c.strip_prefix('#')?;
    (matches!(hex.len(), 3 | 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit()))
        .then(|| c.to_string())
}

/// Style lookup with single-level compositions.
struct Styles<'a>(&'a Table);

impl Styles<'_> {
    fn resolve(&self, id: &str) -> Option<StyleDef> {
        let palette = &self.0.styles;
        if let Some(def) = palette.defs.get(id) {
            return Some(def.clone());
        }
        let comp = palette.compositions.get(id)?;
        let mut def = palette.defs.get(&comp.extends)?.clone();
        let o = &comp.overrides;
        macro_rules! over {
            ($($f:ident),*) => { $( if o.$f.is_some() { def.$f = o.$f.clone(); } )* };
        }
        over!(font_weight, font_style, color, background_color, text_align);
        Some(def)
    }
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
    writeln!(
        buf,
        "<style>text {{ font-family: {}; font-size: {}px; }}</style>",
        escape_xml(&config.font_family),
        num(config.font_size)
    )?;

    for block in &l.header {
        text_block(&mut buf, block)?;
    }

    // Fills first, then text, then rules on top.
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
    for cell in &l.cells {
        text_block(&mut buf, &cell.text)?;
    }
    for cell in l.cells.iter().filter(|c| c.bottom_rule) {
        let y = num(cell.y + cell.height);
        writeln!(
            buf,
            "<line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" stroke=\"#d0d0d0\" stroke-width=\"0.5\"/>",
            num(cell.x),
            num(cell.x + cell.width)
        )?;
    }
    if let Some(y) = l.header_rule {
        writeln!(
            buf,
            "<line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" stroke=\"#333333\" stroke-width=\"1.5\"/>",
            num(l.table_x),
            num(l.table_x + l.table_width),
            y = num(y)
        )?;
    }

    for block in &l.footer {
        text_block(&mut buf, block)?;
    }

    buf.push_str("</svg>\n");
    Ok(buf)
}

/// y of the first baseline of a block (lines are vertically centred in the box).
fn first_line_top(b: &TextBlock) -> f64 {
    let line_h = b.font_size * LINE_HEIGHT;
    b.y + ((b.height - b.lines.len() as f64 * line_h) / 2.0).max(0.0)
}

/// Write a block's lines, vertically centred in its box.
fn text_block(buf: &mut String, b: &TextBlock) -> Result<(), RenderError> {
    let line_h = b.font_size * LINE_HEIGHT;
    let top = first_line_top(b);
    let (x, anchor) = match b.align {
        Align::Left => (b.x, ""),
        Align::Center => (b.x + b.width / 2.0, " text-anchor=\"middle\""),
        Align::Right => (b.x + b.width, " text-anchor=\"end\""),
    };
    for (i, line) in b.lines.iter().enumerate() {
        if line.runs.is_empty() {
            continue;
        }
        // Baseline: centre of the line box plus about a third of the font size.
        let baseline = top + line_h * (i as f64 + 0.5) + b.font_size * 0.35;
        write!(
            buf,
            "<text x=\"{}\" y=\"{}\" font-size=\"{}px\"{anchor} xml:space=\"preserve\">",
            num(x),
            num(baseline),
            num(b.font_size)
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
            if s.sup {
                write!(
                    attrs,
                    " font-size=\"{}px\" baseline-shift=\"super\"",
                    num(b.font_size * SUP_SCALE)
                )?;
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

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            // XML 1.0 forbids most C0 controls even when escaped.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests;
