mod render;
mod xml;

use gridwell_ir::Table;

pub use render::RenderError;

/// PowerPoint (.pptx) writer: converts a gridwell IR Table into a .pptx byte vector.
pub struct PptxWriter;

impl PptxWriter {
    pub fn new() -> Self {
        Self
    }

    /// Render the table to a .pptx file as bytes.
    pub fn render(&self, table: &Table) -> Result<Vec<u8>, RenderError> {
        render::render(table)
    }

    /// Render only the slide XML content (useful for testing/snapshots).
    pub fn render_slide_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_slide_xml(table)
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
