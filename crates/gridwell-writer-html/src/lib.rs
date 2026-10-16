mod base;
mod render;

use gridwell_ir::Table;

pub use render::RenderError;

/// Configuration for HTML output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HtmlWriterConfig {
    /// Use inline styles instead of a `<style>` block.
    pub inline_styles: bool,
    /// Pretty-print with indentation.
    pub pretty_print: bool,
    /// CSS class prefix for generated names.
    pub class_prefix: String,
    /// Include the base stylesheet: gridwell's default look (modeled on gt's
    /// fonts, rules and padding). Its rules have zero specificity, so the table's
    /// own styles and the host page's CSS win over it. Off gives a bare fragment
    /// styled only by the table's styles. Not applied with `inline_styles`, which
    /// writes no `<style>` block.
    pub base_styles: bool,
}

impl Default for HtmlWriterConfig {
    fn default() -> Self {
        Self {
            inline_styles: false,
            pretty_print: true,
            class_prefix: "gw".to_string(),
            base_styles: true,
        }
    }
}

/// HTML writer: converts a gridwell IR Table to semantic HTML5.
pub struct HtmlWriter {
    pub config: HtmlWriterConfig,
}

impl HtmlWriter {
    pub fn new() -> Self {
        Self {
            config: HtmlWriterConfig::default(),
        }
    }

    pub fn with_config(config: HtmlWriterConfig) -> Self {
        Self { config }
    }

    /// Render the table IR to an HTML string.
    pub fn render(&self, table: &Table) -> Result<String, RenderError> {
        render::render(table, &self.config)
    }
}

impl Default for HtmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Check a `class_prefix`: it is written into every class name and selector, so it
/// must be a plain CSS identifier (`[A-Za-z_][A-Za-z0-9_-]*`). The error says why.
pub fn check_class_prefix(v: &str) -> Result<(), String> {
    let mut chars = v.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "class_prefix must start with a letter or `_` and contain only letters, digits, `_` and `-`; got {v:?}"
        ))
    }
}

/// Convenience function to render a table to HTML with default settings.
pub fn render_html(table: &Table) -> Result<String, RenderError> {
    HtmlWriter::new().render(table)
}
