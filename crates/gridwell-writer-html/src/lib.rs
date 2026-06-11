mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for HTML output.
#[derive(Debug, Clone)]
pub struct HtmlWriterConfig {
    /// Use inline styles instead of a `<style>` block.
    pub inline_styles: bool,
    /// Pretty-print with indentation.
    pub pretty_print: bool,
    /// CSS class prefix for generated names.
    pub class_prefix: String,
}

impl Default for HtmlWriterConfig {
    fn default() -> Self {
        Self {
            inline_styles: false,
            pretty_print: true,
            class_prefix: "gw".to_string(),
        }
    }
}

/// HTML writer: converts a gridwell IR Table to semantic HTML5.
pub struct HtmlWriter {
    pub config: HtmlWriterConfig,
}

impl HtmlWriter {
    pub fn new() -> Self {
        Self {
            config: HtmlWriterConfig::default(),
        }
    }

    pub fn with_config(config: HtmlWriterConfig) -> Self {
        Self { config }
    }

    /// Render the table IR to an HTML string.
    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for HtmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to render a table to HTML with default settings.
pub fn render_html(table: &Table) -> Result<String, RenderError> {
    HtmlWriter::new().render(table)
}
