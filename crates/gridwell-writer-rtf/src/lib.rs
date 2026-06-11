mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// RTF writer: converts a gridwell IR Table to RTF.
pub struct RtfWriter;

impl RtfWriter {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table)
    }
}

impl Default for RtfWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_rtf(table: &Table) -> Result<String, RenderError> {
    RtfWriter::new().render(table)
}
