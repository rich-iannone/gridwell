mod render;

/// AST building blocks, shared with the Quarto writer.
pub mod ast {
    pub use crate::render::{
        footnote_lines, inlines, note_lines, null_attr, plain_text, source_note_lines, styled,
        table_block,
    };
}

use gridwell_ir::Table;

pub use render::RenderError;

/// Pandoc AST writer: converts a gridwell IR Table to Pandoc JSON AST.
pub struct PandocWriter;

impl PandocWriter {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table)
    }
}

impl Default for PandocWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_pandoc(table: &Table) -> Result<String, RenderError> {
    PandocWriter::new().render(table)
}
