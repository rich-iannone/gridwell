use gridwell_core::Length;
use gridwell_ir::content::ContentNode;
use gridwell_ir::style::StyleDef;
use gridwell_ir::{Cell, ColumnVisibility, Row, Table};
use std::fmt::Write;
use thiserror::Error;

use crate::TypstWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

struct TypstRenderer<'a> {
    table: &'a Table,
    config: &'a TypstWriterConfig,
    buf: String,
    indent_level: usize,
    /// Hidden columns are excluded from `columns:`, so every cell's position and span
    /// must be projected onto the visible columns.
    visibility: ColumnVisibility,
}

impl<'a> TypstRenderer<'a> {
    fn new(table: &'a Table, config: &'a TypstWriterConfig) -> Self {
        Self {
            table,
            config,
            buf: String::with_capacity(4096),
            indent_level: 0,
            visibility: ColumnVisibility::from_spec(&table.column_spec),
        }
    }

    fn indent(&self) -> String {
        "  ".repeat(self.indent_level)
    }

    fn line(&mut self, content: &str) {
        let indent = self.indent();
        writeln!(self.buf, "{indent}{content}").unwrap();
    }

    fn push(&mut self) {
        self.indent_level += 1;
    }

    fn pop(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.render_title();
        self.render_table();
        self.render_footnotes();
        Ok(self.buf)
    }

    // ─── Title / Subtitle ───

    fn render_title(&mut self) {
        if let Some(ref header) = self.table.header {
            if let Some(ref title) = header.title {
                let text = content_to_typst(&title.content);
                if !text.is_empty() {
                    self.line(&format!(
                        "#align(left)[#text(size: 16pt, weight: \"bold\")[{text}]]"
                    ));
                }
            }
            if let Some(ref subtitle) = header.subtitle {
                let text = content_to_typst(&subtitle.content);
                if !text.is_empty() {
                    self.line(&format!(
                        "#align(left)[#text(size: 12pt, fill: luma(100))[{text}]]"
                    ));
                    self.line("#v(6pt)");
                }
            }
        }
    }

    // ─── Table ───

    fn render_table(&mut self) {
        // With every column hidden there is no table to draw (and `columns: ()` would
        // not compile).
        if self.visibility.visible_len() == 0 {
            return;
        }
        let columns = self.build_columns();
        self.line("#table(");
        self.push();
        self.line(&format!("columns: ({columns}),"));

        // Stroke (minimal by default)
        self.line("stroke: none,");

        // Header
        if !self.table.config.column_labels_hidden {
            self.render_thead();
        }

        // Body
        self.render_tbody();

        self.pop();
        self.line(")");
    }

    fn build_columns(&self) -> String {
        self.table
            .column_spec
            .iter()
            .filter(|c| !c.hidden)
            // Unparseable widths fall back to `auto` rather than emitting invalid Typst.
            .map(|col| typst_length(&col.width).unwrap_or_else(|| "auto".to_string()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render_thead(&mut self) {
        if self.config.repeat_header {
            self.line("table.header(");
        }
        self.push();

        for row in &self.table.table.thead.rows {
            self.render_row(row, true);
        }

        // Horizontal line after header (none when there is no header row)
        if !self.table.table.thead.rows.is_empty() {
            self.line("table.hline(stroke: 0.5pt),");
        }

        self.pop();
        if self.config.repeat_header {
            self.line("),");
        }
    }

    fn render_tbody(&mut self) {
        for (g, group) in self.table.table.tbody.iter().enumerate() {
            // Group label
            if let Some(ref label) = group.label {
                let text = content_to_typst(&label.content);
                let cols = self.visibility.visible_len();
                self.line("table.hline(stroke: 0.5pt),");
                self.line(&format!(
                    "table.cell(colspan: {cols}, fill: luma(240))[#text(weight: \"bold\")[{text}]],"
                ));
                self.line("table.hline(stroke: 0.5pt),");
            }

            // Data rows
            for row in &group.rows {
                self.render_row(row, false);
            }

            // Summary rows
            if !group.summary_rows.is_empty() {
                self.line("table.hline(stroke: 0.3pt),");
                for row in &group.summary_rows {
                    self.render_row(row, false);
                }
            }

            // Separator between groups
            if g < self.table.table.tbody.len() - 1 {
                self.line("table.hline(stroke: 0.3pt),");
            }
        }
    }

    fn render_row(&mut self, row: &Row, is_header: bool) {
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.is_placeholder {
                continue;
            }
            // Cells entirely in hidden columns are dropped; spans crossing hidden
            // columns shrink.
            let Some((_, colspan)) = self.visibility.project(col, cell.colspan as usize) else {
                continue;
            };
            self.render_cell(cell, colspan, is_header);
        }
    }

    fn render_cell(&mut self, cell: &Cell, colspan: usize, is_header: bool) {
        let content = content_to_typst(&cell.content);

        // Build cell attributes
        let mut attrs = Vec::new();

        if colspan > 1 {
            attrs.push(format!("colspan: {colspan}"));
        }
        if cell.rowspan > 1 {
            attrs.push(format!("rowspan: {}", cell.rowspan));
        }

        // Apply style properties
        if let Some(ref style_id) = cell.style_id {
            if let Some(def) = self.resolve_style(style_id) {
                if let Some(ref bg) = def.background_color {
                    if let Some(fill) = color_to_typst(bg) {
                        attrs.push(format!("fill: {fill}"));
                    }
                }
            }
        }

        // Build the content with text styling
        let styled_content = if let Some(ref style_id) = cell.style_id {
            self.style_content(&content, style_id, is_header)
        } else if is_header {
            format!("#text(weight: \"bold\")[{content}]")
        } else {
            content.clone()
        };

        if attrs.is_empty() {
            self.line(&format!("[{styled_content}],"));
        } else {
            let attr_str = attrs.join(", ");
            self.line(&format!("table.cell({attr_str})[{styled_content}],"));
        }
    }

    fn style_content(&self, content: &str, style_id: &str, is_header: bool) -> String {
        let def = match self.resolve_style(style_id) {
            Some(d) => d,
            None => {
                return if is_header {
                    format!("#text(weight: \"bold\")[{content}]")
                } else {
                    content.to_string()
                };
            }
        };

        let mut text_attrs = Vec::new();

        if let Some(ref weight) = def.font_weight {
            if weight.is_bold() {
                text_attrs.push("weight: \"bold\"".to_string());
            }
        } else if is_header {
            text_attrs.push("weight: \"bold\"".to_string());
        }

        if let Some(ref style) = def.font_style {
            if style.is_italic() {
                text_attrs.push("style: \"italic\"".to_string());
            }
        }

        if let Some(size) = def.font_size.as_deref().and_then(typst_length) {
            text_attrs.push(format!("size: {size}"));
        }

        if let Some(ref color) = def.color {
            if let Some(c) = color_to_typst(color) {
                text_attrs.push(format!("fill: {c}"));
            }
        }

        if let Some(ref family) = def.font_family {
            if family.contains("monospace") || family.contains("Courier") {
                text_attrs.push("font: \"Courier New\"".to_string());
            }
        }

        if text_attrs.is_empty() {
            content.to_string()
        } else {
            format!("#text({})[{content}]", text_attrs.join(", "))
        }
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
                self.line("");
                // A blank line between notes: consecutive source lines would be
                // joined into one paragraph.
                for (i, note) in footer.footnotes.iter().enumerate() {
                    if i > 0 {
                        self.line("");
                    }
                    let text = content_to_typst(&note.content);
                    let mark = escape_typst(&note.mark);
                    self.line(&format!("#text(size: 9pt)[#super[{mark}] {text}]"));
                }
            }
            if !footer.source_notes.is_empty() {
                self.line("");
                for (i, note) in footer.source_notes.iter().enumerate() {
                    if i > 0 {
                        self.line("");
                    }
                    let text = content_to_typst(&note.content);
                    self.line(&format!("#text(size: 9pt, fill: luma(100))[{text}]"));
                }
            }
        }
    }
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &TypstWriterConfig) -> Result<String, RenderError> {
    let renderer = TypstRenderer::new(table, config);
    renderer.render()
}

// ─── Content Rendering ───

fn content_to_typst(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => {
                out.push_str(&escape_typst(value));
            }
            ContentNode::StyledText { value, style_id: _ } => {
                out.push_str(&escape_typst(value));
            }
            ContentNode::LineBreak {} => {
                out.push_str("\\ ");
            }
            ContentNode::FootnoteMark { mark_text, .. } => {
                write!(out, "#super[{}]", escape_typst(mark_text)).unwrap();
            }
            ContentNode::Image { alt, .. } => {
                if let Some(ref alt_text) = alt {
                    out.push_str(&escape_typst(alt_text));
                } else {
                    out.push_str("[image]");
                }
            }
            ContentNode::Raw { format, value } => {
                if format == "typst" {
                    out.push_str(value);
                }
            }
            ContentNode::Unknown => {}
        }
    }
    out
}

// ─── Escaping ───

/// Escape text for Typst markup so it renders verbatim.
///
/// Every ASCII punctuation character gets a backslash. Typst accepts `\<char>` for
/// any of them, and escaping all of them (not just the obvious `# [ ] $ \\`) also
/// neutralizes emphasis (`*`, `_`), raw (`` ` ``), comments (`//`, `/*`), labels and
/// references (`<`, `>`, `@`), line-start markers (`=`, `-`, `+`, `/ term:`), and
/// shorthands (`--`, `...`, `~`, `-5` → minus sign, smart quotes). Raw newlines
/// become spaces; explicit breaks are `line_break` content nodes.
fn escape_typst(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    for ch in s.chars() {
        match ch {
            '\r' | '\n' => out.push(' '),
            c if c.is_ascii_punctuation() => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// Convert an IR length (CSS-like) to a Typst length, or `None` if it can't be
/// expressed. Typst has no `px` or `rem`: px → pt at 0.75pt/px (CSS reference pixel),
/// rem → em (Typst has no root font size).
fn typst_length(s: &str) -> Option<String> {
    fn num(v: f64) -> Option<String> {
        // f64 Display never uses exponent notation for these magnitudes and drops
        // a trailing ".0", which is what Typst wants.
        v.is_finite()
            .then(|| format!("{}", (v * 1e4).round() / 1e4))
    }
    let length: Length = s.parse().ok()?;
    Some(match length {
        Length::Auto => "auto".to_string(),
        Length::Px(v) => format!("{}pt", num(v * 0.75)?),
        Length::Pt(v) => format!("{}pt", num(v)?),
        Length::Em(v) | Length::Rem(v) => format!("{}em", num(v)?),
        Length::In(v) => format!("{}in", num(v)?),
        Length::Cm(v) => format!("{}cm", num(v)?),
        Length::Mm(v) => format!("{}mm", num(v)?),
        Length::Percent(v) => format!("{}%", num(v)?),
        Length::Fr(v) => format!("{}fr", num(v)?),
    })
}

// ─── Color ───

fn color_to_typst(color: &str) -> Option<String> {
    let hex = color.strip_prefix('#')?;
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("rgb(\"#{hex}\")"))
    } else {
        None
    }
}

fn merge_style_def(base: &StyleDef, overrides: &StyleDef) -> StyleDef {
    StyleDef {
        font_family: overrides
            .font_family
            .clone()
            .or_else(|| base.font_family.clone()),
        font_size: overrides
            .font_size
            .clone()
            .or_else(|| base.font_size.clone()),
        font_weight: overrides
            .font_weight
            .clone()
            .or_else(|| base.font_weight.clone()),
        font_style: overrides
            .font_style
            .clone()
            .or_else(|| base.font_style.clone()),
        color: overrides.color.clone().or_else(|| base.color.clone()),
        background_color: overrides
            .background_color
            .clone()
            .or_else(|| base.background_color.clone()),
        text_align: overrides
            .text_align
            .clone()
            .or_else(|| base.text_align.clone()),
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
        white_space: overrides
            .white_space
            .clone()
            .or_else(|| base.white_space.clone()),
        padding: overrides.padding.clone().or_else(|| base.padding.clone()),
        border: overrides.border.clone().or_else(|| base.border.clone()),
        indent: overrides.indent.clone().or_else(|| base.indent.clone()),
        word_break: overrides
            .word_break
            .clone()
            .or_else(|| base.word_break.clone()),
        overflow: overrides.overflow.clone().or_else(|| base.overflow.clone()),
        text_overflow: overrides
            .text_overflow
            .clone()
            .or_else(|| base.text_overflow.clone()),
        min_width: overrides
            .min_width
            .clone()
            .or_else(|| base.min_width.clone()),
        max_width: overrides
            .max_width
            .clone()
            .or_else(|| base.max_width.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_convert_to_valid_typst() {
        for (input, want) in [
            ("auto", Some("auto")),
            ("120px", Some("90pt")),
            ("15px", Some("11.25pt")),
            ("12pt", Some("12pt")),
            ("1.5em", Some("1.5em")),
            ("2rem", Some("2em")),
            ("1in", Some("1in")),
            ("2.5cm", Some("2.5cm")),
            ("10mm", Some("10mm")),
            ("25%", Some("25%")),
            ("1fr", Some("1fr")),
            ("2fr", Some("2fr")),
            ("", None),
            ("wide", None),
            ("12", None),
            ("NaNpx", None),
            ("infpt", None),
        ] {
            assert_eq!(typst_length(input).as_deref(), want, "input {input:?}");
        }
    }

    #[test]
    fn escape_backslashes_all_ascii_punctuation_only() {
        assert_eq!(escape_typst("a*b"), "a\\*b");
        assert_eq!(escape_typst("-5 // x"), "\\-5 \\/\\/ x");
        assert_eq!(escape_typst("é 東京 😀 abc 123"), "é 東京 😀 abc 123");
        assert_eq!(escape_typst("a\nb\r\nc"), "a b  c");
        for c in (0u8..128)
            .map(char::from)
            .filter(|c| c.is_ascii_punctuation())
        {
            assert_eq!(escape_typst(&c.to_string()), format!("\\{c}"));
        }
    }
}
