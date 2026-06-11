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

