mod limits;
mod render;
mod xml;

use gridwell_ir::Table;

pub use limits::unsupported;
pub use render::RenderError;

/// Configuration for PPTX rendering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PptxConfig {
    /// Draw the rules of gridwell's default look ([`gridwell_layout::look`], as in
    /// the HTML base stylesheet); the table's own borders still win. Off: only the
    /// table's own borders.
    pub base_styles: bool,
}

impl Default for PptxConfig {
    fn default() -> Self {
        Self { base_styles: true }
    }
}

/// PowerPoint (.pptx) writer: converts a gridwell IR Table into a .pptx byte vector.
pub struct PptxWriter {
    pub config: PptxConfig,
}

impl PptxWriter {
    pub fn new() -> Self {
        Self {
            config: PptxConfig::default(),
        }
    }

    pub fn with_config(config: PptxConfig) -> Self {
        Self { config }
    }

    /// Render the table to a .pptx file as bytes.
    pub fn render(&self, table: &Table) -> Result<Vec<u8>, RenderError> {
        render::render(table, &self.config)
    }

    /// Render only the slide XML content (useful for testing/snapshots).
    pub fn render_slide_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_slide_xml(table, &self.config)
    }
}

impl Default for PptxWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_pptx(table: &Table) -> Result<Vec<u8>, RenderError> {
    PptxWriter::new().render(table)
}
