mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for SVG rendering.
#[derive(Debug, Clone)]
pub struct SvgConfig {
    /// Default font family.
    pub font_family: String,
    /// Default font size in pixels.
    pub font_size: f64,
    /// Row height in pixels.
    pub row_height: f64,
    /// Cell horizontal padding in pixels.
    pub cell_padding_x: f64,
    /// Cell vertical padding in pixels.
    pub cell_padding_y: f64,
    /// Default column width in pixels.
    pub default_col_width: f64,
}

impl Default for SvgConfig {
    fn default() -> Self {
        Self {
            font_family: "Arial, sans-serif".to_string(),
            font_size: 14.0,
            row_height: 28.0,
            cell_padding_x: 8.0,
            cell_padding_y: 4.0,
            default_col_width: 120.0,
        }
    }
}

/// SVG writer: converts a gridwell IR Table to SVG.
pub struct SvgWriter {
    pub config: SvgConfig,
}

impl SvgWriter {
    pub fn new() -> Self {
        Self {
            config: SvgConfig::default(),
        }
    }

    pub fn with_config(config: SvgConfig) -> Self {
        Self { config }
    }

    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for SvgWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_svg(table: &Table) -> Result<String, RenderError> {
    SvgWriter::new().render(table)
}
