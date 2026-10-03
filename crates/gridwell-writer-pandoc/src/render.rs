//! Pandoc JSON AST (pandoc-types 1.23+) from the resolved layout.
//!
//! The building blocks are public so the Quarto writer, which wraps the same table
//! in a cross-referenceable `Div`, shares them instead of copying them.

use gridwell_core::Length;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table};
use gridwell_layout::{resolve, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Renders a gridwell IR Table to a Pandoc JSON AST Table block.
pub fn render(table: &Table) -> Result<String, RenderError> {
    let rt = resolve(table);
    let block = table_block(&rt, true);
    Ok(serde_json::to_string_pretty(&block)?)
}

/// The `Table` block. With `notes_in_foot`, footnotes and source notes go in the
/// table foot (one full-width cell); otherwise the foot is empty and the caller
/// places them.
pub fn table_block(rt: &ResolvedTable, notes_in_foot: bool) -> Value {
    // Table: (Attr, Caption, [ColSpec], TableHead, [TableBody], TableFoot)
    let foot_rows = if notes_in_foot {
        notes_row(rt).into_iter().collect()
    } else {
        Vec::new()
    };
    json!({
        "t": "Table",
        "c": [
            null_attr(),
            caption(rt),
            colspecs(rt),
            [null_attr(), rt.head.rows.iter().map(|r| row(rt, r)).collect::<Vec<_>>()],
            bodies(rt),
            [null_attr(), foot_rows],
        ]
    })
}

pub fn null_attr() -> Value {
    json!(["", [], []])
}

fn caption(rt: &ResolvedTable) -> Value {
    // Caption: (Maybe ShortCaption, [Block])
    let h = &rt.header;
    let blocks: Vec<Value> = h
        .title
        .iter()
        .chain(&h.subtitle)
        .chain(&h.extra_lines)
        .map(|line| json!({ "t": "Para", "c": styled(&line.style, inlines(rt, line.content)) }))
        .collect();
    json!([Value::Null, blocks])
}

fn alignment(align: &HAlign) -> Value {
    match align {
        HAlign::Left => json!({"t": "AlignLeft"}),
        HAlign::Right => json!({"t": "AlignRight"}),
        HAlign::Center => json!({"t": "AlignCenter"}),
        _ => json!({"t": "AlignDefault"}),
    }
}

fn colspecs(rt: &ResolvedTable) -> Value {
    rt.columns
        .iter()
        .map(|col| {
            // ColWidth is a fraction of the text width. Absolute widths assume a
            // 600pt (800px) line; `fr` and `auto` are left to Pandoc.
            let width = match &col.width {
                Some(Length::Percent(p)) => Some(p / 100.0),
                Some(l) => l.to_pt(12.0, 12.0).map(|pt| pt / 600.0),
                None => None,
            };
            let width = match width.filter(|w| w.is_finite() && *w > 0.0) {
                Some(w) => json!({"t": "ColWidth", "c": w}),
                None => json!({"t": "ColWidthDefault"}),
            };
            json!([alignment(&col.align), width])
        })
        .collect()
}

fn bodies(rt: &ResolvedTable) -> Value {
    rt.groups
        .iter()
        .map(|g| {
            // TableBody: (Attr, RowHeadColumns, [Row] intermediate head, [Row] body).
            // A group label is the body's intermediate head: one full-width cell.
            let head: Vec<Value> = match &g.label {
                Some(label) if !rt.columns.is_empty() => vec![json!([
                    null_attr(),
                    [cell_value(
                        json!({"t": "AlignDefault"}),
                        1,
                        rt.columns.len(),
                        styled(&label.style, inlines(rt, label.content)),
                    )]
                ])],
                _ => Vec::new(),
            };
            let rows: Vec<Value> = g
                .rows
                .rows
                .iter()
                .chain(&g.summary_rows.rows)
                .map(|r| row(rt, r))
                .collect();
            json!([null_attr(), rt.stub_cols, head, rows])
        })
        .collect()
}

/// Footnotes and source notes as a single full-width foot row.
fn notes_row(rt: &ResolvedTable) -> Option<Value> {
    if rt.footer.is_empty() || rt.columns.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for line in note_lines(rt) {
        if !out.is_empty() {
            out.push(json!({"t": "LineBreak"}));
        }
        out.extend(line);
    }
    Some(json!([
        null_attr(),
        [json!([
            null_attr(),
            {"t": "AlignDefault"},
            1,
            rt.columns.len(),
            [{"t": "Para", "c": out}]
        ])]
    ]))
}

/// One inline run per footnote (superscript mark, space, text), then one per
/// source note.
pub fn note_lines(rt: &ResolvedTable) -> Vec<Vec<Value>> {
    let mut lines = footnote_lines(rt);
    lines.extend(source_note_lines(rt));
    lines
}

pub fn footnote_lines(rt: &ResolvedTable) -> Vec<Vec<Value>> {
    rt.footer
        .footnotes
        .iter()
        .map(|n| {
            let mut line = vec![
                json!({"t": "Superscript", "c": [{"t": "Str", "c": n.mark}]}),
                json!({"t": "Space"}),
            ];
            line.extend(inlines(rt, n.content));
            styled(&n.style, line)
        })
        .collect()
}

pub fn source_note_lines(rt: &ResolvedTable) -> Vec<Vec<Value>> {
    rt.footer
        .source_notes
        .iter()
        .map(|n| styled(&n.style, inlines(rt, n.content)))
        .collect()
}

fn row(rt: &ResolvedTable, row: &ResolvedRow) -> Value {
    // Row: (Attr, [Cell]); covered positions are implied by spans.
    let cells: Vec<Value> = row.cells().map(|c| cell(rt, c)).collect();
    json!([null_attr(), cells])
}

fn cell(rt: &ResolvedTable, c: &ResolvedCell) -> Value {
    // A cell's own alignment only when its style sets one; otherwise the column's.
    let align = match &c.style.text_align {
        Some(a) => alignment(a),
        None => json!({"t": "AlignDefault"}),
    };
    cell_value(
        align,
        c.rowspan,
        c.colspan,
        styled(&c.style, inlines(rt, c.content)),
    )
}

fn cell_value(align: Value, rowspan: usize, colspan: usize, content: Vec<Value>) -> Value {
    // Cell: (Attr, Alignment, RowSpan, ColSpan, [Block])
    let blocks = if content.is_empty() {
        vec![]
    } else {
        vec![json!({ "t": "Plain", "c": content })]
    };
    json!([null_attr(), align, rowspan, colspan, blocks])
}

/// Wrap inlines in `Strong` / `Emph` for a bold / italic style. Pandoc has no
/// other inline styling.
pub fn styled(style: &ResolvedStyle, mut inlines: Vec<Value>) -> Vec<Value> {
    if inlines.is_empty() {
        return inlines;
    }
    if style.is_italic() {
        inlines = vec![json!({"t": "Emph", "c": inlines})];
    }
    if style.is_bold() {
        inlines = vec![json!({"t": "Strong", "c": inlines})];
    }
    inlines
}

fn words(value: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for (i, word) in value.split(' ').enumerate() {
        if i > 0 {
            out.push(json!({"t": "Space"}));
        }
        if !word.is_empty() {
            out.push(json!({"t": "Str", "c": word}));
        }
    }
    out
}

pub fn inlines(rt: &ResolvedTable, nodes: &[ContentNode]) -> Vec<Value> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => out.extend(words(value)),
            ContentNode::StyledText { value, style_id } => {
                let style = style_id
                    .as_deref()
                    .map(|id| rt.style(id))
                    .unwrap_or_default();
                out.extend(styled(&style, words(value)));
            }
            ContentNode::LineBreak {} => out.push(json!({"t": "LineBreak"})),
            ContentNode::FootnoteMark { mark_text, .. } => out.push(json!({
                "t": "Superscript",
                "c": [{"t": "Str", "c": mark_text}]
            })),
            ContentNode::Image { src, alt, .. } => {
                let alt: Vec<Value> = alt.iter().map(|a| json!({"t": "Str", "c": a})).collect();
                out.push(json!({"t": "Image", "c": [null_attr(), alt, [src, ""]]}));
            }
            ContentNode::Raw { value, .. } => out.push(json!({"t": "RawInline", "c": ["", value]})),
            ContentNode::Unknown => {}
        }
    }
    out
}

/// Content as plain text (for attributes such as Quarto's `tbl-cap`).
pub fn plain_text(nodes: &[ContentNode]) -> String {
    gridwell_layout::plain_text(nodes, " ")
}
