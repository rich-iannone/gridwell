//! A fluent builder DSL for constructing `gridwell_ir::Table` values.
//!
//! The IR types have no `Default` impls or constructors and every field must be
//! set explicitly when serialized, which makes hand-authoring test tables
//! painful. This module provides ergonomic helpers so an example table reads as
//! compact Rust while still producing a fully-populated, *valid* IR.
//!
//! The builder auto-derives `config.header_rows`, `config.body_rows`, and a
//! default `column_spec` (unless columns are supplied), so examples don't have
//! to keep those counts in sync by hand.

use gridwell_ir::cell::{GroupLabel, TableHead, TypedValue};
use gridwell_ir::config::Config;
use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{
    Border, BorderSet, ConditionalSelector, ConditionalStyle, Padding, StyleComposition, StyleDef,
};
use gridwell_ir::{
    Cell, ColumnSpec, Footer, Footnote, Header, HeaderLine, Row, RowGroup, SourceNote,
    StylePalette, Table, TableBlock,
};
use std::collections::HashMap;

// ─────────────────────────── content helpers ───────────────────────────

/// Plain text content node.
pub fn text(value: &str) -> ContentNode {
    ContentNode::Text {
        value: value.to_string(),
    }
}

/// Styled text content node referencing a style def by id.
pub fn styled(value: &str, style_id: &str) -> ContentNode {
    ContentNode::StyledText {
        value: value.to_string(),
        style_id: Some(style_id.to_string()),
    }
}

/// A hard line break within cell content.
pub fn line_break() -> ContentNode {
    ContentNode::LineBreak {}
}

/// An inline image.
pub fn image(src: &str, alt: &str) -> ContentNode {
    ContentNode::Image {
        src: src.to_string(),
        alt: Some(alt.to_string()),
        width: None,
        height: None,
    }
}

/// Raw markup for a specific format (e.g. `("html", "<b>x</b>")`).
pub fn raw(format: &str, value: &str) -> ContentNode {
    ContentNode::Raw {
        format: format.to_string(),
        value: value.to_string(),
    }
}

/// A footnote mark that references a footnote definition id.
pub fn footnote_mark(reference: &str, mark_text: &str) -> ContentNode {
    ContentNode::FootnoteMark {
        reference: reference.to_string(),
        mark_text: mark_text.to_string(),
    }
}

// ─────────────────────────────── cells ────────────────────────────────

/// A cell with plain text content.
pub fn cell(value: &str) -> Cell {
    cell_content(vec![text(value)])
}

/// A cell built from arbitrary content nodes.
pub fn cell_content(content: Vec<ContentNode>) -> Cell {
    Cell {
        content,
        typed_value: None,
        colspan: 1,
        rowspan: 1,
        style_id: None,
        is_stub: false,
        is_placeholder: false,
        scope: None,
        sort_key: None,
        data_type: None,
    }
}

/// An empty placeholder cell (occupies a position covered by a span).
pub fn placeholder() -> Cell {
    Cell {
        is_placeholder: true,
        ..cell_content(vec![])
    }
}

/// Extension methods for tweaking a cell inline.
pub trait CellExt {
    fn colspan(self, n: u32) -> Self;
    fn rowspan(self, n: u32) -> Self;
    fn stub(self) -> Self;
    fn style(self, style_id: &str) -> Self;
    fn scope(self, scope: &str) -> Self;
    fn typed(self, value_type: &str, value: serde_json::Value) -> Self;
    fn data_type(self, data_type: &str) -> Self;
}

impl CellExt for Cell {
    fn colspan(mut self, n: u32) -> Self {
        self.colspan = n;
        self
    }
    fn rowspan(mut self, n: u32) -> Self {
        self.rowspan = n;
        self
    }
    fn stub(mut self) -> Self {
        self.is_stub = true;
        self
    }
    fn style(mut self, style_id: &str) -> Self {
        self.style_id = Some(style_id.to_string());
        self
    }
    fn scope(mut self, scope: &str) -> Self {
        self.scope = Some(scope.to_string());
        self
    }
    fn typed(mut self, value_type: &str, value: serde_json::Value) -> Self {
        self.typed_value = Some(TypedValue {
            value_type: value_type.to_string(),
            value,
        });
        self
    }
    fn data_type(mut self, data_type: &str) -> Self {
        self.data_type = Some(data_type.to_string());
        self
    }
}

// ─────────────────────────────── rows ─────────────────────────────────

/// A row from a list of cells.
pub fn row(cells: Vec<Cell>) -> Row {
    Row {
        role: None,
        style_id: None,
        cells,
    }
}

/// Extension methods for rows.
pub trait RowExt {
    fn role(self, role: &str) -> Self;
    fn style(self, style_id: &str) -> Self;
}

impl RowExt for Row {
    fn role(mut self, role: &str) -> Self {
        self.role = Some(role.to_string());
        self
    }
    fn style(mut self, style_id: &str) -> Self {
        self.style_id = Some(style_id.to_string());
        self
    }
}

// ──────────────────────────── row groups ──────────────────────────────

/// A fluent builder for a single row group.
pub struct GroupBuilder {
    group: RowGroup,
}

/// Start an unlabeled row group.
pub fn group(rows: Vec<Row>) -> GroupBuilder {
    GroupBuilder {
        group: RowGroup {
            group_id: None,
            label: None,
            rows,
            summary_rows: vec![],
        },
    }
}

/// Start a labeled row group.
pub fn labeled_group(label: &str, rows: Vec<Row>) -> GroupBuilder {
    GroupBuilder {
        group: RowGroup {
            group_id: Some(label.to_lowercase().replace(' ', "_")),
            label: Some(GroupLabel {
                content: vec![text(label)],
                style_id: None,
                colspan: None,
            }),
            rows,
            summary_rows: vec![],
        },
    }
}

impl GroupBuilder {
    /// Attach summary rows to this group (requires `stub_cols >= 1`).
    pub fn summary(mut self, rows: Vec<Row>) -> Self {
        self.group.summary_rows = rows;
        self
    }
    /// Style the group label.
    pub fn label_style(mut self, style_id: &str) -> Self {
        if let Some(ref mut label) = self.group.label {
            label.style_id = Some(style_id.to_string());
        }
        self
    }
    /// Finish and return the underlying `RowGroup`.
    pub fn done(self) -> RowGroup {
        self.group
    }
}

// ─────────────────────────── style helpers ────────────────────────────

/// A single border edge.
pub fn border(width: &str, style: &str, color: &str) -> Border {
    Border {
        width: Some(width.to_string()),
        style: Some(style.to_string()),
        color: Some(color.to_string()),
    }
}

/// The same border on all four sides.
pub fn border_all(b: Border) -> BorderSet {
    BorderSet {
        top: Some(b.clone()),
        right: Some(b.clone()),
        bottom: Some(b.clone()),
        left: Some(b),
    }
}

/// Uniform padding on all four sides.
pub fn padding_all(value: &str) -> Padding {
    Padding {
        top: Some(value.to_string()),
        right: Some(value.to_string()),
        bottom: Some(value.to_string()),
        left: Some(value.to_string()),
    }
}

// ─────────────────────────── column helpers ───────────────────────────

/// A column spec with sane defaults; customize with the returned value.
pub fn column(id: &str, label: &str) -> ColumnSpec {
    ColumnSpec {
        id: id.to_string(),
        align: "left".to_string(),
        align_char: None,
        width: "auto".to_string(),
        min_width: None,
        max_width: None,
        style_id: None,
        hidden: false,
        label: Some(label.to_string()),
    }
}

/// Extension methods for column specs.
pub trait ColumnExt {
    fn align(self, align: &str) -> Self;
    fn align_char(self, ch: &str) -> Self;
    fn width(self, width: &str) -> Self;
    fn min_width(self, width: &str) -> Self;
    fn max_width(self, width: &str) -> Self;
    fn style(self, style_id: &str) -> Self;
    fn hidden(self) -> Self;
}

impl ColumnExt for ColumnSpec {
    fn align(mut self, align: &str) -> Self {
        self.align = align.to_string();
        self
    }
    fn align_char(mut self, ch: &str) -> Self {
        self.align_char = Some(ch.to_string());
        self
    }
    fn width(mut self, width: &str) -> Self {
        self.width = width.to_string();
        self
    }
    fn min_width(mut self, width: &str) -> Self {
        self.min_width = Some(width.to_string());
        self
    }
    fn max_width(mut self, width: &str) -> Self {
        self.max_width = Some(width.to_string());
        self
    }
    fn style(mut self, style_id: &str) -> Self {
        self.style_id = Some(style_id.to_string());
        self
    }
    fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
}

// ─────────────────────────── the builder ──────────────────────────────

/// Fluent builder for a table IR.
pub struct TableBuilder {
    cols: u32,
    config: Config,
    styles: StylePalette,
    header: Option<Header>,
    column_spec: Vec<ColumnSpec>,
    thead_rows: Vec<Row>,
    groups: Vec<RowGroup>,
    footnotes: Vec<Footnote>,
    source_notes: Vec<SourceNote>,
    extensions: Option<serde_json::Value>,
}

impl TableBuilder {
    /// Start a builder for a table with `cols` columns.
    pub fn new(cols: u32) -> Self {
        TableBuilder {
            cols,
            config: default_config(cols),
            styles: StylePalette {
                defs: HashMap::new(),
                compositions: HashMap::new(),
                conditionals: vec![],
            },
            header: None,
            column_spec: vec![],
            thead_rows: vec![],
            groups: vec![],
            footnotes: vec![],
            source_notes: vec![],
            extensions: None,
        }
    }

    // ── header ──

    /// Set the title.
    pub fn title(mut self, title: &str) -> Self {
        self.ensure_header().title = Some(HeaderLine {
            content: vec![text(title)],
            style_id: None,
        });
        self
    }

    /// Set the subtitle.
    pub fn subtitle(mut self, subtitle: &str) -> Self {
        self.ensure_header().subtitle = Some(HeaderLine {
            content: vec![text(subtitle)],
            style_id: None,
        });
        self
    }

    /// Add an extra header line.
    pub fn extra_line(mut self, line: &str) -> Self {
        self.ensure_header().extra_lines.push(HeaderLine {
            content: vec![text(line)],
            style_id: None,
        });
        self
    }

    fn ensure_header(&mut self) -> &mut Header {
        self.header.get_or_insert_with(|| Header {
            title: None,
            subtitle: None,
            extra_lines: vec![],
            preheader_content: None,
        })
    }

    // ── config ──

    /// Mutate the config directly for less-common fields.
    pub fn config(mut self, f: impl FnOnce(&mut Config)) -> Self {
        f(&mut self.config);
        self
    }

    /// Enable row striping (optionally over stub/body).
    pub fn striping(mut self, include_stub: bool, include_body: bool) -> Self {
        self.config.row_striping = true;
        self.config.row_striping_include_stub = include_stub;
        self.config.row_striping_include_body = include_body;
        self
    }

    /// Set the number of stub columns.
    pub fn stub_cols(mut self, n: u32) -> Self {
        self.config.stub_cols = n;
        self
    }

    /// Set the table width.
    pub fn table_width(mut self, width: &str) -> Self {
        self.config.table_width = Some(width.to_string());
        self
    }

    /// Hide the column-label row visually.
    pub fn hide_column_labels(mut self) -> Self {
        self.config.column_labels_hidden = true;
        self
    }

    // ── columns ──

    /// Add a single column spec.
    pub fn column(mut self, col: ColumnSpec) -> Self {
        self.column_spec.push(col);
        self
    }

    /// Add multiple column specs at once.
    pub fn columns(mut self, cols: Vec<ColumnSpec>) -> Self {
        self.column_spec.extend(cols);
        self
    }

    // ── styles ──

    /// Register a style definition.
    pub fn style_def(mut self, id: &str, def: StyleDef) -> Self {
        self.styles.defs.insert(id.to_string(), def);
        self
    }

    /// Register a style composition (extends a base def with overrides).
    pub fn composition(mut self, id: &str, extends: &str, overrides: StyleDef) -> Self {
        self.styles.compositions.insert(
            id.to_string(),
            StyleComposition {
                extends: extends.to_string(),
                overrides,
            },
        );
        self
    }

    /// Register a conditional style (e.g. row-parity striping).
    pub fn conditional(mut self, id: &str, selector: ConditionalSelector, style: StyleDef) -> Self {
        self.styles.conditionals.push(ConditionalStyle {
            id: id.to_string(),
            selector,
            style,
        });
        self
    }

    // ── rows ──

    /// Add a header (thead) row.
    pub fn head(mut self, row: Row) -> Self {
        self.thead_rows.push(row);
        self
    }

    /// Add body rows as a single unlabeled group.
    pub fn body(mut self, rows: Vec<Row>) -> Self {
        self.groups.push(group(rows).done());
        self
    }

    /// Add a prebuilt row group.
    pub fn group(mut self, g: GroupBuilder) -> Self {
        self.groups.push(g.done());
        self
    }

    // ── footer ──

    /// Add a footnote definition.
    pub fn footnote(mut self, id: &str, mark: &str, content: &str) -> Self {
        self.footnotes.push(Footnote {
            id: id.to_string(),
            mark: mark.to_string(),
            content: vec![text(content)],
            style_id: None,
        });
        self
    }

    /// Add a source note.
    pub fn source_note(mut self, content: &str) -> Self {
        self.source_notes.push(SourceNote {
            content: vec![text(content)],
            style_id: None,
        });
        self
    }

    /// Set the extensions blob.
    pub fn extensions(mut self, value: serde_json::Value) -> Self {
        self.extensions = Some(value);
        self
    }

    // ── build ──

    /// Finalize into a `Table`, auto-deriving row counts and (if none were
    /// supplied) a default column spec.
    pub fn build(mut self) -> Table {
        // Derive counts from what was added so validation passes.
        self.config.table_cols = self.cols;
        self.config.header_rows = self.thead_rows.len() as u32;
        self.config.body_rows = self.groups.iter().map(|g| g.rows.len() as u32).sum();

        // Auto-generate a default column spec if the author didn't supply one.
        if self.column_spec.is_empty() {
            self.column_spec = (0..self.cols)
                .map(|i| column(&format!("col_{i}"), &format!("col_{i}")))
                .collect();
        }

        let footer = if self.footnotes.is_empty() && self.source_notes.is_empty() {
            None
        } else {
            Some(Footer {
                footnotes: self.footnotes,
                source_notes: self.source_notes,
            })
        };

        Table {
            ir_version: "1.0".to_string(),
            config: self.config,
            styles: self.styles,
            header: self.header,
            column_spec: self.column_spec,
            table: TableBlock {
                thead: TableHead {
                    rows: self.thead_rows,
                },
                tbody: self.groups,
            },
            footer,
            extensions: self.extensions,
        }
    }
}

fn default_config(cols: u32) -> Config {
    Config {
        table_cols: cols,
        header_rows: 0,
        body_rows: 0,
        stub_cols: 0,
        row_striping: false,
        row_striping_include_stub: false,
        row_striping_include_body: false,
        column_labels_hidden: false,
        table_width: None,
        container_width: None,
        container_height: None,
        container_overflow: None,
        locale: "en-US".to_string(),
        page_break_mode: "avoid".to_string(),
        aria_label: None,
        aria_describedby: None,
        summary: None,
    }
}
