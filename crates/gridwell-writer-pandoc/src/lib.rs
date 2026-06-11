mod render;

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
