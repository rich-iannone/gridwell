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

