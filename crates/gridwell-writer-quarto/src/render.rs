use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table};
use serde_json::{json, Value};
use thiserror::Error;

use crate::QuartoConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Renders a gridwell IR Table to a Quarto-flavored Pandoc JSON AST.
///
/// The output is a Pandoc `Div` block wrapping a `Table` block, with
/// Quarto-specific attributes for cross-referencing (`tbl-` prefix IDs)
/// and caption handling.
pub fn render(table: &Table, config: &QuartoConfig) -> Result<String, RenderError> {
    let block = render_quarto_block(table, config);
    let output = serde_json::to_string_pretty(&block)?;
    Ok(output)
}

fn render_quarto_block(table: &Table, config: &QuartoConfig) -> Value {
    let table_block = render_table_block(table);

    // Wrap in a Div with Quarto cross-reference attributes
    let id = config
        .table_id
        .as_ref()
        .map(|id| format!("tbl-{id}"))
        .unwrap_or_default();

    let mut classes = vec!["quarto-table"];
    let mut kv_pairs: Vec<Value> = config
        .extra_attrs
        .iter()
        .map(|(k, v)| json!([k, v]))
        .collect();

    // Add tbl-cap attribute from title if available
    if let Some(ref header) = table.header {
        if let Some(ref title) = header.title {
            let caption_text = content_to_plain_text(&title.content);
            if !caption_text.is_empty() {
                kv_pairs.push(json!(["tbl-cap", caption_text]));
            }
        }
        if let Some(ref subtitle) = header.subtitle {
            let subcaption_text = content_to_plain_text(&subtitle.content);
            if !subcaption_text.is_empty() {
                kv_pairs.push(json!(["tbl-subcap", subcaption_text]));
            }
        }
    }

    // Add cell-level class for Quarto table processing
    classes.push("cell-output-display");

    let attr = json!([
        id,
        classes,
        kv_pairs
    ]);

    // Quarto structure: Div(attr, [Table, ...footnote blocks])
    let mut blocks = vec![table_block];

    // Footnotes as a separate Para block below the table (Quarto style)
    if let Some(ref footer) = table.footer {
        if !footer.footnotes.is_empty() {
            let mut inlines = Vec::new();
            for (i, note) in footer.footnotes.iter().enumerate() {
                if i > 0 {
                    inlines.push(json!({"t": "LineBreak"}));
                }
                inlines.push(json!({
                    "t": "Superscript",
                    "c": [{"t": "Str", "c": note.mark}]
                }));
                inlines.push(json!({"t": "Space"}));
                inlines.extend(content_to_inlines(&note.content));
            }
            blocks.push(json!({
                "t": "Para",
                "c": inlines
            }));
        }

        if !footer.source_notes.is_empty() {
            let mut inlines = Vec::new();
            for (i, note) in footer.source_notes.iter().enumerate() {
                if i > 0 {
                    inlines.push(json!({"t": "LineBreak"}));
                }
                inlines.extend(content_to_inlines(&note.content));
            }
            blocks.push(json!({
                "t": "Para",
                "c": inlines
            }));
        }
    }

    json!({
        "t": "Div",
        "c": [attr, blocks]
    })
}

fn render_table_block(table: &Table) -> Value {
    let attr = null_attr();
    let caption = render_caption(table);
    let colspecs = render_colspecs(table);
    let thead = render_thead(table);
    let tbodies = render_tbodies(table);
    let tfoot = render_tfoot();

    json!({
        "t": "Table",
        "c": [attr, caption, colspecs, thead, tbodies, tfoot]
    })
}

fn null_attr() -> Value {
    json!(["", [], []])
}

fn render_caption(table: &Table) -> Value {
    // Caption = (Maybe [Inline], [[Block]])
    // In Quarto mode, the caption is in the Div attributes (tbl-cap),
    // but we still include it in the Table for compatibility
    let short: Value = Value::Null;
    let mut blocks = Vec::new();

    if let Some(ref header) = table.header {
        if let Some(ref title) = header.title {
            blocks.push(json!({
                "t": "Para",
                "c": content_to_inlines(&title.content)
            }));
        }
        if let Some(ref subtitle) = header.subtitle {
            blocks.push(json!({
                "t": "Para",
                "c": content_to_inlines(&subtitle.content)
            }));
        }
    }

    json!([short, blocks])
}

fn render_colspecs(table: &Table) -> Value {
    let specs: Vec<Value> = table
        .column_spec
        .iter()
        .map(|col| {
            let align = match col.align.as_str() {
                "left" => json!({"t": "AlignLeft"}),
                "right" => json!({"t": "AlignRight"}),
                "center" => json!({"t": "AlignCenter"}),
                _ => json!({"t": "AlignDefault"}),
            };
            let col_width = if col.width == "auto" {
                json!({"t": "ColWidthDefault"})
            } else if let Some(px) = col.width.strip_suffix("px") {
                let frac = px.parse::<f64>().unwrap_or(100.0) / 800.0;
                json!({"t": "ColWidth", "c": frac})
            } else {
                json!({"t": "ColWidthDefault"})
            };
            json!([align, col_width])
        })
        .collect();
    Value::Array(specs)
}

fn render_thead(table: &Table) -> Value {
    let rows: Vec<Value> = if table.config.column_labels_hidden {
        vec![]
    } else {
        table.table.thead.rows.iter().map(render_row).collect()
    };
    json!([null_attr(), rows])
}

fn render_tbodies(table: &Table) -> Value {
    let bodies: Vec<Value> = table
        .table
        .tbody
        .iter()
        .map(|group| {
            let head_rows: Vec<Value> = Vec::new();
            let body_rows: Vec<Value> = group.rows.iter().map(render_row).collect();
            json!([null_attr(), 0, head_rows, body_rows])
        })
        .collect();
    Value::Array(bodies)
}

fn render_tfoot() -> Value {
    // In Quarto mode, footnotes go outside the table as Para blocks
    // so the table footer is always empty
    json!([null_attr(), []])
}

fn render_row(row: &Row) -> Value {
    let cells: Vec<Value> = row
        .cells
        .iter()
        .filter(|c| !c.is_placeholder)
        .map(render_cell)
        .collect();
    json!([null_attr(), cells])
}

fn render_cell(cell: &gridwell_ir::Cell) -> Value {
    let align = json!({"t": "AlignDefault"});
    let blocks = if cell.content.is_empty() {
        vec![]
    } else {
        vec![json!({
            "t": "Plain",
            "c": content_to_inlines(&cell.content)
        })]
    };
    json!([
        null_attr(),
        align,
        cell.rowspan,
        cell.colspan,
        blocks
    ])
}

fn content_to_inlines(nodes: &[ContentNode]) -> Vec<Value> {
    let mut inlines = Vec::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => {
                for (i, word) in value.split(' ').enumerate() {
                    if i > 0 {
                        inlines.push(json!({"t": "Space"}));
                    }
                    if !word.is_empty() {
                        inlines.push(json!({"t": "Str", "c": word}));
                    }
                }
            }
            ContentNode::StyledText { value, style_id } => {
                let inner: Vec<Value> = value
                    .split(' ')
                    .enumerate()
                    .flat_map(|(i, word)| {
                        let mut v = Vec::new();
                        if i > 0 {
                            v.push(json!({"t": "Space"}));
                        }
                        if !word.is_empty() {
                            v.push(json!({"t": "Str", "c": word}));
                        }
                        v
                    })
                    .collect();
                if style_id.as_deref().is_some_and(|s| s.contains("italic")) {
                    inlines.push(json!({"t": "Emph", "c": inner}));
                } else {
                    inlines.extend(inner);
                }
            }
            ContentNode::LineBreak {} => {
                inlines.push(json!({"t": "LineBreak"}));
            }
            ContentNode::FootnoteMark { mark_text, .. } => {
                inlines.push(json!({
                    "t": "Superscript",
                    "c": [{"t": "Str", "c": mark_text}]
                }));
            }
            ContentNode::Image { src, alt, .. } => {
                let alt_inlines = if let Some(alt_text) = alt {
                    vec![json!({"t": "Str", "c": alt_text})]
                } else {
                    vec![]
                };
                inlines.push(json!({
                    "t": "Image",
                    "c": [null_attr(), alt_inlines, [src, ""]]
                }));
            }
            ContentNode::Raw { value, .. } => {
                inlines.push(json!({"t": "RawInline", "c": ["", value]}));
            }
            ContentNode::Unknown => {}
        }
    }
    inlines
}

fn content_to_plain_text(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } | ContentNode::StyledText { value, .. } => {
                out.push_str(value);
            }
            ContentNode::LineBreak {} => out.push(' '),
            ContentNode::FootnoteMark { mark_text, .. } => out.push_str(mark_text),
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
