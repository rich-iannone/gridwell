use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Renders a gridwell IR Table to a Pandoc JSON AST Table block.
///
/// Produces a single-element JSON array containing the Table block
/// in Pandoc's native AST format (pandoc-types 1.23+).
pub fn render(table: &Table) -> Result<String, RenderError> {
    let block = render_table_block(table);
    let output = serde_json::to_string_pretty(&block)?;
    Ok(output)
}

fn render_table_block(table: &Table) -> Value {
    // Pandoc Table: (Attr, Caption, [ColSpec], TableHead, [TableBody], TableFoot)
    let attr = null_attr();
    let caption = render_caption(table);
    let colspecs = render_colspecs(table);
    let thead = render_thead(table);
    let tbodies = render_tbodies(table);
    let tfoot = render_tfoot(table);

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
                // Normalize to fraction (rough: assume 800px total)
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
    // TableHead = (Attr, [Row])
    let rows: Vec<Value> = if table.config.column_labels_hidden {
        vec![]
    } else {
        table
            .table
            .thead
            .rows
            .iter()
            .map(render_row)
            .collect()
    };
    json!([null_attr(), rows])
}

fn render_tbodies(table: &Table) -> Value {
    let bodies: Vec<Value> = table
        .table
        .tbody
        .iter()
        .map(|group| {
            // TableBody = (Attr, RowHeadColumns, [Row], [Row])
            // RowHeadColumns = number of row header columns
            let head_rows: Vec<Value> = Vec::new();
            let body_rows: Vec<Value> = group.rows.iter().map(render_row).collect();
            json!([null_attr(), 0, head_rows, body_rows])
        })
        .collect();
    Value::Array(bodies)
}

fn render_tfoot(table: &Table) -> Value {
    // TableFoot = (Attr, [Row])
    // We put footnotes as a paragraph in a single-cell row
    let mut rows = Vec::new();

    if let Some(ref footer) = table.footer {
        if !footer.footnotes.is_empty() || !footer.source_notes.is_empty() {
            let mut inlines = Vec::new();

            for note in &footer.footnotes {
                if !inlines.is_empty() {
                    inlines.push(json!({"t": "LineBreak"}));
                }
                // Mark as superscript
                inlines.push(json!({
                    "t": "Superscript",
                    "c": [{"t": "Str", "c": note.mark}]
                }));
                inlines.push(json!({"t": "Space"}));
                inlines.extend(content_to_inlines(&note.content));
            }

            for note in &footer.source_notes {
                if !inlines.is_empty() {
                    inlines.push(json!({"t": "LineBreak"}));
                }
                inlines.extend(content_to_inlines(&note.content));
            }

            let num_cols = table.config.table_cols;
            let cell = json!([
                null_attr(),
                {"t": "AlignDefault"},
                1, // rowspan
                num_cols, // colspan
                [{"t": "Para", "c": inlines}]
            ]);
            let row = json!([null_attr(), [cell]]);
            rows.push(row);
        }
    }

    json!([null_attr(), rows])
}

fn render_row(row: &Row) -> Value {
    // Row = (Attr, [Cell])
    let cells: Vec<Value> = row
        .cells
        .iter()
        .filter(|c| !c.is_placeholder)
        .map(render_cell)
        .collect();
    json!([null_attr(), cells])
}

fn render_cell(cell: &gridwell_ir::Cell) -> Value {
    // Cell = (Attr, Alignment, RowSpan, ColSpan, [Block])
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
                // Split on spaces to produce Str/Space tokens
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
                // Wrap in Emph if style_id hints italic (simple heuristic)
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
