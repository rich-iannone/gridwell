use serde::{Deserialize, Serialize};

use crate::content::ContentNode;

/// A table cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub content: Vec<ContentNode>,
    #[serde(default)]
    pub typed_value: Option<TypedValue>,
    #[serde(default = "default_one")]
    pub colspan: u32,
    #[serde(default = "default_one")]
    pub rowspan: u32,
    #[serde(default)]
    pub style_id: Option<String>,
    #[serde(default)]
    pub is_stub: bool,
    #[serde(default)]
    pub is_placeholder: bool,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub sort_key: Option<serde_json::Value>,
    #[serde(default)]
    pub data_type: Option<String>,
}

fn default_one() -> u32 {
    1
}

/// A typed value (raw data alongside display text).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedValue {
    #[serde(rename = "type")]
    pub value_type: String,
    pub value: serde_json::Value,
}

/// A table row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub style_id: Option<String>,
    pub cells: Vec<Cell>,
}

