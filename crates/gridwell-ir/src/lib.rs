pub mod cell;
pub mod config;
pub mod content;
pub mod span;
pub mod style;
pub mod validation;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use cell::{Cell, Row, RowGroup, TableBody, TableHead};
pub use config::Config;
pub use content::ContentNode;
pub use style::{StyleDef, StylePalette};
pub use validation::{validate, ValidationError, ValidationRule};

/// Parse error for IR JSON.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// The top-level table IR structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub ir_version: String,
    pub config: Config,
    pub styles: StylePalette,
    #[serde(default)]
    pub header: Option<Header>,
    pub column_spec: Vec<ColumnSpec>,
    pub table: TableBlock,
    #[serde(default)]
    pub footer: Option<Footer>,
    #[serde(default)]
    pub extensions: Option<serde_json::Value>,
}

impl Table {
    /// Parse a table from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, ParseError> {
        let table: Table = serde_json::from_str(json)?;
        Ok(table)
    }

    /// Serialize the table to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Validate the table, returning all validation errors found.
    pub fn validate(&self) -> Vec<ValidationError> {
        validate(self)
    }
}

/// The header block (title, subtitle, extra lines).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    #[serde(default)]
    pub title: Option<HeaderLine>,
    #[serde(default)]
    pub subtitle: Option<HeaderLine>,
    #[serde(default)]
    pub extra_lines: Vec<HeaderLine>,
    #[serde(default)]
    pub preheader_content: Option<serde_json::Value>,
}

/// A single header line (title or subtitle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderLine {
    pub content: Vec<ContentNode>,
    #[serde(default)]
    pub style_id: Option<String>,
}

