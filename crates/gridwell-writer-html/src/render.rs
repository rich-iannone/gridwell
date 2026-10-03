use gridwell_core::{Color, FontSize, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{Border, BorderSet, Padding, StyleDef};
use gridwell_ir::{BorderStyle, Cell, ColumnVisibility, Keyword, Row, Table};
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
    /// Hidden columns get no `<col>` and no cells; spans crossing them shrink.
    visibility: ColumnVisibility,
}

impl<'a> HtmlRenderer<'a> {
    fn new(table: &'a Table, config: &'a HtmlWriterConfig) -> Self {
        Self {
            table,
            config,
            buf: String::with_capacity(4096),
            indent_level: 0,
            visibility: ColumnVisibility::from_spec(&table.column_spec),
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
        if let Some(w) = css_length(self.table.config.container_width.as_deref()) {
            style_parts.push(format!("max-width: {w}"));
        }
        if let Some(h) = css_length(self.table.config.container_height.as_deref()) {
            style_parts.push(format!("max-height: {h}"));
        }
        if let Some(o) = known(&self.table.config.container_overflow) {
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
        if styles.defs.is_empty()
            && styles.compositions.is_empty()
            && styles.conditionals.is_empty()
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

        if let Some(w) = css_length(self.table.config.table_width.as_deref()) {
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
        // An unparseable width is treated as auto.
        let width = |c: &gridwell_ir::ColumnSpec| {
            css_length(Some(&c.width)).filter(|w| w.as_str() != "auto")
        };
        let has_widths = self
            .table
            .column_spec
            .iter()
            .any(|c| !c.hidden && width(c).is_some());

        if !has_widths {
            return;
        }

        self.write_line("<colgroup>");
        self.push_indent();
        for col in &self.table.column_spec {
            if col.hidden {
                continue;
            }
            match width(col) {
                Some(w) => self.write_line(&format!("<col style=\"width: {w}\">")),
                None => self.write_line("<col>"),
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
                // The label spans the full (visible) width; validation guarantees a
                // declared colspan is either absent or the full table width.
                let colspan = self.visibility.visible_len();
                let class = self.style_class_attr(&label.style_id);
                self.write_line("<tr>");
                self.push_indent();
                self.write_line(&format!("<td colspan=\"{colspan}\"{class}>",));
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

        for (col, cell) in row.cells.iter().enumerate() {
            if cell.is_placeholder {
                continue; // Spanned-over positions are not rendered
            }
            // Cells entirely in hidden columns are not rendered at all.
            let Some((_, colspan)) = self.visibility.project(col, cell.colspan as usize) else {
                continue;
            };
            self.render_cell(cell, colspan, is_header);
        }

        self.pop_indent();
        self.write_line("</tr>");
    }

    fn render_cell(&mut self, cell: &Cell, colspan: usize, is_header: bool) {
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
                    attrs.push(format!("style=\"{}\"", escape_attr(&css)));
                }
            }
        }

        // Scope for header cells
        if is_header {
            if let Some(scope) = known(&cell.scope) {
                attrs.push(format!("scope=\"{scope}\""));
            }
        }

        // Colspan/rowspan
        if colspan > 1 {
            attrs.push(format!("colspan=\"{colspan}\""));
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
                let _ = write!(self.buf, "{indent}<sup>{}</sup> ", escape_html(&note.mark));
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
                                    "<span style=\"{}\">{}</span>",
                                    escape_attr(&css),
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
                    if let Some(w) = css_length(width.as_deref()) {
                        style_parts.push(format!("width: {w}"));
                    }
                    if let Some(h) = css_length(height.as_deref()) {
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

/// CSS declarations for a style. Every free-form value is parsed and re-emitted in
/// normalized form (or dropped if it doesn't parse), so the result is safe inside
/// a `<style>` block; callers still attribute-escape it for `style="…"` (font
/// family names may contain quotes).
fn style_def_to_css(def: &StyleDef) -> String {
    let mut parts = Vec::new();

    if let Some(v) = def.font_family.as_deref().and_then(css_font_family) {
        parts.push(format!("font-family: {v}"));
    }
    if let Some(v) = def.font_size.as_deref().and_then(css_font_size) {
        parts.push(format!("font-size: {v}"));
    }
    if let Some(v) = known(&def.font_weight) {
        parts.push(format!("font-weight: {v}"));
    }
    if let Some(v) = known(&def.font_style) {
        parts.push(format!("font-style: {v}"));
    }
    if let Some(v) = css_color(def.color.as_deref()) {
        parts.push(format!("color: {v}"));
    }
    if let Some(v) = css_color(def.background_color.as_deref()) {
        parts.push(format!("background-color: {v}"));
    }
    if let Some(v) = known(&def.text_align) {
        parts.push(format!("text-align: {v}"));
    }
    if let Some(v) = known(&def.vertical_align) {
        parts.push(format!("vertical-align: {v}"));
    }
    if let Some(v) = known(&def.text_transform) {
        parts.push(format!("text-transform: {v}"));
    }
    if let Some(v) = known(&def.text_decoration) {
        parts.push(format!("text-decoration: {v}"));
    }
    if let Some(v) = known(&def.white_space) {
        parts.push(format!("white-space: {v}"));
    }
    if let Some(v) = css_length(def.indent.as_deref()) {
        parts.push(format!("text-indent: {v}"));
    }
    if let Some(v) = known(&def.word_break) {
        parts.push(format!("word-break: {v}"));
    }
    if let Some(v) = known(&def.overflow) {
        parts.push(format!("overflow: {v}"));
    }
    if let Some(v) = known(&def.text_overflow) {
        parts.push(format!("text-overflow: {v}"));
    }
    if let Some(v) = css_length(def.min_width.as_deref()) {
        parts.push(format!("min-width: {v}"));
    }
    if let Some(v) = css_length(def.max_width.as_deref()) {
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
    let side = |v: &Option<String>| css_length(v.as_deref()).unwrap_or_else(|| "0".into());
    let (top, right, bottom, left) = (side(&p.top), side(&p.right), side(&p.bottom), side(&p.left));

    if [&top, &right, &bottom, &left]
        .iter()
        .all(|v| v.as_str() == "0")
    {
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
    let style = known(&b.style)?;
    if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
        return None;
    }
    let width = css_length(b.width.as_deref()).unwrap_or_else(|| "1px".into());
    let color = css_color(b.color.as_deref()).unwrap_or_else(|| "currentColor".into());
    Some(format!("{width} {style} {color}"))
}

// ─── CSS values ───

/// A keyword field's value if it is one of the known values. Unknown values keep
/// their source text (and `Display` it verbatim), so they must never reach CSS.
fn known<K: Keyword>(v: &Option<K>) -> Option<&K> {
    v.as_ref().filter(|k| k.is_known())
}

/// A length in normalized CSS form (`None` if absent or unparseable). A unitless
/// zero comes out as `0`.
fn css_length(v: Option<&str>) -> Option<String> {
    match v?.parse::<Length>().ok()? {
        Length::Px(0.0) => Some("0".into()),
        l => Some(l.to_string()),
    }
}

/// A colour in normalized CSS form (`None` if absent or unparseable).
fn css_color(v: Option<&str>) -> Option<String> {
    let c = v?.parse::<Color>().ok()?;
    Some(if c.is_transparent() {
        "transparent".into()
    } else {
        c.to_css()
    })
}

/// A font size: a validated length or keyword. Valid sizes contain only ASCII
/// letters, digits, `.`, `+`, `-` and `%`, so the lowercased source is safe CSS.
fn css_font_size(v: &str) -> Option<String> {
    v.parse::<FontSize>().ok()?;
    Some(v.trim().to_ascii_lowercase())
}

/// A font-family list with everything that could end a declaration, a rule or a
/// `<style>` element removed (`;`, `{`, `}`, `<`, `>`, `\`, `/`, `:`, `(`, `)`, `@`,
/// `!`, control characters). Names, commas, spaces, hyphens and quotes survive.
fn css_font_family(v: &str) -> Option<String> {
    let cleaned: String = v
        .chars()
        .filter(|&c| c.is_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '.' | '"' | '\''))
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
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
