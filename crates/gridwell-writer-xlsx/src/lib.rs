mod render;
mod xml;

use gridwell_ir::Table;

pub use render::RenderError;

/// Excel (.xlsx) writer: converts a gridwell IR Table into an .xlsx byte vector.
pub struct XlsxWriter;

impl XlsxWriter {
    pub fn new() -> Self {
        Self
    }

    /// Render the table to an .xlsx file as bytes.
    pub fn render(&self, table: &Table) -> Result<Vec<u8>, RenderError> {
        render::render(table)
    }

    /// Render only the sheet XML content (useful for testing/snapshots).
    pub fn render_sheet_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_sheet_xml(table)
    }
}

impl Default for XlsxWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_xlsx(table: &Table) -> Result<Vec<u8>, RenderError> {
    XlsxWriter::new().render(table)
}
