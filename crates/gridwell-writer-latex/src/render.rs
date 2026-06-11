use gridwell_ir::content::ContentNode;
use gridwell_ir::style::StyleDef;
use gridwell_ir::{Cell, Row, Table};
use std::fmt::Write;
use thiserror::Error;

use crate::LatexWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

struct LatexRenderer<'a> {
    table: &'a Table,
    config: &'a LatexWriterConfig,
    buf: String,
}

impl<'a> LatexRenderer<'a> {
    fn new(table: &'a Table, config: &'a LatexWriterConfig) -> Self {
        Self {
            table,
            config,
            buf: String::with_capacity(4096),
        }
    }

    fn use_longtable(&self) -> bool {
        if self.config.longtable {
            return true;
        }
        if let Some(threshold) = self.config.longtable_threshold {
            return self.table.config.body_rows > threshold;
        }
        false
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.render_title();
        self.render_begin();
        self.render_toprule();
        self.render_thead();
        self.render_tbody();
        self.render_bottomrule();
        self.render_end();
        self.render_footnotes();
        Ok(self.buf)
    }

    // ─── Title / Subtitle ───

    fn render_title(&mut self) {
        if let Some(ref header) = self.table.header {
            if let Some(ref title) = header.title {
                let text = content_to_latex(&title.content);
                if !text.is_empty() {
                    writeln!(self.buf, "{{\\large\\bfseries {text}}}\\\\").unwrap();
                }
            }
            if let Some(ref subtitle) = header.subtitle {
                let text = content_to_latex(&subtitle.content);
                if !text.is_empty() {
                    writeln!(self.buf, "{{\\small {text}}}\\\\[6pt]").unwrap();
                }
            }
        }
    }

    // ─── Table Environment ───

    fn render_begin(&mut self) {
        let col_spec = self.build_column_spec();
        if self.use_longtable() {
            writeln!(self.buf, "\\begin{{longtable}}{{{col_spec}}}").unwrap();
        } else {
            writeln!(self.buf, "\\begin{{tabular}}{{{col_spec}}}").unwrap();
        }
    }

    fn render_end(&mut self) {
        if self.use_longtable() {
            writeln!(self.buf, "\\end{{longtable}}").unwrap();
        } else {
            writeln!(self.buf, "\\end{{tabular}}").unwrap();
        }
    }

    fn build_column_spec(&self) -> String {
        self.table
            .column_spec
            .iter()
            .filter(|c| !c.hidden)
            .map(|col| {
                if col.width != "auto" {
                    // Use p{width} for fixed-width columns
                    format!("p{{{}}}", col.width)
                } else {
                    match col.align.as_str() {
                        "right" => "r".to_string(),
                        "center" => "c".to_string(),
                        _ => "l".to_string(),
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    // ─── Rules ───

    fn render_toprule(&mut self) {
        if self.config.booktabs {
            writeln!(self.buf, "\\toprule").unwrap();
        } else {
            writeln!(self.buf, "\\hline").unwrap();
        }
    }

    fn render_midrule(&mut self) {
        if self.config.booktabs {
            writeln!(self.buf, "\\midrule").unwrap();
        } else {
            writeln!(self.buf, "\\hline").unwrap();
        }
    }

    fn render_bottomrule(&mut self) {
        if self.config.booktabs {
            writeln!(self.buf, "\\bottomrule").unwrap();
        } else {
            writeln!(self.buf, "\\hline").unwrap();
        }
    }

    // ─── Thead ───

    fn render_thead(&mut self) {
        if self.table.config.column_labels_hidden {
            return;
        }

        for row in &self.table.table.thead.rows {
            self.render_row(row);
        }
        self.render_midrule();

        // For longtable, mark end of header
        if self.use_longtable() {
            writeln!(self.buf, "\\endhead").unwrap();
        }
    }

    // ─── Tbody ───

    fn render_tbody(&mut self) {
        for (g, group) in self.table.table.tbody.iter().enumerate() {
            // Group label
            if let Some(ref label) = group.label {
                let text = content_to_latex(&label.content);
                let cols = self.table.config.table_cols;
                if self.config.booktabs {
                    writeln!(self.buf, "\\midrule").unwrap();
                }
                writeln!(
                    self.buf,
                    "\\multicolumn{{{cols}}}{{l}}{{\\bfseries {text}}} \\\\"
                )
                .unwrap();
                if self.config.booktabs {
                    writeln!(self.buf, "\\midrule").unwrap();
                }
            }

            // Data rows
            for row in &group.rows {
                self.render_row(row);
            }

            // Summary rows
            if !group.summary_rows.is_empty() {
                // Light separator before summary
                if self.config.booktabs {
                    writeln!(self.buf, "\\cmidrule(lr){{1-{}}}", self.table.config.table_cols)
                        .unwrap();
                }
                for row in &group.summary_rows {
                    self.render_row(row);
                }
            }

            // Separator between groups (not after last)
            if g < self.table.table.tbody.len() - 1 && group.label.is_none() {
                writeln!(self.buf, "\\addlinespace").unwrap();
            }
        }
    }

    fn render_row(&mut self, row: &Row) {
        let cells: Vec<String> = row
            .cells
            .iter()
            .filter(|c| !c.is_placeholder)
            .map(|cell| self.render_cell(cell))
            .collect();

        writeln!(self.buf, "{} \\\\", cells.join(" & ")).unwrap();
    }

    fn render_cell(&self, cell: &Cell) -> String {
        let content = content_to_latex(&cell.content);

        // Apply style (bold, italic, color)
        let styled = if let Some(ref style_id) = cell.style_id {
            self.apply_style(&content, style_id)
        } else {
            content
        };

        // Handle multicolumn
        if cell.colspan > 1 {
            let align = self.cell_alignment(cell);
            return format!("\\multicolumn{{{}}}{{{align}}}{{{styled}}}", cell.colspan);
        }

        // Handle multirow
        if cell.rowspan > 1 {
            return format!("\\multirow{{{}}}{{*}}{{{styled}}}", cell.rowspan);
        }

        styled
    }

    fn cell_alignment(&self, cell: &Cell) -> &str {
        // Try to infer alignment from the style
        if let Some(ref style_id) = cell.style_id {
            if let Some(def) = self.resolve_style(style_id) {
                if let Some(ref align) = def.text_align {
                    return match align.as_str() {
                        "right" => "r",
                        "center" => "c",
                        _ => "l",
                    };
                }
            }
        }
        "l"
    }

    fn apply_style(&self, content: &str, style_id: &str) -> String {
        let def = match self.resolve_style(style_id) {
            Some(d) => d,
            None => return content.to_string(),
        };

        let mut result = content.to_string();

        // Apply font weight
        if let Some(ref weight) = def.font_weight {
            if weight == "bold" {
                result = format!("\\textbf{{{result}}}");
            }
        }

        // Apply font style
        if let Some(ref style) = def.font_style {
            if style == "italic" {
                result = format!("\\textit{{{result}}}");
            }
        }

        // Apply color
        if let Some(ref color) = def.color {
            if let Some(latex_color) = hex_to_latex_color(color) {
                result = format!("\\textcolor{{{latex_color}}}{{{result}}}");
            }
        }

        // Apply background color
        if let Some(ref bg) = def.background_color {
            if let Some(latex_color) = hex_to_latex_color(bg) {
                result = format!("\\cellcolor{{{latex_color}}}{result}");
            }
        }

        // Apply monospace font
        if let Some(ref family) = def.font_family {
            if family.contains("monospace") || family.contains("Courier") {
                result = format!("\\texttt{{{result}}}");
            }
        }

        result
    }

    fn resolve_style(&self, id: &str) -> Option<StyleDef> {
        if let Some(def) = self.table.styles.defs.get(id) {
            return Some(def.clone());
        }
        if let Some(comp) = self.table.styles.compositions.get(id) {
            if let Some(base) = self.table.styles.defs.get(&comp.extends) {
                return Some(merge_style_def(base, &comp.overrides));
            }
        }
        None
    }

    // ─── Footnotes ───

    fn render_footnotes(&mut self) {
        if let Some(ref footer) = self.table.footer {
            if !footer.footnotes.is_empty() {
                writeln!(self.buf).unwrap();
                for note in &footer.footnotes {
                    let text = content_to_latex(&note.content);
                    writeln!(
                        self.buf,
                        "\\textsuperscript{{{mark}}} {text}\\\\",
                        mark = escape_latex(&note.mark)
                    )
                    .unwrap();
                }
            }
            if !footer.source_notes.is_empty() {
                writeln!(self.buf).unwrap();
                for note in &footer.source_notes {
                    let text = content_to_latex(&note.content);
                    writeln!(self.buf, "{{\\footnotesize {text}}}\\\\").unwrap();
                }
            }
        }
    }
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &LatexWriterConfig) -> Result<String, RenderError> {
    let renderer = LatexRenderer::new(table, config);
    renderer.render()
}

// ─── Content Rendering ───

fn content_to_latex(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => {
                out.push_str(&escape_latex(value));
            }
            ContentNode::StyledText { value, style_id: _ } => {
                // Style is applied at the cell level; here we just output the text
                out.push_str(&escape_latex(value));
            }
            ContentNode::LineBreak {} => {
                out.push_str("\\newline ");
            }
            ContentNode::FootnoteMark { mark_text, .. } => {
                write!(out, "\\textsuperscript{{{}}}", escape_latex(mark_text)).unwrap();
            }
            ContentNode::Image { alt, .. } => {
                // LaTeX can't inline arbitrary images easily; use alt text
                if let Some(ref alt_text) = alt {
                    out.push_str(&escape_latex(alt_text));
                } else {
                    out.push_str("[image]");
                }
            }
            ContentNode::Raw { format, value } => {
                if format == "latex" {
                    out.push_str(value);
                }
            }
            ContentNode::Unknown => {}
        }
    }
    out
}

// ─── LaTeX Escaping ───

fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '\\' => out.push_str("\\textbackslash{}"),
            _ => out.push(c),
        }
    }
    out
}

// ─── Color Handling ───

fn hex_to_latex_color(color: &str) -> Option<String> {
    let hex = color.strip_prefix('#')?;
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("[HTML]{{{}}}", hex.to_uppercase()))
    } else {
        None
    }
}

fn merge_style_def(base: &StyleDef, overrides: &StyleDef) -> StyleDef {
    StyleDef {
        font_family: overrides.font_family.clone().or_else(|| base.font_family.clone()),
        font_size: overrides.font_size.clone().or_else(|| base.font_size.clone()),
        font_weight: overrides.font_weight.clone().or_else(|| base.font_weight.clone()),
        font_style: overrides.font_style.clone().or_else(|| base.font_style.clone()),
        color: overrides.color.clone().or_else(|| base.color.clone()),
        background_color: overrides
            .background_color
            .clone()
            .or_else(|| base.background_color.clone()),
        text_align: overrides.text_align.clone().or_else(|| base.text_align.clone()),
        vertical_align: overrides
            .vertical_align
            .clone()
            .or_else(|| base.vertical_align.clone()),
        text_transform: overrides
            .text_transform
            .clone()
            .or_else(|| base.text_transform.clone()),
        text_decoration: overrides
            .text_decoration
            .clone()
            .or_else(|| base.text_decoration.clone()),
        white_space: overrides.white_space.clone().or_else(|| base.white_space.clone()),
        padding: overrides.padding.clone().or_else(|| base.padding.clone()),
        border: overrides.border.clone().or_else(|| base.border.clone()),
        indent: overrides.indent.clone().or_else(|| base.indent.clone()),
        word_break: overrides.word_break.clone().or_else(|| base.word_break.clone()),
        overflow: overrides.overflow.clone().or_else(|| base.overflow.clone()),
        text_overflow: overrides
            .text_overflow
            .clone()
            .or_else(|| base.text_overflow.clone()),
        min_width: overrides.min_width.clone().or_else(|| base.min_width.clone()),
        max_width: overrides.max_width.clone().or_else(|| base.max_width.clone()),
    }
}
