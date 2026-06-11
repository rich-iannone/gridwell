mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for Typst output.
#[derive(Debug, Clone)]
pub struct TypstWriterConfig {
    /// Whether to repeat the header on each page.
    pub repeat_header: bool,
}

impl Default for TypstWriterConfig {
    fn default() -> Self {
        Self {
            repeat_header: true,
        }
    }
}

/// Typst writer: converts a gridwell IR Table to Typst markup.
pub struct TypstWriter {
    pub config: TypstWriterConfig,
}

impl TypstWriter {
    pub fn new() -> Self {
        Self {
            config: TypstWriterConfig::default(),
        }
    }

    pub fn with_config(config: TypstWriterConfig) -> Self {
        Self { config }
    }

    /// Render the table IR to a Typst string.
    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for TypstWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to render a table to Typst with default settings.
pub fn render_typst(table: &Table) -> Result<String, RenderError> {
    TypstWriter::new().render(table)
}
