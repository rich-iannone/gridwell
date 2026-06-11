use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table};
use std::fmt::Write;
use thiserror::Error;

use crate::SvgConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

struct SvgRenderer<'a> {
    table: &'a Table,
    config: &'a SvgConfig,
    buf: String,
    col_widths: Vec<f64>,
    y_offset: f64,
}

impl<'a> SvgRenderer<'a> {
    fn new(table: &'a Table, config: &'a SvgConfig) -> Self {
        let col_widths: Vec<f64> = table
            .column_spec
            .iter()
            .map(|col| {
                if col.width == "auto" || col.hidden {
                    config.default_col_width
                } else if let Some(px) = col.width.strip_suffix("px") {
                    px.parse::<f64>().unwrap_or(config.default_col_width)
                } else {
                    config.default_col_width
                }
            })
            .collect();

        Self {
            table,
            config,
            buf: String::with_capacity(8192),
            col_widths,
            y_offset: 0.0,
        }
    }

    fn total_width(&self) -> f64 {
        self.col_widths.iter().sum()
    }

    fn render(mut self) -> Result<String, RenderError> {
        // Calculate total height (rough estimate first, then accurate)
        let total_height = self.estimate_height();
        let total_width = self.total_width();

        // SVG header
        writeln!(
            self.buf,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_width}\" \
             height=\"{total_height}\" viewBox=\"0 0 {total_width} {total_height}\">"
        )?;
        writeln!(
            self.buf,
            "<style>text {{ font-family: {}; font-size: {}px; }} </style>",
            escape_xml(&self.config.font_family),
            self.config.font_size
        )?;

        // Title/subtitle
        self.render_title()?;

        // Header rows
        if !self.table.config.column_labels_hidden {
            for row in &self.table.table.thead.rows {
                self.render_row(row, true)?;
            }
        }

        // Body rows
        for group in &self.table.table.tbody {
            if let Some(ref label) = group.label {
                self.render_group_label(label)?;
            }
            for row in &group.rows {
                self.render_row(row, false)?;
            }
            for row in &group.summary_rows {
                self.render_row(row, false)?;
            }
        }

        // Footnotes
        self.render_footnotes()?;

        self.buf.push_str("</svg>\n");
        Ok(self.buf)
    }

    fn estimate_height(&self) -> f64 {
        let mut rows = 0;

        // Title + subtitle
        if let Some(ref header) = self.table.header {
            if header.title.is_some() {
                rows += 1;
            }
            if header.subtitle.is_some() {
                rows += 1;
            }
        }

        // Header rows
        if !self.table.config.column_labels_hidden {
            rows += self.table.table.thead.rows.len();
        }

        // Body rows
        for group in &self.table.table.tbody {
            if group.label.is_some() {
                rows += 1;
            }
            rows += group.rows.len();
            rows += group.summary_rows.len();
        }

        // Footnotes
        if let Some(ref footer) = self.table.footer {
            rows += footer.footnotes.len();
            rows += footer.source_notes.len();
        }

        (rows as f64) * self.config.row_height + self.config.cell_padding_y * 2.0
    }

    fn render_title(&mut self) -> Result<(), RenderError> {
        if let Some(ref header) = self.table.header {
            if let Some(ref title) = header.title {
                let text = content_to_text(&title.content);
                let x = self.config.cell_padding_x;
                self.y_offset += self.config.row_height;
                writeln!(
                    self.buf,
                    "<text x=\"{x}\" y=\"{y}\" font-weight=\"bold\" \
                     font-size=\"{fs}px\">{text}</text>",
                    y = self.y_offset - self.config.cell_padding_y,
                    fs = self.config.font_size * 1.4,
                    text = escape_xml(&text)
                )?;
            }
            if let Some(ref subtitle) = header.subtitle {
                let text = content_to_text(&subtitle.content);
                let x = self.config.cell_padding_x;
                self.y_offset += self.config.row_height;
                writeln!(
                    self.buf,
                    "<text x=\"{x}\" y=\"{y}\" font-size=\"{fs}px\">{text}</text>",
                    y = self.y_offset - self.config.cell_padding_y,
                    fs = self.config.font_size * 1.1,
                    text = escape_xml(&text)
                )?;
            }
        }
        Ok(())
    }

    fn render_group_label(
        &mut self,
        label: &gridwell_ir::cell::GroupLabel,
    ) -> Result<(), RenderError> {
        let text = content_to_text(&label.content);
        let total_width = self.total_width();
        self.y_offset += self.config.row_height;

        // Background
        writeln!(
            self.buf,
            "<rect x=\"0\" y=\"{y}\" width=\"{total_width}\" \
             height=\"{h}\" fill=\"#f0f0f0\"/>",
            y = self.y_offset - self.config.row_height,
            h = self.config.row_height,
        )?;

        writeln!(
            self.buf,
            "<text x=\"{x}\" y=\"{y}\" font-weight=\"bold\">{text}</text>",
            x = self.config.cell_padding_x,
            y = self.y_offset - self.config.cell_padding_y,
            text = escape_xml(&text)
        )?;
        Ok(())
    }

    fn render_row(&mut self, row: &Row, is_header: bool) -> Result<(), RenderError> {
        self.y_offset += self.config.row_height;
        let y_top = self.y_offset - self.config.row_height;

        let mut x = 0.0;
        let mut col_idx = 0;

        for cell in &row.cells {
            if cell.is_placeholder {
                x += self.col_widths.get(col_idx).copied().unwrap_or(0.0);
                col_idx += 1;
                continue;
            }

            let span = cell.colspan as usize;
            let cell_width: f64 = self.col_widths
                [col_idx..col_idx + span.min(self.col_widths.len() - col_idx)]
                .iter()
                .sum();

            // Cell background
            if let Some(ref style_id) = cell.style_id {
                if let Some(def) = self.table.styles.defs.get(style_id.as_str()) {
                    if let Some(ref bg) = def.background_color {
                        writeln!(
                            self.buf,
                            "<rect x=\"{x}\" y=\"{y_top}\" width=\"{cell_width}\" \
                             height=\"{h}\" fill=\"{bg}\"/>",
                            h = self.config.row_height,
                        )?;
                    }
                }
            }

            // Cell bottom border
            writeln!(
                self.buf,
                "<line x1=\"{x}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" \
                 stroke=\"#d0d0d0\" stroke-width=\"0.5\"/>",
                y = self.y_offset,
                x2 = x + cell_width,
            )?;

            // Cell text
            let text = content_to_text(&cell.content);
            if !text.is_empty() {
                let tx = x + self.config.cell_padding_x;
                let ty = self.y_offset - self.config.cell_padding_y;
                let weight = if is_header { " font-weight=\"bold\"" } else { "" };
                let mut color_attr = String::new();
                if let Some(ref style_id) = cell.style_id {
                    if let Some(def) = self.table.styles.defs.get(style_id.as_str()) {
                        if let Some(ref color) = def.color {
                            write!(color_attr, " fill=\"{color}\"")?;
                        }
                    }
                }
                writeln!(
                    self.buf,
                    "<text x=\"{tx}\" y=\"{ty}\"{weight}{color_attr}>{text}</text>",
                    text = escape_xml(&text)
                )?;
            }

            x += cell_width;
            col_idx += span;
        }

        // Header bottom border (thicker)
        if is_header {
            let total_width = self.total_width();
            writeln!(
                self.buf,
                "<line x1=\"0\" y1=\"{y}\" x2=\"{total_width}\" y2=\"{y}\" \
                 stroke=\"#333333\" stroke-width=\"1.5\"/>",
                y = self.y_offset,
            )?;
        }

        Ok(())
    }

    fn render_footnotes(&mut self) -> Result<(), RenderError> {
        if let Some(ref footer) = self.table.footer {
            if !footer.footnotes.is_empty() {
                self.y_offset += self.config.cell_padding_y * 2.0;
                for note in &footer.footnotes {
                    self.y_offset += self.config.row_height * 0.75;
                    let text = content_to_text(&note.content);
                    let x = self.config.cell_padding_x;
                    writeln!(
                        self.buf,
                        "<text x=\"{x}\" y=\"{y}\" font-size=\"{fs}px\">\
                         <tspan font-size=\"{sfs}px\" baseline-shift=\"super\">{mark}</tspan> \
                         {text}</text>",
                        y = self.y_offset - self.config.cell_padding_y,
                        fs = self.config.font_size * 0.85,
                        sfs = self.config.font_size * 0.7,
                        mark = escape_xml(&note.mark),
                        text = escape_xml(&text)
                    )?;
                }
            }
            if !footer.source_notes.is_empty() {
                self.y_offset += self.config.cell_padding_y * 2.0;
                for note in &footer.source_notes {
                    self.y_offset += self.config.row_height * 0.75;
                    let text = content_to_text(&note.content);
                    let x = self.config.cell_padding_x;
                    writeln!(
                        self.buf,
                        "<text x=\"{x}\" y=\"{y}\" font-size=\"{fs}px\">{text}</text>",
                        y = self.y_offset - self.config.cell_padding_y,
                        fs = self.config.font_size * 0.85,
                        text = escape_xml(&text)
                    )?;
                }
            }
        }
        Ok(())
    }
}

pub fn render(table: &Table, config: &SvgConfig) -> Result<String, RenderError> {
    SvgRenderer::new(table, config).render()
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

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
