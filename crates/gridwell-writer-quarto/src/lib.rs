mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for Quarto AST output.
#[derive(Debug, Clone, Default)]
pub struct QuartoConfig {
    /// Table identifier for cross-referencing (used as `tbl-<id>`).
    /// If None, no cross-reference wrapper is emitted.
    pub table_id: Option<String>,
    /// Additional Quarto attributes to add to the wrapper div.
    pub extra_attrs: Vec<(String, String)>,
}

/// Quarto AST writer: converts a gridwell IR Table to Quarto-flavored Pandoc JSON AST.
pub struct QuartoWriter {
    pub config: QuartoConfig,
}

impl QuartoWriter {
    pub fn new() -> Self {
        Self {
            config: QuartoConfig::default(),
        }
    }

    pub fn with_config(config: QuartoConfig) -> Self {
        Self { config }
    }

    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for QuartoWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_quarto(table: &Table) -> Result<String, RenderError> {
    QuartoWriter::new().render(table)
}
