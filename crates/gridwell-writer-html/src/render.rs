use gridwell_core::{Color, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{is_valid_style_id, HAlign, Keyword, RowRole, Table};
use gridwell_layout::{
    resolve, ResolvedBorder, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable, SectionKind,
    Sides,
};
use std::fmt::Write;
use thiserror::Error;

use crate::HtmlWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
    /// The configuration can't be written safely (e.g. a `class_prefix` that is not
    /// a CSS identifier).
    #[error("invalid HTML writer config: {0}")]
    Config(String),
}

/// Internal writer state.
struct HtmlRenderer<'r, 'a> {
    rt: &'r ResolvedTable<'a>,
    table: &'a Table,
    config: &'r HtmlWriterConfig,
    buf: String,
    indent_level: usize,
    /// Cascaded cell styles that the cell's own classes don't already produce
    /// (column defaults, striping, conditionals): emitted as generated classes
    /// `{prefix}__c{n}`, in first-use order.
    cell_styles: Vec<ResolvedStyle>,
    /// Alignment classes `{prefix}__al_<value>` the cells use, in a fixed order.
    alignments: Vec<&'static str>,
    /// Defined style ids that aren't valid ([`is_valid_style_id`]), sorted: they get
    /// generated classes `{prefix}__s{n}` instead of their names. Only reachable
    /// when the writer is handed unvalidated IR.
    unsafe_style_ids: Vec<&'a str>,
}

impl<'r, 'a> HtmlRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>, config: &'r HtmlWriterConfig) -> Self {
        let mut r = Self {
            rt,
            table: rt.table,
            config,
            buf: String::with_capacity(4096),
            indent_level: 0,
            cell_styles: Vec::new(),
            alignments: Vec::new(),
            unsafe_style_ids: {
                let styles = &rt.table.styles;
                let mut ids: Vec<&str> = styles
                    .defs
                    .keys()
                    .chain(styles.compositions.keys())
                    .map(String::as_str)
                    .filter(|id| !is_valid_style_id(id))
                    .collect();
                ids.sort_unstable();
                ids.dedup();
                ids
            },
        };
        if !config.inline_styles {
            for section in rt.sections() {
                let is_header = section.kind == gridwell_layout::SectionKind::Head;
                for row in &section.rows {
                    for cell in row.cells() {
                        if let Some(a) = cell_alignment(cell, is_header) {
                            if !r.alignments.contains(&a) {
                                r.alignments.push(a);
                            }
                        }
                        if r.needs_generated_class(row, cell)
                            && !r.cell_styles.contains(&cell.style)
                        {
                            r.cell_styles.push(cell.style.clone());
                        }
                    }
                }
            }
        }
        r
    }

    /// Whether a cell's cascaded style differs from what its row and cell classes
    /// give it (the row's class is on the `<tr>`, the cell's on the `<td>`).
    fn needs_generated_class(&self, row: &ResolvedRow, cell: &ResolvedCell) -> bool {
        let ids = [row.row.style_id.as_deref(), cell.cell.style_id.as_deref()];
        cell.style != self.rt.styles(ids.into_iter().flatten())
    }

    /// The generated class for a cell, if it needs one.
    fn generated_class(&self, row: &ResolvedRow, cell: &ResolvedCell) -> Option<String> {
        if self.config.inline_styles || !self.needs_generated_class(row, cell) {
            return None;
        }
        let n = self.cell_styles.iter().position(|s| *s == cell.style)?;
        Some(format!("{}__c{n}", self.config.class_prefix))
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
            && self.cell_styles.is_empty()
            && self.alignments.is_empty()
        {
            return;
        }

        self.write_line("<style>");
        self.push_indent();

        // Generated rules (alignment, cascaded cell styles) are scoped under the
        // table class: two classes outrank the `.something td { text-align: … }`
        // rules a host page commonly has, which a bare class would lose to. A
        // cell's alignment already reflects any style that sets one, so this
        // can't contradict a named style class.
        let p = &self.config.class_prefix;
        let mut alignments = self.alignments.clone();
        alignments.sort_unstable();
        for a in alignments {
            let line = format!(".{p}_table .{p}__al_{a} {{ text-align: {a} }}");
            self.write_line(&line);
        }

        // Emit style definitions as CSS classes
        let mut style_ids: Vec<&String> = styles.defs.keys().collect();
        style_ids.sort();
        for id in style_ids {
            let css = style_css(&self.rt.style(id));
            if let (Some(class), false) = (self.style_class(id), css.is_empty()) {
                self.write_line(&format!(".{class} {{ {css} }}"));
            }
        }

        // Emit compositions
        let mut comp_ids: Vec<&String> = styles.compositions.keys().collect();
        comp_ids.sort();
        for id in comp_ids {
            // A composition resolves to its base plus overrides (nothing if the
            // base is missing).
            let css = style_css(&self.rt.style(id));
            if let (Some(class), false) = (self.style_class(id), css.is_empty()) {
                self.write_line(&format!(".{class} {{ {css} }}"));
            }
        }

        // Generated classes for cascaded cell styles; after the named styles, so
        // they win where both apply.
        let generated: Vec<String> = self
            .cell_styles
            .iter()
            .enumerate()
            .map(|(n, style)| {
                let p = &self.config.class_prefix;
                format!(".{p}_table .{p}__c{n} {{ {} }}", style_css(style))
            })
            .collect();
        for line in generated {
            self.write_line(&line);
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
            let class = self.part_attrs("title", &title.style_id);
            self.write_line(&format!("<div{class} role=\"heading\" aria-level=\"1\">"));
            self.push_indent();
            self.render_content_nodes(&title.content);
            self.pop_indent();
            self.write_line("</div>");
        }

        if let Some(ref subtitle) = header.subtitle {
            let class = self.part_attrs("subtitle", &subtitle.style_id);
            self.write_line(&format!("<div{class} role=\"heading\" aria-level=\"2\">"));
            self.push_indent();
            self.render_content_nodes(&subtitle.content);
            self.pop_indent();
            self.write_line("</div>");
        }

        for line in &header.extra_lines {
            let class = self.part_attrs("header_line", &line.style_id);
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
        let widths: Vec<Option<String>> = self
            .rt
            .columns
            .iter()
            .map(|c| c.width.as_ref().map(length_css))
            .collect();
        if widths.iter().all(Option::is_none) {
            return;
        }

        self.write_line("<colgroup>");
        self.push_indent();
        for w in widths {
            match w {
                Some(w) => self.write_line(&format!("<col style=\"width: {w}\">")),
                None => self.write_line("<col>"),
            }
        }
        self.pop_indent();
        self.write_line("</colgroup>");
    }

    fn render_thead(&mut self) {
        let rt = self.rt;
        if rt.head.is_empty() {
            return;
        }

        self.write_line("<thead>");
        self.push_indent();
        for row in &rt.head.rows {
            self.render_row(row, SectionKind::Head);
        }
        self.pop_indent();
        self.write_line("</thead>");
    }

    fn render_tbody(&mut self) {
        let rt = self.rt;
        for group in &rt.groups {
            self.write_line("<tbody>");
            self.push_indent();

            // Group label row, spanning the full (visible) width.
            if let Some(label) = &group.label {
                let colspan = rt.columns.len();
                let class = self.part_attrs("group_label", &label.style_id.map(str::to_string));
                self.write_line("<tr>");
                self.push_indent();
                self.write_line(&format!("<td colspan=\"{colspan}\"{class}>",));
                self.push_indent();
                self.render_content_nodes(label.content);
                self.pop_indent();
                self.write_line("</td>");
                self.pop_indent();
                self.write_line("</tr>");
            }

            for row in &group.rows.rows {
                self.render_row(row, SectionKind::Body);
            }
            for row in &group.summary_rows.rows {
                self.render_row(row, SectionKind::Summary);
            }

            self.pop_indent();
            self.write_line("</tbody>");
        }
    }

    fn render_row(&mut self, row: &ResolvedRow, kind: SectionKind) {
        let row_class = match row_part(row, kind) {
            Some(part) => self.part_attrs(part, &row.row.style_id),
            None => self.style_class_attr(&row.row.style_id),
        };
        self.write_line(&format!("<tr{row_class}>"));
        self.push_indent();
        // Covered positions are implied by the origins' spans.
        for cell in row.cells() {
            self.render_cell(row, cell, kind);
        }
        self.pop_indent();
        self.write_line("</tr>");
    }

    fn render_cell(&mut self, row: &ResolvedRow, cell: &ResolvedCell, kind: SectionKind) {
        let is_header = kind == SectionKind::Head;
        let tag = if is_header { "th" } else { "td" };
        let mut attrs = Vec::new();
        let part = cell_part(row, cell, kind).map(|part| self.part_class(part));

        let align = cell_alignment(cell, is_header);
        if self.config.inline_styles {
            let mut css = style_css(&cell.style);
            // The cascade's own text-align, if any, already equals the cell's.
            if let (Some(a), None) = (align, &cell.style.text_align) {
                if !css.is_empty() {
                    css.push_str("; ");
                }
                css.push_str(&format!("text-align: {a}"));
            }
            if let Some(part) = part {
                attrs.push(format!("class=\"{part}\""));
            }
            if !css.is_empty() {
                attrs.push(format!("style=\"{}\"", escape_attr(&css)));
            }
        } else {
            let mut classes: Vec<String> = part.into_iter().collect();
            if let Some(a) = align {
                classes.push(format!("{}__al_{a}", self.config.class_prefix));
            }
            if let Some(id) = &cell.cell.style_id {
                classes.extend(self.style_class(id));
            }
            classes.extend(self.generated_class(row, cell));
            if !classes.is_empty() {
                attrs.push(format!("class=\"{}\"", classes.join(" ")));
            }
        }

        if is_header {
            if let Some(scope) = &cell.scope {
                attrs.push(format!("scope=\"{scope}\""));
            }
        }

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
        if let Some(text) = single_text_content(cell.content) {
            self.write_line(&format!("<{tag}{attr_str}>{}</{tag}>", escape_html(text)));
            return;
        }

        if cell.content.is_empty() {
            self.write_line(&format!("<{tag}{attr_str}></{tag}>"));
            return;
        }

        self.write_line(&format!("<{tag}{attr_str}>"));
        self.push_indent();
        self.render_content_nodes(cell.content);
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
                let class = self.part_attrs("footnote", &note.style_id);
                self.write_line(&format!("<p id=\"{}\"{class}>", escape_attr(&note.id)));
                self.push_indent();
                let indent = self.indent();
                let mark_class = self.part_class("footnote_mark");
                let _ = write!(
                    self.buf,
                    "{indent}<sup class=\"{mark_class}\">{}</sup> ",
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
                let class = self.part_attrs("source_note", &note.style_id);
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
                            let style = self.rt.style(id);
                            if !style.is_empty() {
                                let css = style_css(&style);
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
                            let class = self
                                .style_class(id)
                                .map(|c| format!(" class=\"{c}\""))
                                .unwrap_or_default();
                            let _ = write!(self.buf, "<span{class}>{}</span>", escape_html(value));
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
                    let mark_class = self.part_class("footnote_mark");
                    let _ = write!(
                        self.buf,
                        "<sup class=\"{mark_class}\"><a href=\"#{}\">{}</a></sup>",
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

    /// The class naming a structural part of the table, e.g. `gw_col_label`.
    fn part_class(&self, part: &str) -> String {
        format!("{}_{part}", self.config.class_prefix)
    }

    /// Attributes for an element that is a structural `part` and may have a style:
    /// the part's class plus the style's class (or, with inline styles, the part's
    /// class and the style's `style` attribute).
    fn part_attrs(&self, part: &str, style_id: &Option<String>) -> String {
        let part = self.part_class(part);
        match style_id.as_deref().and_then(|id| self.style_class(id)) {
            Some(class) if !self.config.inline_styles => format!(" class=\"{part} {class}\""),
            _ => format!(" class=\"{part}\"{}", self.style_class_attr(style_id)),
        }
    }

    /// The class for a style: `{prefix}_style-{id}`, a namespace that can't meet
    /// the part classes (`{prefix}_{part}` names have no `-`) or the generated ones
    /// (`{prefix}__…`). An id that isn't a valid style id is never written into
    /// markup: a defined one gets a generated `{prefix}__s{n}`, an undefined one
    /// no class (it has no style to apply).
    fn style_class(&self, id: &str) -> Option<String> {
        let p = &self.config.class_prefix;
        if is_valid_style_id(id) {
            Some(format!("{p}_style-{id}"))
        } else {
            let n = self.unsafe_style_ids.binary_search(&id).ok()?;
            Some(format!("{p}__s{n}"))
        }
    }

    fn style_class_attr(&self, style_id: &Option<String>) -> String {
        match style_id {
            Some(id) if !self.config.inline_styles => self
                .style_class(id)
                .map(|c| format!(" class=\"{c}\""))
                .unwrap_or_default(),
            Some(id) if self.config.inline_styles => {
                let css = style_css(&self.rt.style(id));
                if css.is_empty() {
                    String::new()
                } else {
                    format!(" style=\"{}\"", escape_attr(&css))
                }
            }
            _ => String::new(),
        }
    }
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &HtmlWriterConfig) -> Result<String, RenderError> {
    crate::check_class_prefix(&config.class_prefix).map_err(RenderError::Config)?;
    let rt = resolve(table);
    HtmlRenderer::new(&rt, config).render()
}

/// The `text-align` a cell needs, if any: header cells always (browsers center
/// `<th>` by default), body cells unless left-aligned (the default for `<td>`).
/// Decimal (`char`) alignment is approximated by right alignment.
fn cell_alignment(cell: &ResolvedCell, is_header: bool) -> Option<&'static str> {
    let a = match cell.align {
        HAlign::Center => "center",
        HAlign::Right | HAlign::Char => "right",
        HAlign::Justify => "justify",
        _ => "left",
    };
    (is_header || a != "left").then_some(a)
}

// ─── CSS Generation ───

/// CSS declarations for a resolved style. Every value is typed (parsed colors and
/// lengths, known keywords, a sanitized family list), so the result is safe inside a
/// The structural part a row is, for its class: column labels or spanners in the
/// head (a head row without a role counts as column labels), summary rows. Data
/// rows have none.
fn row_part(row: &ResolvedRow, kind: SectionKind) -> Option<&'static str> {
    match kind {
        SectionKind::Head => Some(match row.role {
            Some(RowRole::SpannerLabel) => "spanners",
            _ => "col_labels",
        }),
        SectionKind::Body => None,
        SectionKind::Summary => Some(match row.role {
            Some(RowRole::GrandSummary) => "grand_summary",
            _ => "summary",
        }),
    }
}

/// The structural part a cell is, for its class: the stubhead and stub, column
/// labels, and spanners (only those with content, so an empty filler cell in a
/// spanner row isn't drawn like a spanner).
fn cell_part(row: &ResolvedRow, cell: &ResolvedCell, kind: SectionKind) -> Option<&'static str> {
    match kind {
        SectionKind::Head if cell.is_stub => Some("stubhead"),
        SectionKind::Head => match row.role {
            Some(RowRole::SpannerLabel) => (!cell.content.is_empty()).then_some("spanner"),
            _ => Some("col_label"),
        },
        SectionKind::Body | SectionKind::Summary => cell.is_stub.then_some("stub"),
    }
}

/// `<style>` block; callers still attribute-escape it for `style="…"` (family names
/// may contain quotes).
fn style_css(s: &ResolvedStyle) -> String {
    let mut parts = Vec::new();
    let mut push = |prop: &str, v: Option<String>| {
        if let Some(v) = v {
            parts.push(format!("{prop}: {v}"));
        }
    };
    push("font-family", s.font_family.clone());
    push("font-size", s.font_size.as_ref().map(|f| f.to_string()));
    push("font-weight", s.font_weight.as_ref().map(|v| v.to_string()));
    push("font-style", s.font_style.as_ref().map(|v| v.to_string()));
    push("color", s.color.map(color_css));
    push("background-color", s.background_color.map(color_css));
    push("text-align", s.text_align.as_ref().map(|v| v.to_string()));
    push(
        "vertical-align",
        s.vertical_align.as_ref().map(|v| v.to_string()),
    );
    push(
        "text-transform",
        s.text_transform.as_ref().map(|v| v.to_string()),
    );
    push(
        "text-decoration",
        s.text_decoration.as_ref().map(|v| v.to_string()),
    );
    push("white-space", s.white_space.as_ref().map(|v| v.to_string()));
    push("text-indent", s.indent.as_ref().map(length_css));
    push("word-break", s.word_break.as_ref().map(|v| v.to_string()));
    push("overflow", s.overflow.as_ref().map(|v| v.to_string()));
    push(
        "text-overflow",
        s.text_overflow.as_ref().map(|v| v.to_string()),
    );
    push("min-width", s.min_width.as_ref().map(length_css));
    push("max-width", s.max_width.as_ref().map(length_css));
    if let Some(css) = padding_css(&s.padding) {
        parts.push(css);
    }
    for (side, b) in s.border.iter() {
        if let Some(b) = b {
            parts.push(format!("border-{side}: {}", border_css(b)));
        }
    }
    parts.join("; ")
}

fn padding_css(p: &Sides<Option<Length>>) -> Option<String> {
    let side = |v: &Option<Length>| v.as_ref().map_or_else(|| "0".to_string(), length_css);
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

fn border_css(b: &ResolvedBorder) -> String {
    let width = b
        .width
        .as_ref()
        .map_or_else(|| "1px".to_string(), length_css);
    let color = b
        .color
        .map_or_else(|| "currentColor".to_string(), color_css);
    format!("{width} {} {color}", b.style)
}

/// A length in CSS form; a zero length is plain `0`.
fn length_css(l: &Length) -> String {
    match l {
        Length::Px(v) if *v == 0.0 => "0".into(),
        l => l.to_string(),
    }
}

fn color_css(c: Color) -> String {
    if c.is_transparent() {
        "transparent".into()
    } else {
        c.to_css()
    }
}

// ─── CSS values from raw IR strings (config and image sizes) ───

/// A keyword field's value if it is one of the known values. Unknown values keep
/// their source text (and `Display` it verbatim), so they must never reach CSS.
fn known<K: Keyword>(v: &Option<K>) -> Option<&K> {
    v.as_ref().filter(|k| k.is_known())
}

/// A length in normalized CSS form (`None` if absent or unparseable).
fn css_length(v: Option<&str>) -> Option<String> {
    v?.parse::<Length>().ok().as_ref().map(length_css)
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
