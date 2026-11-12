mod date;
mod limits;
mod number;
mod render;

use gridwell_ir::Table;

pub use limits::unsupported;
pub use render::RenderError;

/// Configuration for XLSX rendering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct XlsxConfig {
    /// Draw the rules of gridwell's default look ([`gridwell_layout::look`], as in
    /// the HTML base stylesheet) as cell borders, and hide the sheet's gridlines;
    /// the table's own borders still win. Off: only the table's own borders, over
    /// the gridlines.
    pub base_styles: bool,
}

impl Default for XlsxConfig {
    fn default() -> Self {
        Self { base_styles: true }
    }
}

/// Excel (.xlsx) writer: converts a gridwell IR Table into an .xlsx byte vector.
pub struct XlsxWriter {
    pub config: XlsxConfig,
}

impl XlsxWriter {
    pub fn new() -> Self {
        Self {
            config: XlsxConfig::default(),
        }
    }

    pub fn with_config(config: XlsxConfig) -> Self {
        Self { config }
    }

    /// Render the table to an .xlsx file as bytes.
    pub fn render(&self, table: &Table) -> Result<Vec<u8>, RenderError> {
        render::render(table, &self.config)
    }

    /// Render only the sheet XML content (useful for testing/snapshots).
    pub fn render_sheet_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_sheet_xml(table, &self.config)
    }

    /// The `styles.xml` part: the cell formats the sheet uses.
    pub fn render_styles_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_styles_xml(table, &self.config)
    }

    /// The `sharedStrings.xml` part: the sheet's rich-text strings.
    pub fn render_shared_strings_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_shared_strings_xml(table, &self.config)
    }

    /// The `drawing1.xml` part: the images drawn over cells ("" without any).
    pub fn render_drawing_xml(&self, table: &Table) -> Result<String, RenderError> {
        render::render_drawing_xml(table, &self.config)
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
