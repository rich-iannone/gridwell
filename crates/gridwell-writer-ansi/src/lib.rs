mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for ANSI terminal rendering.
#[derive(Debug, Clone)]
pub struct AnsiConfig {
    /// Use box-drawing characters for borders.
    pub box_drawing: bool,
    /// Use 24-bit (true color) ANSI escapes.
    pub true_color: bool,
    /// Maximum total table width in columns (0 = no limit).
    pub max_width: usize,
}

impl Default for AnsiConfig {
    fn default() -> Self {
        Self {
            box_drawing: true,
            true_color: true,
            max_width: 0,
        }
    }
}

/// ANSI terminal writer: converts a gridwell IR Table to terminal output.
pub struct AnsiWriter {
    pub config: AnsiConfig,
}

impl AnsiWriter {
    pub fn new() -> Self {
        Self {
            config: AnsiConfig::default(),
        }
    }

    pub fn with_config(config: AnsiConfig) -> Self {
        Self { config }
    }

    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for AnsiWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_ansi(table: &Table) -> Result<String, RenderError> {
    AnsiWriter::new().render(table)
}
