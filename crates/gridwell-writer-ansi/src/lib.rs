mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for ANSI terminal rendering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnsiConfig {
    /// Use box-drawing characters for borders.
    pub box_drawing: bool,
    /// Use 24-bit (true color) ANSI escapes.
    pub true_color: bool,
    /// Maximum total table width in columns (0 = no limit).
    pub max_width: usize,
    /// Paint cell backgrounds (fills, striping) with 24-bit escapes. Off by
    /// default: fills are designed for light pages and show as bright bars on dark
    /// terminals. Needs `true_color`.
    pub background_colors: bool,
}

impl Default for AnsiConfig {
    fn default() -> Self {
        Self {
            box_drawing: true,
            true_color: true,
            max_width: 0,
            background_colors: false,
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
