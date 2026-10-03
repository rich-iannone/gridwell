use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table};
use gridwell_layout::{resolve, ResolvedStyle, ResolvedTable, Section, Slot};
use std::fmt::Write;
use thiserror::Error;
use unicode_width::UnicodeWidthStr;

use crate::AnsiConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

// Box-drawing characters (light)
const TL: &str = "┌";
const TR: &str = "┐";
const BL: &str = "└";
const BR: &str = "┘";
const H: &str = "─";
const V: &str = "│";
const TJ: &str = "┬";
const BJ: &str = "┴";
const LJ: &str = "├";
const RJ: &str = "┤";
const CJ: &str = "┼";

// Plain ASCII fallback
const P_TL: &str = "+";
const P_TR: &str = "+";
const P_BL: &str = "+";
const P_BR: &str = "+";
const P_H: &str = "-";
const P_V: &str = "|";
const P_TJ: &str = "+";
const P_BJ: &str = "+";
const P_LJ: &str = "+";
const P_RJ: &str = "+";
const P_CJ: &str = "+";

struct BoxChars {
    tl: &'static str,
    tr: &'static str,
    bl: &'static str,
    br: &'static str,
    h: &'static str,
    v: &'static str,
    tj: &'static str,
    bj: &'static str,
    lj: &'static str,
    rj: &'static str,
    cj: &'static str,
}

impl BoxChars {
    fn unicode() -> Self {
        Self {
            tl: TL,
            tr: TR,
            bl: BL,
            br: BR,
            h: H,
            v: V,
            tj: TJ,
            bj: BJ,
            lj: LJ,
            rj: RJ,
            cj: CJ,
        }
    }

    fn ascii() -> Self {
        Self {
            tl: P_TL,
            tr: P_TR,
            bl: P_BL,
            br: P_BR,
            h: P_H,
            v: P_V,
            tj: P_TJ,
            bj: P_BJ,
            lj: P_LJ,
            rj: P_RJ,
            cj: P_CJ,
        }
    }
}

const DEFAULT_COL_WIDTH: usize = 12;
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";

struct AnsiRenderer<'r, 'a> {
    rt: &'r ResolvedTable<'a>,
    config: &'r AnsiConfig,
    buf: String,
    col_widths: Vec<usize>,
    bc: BoxChars,
}

impl<'r, 'a> AnsiRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>, config: &'r AnsiConfig) -> Self {
        let bc = if config.box_drawing {
            BoxChars::unicode()
        } else {
            BoxChars::ascii()
        };
        Self {
            rt,
            config,
            buf: String::with_capacity(4096),
            col_widths: fit_widths(Self::compute_col_widths(rt), config.max_width),
            bc,
        }
    }

    /// Each visible column is as wide as its widest single-column cell plus one
    /// space of padding on each side (at least `DEFAULT_COL_WIDTH`).
    fn compute_col_widths(rt: &ResolvedTable) -> Vec<usize> {
        let mut widths = vec![DEFAULT_COL_WIDTH; rt.columns.len()];
        for section in rt.sections() {
            for row in &section.rows {
                for cell in row.cells().filter(|c| c.colspan == 1) {
                    let w = UnicodeWidthStr::width(content_to_text(cell.content).as_str());
                    widths[cell.col] = widths[cell.col].max(w + 2);
                }
            }
        }
        widths
    }

    /// Width of `span` columns starting at `col`, including the separators between
    /// them.
    fn span_width(&self, col: usize, span: usize) -> usize {
        self.col_widths[col..col + span].iter().sum::<usize>() + span - 1
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.render_title()?;

        // With every column hidden there is no grid to draw.
        if !self.rt.is_empty() {
            self.render_grid()?;
        }

        self.render_footnotes()?;
        Ok(self.buf)
    }

    fn render_grid(&mut self) -> Result<(), RenderError> {
        let rt = self.rt;
        self.write_border_line(self.bc.tl, self.bc.tj, self.bc.tr)?;

        if !rt.head.is_empty() {
            self.write_section(&rt.head, true)?;
            self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
        }

        let group_count = rt.groups.len();
        for (gi, group) in rt.groups.iter().enumerate() {
            if let Some(label) = &group.label {
                let text = content_to_text(label.content);
                self.write_group_label(&text)?;
                self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
            }

            self.write_section(&group.rows, false)?;

            if !group.summary_rows.is_empty() {
                self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
                self.write_section(&group.summary_rows, true)?;
            }

            if gi + 1 < group_count {
                self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
            }
        }

        self.write_border_line(self.bc.bl, self.bc.bj, self.bc.br)
    }

    fn render_title(&mut self) -> Result<(), RenderError> {
        let header = &self.rt.header;
        if let Some(title) = &header.title {
            let text = content_to_text(title.content);
            writeln!(self.buf, "{BOLD}{text}{RESET}")?;
        }
        if let Some(subtitle) = &header.subtitle {
            let text = content_to_text(subtitle.content);
            writeln!(self.buf, "{text}")?;
        }
        Ok(())
    }

    fn write_border_line(&mut self, left: &str, mid: &str, right: &str) -> Result<(), RenderError> {
        self.buf.push_str(left);
        for (i, w) in self.col_widths.iter().enumerate() {
            if i > 0 {
                self.buf.push_str(mid);
            }
            for _ in 0..*w {
                self.buf.push_str(self.bc.h);
            }
        }
        self.buf.push_str(right);
        self.buf.push('\n');
        Ok(())
    }

    fn write_section(&mut self, section: &Section, strong: bool) -> Result<(), RenderError> {
        for r in 0..section.rows.len() {
            self.write_data_row(section, r, strong)?;
        }
        Ok(())
    }

    /// One text line of the grid. Covered positions are blank: a cell spanning
    /// rows is drawn in its first row, and the rows below show an empty box of the
    /// same width.
    fn write_data_row(
        &mut self,
        section: &Section,
        r: usize,
        strong: bool,
    ) -> Result<(), RenderError> {
        let row = &section.rows[r];
        self.buf.push_str(self.bc.v);
        let mut first = true;
        let mut sep = |buf: &mut String, v: &str| {
            if !std::mem::take(&mut first) {
                buf.push_str(v);
            }
        };
        let v = self.bc.v;
        for (col, slot) in row.slots.iter().enumerate() {
            match slot {
                Slot::Origin(cell) => {
                    sep(&mut self.buf, v);
                    let width = self.span_width(col, cell.colspan);
                    let text = content_to_text(cell.content);
                    let (open, close) = self.escapes(&cell.style, strong);
                    self.buf.push_str(&open);
                    self.buf.push_str(&fit(&text, width, &cell.align));
                    self.buf.push_str(close);
                }
                Slot::CoveredV {
                    origin_row,
                    origin_col,
                    ..
                } if *origin_col == col => {
                    sep(&mut self.buf, v);
                    let span = section.rows[*origin_row].slots[col]
                        .origin()
                        .map_or(1, |c| c.colspan);
                    let width = self.span_width(col, span);
                    self.buf.push_str(&" ".repeat(width));
                }
                Slot::CoveredH { .. } | Slot::CoveredV { .. } => {}
                Slot::Empty => {
                    sep(&mut self.buf, v);
                    self.buf.push_str(&" ".repeat(self.col_widths[col]));
                }
            }
        }
        self.buf.push_str(self.bc.v);
        self.buf.push('\n');
        Ok(())
    }

    /// Opening escapes for a cell and the matching reset (empty if unstyled).
    fn escapes(&self, style: &ResolvedStyle, strong: bool) -> (String, &'static str) {
        let mut open = String::new();
        if strong || style.is_bold() {
            open.push_str(BOLD);
        }
        if style.is_italic() {
            open.push_str(ITALIC);
        }
        if self.config.true_color {
            if let Some(c) = style.paint() {
                // Terminals have no alpha: show the colour as it would look on white.
                let c = c.flatten();
                let _ = write!(open, "\x1b[38;2;{};{};{}m", c.r, c.g, c.b);
            }
            if self.config.background_colors {
                if let Some(c) = style.fill() {
                    let c = c.flatten();
                    let _ = write!(open, "\x1b[48;2;{};{};{}m", c.r, c.g, c.b);
                }
            }
        }
        let close = if open.is_empty() { "" } else { RESET };
        (open, close)
    }

    fn write_group_label(&mut self, text: &str) -> Result<(), RenderError> {
        let inner = self.span_width(0, self.col_widths.len());
        self.buf.push_str(self.bc.v);
        write!(self.buf, "{BOLD}")?;
        self.buf.push_str(&fit(text, inner, &HAlign::Left));
        write!(self.buf, "{RESET}")?;
        self.buf.push_str(self.bc.v);
        self.buf.push('\n');
        Ok(())
    }

    fn render_footnotes(&mut self) -> Result<(), RenderError> {
        for note in &self.rt.footer.footnotes {
            let text = content_to_text(note.content);
            let mark = sanitize(note.mark);
            writeln!(self.buf, "  {mark} {text}")?;
        }
        for note in &self.rt.footer.source_notes {
            let text = content_to_text(note.content);
            writeln!(self.buf, "  {text}")?;
        }
        Ok(())
    }
}

pub fn render(table: &Table, config: &AnsiConfig) -> Result<String, RenderError> {
    let rt = resolve(table);
    AnsiRenderer::new(&rt, config).render()
}

/// Narrowest a column may be shrunk to: one character plus padding.
const MIN_COL_WIDTH: usize = 3;

/// Shrink columns until the whole grid (borders included) is at most `max_width`
/// terminal columns, taking one column at a time from the widest. `0` means no
/// limit. Cells that no longer fit are truncated with `…`. A grid with more columns
/// than fit even at the minimum width stays at the minimum.
fn fit_widths(mut widths: Vec<usize>, max_width: usize) -> Vec<usize> {
    if max_width == 0 || widths.is_empty() {
        return widths;
    }
    // Content widths plus one border character per column and the closing border.
    let total = |w: &[usize]| w.iter().sum::<usize>() + w.len() + 1;
    while total(&widths) > max_width {
        let (i, &widest) = widths
            .iter()
            .enumerate()
            .max_by_key(|&(i, w)| (*w, std::cmp::Reverse(i)))
            .expect("non-empty");
        if widest <= MIN_COL_WIDTH {
            break;
        }
        widths[i] -= 1;
    }
    widths
}

/// Remove control characters (C0, DEL, C1 — including ESC) so table text can't
/// emit terminal escape sequences or break the grid; tabs and newlines become
/// spaces.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

fn content_to_text(nodes: &[ContentNode]) -> String {
    sanitize(&gridwell_layout::plain_text(nodes, " "))
}

/// `text` laid out in exactly `width` terminal columns: one space of padding on
/// each side, aligned, and truncated with `…` if it doesn't fit.
fn fit(text: &str, width: usize, align: &HAlign) -> String {
    let inner = width.saturating_sub(2);
    let mut shown = String::new();
    let mut used = 0;
    if UnicodeWidthStr::width(text) <= inner {
        shown.push_str(text);
        used = UnicodeWidthStr::width(text);
    } else if inner > 0 {
        for c in text.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + cw > inner - 1 {
                break;
            }
            shown.push(c);
            used += cw;
        }
        shown.push('…');
        used += 1;
    }
    let pad = width.saturating_sub(used);
    let left = match align {
        HAlign::Right => pad.saturating_sub(1),
        HAlign::Center => pad / 2,
        _ => pad.min(1),
    };
    format!("{}{shown}{}", " ".repeat(left), " ".repeat(pad - left))
}

#[cfg(test)]
mod tests {
    use super::fit_widths;

    #[test]
    fn widths_shrink_from_the_widest_until_they_fit() {
        assert_eq!(fit_widths(vec![12, 30, 12], 0), vec![12, 30, 12]);
        // 12 + 30 + 12 + 4 borders = 58.
        assert_eq!(fit_widths(vec![12, 30, 12], 58), vec![12, 30, 12]);
        assert_eq!(fit_widths(vec![12, 30, 12], 40), vec![12, 12, 12]);
        assert_eq!(fit_widths(vec![12, 30, 12], 37), vec![11, 11, 11]);
        // Never below the minimum, even if that overflows.
        assert_eq!(fit_widths(vec![12, 12], 4), vec![3, 3]);
    }
}
