mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for LaTeX output.
#[derive(Debug, Clone)]
pub struct LatexWriterConfig {
    /// Use longtable environment for multi-page tables.
    pub longtable: bool,
    /// Row count threshold to auto-switch to longtable.
    pub longtable_threshold: Option<u32>,
    /// Include booktabs package rules (\toprule, \midrule, \bottomrule).
    pub booktabs: bool,
}

impl Default for LatexWriterConfig {
    fn default() -> Self {
        Self {
            longtable: false,
            longtable_threshold: Some(30),
            booktabs: true,
        }
    }
}

/// LaTeX writer: converts a gridwell IR Table to LaTeX.
pub struct LatexWriter {
    pub config: LatexWriterConfig,
}

impl LatexWriter {
    pub fn new() -> Self {
        Self {
            config: LatexWriterConfig::default(),
        }
    }

    pub fn with_config(config: LatexWriterConfig) -> Self {
        Self { config }
    }

    /// Render the table IR to a LaTeX string.
    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for LatexWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to render a table to LaTeX with default settings.
pub fn render_latex(table: &Table) -> Result<String, RenderError> {
    LatexWriter::new().render(table)
}
