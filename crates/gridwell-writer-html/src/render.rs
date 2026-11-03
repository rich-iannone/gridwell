use gridwell_core::{Color, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{is_valid_style_id, CellScope, HAlign, Keyword, RowRole, Table, TextDirection};
use gridwell_layout::look::{EdgePart, EdgeRules};
use gridwell_layout::{
    resolve_with, AlignPad, ResolveOptions, ResolvedBorder, ResolvedCell, ResolvedRow,
    ResolvedStyle, ResolvedTable, Section, SectionKind, Sides, Slot,
};
use std::collections::HashMap;
use std::fmt::Write;
use thiserror::Error;

use crate::ids::{id_text, is_blank, prefix_svg_ids, IdRegistry};
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
    /// Some cell ends with padding that aligns it on a character, hidden by the
    /// class `{prefix}__pad`.
    pads: bool,
    /// Defined style ids that aren't valid ([`is_valid_style_id`]), sorted: they get
    /// generated classes `{prefix}__s{n}` instead of their names. Only reachable
    /// when the writer is handed unvalidated IR.
    unsafe_style_ids: Vec<&'a str>,
    /// Makes the ids of header cells, the title and subtitle.
    ids: IdRegistry,
    /// The ids of the head's cells and of the title and subtitle.
    head_ids: HeadIds,
    /// While a body cell is written: the prefix that makes the ids in its SVGs
    /// unique.
    svg_prefix: Option<String>,
    /// Summary rows written so far, which number their cells' SVG ids.
    summary_rows: usize,
    /// The table's edges its cells draw (classes `{prefix}_cells_*` on the
    /// table, `{prefix}_rule_*` on the cells carrying the rule).
    edges: EdgeRules,
}

/// The ids made before the body (in this order, as Great Tables makes them): the
/// stubhead's, the column labels', the spanners', the title's and the subtitle's.
#[derive(Default)]
struct HeadIds {
    /// The id of each head cell that has one, by its origin (head row, visible
    /// column).
    cells: HashMap<(usize, usize), String>,
    /// Per visible column, the ids of the spanners over it, top down.
    spanners: Vec<Vec<String>>,
    /// Per visible column, the ids of its column labels.
    labels: Vec<Vec<String>>,
    title: Option<String>,
    subtitle: Option<String>,
}

impl HeadIds {
    fn new(rt: &ResolvedTable, ids: &mut IdRegistry) -> Self {
        let n = rt.columns.len();
        let mut h = HeadIds {
            spanners: vec![Vec::new(); n],
            labels: vec![Vec::new(); n],
            ..HeadIds::default()
        };
        let rows = || rt.head.rows.iter().enumerate();
        let spanner_row = |row: &ResolvedRow| row.role == Some(RowRole::SpannerLabel);
        // The stubhead, if it has a label.
        for (r, row) in rows() {
            for cell in row.cells().filter(|c| c.is_stub && !is_blank(&c.content)) {
                let id = ids.make(&id_text(&cell.content), "stubhead");
                h.cells.insert((r, cell.col), id);
            }
        }
        // Column labels, by their column's id.
        for (r, row) in rows().filter(|(_, row)| !spanner_row(row)) {
            for cell in row.cells().filter(|c| !c.is_stub) {
                let column = rt.table.column_spec.get(cell.grid_col);
                let id = ids.make(column.map_or("", |c| &c.id), "column");
                for list in h.labels.iter_mut().skip(cell.col).take(cell.colspan) {
                    list.push(id.clone());
                }
                h.cells.insert((r, cell.col), id);
            }
        }
        // Spanners (not the blank cells beside them), by their label.
        for (r, row) in rows().filter(|(_, row)| spanner_row(row)) {
            for cell in row.cells().filter(|c| !c.is_stub && !is_blank(&c.content)) {
                let id = ids.make(&id_text(&cell.content), "spanner");
                for list in h.spanners.iter_mut().skip(cell.col).take(cell.colspan) {
                    list.push(id.clone());
                }
                h.cells.insert((r, cell.col), id);
            }
        }
        if rt.header.title.is_some() {
            h.title = Some(ids.make("title", "title"));
        }
        if rt.header.subtitle.is_some() {
            h.subtitle = Some(ids.make("subtitle", "subtitle"));
        }
        h
    }

    /// The ids of the spanners and column labels over visible columns `cols`,
    /// spanners first.
    fn columns(&self, cols: std::ops::Range<usize>) -> Vec<&String> {
        let mut out: Vec<&String> = Vec::new();
        for lists in [&self.spanners, &self.labels] {
            for id in cols.clone().filter_map(|c| lists.get(c)).flatten() {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }
}

/// What a row's cells need for their `id` and `headers` attributes.
enum RowIds<'s> {
    /// A head row: its index in the head.
    Head(usize),
    /// A row below the head: its index in its section, the ids of the section's
    /// stub cells (by origin), the ids of the row's headers (its group's label,
    /// its stub cells), and what numbers its cells' SVG ids.
    Body {
        r: usize,
        stubs: &'s HashMap<(usize, usize), String>,
        headers: &'s [String],
        svg_row: String,
    },
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
            pads: false,
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
            ids: IdRegistry::new(
                config.table_id.as_deref(),
                rt.footer.footnotes.iter().map(|n| n.id),
            ),
            head_ids: HeadIds::default(),
            svg_prefix: None,
            summary_rows: 0,
            edges: EdgeRules::new(rt),
        };
        r.head_ids = HeadIds::new(rt, &mut r.ids);
        if !config.inline_styles {
            for label in rt.groups.iter().filter_map(|g| g.label.as_ref()) {
                let a = group_label_alignment(label, directed(rt.table));
                if !r.alignments.contains(&a) {
                    r.alignments.push(a);
                }
            }
            for section in rt.sections() {
                for row in &section.rows {
                    for cell in row.cells() {
                        let is_header = is_header_cell(cell, section.kind);
                        if let Some(a) = cell_alignment(cell, is_header, directed(rt.table)) {
                            if !r.alignments.contains(&a) {
                                r.alignments.push(a);
                            }
                        }
                        r.pads |= cell.align_pad.is_some();
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
    /// give it (the row's class is on the `<tr>`, the cell's on the `<td>`), or its
    /// row's style sets borders: in the collapsed-border model a border on a `<tr>`
    /// doesn't reliably beat one on its cells (a `none` always loses to the base
    /// stylesheet's hairline), so the cell carries it.
    fn needs_generated_class(&self, row: &ResolvedRow, cell: &ResolvedCell) -> bool {
        let ids = [row.row.style_id.as_deref(), cell.cell.style_id.as_deref()];
        if cell.style != self.rt.styles(ids.into_iter().flatten()) {
            return true;
        }
        let row_style = self.rt.styles(row.row.style_id.as_deref());
        row_style.border.iter().any(|(_, b)| b.is_some())
            || row_style.border_cleared.iter().any(|(_, c)| *c)
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
        if self.config.standalone {
            return Ok(self.document(&self.buf));
        }
        Ok(self.buf)
    }

    // ─── Standalone document ───

    /// `fragment` as a complete UTF-8 document, titled after the table.
    fn document(&self, fragment: &str) -> String {
        let nl = self.nl();
        let lang = self
            .table
            .config
            .lang
            .clone()
            .or_else(|| html_lang(&self.table.config.locale))
            .map(|l| format!(" lang=\"{}\"", escape_attr(&l)))
            .unwrap_or_default();
        let title = escape_html(&self.document_title());
        let mut out = String::with_capacity(fragment.len() + 256);
        for line in [
            "<!doctype html>".to_string(),
            format!("<html{lang}>"),
            "<head>".to_string(),
            "<meta charset=\"utf-8\">".to_string(),
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">".to_string(),
            format!("<title>{title}</title>"),
            "</head>".to_string(),
            "<body>".to_string(),
        ] {
            out.push_str(&line);
            out.push_str(nl);
        }
        out.push_str(fragment);
        if !fragment.ends_with('\n') {
            out.push_str(nl);
        }
        out.push_str("</body>");
        out.push_str(nl);
        out.push_str("</html>");
        out.push_str(nl);
        out
    }

    /// The document's `<title>`: the table's title as plain text (footnote marks
    /// left out, whitespace collapsed), else its `aria_label`, else "Table".
    fn document_title(&self) -> String {
        let title = self
            .table
            .header
            .as_ref()
            .and_then(|h| h.title.as_ref())
            .map(|t| {
                let nodes: Vec<ContentNode> = t
                    .content
                    .iter()
                    .filter(|n| !matches!(n, ContentNode::FootnoteMark { .. }))
                    .cloned()
                    .collect();
                gridwell_layout::plain_text(&nodes, " ")
            })
            .or_else(|| self.table.config.aria_label.clone())
            .unwrap_or_default();
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            "Table".to_string()
        } else {
            title
        }
    }

    // ─── Container ───

    fn render_container_open(&mut self) {
        let mut attrs = Vec::new();
        if let Some(id) = &self.config.table_id {
            attrs.push(format!("id=\"{}\"", escape_attr(id)));
        }
        attrs.push(format!("class=\"{}\"", self.container_class()));
        if let Some(lang) = &self.table.config.lang {
            attrs.push(format!("lang=\"{}\"", escape_attr(lang)));
        }
        if let Some(dir) = known(&self.table.config.dir) {
            attrs.push(format!("dir=\"{dir}\""));
        }

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
        if !self.config.base_styles
            && styles.defs.is_empty()
            && styles.compositions.is_empty()
            && self.cell_styles.is_empty()
            && self.alignments.is_empty()
            && !self.pads
        {
            return;
        }

        self.write_line("<style>");
        self.push_indent();

        // The base look first: zero specificity, so everything below wins.
        if self.config.base_styles {
            let rtl = self.table.config.dir == Some(TextDirection::Rtl);
            for line in crate::base::rules(&self.config.class_prefix, rtl) {
                self.write_line(&line);
            }
        }

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
        if self.pads {
            self.write_line(&format!(".{p}_table .{p}__pad {{ visibility: hidden }}"));
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
            let id = id_attr(self.head_ids.title.as_deref());
            self.write_line(&format!(
                "<div{class}{id} role=\"heading\" aria-level=\"1\">"
            ));
            self.push_indent();
            self.render_content_nodes(&title.content);
            self.pop_indent();
            self.write_line("</div>");
        }

        if let Some(ref subtitle) = header.subtitle {
            let class = self.part_attrs("subtitle", &subtitle.style_id);
            let id = id_attr(self.head_ids.subtitle.as_deref());
            self.write_line(&format!(
                "<div{class}{id} role=\"heading\" aria-level=\"2\">"
            ));
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
        let p = &self.config.class_prefix;
        let mut classes = vec![format!("{p}_table")];
        // Edges drawn by the cells: the table's and the head's rules are off there.
        if !self.config.inline_styles {
            let e = &self.edges;
            for (on, edge) in [(e.top, "top"), (e.head, "head"), (e.bottom, "bottom")] {
                if on {
                    classes.push(format!("{p}_cells_{edge}"));
                }
            }
        }
        attrs.push(format!("class=\"{}\"", classes.join(" ")));

        // The title names the table and the subtitle describes it, unless the
        // config names it (`aria_label`, or a caption from `summary`) or says what
        // describes it.
        let config = &self.table.config;
        if let Some(ref aria_label) = config.aria_label {
            attrs.push(format!("aria-label=\"{}\"", escape_attr(aria_label)));
        } else if let (Some(title), None) = (&self.head_ids.title, &config.summary) {
            attrs.push(format!("aria-labelledby=\"{title}\""));
        }
        if let Some(ref ids) = config.aria_describedby {
            attrs.push(format!("aria-describedby=\"{}\"", escape_attr(ids)));
        } else if let Some(subtitle) = &self.head_ids.subtitle {
            attrs.push(format!("aria-describedby=\"{subtitle}\""));
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
        for (r, row) in rt.head.rows.iter().enumerate() {
            self.render_row(row, SectionKind::Head, EdgePart::Head, RowIds::Head(r));
        }
        self.pop_indent();
        self.write_line("</thead>");
    }

    fn render_tbody(&mut self) {
        let rt = self.rt;
        for (g, group) in rt.groups.iter().enumerate() {
            self.write_line("<tbody>");
            self.push_indent();

            // The group's key, which its label's and summary rows' ids are made
            // from: its id, else its label.
            let key = match (group.id, &group.label) {
                (Some(id), _) if !id.is_empty() => id.to_string(),
                (_, Some(label)) => id_text(&label.content),
                _ => String::new(),
            };

            // Group label row, spanning the full (visible) width: the header of
            // the group's rows.
            let mut group_id = None;
            if let Some(label) = &group.label {
                let id = self.ids.make(&key, "group");
                let colspan = rt.columns.len();
                let scope = if colspan > 1 { "colgroup" } else { "col" };
                let attrs = self.group_label_attrs(label);
                self.write_line("<tr>");
                self.push_indent();
                self.write_line(&format!(
                    "<th colspan=\"{colspan}\" scope=\"{scope}\"{attrs} id=\"{id}\">"
                ));
                self.push_indent();
                self.render_content_nodes(&label.content);
                self.pop_indent();
                self.write_line("</th>");
                self.pop_indent();
                self.write_line("</tr>");
                group_id = Some(id);
            }

            let id = group_id.as_deref();
            self.render_section(&group.rows, EdgePart::Rows(g), id, &key);
            self.render_section(&group.summary_rows, EdgePart::Summary(g), id, &key);

            self.pop_indent();
            self.write_line("</tbody>");
        }
    }

    /// A group label's attributes after its `colspan` and `scope`: its classes
    /// (or style) and, as for any header cell (a browser centers `<th>`), its
    /// alignment.
    fn group_label_attrs(&self, label: &gridwell_layout::Line) -> String {
        let part = self.part_class("group_label");
        let align = group_label_alignment(label, directed(self.table));
        let style_id = label.style_id.map(str::to_string);
        if self.config.inline_styles {
            let mut css = style_id
                .as_deref()
                .map(|id| style_css(&self.rt.style(id)))
                .unwrap_or_default();
            if label.style.text_align.is_none() {
                if !css.is_empty() {
                    css.push_str("; ");
                }
                css.push_str(&format!("text-align: {align}"));
            }
            format!(" class=\"{part}\" style=\"{}\"", escape_attr(&css))
        } else {
            let mut classes = vec![part, format!("{}__al_{align}", self.config.class_prefix)];
            classes.extend(style_id.as_deref().and_then(|id| self.style_class(id)));
            format!(" class=\"{}\"", classes.join(" "))
        }
    }

    /// A group's data or summary rows. Each stub cell gets an id (`stub-{row}`,
    /// with `-{column}` when the stub has several columns; a summary row's label
    /// `summary-{group}-{label}`, or `grand-summary-{label}`), and each other cell
    /// lists its headers: the group's label (`group_id`; not for a grand summary),
    /// the stub cells covering its row, and the spanners and column labels over it.
    fn render_section(
        &mut self,
        section: &Section,
        part: EdgePart,
        group_id: Option<&str>,
        key: &str,
    ) {
        let mut stubs: HashMap<(usize, usize), String> = HashMap::new();
        for (r, row) in section.rows.iter().enumerate() {
            let grand = row.role == Some(RowRole::GrandSummary);
            for cell in row.cells().filter(|c| c.is_stub) {
                let id = if section.kind == SectionKind::Summary {
                    let label = id_text(&cell.content);
                    let base = match (grand, key) {
                        (true, _) => format!("grand-summary-{label}"),
                        (false, "") => format!("summary-{label}"),
                        (false, key) => format!("summary-{key}-{label}"),
                    };
                    self.ids.make(&base, "summary")
                } else if self.rt.stub_cols > 1 {
                    let base = format!("stub-{}-{}", row.number, cell.col + 1);
                    self.ids.make(&base, "stub")
                } else {
                    self.ids.make(&format!("stub-{}", row.number), "stub")
                };
                stubs.insert((r, cell.col), id);
            }

            let mut headers: Vec<String> = group_id
                .filter(|_| !grand)
                .map(str::to_string)
                .into_iter()
                .collect();
            for (c, slot) in row.slots.iter().enumerate() {
                let origin = match slot {
                    Slot::Origin(_) => (r, c),
                    Slot::CoveredH { origin_col } => (r, *origin_col),
                    Slot::CoveredV {
                        origin_row,
                        origin_col,
                        ..
                    } => (*origin_row, *origin_col),
                    Slot::Empty => continue,
                };
                if let Some(id) = stubs.get(&origin) {
                    if !headers.contains(id) {
                        headers.push(id.clone());
                    }
                }
            }

            let svg_row = if section.kind == SectionKind::Summary {
                self.summary_rows += 1;
                format!("summary-{}", self.summary_rows)
            } else {
                (row.number - 1).to_string()
            };
            let ids = RowIds::Body {
                r,
                stubs: &stubs,
                headers: &headers,
                svg_row,
            };
            self.render_row(row, section.kind, part, ids);
        }
    }

    fn render_row(&mut self, row: &ResolvedRow, kind: SectionKind, part: EdgePart, ids: RowIds) {
        let row_class = match row_part(row, kind) {
            Some(part) => self.part_attrs(part, &row.row.style_id),
            None => self.style_class_attr(&row.row.style_id),
        };
        self.write_line(&format!("<tr{row_class}>"));
        self.push_indent();
        // Covered positions are implied by the origins' spans.
        for cell in row.cells() {
            self.render_cell(row, cell, kind, part, &ids);
        }
        self.pop_indent();
        self.write_line("</tr>");
    }

    fn render_cell(
        &mut self,
        row: &ResolvedRow,
        cell: &ResolvedCell,
        kind: SectionKind,
        edge_part: EdgePart,
        ids: &RowIds,
    ) {
        let is_header = is_header_cell(cell, kind);
        let tag = if is_header { "th" } else { "td" };
        let mut attrs = Vec::new();
        let part = cell_part(row, cell, kind).map(|part| self.part_class(part));

        let align = cell_alignment(cell, is_header, directed(self.table));
        if self.config.inline_styles {
            let mut css = style_css(&cell.style);
            // The cascade's own text-align, if any, already equals the cell's.
            if let (Some(a), None) = (align, &cell.style.text_align) {
                if !css.is_empty() {
                    css.push_str("; ");
                }
                css.push_str(&format!("text-align: {a}"));
            }
            // Digits as wide as each other, for the padding to be exact (the
            // base stylesheet does this for every right-aligned body cell).
            if cell.align_char.is_some() {
                if !css.is_empty() {
                    css.push_str("; ");
                }
                css.push_str("font-variant-numeric: tabular-nums");
            }
            if let Some(part) = part {
                attrs.push(format!("class=\"{part}\""));
            }
            if !css.is_empty() {
                attrs.push(format!("style=\"{}\"", escape_attr(&css)));
            }
        } else {
            let mut classes: Vec<String> = part.into_iter().collect();
            let r = match ids {
                RowIds::Head(r) | RowIds::Body { r, .. } => *r,
            };
            let p = &self.config.class_prefix;
            if self.edges.carries_top(edge_part, r, cell.col) {
                classes.push(format!("{p}_rule_top"));
            }
            if self.edges.carries_bottom(edge_part, r, cell.col) {
                classes.push(format!("{p}_rule_bottom"));
            }
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

        if let Some(scope) = cell_scope(cell, kind) {
            attrs.push(format!("scope=\"{scope}\""));
        }

        if cell.colspan > 1 {
            attrs.push(format!("colspan=\"{}\"", cell.colspan));
        }
        if cell.rowspan > 1 {
            attrs.push(format!("rowspan=\"{}\"", cell.rowspan));
        }

        // Header cells have ids, and the cells they head list them.
        match ids {
            RowIds::Head(r) => {
                if let Some(id) = self.head_ids.cells.get(&(*r, cell.col)) {
                    attrs.push(format!("id=\"{id}\""));
                }
            }
            RowIds::Body {
                r,
                stubs,
                headers,
                svg_row,
            } => {
                if cell.is_stub {
                    if let Some(id) = stubs.get(&(*r, cell.col)) {
                        attrs.push(format!("id=\"{id}\""));
                    }
                } else {
                    let mut all: Vec<&String> = headers.iter().collect();
                    for id in self.head_ids.columns(cell.col..cell.col + cell.colspan) {
                        if !all.contains(&id) {
                            all.push(id);
                        }
                    }
                    if !all.is_empty() {
                        let all: Vec<&str> = all.into_iter().map(String::as_str).collect();
                        attrs.push(format!("headers=\"{}\"", all.join(" ")));
                    }
                }
                let column = self.table.column_spec.get(cell.grid_col);
                let column = column.map_or("", |c| c.id.as_str());
                self.svg_prefix = Some(format!("{}{column}-{svg_row}", self.ids.prefix));
            }
        }

        let attr_str = if attrs.is_empty() {
            String::new()
        } else {
            format!(" {}", attrs.join(" "))
        };

        let pad = cell.align_pad.map(|pad| self.align_pad(&pad));

        // For simple single-text cells, render inline
        if let Some(text) = single_text_content(&cell.content) {
            let pad = pad.unwrap_or_default();
            let text = escape_html(text);
            self.write_line(&format!("<{tag}{attr_str}>{text}{pad}</{tag}>"));
            self.svg_prefix = None;
            return;
        }

        if cell.content.is_empty() {
            self.write_line(&format!("<{tag}{attr_str}></{tag}>"));
            self.svg_prefix = None;
            return;
        }

        self.write_line(&format!("<{tag}{attr_str}>"));
        self.push_indent();
        self.render_content_nodes(&cell.content);
        if let Some(pad) = pad {
            self.write_line(&pad);
        }
        self.pop_indent();
        self.write_line(&format!("</{tag}>"));
        self.svg_prefix = None;
    }

    /// The padding that aligns a cell on a character: its text, invisible (and
    /// not read out), so it takes the room it would in the cell's font.
    fn align_pad(&self, pad: &AlignPad) -> String {
        let hidden = if self.config.inline_styles {
            "style=\"visibility: hidden\"".to_string()
        } else {
            format!("class=\"{}__pad\"", self.config.class_prefix)
        };
        let text = escape_html(&pad.text());
        format!("<span {hidden} aria-hidden=\"true\">{text}</span>")
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
                        match &self.svg_prefix {
                            Some(p) if value.contains("<svg") => {
                                let _ = write!(self.buf, "{}", prefix_svg_ids(value, p));
                            }
                            _ => {
                                let _ = write!(self.buf, "{value}");
                            }
                        }
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
    crate::check_config(config).map_err(RenderError::Config)?;
    // CSS `text-transform` transforms the text, and raw HTML is written as it
    // is: the content stays as written.
    let opts = ResolveOptions {
        transform_text: false,
        lower_html: false,
    };
    let rt = resolve_with(table, opts);
    HtmlRenderer::new(&rt, config).render()
}

/// The table's locale as an HTML `lang` value (`en_US` → `en-US`), if it looks
/// like a language tag at all: ASCII letters, digits and `-`, starting with a letter.
fn html_lang(locale: &str) -> Option<String> {
    let tag = locale.trim().replace('_', "-");
    let ok = tag.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    ok.then_some(tag)
}

/// Whether a cell is a header cell (`<th>`): every head cell, the stub (its row's
/// label), and any other cell given a `scope`, which only a header cell can carry.
fn is_header_cell(cell: &ResolvedCell, kind: SectionKind) -> bool {
    kind == SectionKind::Head || cell.is_stub || cell.scope.is_some()
}

/// A header cell's `scope`: its own, else, for a stub below the head, `row`, or
/// `rowgroup` when it spans rows (it heads them all), as in Great Tables and gt.
fn cell_scope(cell: &ResolvedCell, kind: SectionKind) -> Option<CellScope> {
    let stub = kind != SectionKind::Head && cell.is_stub;
    let scope = if cell.rowspan > 1 {
        CellScope::RowGroup
    } else {
        CellScope::Row
    };
    cell.scope.clone().or_else(|| stub.then_some(scope))
}

/// The `text-align` a cell needs, if any: header cells always (browsers center
/// `<th>` by default), body cells unless left-aligned (the default for `<td>`,
/// but only left to right: `directed`, it is the start). A cell aligned on a
/// character is right-aligned, its padding lining it up ([`AlignPad`]).
fn cell_alignment(cell: &ResolvedCell, is_header: bool, directed: bool) -> Option<&'static str> {
    let a = halign_css(&cell.align);
    (is_header || directed || a != "left").then_some(a)
}

/// A group label's `text-align`: its style's, else left, or the start when
/// `directed`.
fn group_label_alignment(label: &gridwell_layout::Line, directed: bool) -> &'static str {
    let default = if directed { "start" } else { "left" };
    label.style.text_align.as_ref().map_or(default, halign_css)
}

/// The table sets a direction that may be right to left (`rtl` or `auto`):
/// alignment, which is physical, is then said outright.
fn directed(table: &Table) -> bool {
    matches!(
        table.config.dir,
        Some(TextDirection::Rtl | TextDirection::Auto)
    )
}

fn halign_css(a: &HAlign) -> &'static str {
    match a {
        HAlign::Center => "center",
        HAlign::Right | HAlign::Char => "right",
        HAlign::Justify => "justify",
        _ => "left",
    }
}

/// An ` id="…"` attribute, for an id the registry made (it needs no escaping).
fn id_attr(id: Option<&str>) -> String {
    id.map(|id| format!(" id=\"{id}\"")).unwrap_or_default()
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
    // An explicitly removed border also removes the base stylesheet's.
    for (side, cleared) in s.border_cleared.iter() {
        if *cleared {
            parts.push(format!("border-{side}: none"));
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
