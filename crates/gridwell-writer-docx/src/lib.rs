mod render;
mod xml;

use gridwell_ir::Table;

pub use render::RenderError;

/// Word (.docx) writer: converts a gridwell IR Table into a .docx byte vector.
pub struct DocxWriter;

impl DocxWriter {
    pub fn new() -> Self {
        Self
    }

    /// Render the table to a .docx file as bytes.
    pub fn render(&self, table: &Table) -> Result<Vec<u8>, RenderError> {
        render::render(table)
    }

    /// Render only the `word/document.xml` content (useful for testing/snapshots).
    pub fn render_document_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_document_xml(table)
    }
}

impl Default for DocxWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_docx(table: &Table) -> Result<Vec<u8>, RenderError> {
    DocxWriter::new().render(table)
}
