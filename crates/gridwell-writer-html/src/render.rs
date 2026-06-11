use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{Border, BorderSet, Padding, StyleDef};
use gridwell_ir::{Cell, Row, Table};
use std::fmt::Write;
use thiserror::Error;

use crate::HtmlWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Internal writer state.
struct HtmlRenderer<'a> {
    table: &'a Table,
    config: &'a HtmlWriterConfig,
    buf: String,
    indent_level: usize,
}

impl<'a> HtmlRenderer<'a> {
    fn new(table: &'a Table, config: &'a HtmlWriterConfig) -> Self {
        Self {
            table,
            config,
            buf: String::with_capacity(4096),
            indent_level: 0,
        }
    }

    fn indent(&self) -> String {
        if self.config.pretty_print {
            "  ".repeat(self.indent_level)
        } else {
            String::new()
        }
    }

    fn nl(&self) -> &'static str {
        if self.config.pretty_print {
            "\n"
        } else {
            ""
        }
    }

    fn write_line(&mut self, content: &str) {
        let indent = self.indent();
        if self.config.pretty_print {
            let _ = writeln!(self.buf, "{indent}{content}");
        } else {
            let _ = write!(self.buf, "{content}");
        }
    }

    fn push_indent(&mut self) {
        self.indent_level += 1;
    }

    fn pop_indent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.render_container_open();
        self.render_style_block();
        self.render_header_section();
        self.render_table();
        self.render_footer_section();
        self.render_container_close();
        Ok(self.buf)
    }

    // ─── Container ───

    fn render_container_open(&mut self) {
        let mut attrs = Vec::new();
        attrs.push(format!("class=\"{}\"", self.container_class()));

        let mut style_parts = Vec::new();
        if let Some(ref w) = self.table.config.container_width {
            style_parts.push(format!("max-width: {w}"));
        }
        if let Some(ref h) = self.table.config.container_height {
            style_parts.push(format!("max-height: {h}"));
        }
        if let Some(ref o) = self.table.config.container_overflow {
            style_parts.push(format!("overflow: {o}"));
        }
        if !style_parts.is_empty() {
            attrs.push(format!("style=\"{}\"", style_parts.join("; ")));
        }

        self.write_line(&format!("<div {}>", attrs.join(" ")));
        self.push_indent();
    }

    fn render_container_close(&mut self) {
        self.pop_indent();
        self.write_line("</div>");
    }

    fn container_class(&self) -> String {
        format!("{}_container", self.config.class_prefix)
    }

    // ─── Style Block ───

    fn render_style_block(&mut self) {
        if self.config.inline_styles {
            return;
        }

        let styles = &self.table.styles;
        if styles.defs.is_empty() && styles.compositions.is_empty() && styles.conditionals.is_empty()
        {
            return;
        }

        self.write_line("<style>");
        self.push_indent();

        // Emit style definitions as CSS classes
        let mut style_ids: Vec<&String> = styles.defs.keys().collect();
        style_ids.sort();
        for id in style_ids {
            let def = &styles.defs[id];
            let class_name = format!(".{}_{}", self.config.class_prefix, id);
            let css = style_def_to_css(def);
            if !css.is_empty() {
                self.write_line(&format!("{class_name} {{ {css} }}"));
            }
        }

        // Emit compositions
        let mut comp_ids: Vec<&String> = styles.compositions.keys().collect();
        comp_ids.sort();
        for id in comp_ids {
            let comp = &styles.compositions[id];
            // Resolve the full style (base + overrides)
            if let Some(base) = styles.defs.get(&comp.extends) {
                let merged = merge_style_def(base, &comp.overrides);
                let class_name = format!(".{}_{}", self.config.class_prefix, id);
                let css = style_def_to_css(&merged);
                if !css.is_empty() {
                    self.write_line(&format!("{class_name} {{ {css} }}"));
                }
            }
        }

        self.pop_indent();
        self.write_line("</style>");
    }

    // ─── Header Section (title/subtitle) ───

    fn render_header_section(&mut self) {
        let header = match &self.table.header {
            Some(h) => h,
            None => return,
        };

        if header.title.is_none() && header.subtitle.is_none() && header.extra_lines.is_empty() {
            return;
        }

        if let Some(ref title) = header.title {
            let class = self.style_class_attr(&title.style_id);
            self.write_line(&format!("<div{class} role=\"heading\" aria-level=\"1\">"));
            self.push_indent();
            self.render_content_nodes(&title.content);
            self.pop_indent();
            self.write_line("</div>");
        }

        if let Some(ref subtitle) = header.subtitle {
            let class = self.style_class_attr(&subtitle.style_id);
            self.write_line(&format!("<div{class} role=\"heading\" aria-level=\"2\">"));
            self.push_indent();
            self.render_content_nodes(&subtitle.content);
            self.pop_indent();
            self.write_line("</div>");
        }

        for line in &header.extra_lines {
            let class = self.style_class_attr(&line.style_id);
            self.write_line(&format!("<div{class}>"));
            self.push_indent();
            self.render_content_nodes(&line.content);
            self.pop_indent();
            self.write_line("</div>");
        }
    }

    // ─── Table ───

    fn render_table(&mut self) {
        let mut attrs = Vec::new();
        attrs.push(format!("class=\"{}_table\"", self.config.class_prefix));

        if let Some(ref aria_label) = self.table.config.aria_label {
            attrs.push(format!("aria-label=\"{}\"", escape_attr(aria_label)));
        }

        if let Some(ref w) = self.table.config.table_width {
            attrs.push(format!("style=\"width: {w}\""));
        }

        self.write_line(&format!("<table {}>", attrs.join(" ")));
        self.push_indent();

        // Caption (from config.summary)
        if let Some(ref summary) = self.table.config.summary {
            self.write_line(&format!(
                "<caption class=\"{}_caption\">{}</caption>",
                self.config.class_prefix,
                escape_html(summary)
            ));
        }

        // Colgroup
        self.render_colgroup();

        // Thead
        self.render_thead();

        // Tbody groups
        self.render_tbody();

        self.pop_indent();
        self.write_line("</table>");
    }

    fn render_colgroup(&mut self) {
        let has_widths = self
            .table
            .column_spec
            .iter()
            .any(|c| c.width != "auto" && !c.hidden);

        if !has_widths {
            return;
        }

        self.write_line("<colgroup>");
        self.push_indent();
        for col in &self.table.column_spec {
            if col.hidden {
                continue;
            }
            if col.width == "auto" {
                self.write_line("<col>");
            } else {
                self.write_line(&format!("<col style=\"width: {}\">", col.width));
            }
        }
        self.pop_indent();
        self.write_line("</colgroup>");
    }

    fn render_thead(&mut self) {
        if self.table.table.thead.rows.is_empty() {
            return;
        }

        if self.table.config.column_labels_hidden {
            return;
        }

        self.write_line("<thead>");
        self.push_indent();

        for row in &self.table.table.thead.rows {
            self.render_row(row, true);
        }

        self.pop_indent();
        self.write_line("</thead>");
    }

    fn render_tbody(&mut self) {
        for group in &self.table.table.tbody {
            self.write_line("<tbody>");
            self.push_indent();

            // Group label row
            if let Some(ref label) = group.label {
                let colspan = label
                    .colspan
                    .unwrap_or(self.table.config.table_cols);
                let class = self.style_class_attr(&label.style_id);
                self.write_line("<tr>");
                self.push_indent();
                self.write_line(&format!(
                    "<td colspan=\"{colspan}\"{class}>",
                ));
                self.push_indent();
                self.render_content_nodes(&label.content);
                self.pop_indent();
                self.write_line("</td>");
                self.pop_indent();
                self.write_line("</tr>");
            }

            // Data rows
            for row in &group.rows {
                self.render_row(row, false);
            }

            // Summary rows
            for row in &group.summary_rows {
                self.render_row(row, false);
            }

            self.pop_indent();
            self.write_line("</tbody>");
        }
    }

    fn render_row(&mut self, row: &Row, is_header: bool) {
        let row_class = self.style_class_attr(&row.style_id);
        self.write_line(&format!("<tr{row_class}>"));
        self.push_indent();

        for cell in &row.cells {
            if cell.is_placeholder {
                continue; // Spanned-over positions are not rendered
            }
            self.render_cell(cell, is_header);
        }

        self.pop_indent();
        self.write_line("</tr>");
    }

    fn render_cell(&mut self, cell: &Cell, is_header: bool) {
        let tag = if is_header { "th" } else { "td" };
        let mut attrs = Vec::new();

        // Class from style_id
        if let Some(ref style_id) = cell.style_id {
            if !self.config.inline_styles {
                attrs.push(format!(
                    "class=\"{}_{}\"",
                    self.config.class_prefix, style_id
                ));
            } else if let Some(style_def) = self.resolve_style(style_id) {
                let css = style_def_to_css(&style_def);
                if !css.is_empty() {
                    attrs.push(format!("style=\"{css}\""));
                }
            }
        }

        // Scope for header cells
        if is_header {
            if let Some(ref scope) = cell.scope {
                attrs.push(format!("scope=\"{scope}\""));
            }
        }

        // Colspan/rowspan
        if cell.colspan > 1 {
            attrs.push(format!("colspan=\"{}\"", cell.colspan));
        }
        if cell.rowspan > 1 {
            attrs.push(format!("rowspan=\"{}\"", cell.rowspan));
        }

        let attr_str = if attrs.is_empty() {
            String::new()
        } else {
            format!(" {}", attrs.join(" "))
        };

        // For simple single-text cells, render inline
        if cell.content.len() == 1 {
            if let Some(text) = single_text_content(&cell.content) {
                self.write_line(&format!("<{tag}{attr_str}>{}</{tag}>", escape_html(text)));
                return;
            }
        }

        if cell.content.is_empty() {
            self.write_line(&format!("<{tag}{attr_str}></{tag}>"));
            return;
        }

        self.write_line(&format!("<{tag}{attr_str}>"));
        self.push_indent();
        self.render_content_nodes(&cell.content);
        self.pop_indent();
        self.write_line(&format!("</{tag}>"));
    }

    // ─── Footer ───

    fn render_footer_section(&mut self) {
        let footer = match &self.table.footer {
            Some(f) => f,
            None => return,
        };

        if footer.footnotes.is_empty() && footer.source_notes.is_empty() {
            return;
        }

        self.write_line(&format!(
            "<div class=\"{}_footer\">",
            self.config.class_prefix
        ));
        self.push_indent();

        // Footnotes
        if !footer.footnotes.is_empty() {
            self.write_line(&format!(
                "<div class=\"{}_footnotes\">",
                self.config.class_prefix
            ));
            self.push_indent();
            for note in &footer.footnotes {
                let class = self.style_class_attr(&note.style_id);
                self.write_line(&format!(
                    "<p id=\"{}\" {}>",
                    escape_attr(&note.id),
                    class.trim()
                ));
                self.push_indent();
                let indent = self.indent();
                let _ = write!(
                    self.buf,
                    "{indent}<sup>{}</sup> ",
                    escape_html(&note.mark)
                );
                self.render_content_nodes_inline(&note.content);
                let nl = self.nl();
                let _ = write!(self.buf, "{nl}");
                self.pop_indent();
                self.write_line("</p>");
            }
            self.pop_indent();
            self.write_line("</div>");
        }

        // Source notes
        if !footer.source_notes.is_empty() {
            self.write_line(&format!(
                "<div class=\"{}_source_notes\">",
                self.config.class_prefix
            ));
            self.push_indent();
            for note in &footer.source_notes {
                let class = self.style_class_attr(&note.style_id);
                self.write_line(&format!("<p{class}>"));
                self.push_indent();
                self.render_content_nodes(&note.content);
                self.pop_indent();
                self.write_line("</p>");
            }
            self.pop_indent();
            self.write_line("</div>");
        }

        self.pop_indent();
        self.write_line("</div>");
    }

    // ─── Content Nodes ───

    fn render_content_nodes(&mut self, nodes: &[ContentNode]) {
        let indent = self.indent();
        let _ = write!(self.buf, "{indent}");
        self.render_content_nodes_inline(nodes);
        let nl = self.nl();
        let _ = write!(self.buf, "{nl}");
    }

    fn render_content_nodes_inline(&mut self, nodes: &[ContentNode]) {
        for node in nodes {
            match node {
                ContentNode::Text { value } => {
                    let _ = write!(self.buf, "{}", escape_html(value));
                }
                ContentNode::StyledText { value, style_id } => {
                    if let Some(ref id) = style_id {
                        if self.config.inline_styles {
                            if let Some(style_def) = self.resolve_style(id) {
                                let css = style_def_to_css(&style_def);
                                let _ = write!(
                                    self.buf,
                                    "<span style=\"{css}\">{}</span>",
                                    escape_html(value)
                                );
                            } else {
                                let _ = write!(self.buf, "<span>{}</span>", escape_html(value));
                            }
                        } else {
                            let _ = write!(
                                self.buf,
                                "<span class=\"{}_{}\">{}</span>",
                                self.config.class_prefix,
                                id,
                                escape_html(value)
                            );
                        }
                    } else {
                        let _ = write!(self.buf, "<span>{}</span>", escape_html(value));
                    }
                }
                ContentNode::LineBreak {} => {
                    let _ = write!(self.buf, "<br>");
                }
                ContentNode::FootnoteMark {
                    reference,
                    mark_text,
                } => {
                    let _ = write!(
                        self.buf,
                        "<sup><a href=\"#{}\">{}</a></sup>",
                        escape_attr(reference),
                        escape_html(mark_text)
                    );
                }
                ContentNode::Image {
                    src,
                    alt,
                    width,
                    height,
                } => {
                    let mut img_attrs = vec![format!("src=\"{}\"", escape_attr(src))];
                    if let Some(ref alt_text) = alt {
                        img_attrs.push(format!("alt=\"{}\"", escape_attr(alt_text)));
                    }
                    let mut style_parts = Vec::new();
                    if let Some(ref w) = width {
                        style_parts.push(format!("width: {w}"));
                    }
                    if let Some(ref h) = height {
                        style_parts.push(format!("height: {h}"));
                    }
                    if !style_parts.is_empty() {
                        img_attrs.push(format!("style=\"{}\"", style_parts.join("; ")));
                    }
                    let _ = write!(self.buf, "<img {}>", img_attrs.join(" "));
                }
                ContentNode::Raw { format, value } => {
                    if format == "html" {
                        let _ = write!(self.buf, "{value}");
                    }
                    // Non-HTML raw content is skipped
                }
                ContentNode::Unknown => {}
            }
        }
    }

    // ─── Helpers ───

    fn style_class_attr(&self, style_id: &Option<String>) -> String {
        match style_id {
            Some(id) if !self.config.inline_styles => {
                format!(" class=\"{}_{}\"", self.config.class_prefix, id)
            }
            Some(id) if self.config.inline_styles => {
                if let Some(style_def) = self.resolve_style(id) {
                    let css = style_def_to_css(&style_def);
                    if !css.is_empty() {
                        return format!(" style=\"{css}\"");
                    }
                }
                String::new()
            }
            _ => String::new(),
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
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &HtmlWriterConfig) -> Result<String, RenderError> {
    let renderer = HtmlRenderer::new(table, config);
    renderer.render()
}

// ─── CSS Generation ───

fn style_def_to_css(def: &StyleDef) -> String {
    let mut parts = Vec::new();

    if let Some(ref v) = def.font_family {
        parts.push(format!("font-family: {v}"));
    }
    if let Some(ref v) = def.font_size {
        parts.push(format!("font-size: {v}"));
    }
    if let Some(ref v) = def.font_weight {
        parts.push(format!("font-weight: {v}"));
    }
    if let Some(ref v) = def.font_style {
        parts.push(format!("font-style: {v}"));
    }
    if let Some(ref v) = def.color {
        parts.push(format!("color: {v}"));
    }
    if let Some(ref v) = def.background_color {
        parts.push(format!("background-color: {v}"));
    }
    if let Some(ref v) = def.text_align {
        parts.push(format!("text-align: {v}"));
    }
    if let Some(ref v) = def.vertical_align {
        parts.push(format!("vertical-align: {v}"));
    }
    if let Some(ref v) = def.text_transform {
        parts.push(format!("text-transform: {v}"));
    }
    if let Some(ref v) = def.text_decoration {
        parts.push(format!("text-decoration: {v}"));
    }
    if let Some(ref v) = def.white_space {
        parts.push(format!("white-space: {v}"));
    }
    if let Some(ref v) = def.indent {
        parts.push(format!("text-indent: {v}"));
    }
    if let Some(ref v) = def.word_break {
        parts.push(format!("word-break: {v}"));
    }
    if let Some(ref v) = def.overflow {
        parts.push(format!("overflow: {v}"));
    }
    if let Some(ref v) = def.text_overflow {
        parts.push(format!("text-overflow: {v}"));
    }
    if let Some(ref v) = def.min_width {
        parts.push(format!("min-width: {v}"));
    }
    if let Some(ref v) = def.max_width {
        parts.push(format!("max-width: {v}"));
    }

    // Padding
    if let Some(ref p) = def.padding {
        if let Some(css) = padding_to_css(p) {
            parts.push(css);
        }
    }

    // Borders
    if let Some(ref b) = def.border {
        parts.extend(border_set_to_css(b));
    }

    parts.join("; ")
}

fn padding_to_css(p: &Padding) -> Option<String> {
    let top = p.top.as_deref().unwrap_or("0");
    let right = p.right.as_deref().unwrap_or("0");
    let bottom = p.bottom.as_deref().unwrap_or("0");
    let left = p.left.as_deref().unwrap_or("0");

    if top == "0" && right == "0" && bottom == "0" && left == "0" {
        return None;
    }

    // Use shorthand where possible
    if top == bottom && left == right && top == left {
        Some(format!("padding: {top}"))
    } else if top == bottom && left == right {
        Some(format!("padding: {top} {right}"))
    } else {
        Some(format!("padding: {top} {right} {bottom} {left}"))
    }
}

fn border_set_to_css(b: &BorderSet) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(ref t) = b.top {
        if let Some(css) = border_to_css(t) {
            parts.push(format!("border-top: {css}"));
        }
    }
    if let Some(ref r) = b.right {
        if let Some(css) = border_to_css(r) {
            parts.push(format!("border-right: {css}"));
        }
    }
    if let Some(ref bo) = b.bottom {
        if let Some(css) = border_to_css(bo) {
            parts.push(format!("border-bottom: {css}"));
        }
    }
    if let Some(ref l) = b.left {
        if let Some(css) = border_to_css(l) {
            parts.push(format!("border-left: {css}"));
        }
    }
    parts
}

fn border_to_css(b: &Border) -> Option<String> {
    let style = b.style.as_deref().unwrap_or("none");
    if style == "none" {
        return None;
    }
    let width = b.width.as_deref().unwrap_or("1px");
    let color = b.color.as_deref().unwrap_or("currentColor");
    Some(format!("{width} {style} {color}"))
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

// ─── HTML Escaping ───

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn single_text_content(nodes: &[ContentNode]) -> Option<&str> {
    if nodes.len() == 1 {
        if let ContentNode::Text { value } = &nodes[0] {
            return Some(value);
        }
    }
    None
}
