use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table};
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

struct AnsiRenderer<'a> {
    table: &'a Table,
    config: &'a AnsiConfig,
    buf: String,
    col_widths: Vec<usize>,
    bc: BoxChars,
}

impl<'a> AnsiRenderer<'a> {
    fn new(table: &'a Table, config: &'a AnsiConfig) -> Self {
        let bc = if config.box_drawing {
            BoxChars::unicode()
        } else {
            BoxChars::ascii()
        };

        let col_widths = Self::compute_col_widths(table);

        Self {
            table,
            config,
            buf: String::with_capacity(4096),
            col_widths,
            bc,
        }
    }

    fn compute_col_widths(table: &Table) -> Vec<usize> {
        let num_cols = table.config.table_cols as usize;
        let mut widths = vec![DEFAULT_COL_WIDTH; num_cols];

        // Measure content widths from all rows
        let all_rows: Vec<&Row> = {
            let mut rows: Vec<&Row> = table.table.thead.rows.iter().collect();
            for group in &table.table.tbody {
                rows.extend(group.rows.iter());
                rows.extend(group.summary_rows.iter());
            }
            rows
        };

        for row in &all_rows {
            let mut col_idx = 0;
            for cell in &row.cells {
                if col_idx >= num_cols {
                    break;
                }
                if cell.is_placeholder {
                    col_idx += 1;
                    continue;
                }
                if cell.colspan == 1 {
                    let text = content_to_text(&cell.content);
                    let w = UnicodeWidthStr::width(text.as_str());
                    widths[col_idx] = widths[col_idx].max(w + 2); // +2 for padding
                }
                col_idx += cell.colspan as usize;
            }
        }

        widths
    }

    fn render(mut self) -> Result<String, RenderError> {
        // Title
        self.render_title()?;

        // Top border
        self.write_border_line(self.bc.tl, self.bc.tj, self.bc.tr)?;

        // Header rows
        if !self.table.config.column_labels_hidden {
            for row in &self.table.table.thead.rows {
                self.write_data_row(row, true)?;
            }
            // Header/body separator
            self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
        }

        // Body rows
        let group_count = self.table.table.tbody.len();
        for (gi, group) in self.table.table.tbody.iter().enumerate() {
            if let Some(ref label) = group.label {
                let text = content_to_text(&label.content);
                self.write_group_label(&text)?;
                self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
            }

            for row in &group.rows {
                self.write_data_row(row, false)?;
            }

            if !group.summary_rows.is_empty() {
                self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
                for row in &group.summary_rows {
                    self.write_data_row(row, true)?;
                }
            }

            // Group separator (not after last)
            if gi < group_count - 1 {
                self.write_border_line(self.bc.lj, self.bc.cj, self.bc.rj)?;
            }
        }

        // Bottom border
        self.write_border_line(self.bc.bl, self.bc.bj, self.bc.br)?;

        // Footnotes
        self.render_footnotes()?;

        Ok(self.buf)
    }

    fn render_title(&mut self) -> Result<(), RenderError> {
        if let Some(ref header) = self.table.header {
            if let Some(ref title) = header.title {
                let text = content_to_text(&title.content);
                writeln!(self.buf, "{BOLD}{text}{RESET}")?;
            }
            if let Some(ref subtitle) = header.subtitle {
                let text = content_to_text(&subtitle.content);
                writeln!(self.buf, "{text}")?;
            }
        }
        Ok(())
    }

    fn write_border_line(
        &mut self,
        left: &str,
        mid: &str,
        right: &str,
    ) -> Result<(), RenderError> {
        self.buf.push_str(left);
        for (i, w) in self.col_widths.iter().enumerate() {
            for _ in 0..*w {
                self.buf.push_str(self.bc.h);
            }
            if i < self.col_widths.len() - 1 {
                self.buf.push_str(mid);
            }
        }
        self.buf.push_str(right);
        self.buf.push('\n');
        Ok(())
    }

    fn write_data_row(&mut self, row: &Row, is_bold: bool) -> Result<(), RenderError> {
        self.buf.push_str(self.bc.v);
        let mut col_idx = 0;

        for cell in &row.cells {
            if col_idx >= self.col_widths.len() {
                break;
            }
            if cell.is_placeholder {
                col_idx += 1;
                continue;
            }

            let span = cell.colspan as usize;
            let total_w: usize = self.col_widths[col_idx..col_idx + span].iter().sum::<usize>()
                + (span - 1); // account for removed separators

            let text = content_to_text(&cell.content);
            let display_width = UnicodeWidthStr::width(text.as_str());

            let mut cell_text = String::new();

            // Apply styling
            let mut has_style = false;
            if is_bold {
                cell_text.push_str(BOLD);
                has_style = true;
            } else if let Some(ref style_id) = cell.style_id {
                if let Some(def) = self.table.styles.defs.get(style_id.as_str()) {
                    if def.font_weight.as_deref() == Some("bold") {
                        cell_text.push_str(BOLD);
                        has_style = true;
                    }
                    if def.font_style.as_deref() == Some("italic") {
                        cell_text.push_str(ITALIC);
                        has_style = true;
                    }
                    if let Some(ref color) = def.color {
                        if self.config.true_color {
                            if let Some(esc) = fg_24bit(color) {
                                cell_text.push_str(&esc);
                                has_style = true;
                            }
                        }
                    }
                }
            }

            // Pad and truncate text
            let padded = if display_width >= total_w {
                truncate_to_width(&text, total_w.saturating_sub(1))
            } else {
                let pad = total_w - display_width;
                let lpad = pad / 2;
                let rpad = pad - lpad;
                format!(
                    "{}{text}{}",
                    " ".repeat(lpad),
                    " ".repeat(rpad)
                )
            };

            cell_text.push_str(&padded);
            if has_style {
                cell_text.push_str(RESET);
            }

            self.buf.push_str(&cell_text);

            col_idx += span;
            if col_idx < self.col_widths.len() {
                self.buf.push_str(self.bc.v);
            }
        }

        // Fill remaining columns (if cells ended early due to spans)
        while col_idx < self.col_widths.len() {
            let w = self.col_widths[col_idx];
            for _ in 0..w {
                self.buf.push(' ');
            }
            col_idx += 1;
            if col_idx < self.col_widths.len() {
                self.buf.push_str(self.bc.v);
            }
        }

        self.buf.push_str(self.bc.v);
        self.buf.push('\n');
        Ok(())
    }

    fn write_group_label(&mut self, text: &str) -> Result<(), RenderError> {
        let total_inner: usize =
            self.col_widths.iter().sum::<usize>() + self.col_widths.len() - 1;
        let display_width = UnicodeWidthStr::width(text);

        self.buf.push_str(self.bc.v);
        write!(self.buf, "{BOLD}")?;
        self.buf.push(' ');
        self.buf.push_str(text);
        let remaining = total_inner.saturating_sub(display_width + 1);
        for _ in 0..remaining {
            self.buf.push(' ');
        }
        write!(self.buf, "{RESET}")?;
        self.buf.push_str(self.bc.v);
        self.buf.push('\n');
        Ok(())
    }

    fn render_footnotes(&mut self) -> Result<(), RenderError> {
        if let Some(ref footer) = self.table.footer {
            if !footer.footnotes.is_empty() {
                for note in &footer.footnotes {
                    let text = content_to_text(&note.content);
                    writeln!(self.buf, "  {} {text}", note.mark)?;
                }
            }
            if !footer.source_notes.is_empty() {
                for note in &footer.source_notes {
                    let text = content_to_text(&note.content);
                    writeln!(self.buf, "  {text}")?;
                }
            }
        }
        Ok(())
    }
}

pub fn render(table: &Table, config: &AnsiConfig) -> Result<String, RenderError> {
    AnsiRenderer::new(table, config).render()
}

fn content_to_text(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => out.push_str(value),
            ContentNode::StyledText { value, .. } => out.push_str(value),
            ContentNode::LineBreak {} => out.push(' '),
            ContentNode::FootnoteMark { mark_text, .. } => {
                out.push_str(mark_text);
            }
            ContentNode::Image { alt, .. } => {
                if let Some(alt_text) = alt {
                    out.push_str(alt_text);
                }
            }
            ContentNode::Raw { .. } | ContentNode::Unknown => {}
        }
    }
    out
}

fn fg_24bit(hex: &str) -> Option<String> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(format!("\x1b[38;2;{r};{g};{b}m"))
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if current_width + cw > max_width {
            result.push('…');
            break;
        }
        result.push(c);
        current_width += cw;
    }
    // Pad to max_width
    while current_width < max_width {
        result.push(' ');
        current_width += 1;
    }
    result
}
