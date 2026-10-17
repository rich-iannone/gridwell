pub mod measure;
pub mod metrics;
mod render;

use gridwell_ir::Table;
use gridwell_layout::look::LOOK;

pub use render::RenderError;

/// The default font list: Roboto, whose widths text is drawn at, then the
/// Arial-compatible fonts cells also leave room for (see [`measure`]), for viewers
/// that ignore `textLength`.
pub const FONT_FAMILY: &str = "Roboto, Helvetica, Arial, 'Liberation Sans', Arimo, sans-serif";

/// Configuration for SVG rendering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SvgConfig {
    /// Default font family.
    pub font_family: String,
    /// Default font size in pixels.
    pub font_size: f64,
    /// Minimum row height in pixels (0: rows are as tall as their content).
    pub row_height: f64,
    /// Horizontal padding of body cells in pixels (column labels and spanners use the
    /// default look's own).
    pub cell_padding_x: f64,
    /// Vertical padding of body cells in pixels.
    pub cell_padding_y: f64,
    /// Width in px: heading and note lines wrap at the larger of the table's width
    /// and three times this.
    pub default_col_width: f64,
    /// Draw gridwell's default look ([`gridwell_layout::look`], as in the HTML base
    /// stylesheet): the rules, the text color. Off: no rules, black text, the same
    /// layout.
    pub base_styles: bool,
}

impl Default for SvgConfig {
    fn default() -> Self {
        Self {
            font_family: FONT_FAMILY.to_string(),
            font_size: LOOK.font_size_px,
            row_height: 0.0,
            cell_padding_x: LOOK.cell_pad.left,
            cell_padding_y: LOOK.cell_pad.top,
            default_col_width: 120.0,
            base_styles: true,
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
