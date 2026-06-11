use serde::{Deserialize, Serialize};

/// Table-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub table_cols: u32,
    #[serde(default)]
    pub header_rows: u32,
    #[serde(default)]
    pub body_rows: u32,
    #[serde(default)]
    pub stub_cols: u32,

    #[serde(default)]
    pub row_striping: bool,
    #[serde(default)]
    pub row_striping_include_stub: bool,
    #[serde(default)]
    pub row_striping_include_body: bool,
    #[serde(default)]
    pub column_labels_hidden: bool,

    #[serde(default)]
    pub table_width: Option<String>,
    #[serde(default)]
    pub container_width: Option<String>,
    #[serde(default)]
    pub container_height: Option<String>,
    #[serde(default)]
    pub container_overflow: Option<String>,

    #[serde(default = "default_locale")]
    pub locale: String,

    #[serde(default = "default_page_break_mode")]
    pub page_break_mode: String,

    #[serde(default)]
    pub aria_label: Option<String>,
    #[serde(default)]
    pub aria_describedby: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

fn default_locale() -> String {
    "en-US".to_string()
}

fn default_page_break_mode() -> String {
    "avoid".to_string()
}
