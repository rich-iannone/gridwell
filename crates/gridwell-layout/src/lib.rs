//! The resolved layout shared by every gridwell writer.
//!
//! [`resolve`] turns IR into a [`ResolvedTable`]: the grid as it should appear, with
//! everything a writer would otherwise re-derive (and, historically, re-derived
//! differently in each writer) already worked out:
//!
//! - **Visible grid.** Hidden columns are gone. Spans crossing a hidden column shrink;
//!   a cell lying entirely in hidden columns disappears; a cell whose origin is hidden
//!   but whose span reaches a visible column starts at the first visible one.
//! - **Slots.** Every row has exactly one [`Slot`] per visible column: a cell's
//!   origin, or a position it covers from the left ([`Slot::CoveredH`]) or from above
//!   ([`Slot::CoveredV`]). Writers that skip covered positions (HTML, Typst, Pandoc)
//!   and writers that emit continuation cells (RTF, DOCX, PPTX, LaTeX) both read the
//!   distinction from the type; see also [`Section::continuation_rows`].
//! - **Styles.** Each cell carries its final [`ResolvedStyle`] after the cascade
//!   (column default → striping → conditionals → row → cell; see [`style`]), with
//!   every value parsed and every invalid value dropped.
//! - **Column labels.** With `config.column_labels_hidden`, the head is empty.
//!
//! Resolution never panics, on any IR. On valid IR, every row's slots tile the
//! visible columns exactly; on invalid IR, positions no cell can claim are
//! [`Slot::Empty`].

pub mod style;
pub mod text;

use gridwell_core::Length;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{
    resolve_slots, Cell, CellScope, ColumnSpec, ColumnVisibility, HAlign, Keyword, RowParity,
    RowRole, SelectorScope, Table, VMerge,
};
pub use style::{ResolvedBorder, ResolvedStyle, Sides};
pub use text::plain_text;

use gridwell_core::Color;
use gridwell_ir::cell::{Row, TypedValue};
use gridwell_ir::style::{ConditionalSelector, StyleDef};

/// Background of striped rows when `config.row_striping` is on: gt's default,
/// a 5% grey. Formats without alpha show it composited over white.
pub const STRIPE_COLOR: Color = Color::new(128, 128, 128, 13);

/// A table ready for a writer.
#[derive(Debug, Clone)]
pub struct ResolvedTable<'a> {
    /// The source IR, for configuration the layout does not model.
    pub table: &'a Table,
    /// The visible columns, left to right.
    pub columns: Vec<ResolvedColumn<'a>>,
    /// How many of the visible columns are stub columns (they come first).
    pub stub_cols: usize,
    pub header: ResolvedHeader<'a>,
    /// Column-label rows; empty when `config.column_labels_hidden`.
    pub head: Section<'a>,
    pub groups: Vec<Group<'a>>,
    pub footer: ResolvedFooter<'a>,
}

impl<'a> ResolvedTable<'a> {
    /// Every section in document order: head, then each group's rows and summary
    /// rows.
    pub fn sections(&self) -> impl Iterator<Item = &Section<'a>> {
        std::iter::once(&self.head)
            .chain(self.groups.iter().flat_map(|g| [&g.rows, &g.summary_rows]))
    }

    /// The style a style id names on its own (no cascade): for inline
    /// `styled_text` runs. Unknown ids give the empty style.
    pub fn style(&self, id: &str) -> ResolvedStyle {
        style::resolve_id(self.table, Some(id))
    }

    /// Several style ids laid over each other, later ones winning (no column,
    /// striping or conditional layers). Unknown ids contribute nothing.
    pub fn styles<'s>(&self, ids: impl IntoIterator<Item = &'s str>) -> ResolvedStyle {
        let mut def = StyleDef::default();
        for id in ids {
            if let Some(d) = style::lookup(self.table, id) {
                style::overlay(&mut def, &d);
            }
        }
        ResolvedStyle::from_def(&def)
    }

    /// Every piece of content the writer will emit, in document order: header lines,
    /// cells (visible ones only), group labels, footnotes, source notes. For writers
    /// that must collect things (colours, fonts) before writing.
    pub fn contents(&self) -> Vec<&'a [ContentNode]> {
        let h = &self.header;
        let mut out: Vec<&'a [ContentNode]> = h
            .title
            .iter()
            .chain(&h.subtitle)
            .chain(&h.extra_lines)
            .map(|l| l.content)
            .collect();
        out.extend(
            self.head
                .rows
                .iter()
                .flat_map(|r| r.cells().map(|c| c.content)),
        );
        for g in &self.groups {
            out.extend(g.label.iter().map(|l| l.content));
            for r in g.rows.rows.iter().chain(&g.summary_rows.rows) {
                out.extend(r.cells().map(|c| c.content));
            }
        }
        out.extend(self.footer.footnotes.iter().map(|n| n.content));
        out.extend(self.footer.source_notes.iter().map(|n| n.content));
        out
    }

    /// True if no column is visible: writers emit no table body at all.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
}

/// A visible column.
#[derive(Debug, Clone)]
pub struct ResolvedColumn<'a> {
    /// Index of this column in `column_spec` (the IR grid).
    pub grid_index: usize,
    pub spec: &'a ColumnSpec,
    /// Alignment; an unknown value falls back to left.
    pub align: HAlign,
    /// Declared width; `None` for `auto` or an unparseable value.
    pub width: Option<Length>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub is_stub: bool,
}

/// Title, subtitle and extra header lines.
#[derive(Debug, Clone, Default)]
pub struct ResolvedHeader<'a> {
    pub title: Option<Line<'a>>,
    pub subtitle: Option<Line<'a>>,
    pub extra_lines: Vec<Line<'a>>,
}

impl ResolvedHeader<'_> {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.subtitle.is_none() && self.extra_lines.is_empty()
    }
}

/// A run of content outside the grid (header line, group label, note) and its style.
#[derive(Debug, Clone)]
pub struct Line<'a> {
    pub content: &'a [ContentNode],
    /// The style id as written (for formats that reference styles by name).
    pub style_id: Option<&'a str>,
    pub style: ResolvedStyle,
}

/// Footnotes and source notes.
#[derive(Debug, Clone, Default)]
pub struct ResolvedFooter<'a> {
    pub footnotes: Vec<Footnote<'a>>,
    pub source_notes: Vec<Line<'a>>,
}

impl<'a> ResolvedFooter<'a> {
    pub fn is_empty(&self) -> bool {
        self.footnotes.is_empty() && self.source_notes.is_empty()
    }

    /// The footnote a `footnote_mark`'s `ref` points at.
    pub fn footnote(&self, id: &str) -> Option<&Footnote<'a>> {
        self.footnotes.iter().find(|n| n.id == id)
    }
}

/// A footnote definition.
#[derive(Debug, Clone)]
pub struct Footnote<'a> {
    pub id: &'a str,
    pub mark: &'a str,
    pub content: &'a [ContentNode],
    pub style_id: Option<&'a str>,
    pub style: ResolvedStyle,
    /// Whether any `footnote_mark` in the table (cells, header, labels, notes)
    /// refers to this footnote. Unreferenced footnotes are still rendered.
    pub referenced: bool,
}

/// A row group.
#[derive(Debug, Clone)]
pub struct Group<'a> {
    pub id: Option<&'a str>,
    /// Spans the full visible width.
    pub label: Option<Line<'a>>,
    pub rows: Section<'a>,
    pub summary_rows: Section<'a>,
}

/// Which part of the table a section is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// Column labels (thead).
    Head,
    /// A group's data rows.
    Body,
    /// A group's summary rows.
    Summary,
}

/// A span-closed run of rows: no span crosses a section boundary.
#[derive(Debug, Clone)]
pub struct Section<'a> {
    pub kind: SectionKind,
    pub rows: Vec<ResolvedRow<'a>>,
}

/// A row of the visible grid.
#[derive(Debug, Clone)]
pub struct ResolvedRow<'a> {
    pub row: &'a Row,
    /// The row's role; an unknown value is `None`.
    pub role: Option<RowRole>,
    /// Position among rows of the same kind, from 1 (CSS `:nth-child` numbering):
    /// head rows count within the head, data rows across all groups, summary rows
    /// within their group.
    pub number: usize,
    /// Whether row striping colours this row (`config.row_striping`, even rows).
    pub striped: bool,
    /// Exactly one slot per visible column.
    pub slots: Vec<Slot<'a>>,
}

impl<'a> ResolvedRow<'a> {
    /// The cells that start in this row, left to right.
    pub fn cells(&self) -> impl Iterator<Item = &ResolvedCell<'a>> {
        self.slots.iter().filter_map(Slot::origin)
    }
}

/// One position of the visible grid.
#[derive(Debug, Clone)]
pub enum Slot<'a> {
    /// A cell starts here.
    Origin(Box<ResolvedCell<'a>>),
    /// Covered by a cell starting earlier in the same row.
    CoveredH { origin_col: usize },
    /// Covered by a cell starting in an earlier row of the section. For a 2D span,
    /// `origin_col` may be left of this position. `last` marks the span's bottom row.
    CoveredV {
        origin_row: usize,
        origin_col: usize,
        last: bool,
    },
    /// No cell covers this position (only in invalid IR).
    Empty,
}

impl<'a> Slot<'a> {
    pub fn origin(&self) -> Option<&ResolvedCell<'a>> {
        match self {
            Slot::Origin(c) => Some(c),
            _ => None,
        }
    }
}

/// A cell, positioned in the visible grid, with its style resolved.
#[derive(Debug, Clone)]
pub struct ResolvedCell<'a> {
    pub cell: &'a Cell,
    pub content: &'a [ContentNode],
    /// Visible column of the cell's left edge.
    pub col: usize,
    /// Grid column (in `column_spec`) of the cell's visible left edge.
    pub grid_col: usize,
    /// Width and height in visible columns and section rows (at least 1).
    pub colspan: usize,
    pub rowspan: usize,
    /// The cascaded style.
    pub style: ResolvedStyle,
    /// Horizontal alignment: the style's; else centred for a header cell spanning
    /// several columns (a spanner label); else the column's.
    pub align: HAlign,
    /// In the head section.
    pub is_header: bool,
    /// In a stub column, or flagged `is_stub`.
    pub is_stub: bool,
    /// The cell's `scope`; an unknown value is `None`.
    pub scope: Option<CellScope>,
    pub typed_value: Option<&'a TypedValue>,
}

/// One cell as emitted by formats that model rowspans with explicit continuation
/// cells (RTF, DOCX). See [`Section::continuation_rows`].
#[derive(Debug, Clone, Copy)]
pub struct MergeCell<'r, 'a> {
    /// Visible column of the left edge.
    pub col: usize,
    /// Width in visible columns.
    pub span: usize,
    /// The cell for origins; `None` for continuations and empty positions.
    pub cell: Option<&'r ResolvedCell<'a>>,
    pub vmerge: VMerge,
}

impl<'a> Section<'a> {
    /// Lay the section out for formats with continuation cells: each row lists the
    /// cells to emit, left to right, and their spans tile the visible columns.
    ///
    /// An origin is one cell as wide as its colspan, [`VMerge::Start`] if it spans
    /// rows. Below it, each covered row emits one [`VMerge::Continue`] cell of the
    /// same width. Empty positions (invalid IR) emit one-column empty cells.
    pub fn continuation_rows(&self) -> Vec<Vec<MergeCell<'_, 'a>>> {
        self.rows
            .iter()
            .map(|row| {
                let mut out = Vec::new();
                for (col, slot) in row.slots.iter().enumerate() {
                    match slot {
                        Slot::Origin(c) => out.push(MergeCell {
                            col,
                            span: c.colspan,
                            cell: Some(c),
                            vmerge: if c.rowspan > 1 {
                                VMerge::Start
                            } else {
                                VMerge::None
                            },
                        }),
                        Slot::CoveredV {
                            origin_row,
                            origin_col,
                            ..
                        } if *origin_col == col => {
                            let span = self.rows[*origin_row].slots[col]
                                .origin()
                                .map_or(1, |c| c.colspan);
                            out.push(MergeCell {
                                col,
                                span,
                                cell: None,
                                vmerge: VMerge::Continue,
                            });
                        }
                        Slot::CoveredH { .. } | Slot::CoveredV { .. } => {}
                        Slot::Empty => out.push(MergeCell {
                            col,
                            span: 1,
                            cell: None,
                            vmerge: VMerge::None,
                        }),
                    }
                }
                out
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Resolve a table for writing. Never panics; intended for validated IR.
pub fn resolve(table: &Table) -> ResolvedTable<'_> {
    let visibility = ColumnVisibility::from_spec(&table.column_spec);
    // The IR grid is `config.table_cols` wide; `column_spec` should match it, but
    // only columns that have a spec can be shown.
    let grid_stub = table.config.stub_cols as usize;
    let columns: Vec<ResolvedColumn<'_>> = visibility
        .visible_columns()
        .map(|i| {
            let spec = &table.column_spec[i];
            ResolvedColumn {
                grid_index: i,
                spec,
                align: if spec.align.is_known() {
                    spec.align.clone()
                } else {
                    HAlign::Left
                },
                width: spec
                    .width
                    .parse()
                    .ok()
                    .filter(|l| !matches!(l, Length::Auto)),
                min_width: spec.min_width.as_deref().and_then(|s| s.parse().ok()),
                max_width: spec.max_width.as_deref().and_then(|s| s.parse().ok()),
                is_stub: i < grid_stub,
            }
        })
        .collect();
    let stub_cols = columns.iter().filter(|c| c.is_stub).count();

    let ctx = Ctx {
        table,
        visibility: &visibility,
        columns: &columns,
    };

    let head = if table.config.column_labels_hidden {
        Section {
            kind: SectionKind::Head,
            rows: Vec::new(),
        }
    } else {
        ctx.section(&table.table.thead.rows, SectionKind::Head, 0)
    };

    let mut body_rows_before = 0;
    let groups = table
        .table
        .tbody
        .iter()
        .map(|g| {
            let rows = ctx.section(&g.rows, SectionKind::Body, body_rows_before);
            body_rows_before += g.rows.len();
            Group {
                id: g.group_id.as_deref(),
                label: g
                    .label
                    .as_ref()
                    .map(|l| line(table, &l.content, l.style_id.as_deref())),
                rows,
                summary_rows: ctx.section(&g.summary_rows, SectionKind::Summary, 0),
            }
        })
        .collect();

    let header = table
        .header
        .as_ref()
        .map(|h| ResolvedHeader {
            title: h
                .title
                .as_ref()
                .map(|l| line(table, &l.content, l.style_id.as_deref())),
            subtitle: h
                .subtitle
                .as_ref()
                .map(|l| line(table, &l.content, l.style_id.as_deref())),
            extra_lines: h
                .extra_lines
                .iter()
                .map(|l| line(table, &l.content, l.style_id.as_deref()))
                .collect(),
        })
        .unwrap_or_default();
    let referenced = footnote_refs(table);
    let footer = table
        .footer
        .as_ref()
        .map(|f| ResolvedFooter {
            footnotes: f
                .footnotes
                .iter()
                .map(|n| Footnote {
                    id: &n.id,
                    mark: &n.mark,
                    content: &n.content,
                    style_id: n.style_id.as_deref(),
                    style: style::resolve_id(table, n.style_id.as_deref()),
                    referenced: referenced.contains(n.id.as_str()),
                })
                .collect(),
            source_notes: f
                .source_notes
                .iter()
                .map(|n| line(table, &n.content, n.style_id.as_deref()))
                .collect(),
        })
        .unwrap_or_default();

    ResolvedTable {
        table,
        columns,
        stub_cols,
        header,
        head,
        groups,
        footer,
    }
}

/// Every footnote id some `footnote_mark` refers to, anywhere in the table.
fn footnote_refs(table: &Table) -> std::collections::HashSet<&str> {
    let mut contents: Vec<&[ContentNode]> = Vec::new();
    if let Some(h) = &table.header {
        let lines = h.title.iter().chain(&h.subtitle).chain(&h.extra_lines);
        contents.extend(lines.map(|l| l.content.as_slice()));
    }
    let groups = &table.table.tbody;
    let rows = table.table.thead.rows.iter().chain(
        groups
            .iter()
            .flat_map(|g| g.rows.iter().chain(&g.summary_rows)),
    );
    contents.extend(rows.flat_map(|r| r.cells.iter().map(|c| c.content.as_slice())));
    contents.extend(
        groups
            .iter()
            .filter_map(|g| g.label.as_ref())
            .map(|l| l.content.as_slice()),
    );
    if let Some(f) = &table.footer {
        contents.extend(f.footnotes.iter().map(|n| n.content.as_slice()));
        contents.extend(f.source_notes.iter().map(|n| n.content.as_slice()));
    }
    contents
        .into_iter()
        .flatten()
        .filter_map(|n| match n {
            ContentNode::FootnoteMark { reference, .. } => Some(reference.as_str()),
            _ => None,
        })
        .collect()
}

fn line<'a>(table: &'a Table, content: &'a [ContentNode], style_id: Option<&'a str>) -> Line<'a> {
    Line {
        content,
        style_id,
        style: style::resolve_id(table, style_id),
    }
}

struct Ctx<'t, 'a> {
    table: &'a Table,
    visibility: &'t ColumnVisibility,
    columns: &'t [ResolvedColumn<'a>],
}

impl<'a> Ctx<'_, 'a> {
    /// Resolve one section. `numbered_before` is how many rows of the same kind
    /// precede it (data rows are numbered across groups).
    fn section(&self, rows: &'a [Row], kind: SectionKind, numbered_before: usize) -> Section<'a> {
        let width = self.visibility.visible_len();
        let ir_slots = resolve_slots(rows);

        // Place origins in the visible grid, row-major. In invalid IR an origin may
        // land on a position already claimed: it is dropped, so the grid stays a
        // tiling (every covered slot points at the origin that covers it).
        let mut slots: Vec<Vec<Slot<'a>>> = rows
            .iter()
            .map(|_| (0..width).map(|_| Slot::Empty).collect())
            .collect();
        let mut placed: Vec<(usize, usize, &'a Cell, usize, usize, usize)> = Vec::new();
        for (r, (row, row_slots)) in rows.iter().zip(&ir_slots).enumerate() {
            for (c, (cell, slot)) in row.cells.iter().zip(row_slots).enumerate() {
                let gridwell_ir::Slot::Origin { colspan, rowspan } = *slot else {
                    continue;
                };
                let Some((vc, vspan)) = self.visibility.project(c, colspan) else {
                    continue;
                };
                let claimable = (r..r + rowspan)
                    .all(|rr| (vc..vc + vspan).all(|cc| matches!(slots[rr][cc], Slot::Empty)));
                if !claimable {
                    continue;
                }
                for (dr, slot_row) in slots[r..r + rowspan].iter_mut().enumerate() {
                    for (dc, s) in slot_row[vc..vc + vspan].iter_mut().enumerate() {
                        *s = match (dr, dc) {
                            (0, 0) => continue, // the origin, filled in below
                            (0, _) => Slot::CoveredH { origin_col: vc },
                            _ => Slot::CoveredV {
                                origin_row: r,
                                origin_col: vc,
                                last: dr == rowspan - 1,
                            },
                        };
                    }
                }
                // Mark the origin as claimed until the cell is built.
                slots[r][vc] = Slot::CoveredH { origin_col: vc };
                // The cell's column is its visible left edge: a cell whose origin
                // column is hidden behaves as if it started at the first visible
                // column it covers (column style, alignment, stub status).
                placed.push((r, vc, cell, self.columns[vc].grid_index, vspan, rowspan));
            }
        }

        let resolved_rows: Vec<(usize, bool)> = (0..rows.len())
            .map(|r| {
                let number = numbered_before + r + 1;
                let striped =
                    kind == SectionKind::Body && self.table.config.row_striping && number % 2 == 0;
                (number, striped)
            })
            .collect();

        for (r, vc, cell, grid_col, colspan, rowspan) in placed {
            let (number, striped) = resolved_rows[r];
            let style = self.cascade(&rows[r], cell, grid_col, kind, number, striped);
            let column = &self.columns[vc];
            // A header cell spanning several columns (a spanner label) is centred
            // over them, as in gt; any other cell takes its column's alignment.
            // An explicit style alignment wins over both.
            let default_align = if kind == SectionKind::Head && colspan > 1 {
                HAlign::Center
            } else {
                column.align.clone()
            };
            let align = style.text_align.clone().unwrap_or(default_align);
            slots[r][vc] = Slot::Origin(Box::new(ResolvedCell {
                cell,
                content: &cell.content,
                col: vc,
                grid_col,
                colspan,
                rowspan,
                align,
                is_header: kind == SectionKind::Head,
                is_stub: cell.is_stub || grid_col < self.table.config.stub_cols as usize,
                scope: cell.scope.clone().filter(|s| s.is_known()),
                typed_value: cell.typed_value.as_ref(),
                style,
            }));
        }

        Section {
            kind,
            rows: rows
                .iter()
                .zip(slots)
                .zip(resolved_rows)
                .map(|((row, slots), (number, striped))| ResolvedRow {
                    row,
                    role: row.role.clone().filter(|r| r.is_known()),
                    number,
                    striped,
                    slots,
                })
                .collect(),
        }
    }

    /// The style cascade for one cell (see [`style`]).
    fn cascade(
        &self,
        row: &Row,
        cell: &Cell,
        grid_col: usize,
        kind: SectionKind,
        number: usize,
        striped: bool,
    ) -> ResolvedStyle {
        let table = self.table;
        let mut def = StyleDef::default();
        let apply_id = |def: &mut StyleDef, id: Option<&str>| {
            if let Some(d) = id.and_then(|id| style::lookup(table, id)) {
                style::overlay(def, &d);
            }
        };

        // 1. Column default: body and summary cells only (column labels are styled
        //    by their own row and cell styles).
        if kind != SectionKind::Head {
            let col_style = table
                .column_spec
                .get(grid_col)
                .and_then(|c| c.style_id.as_deref());
            apply_id(&mut def, col_style);
        }

        // 2. Striping.
        if striped {
            let is_stub = cell.is_stub || grid_col < table.config.stub_cols as usize;
            let include = if is_stub {
                table.config.row_striping_include_stub
            } else {
                table.config.row_striping_include_body
            };
            if include {
                def.background_color = Some(STRIPE_COLOR.to_css());
            }
        }

        // 3. Conditionals, in order.
        for cond in &table.styles.conditionals {
            if selector_matches(&cond.selector, kind, number) {
                style::overlay(&mut def, &cond.style);
            }
        }

        // 4. Row, 5. cell.
        apply_id(&mut def, row.style_id.as_deref());
        apply_id(&mut def, cell.style_id.as_deref());

        ResolvedStyle::from_def(&def)
    }
}

/// Whether a conditional selector applies to a row. No scope means the whole table;
/// `tbody` means data rows (not summary rows). Parity uses the row's 1-based
/// [`ResolvedRow::number`], like CSS `:nth-child(odd|even)`. Unknown selector values
/// match nothing.
fn selector_matches(sel: &ConditionalSelector, kind: SectionKind, number: usize) -> bool {
    let scope_ok = match &sel.scope {
        None | Some(SelectorScope::Table) => true,
        Some(SelectorScope::Tbody) => kind == SectionKind::Body,
        Some(SelectorScope::Thead) => kind == SectionKind::Head,
        Some(SelectorScope::Unknown(_)) => false,
    };
    let parity_ok = match &sel.row_parity {
        None => true,
        Some(RowParity::Even) => number % 2 == 0,
        Some(RowParity::Odd) => number % 2 == 1,
        Some(RowParity::Unknown(_)) => false,
    };
    scope_ok && parity_ok
}
