//! Quarto-flavoured Pandoc AST: the Pandoc writer's table inside a `Div` with
//! cross-reference attributes, and the notes as paragraphs after the table.

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_writer_pandoc::ast;
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
    let rt = resolve(table);

    let id = config
        .table_id
        .as_ref()
        .map(|id| format!("tbl-{id}"))
        .unwrap_or_default();
    let mut kv_pairs: Vec<Value> = config
        .extra_attrs
        .iter()
        .map(|(k, v)| json!([k, v]))
        .collect();
    // The caption also stays in the Table itself, for compatibility.
    if let Some(title) = &rt.header.title {
        let text = ast::plain_text(title.content);
        if !text.is_empty() {
            kv_pairs.push(json!(["tbl-cap", text]));
        }
    }
    if let Some(subtitle) = &rt.header.subtitle {
        let text = ast::plain_text(subtitle.content);
        if !text.is_empty() {
            kv_pairs.push(json!(["tbl-subcap", text]));
        }
    }
    let attr = json!([id, ["quarto-table", "cell-output-display"], kv_pairs]);

    // Div(attr, [Table, footnotes Para, source notes Para]).
    let mut blocks = vec![ast::table_block(&rt, false)];
    for lines in [ast::footnote_lines(&rt), ast::source_note_lines(&rt)] {
        if lines.is_empty() {
            continue;
        }
        let mut inlines = Vec::new();
        for line in lines {
            if !inlines.is_empty() {
                inlines.push(json!({"t": "LineBreak"}));
            }
            inlines.extend(line);
        }
        blocks.push(json!({ "t": "Para", "c": inlines }));
    }

    let block = json!({ "t": "Div", "c": [attr, blocks] });
    Ok(serde_json::to_string_pretty(&block)?)
}
