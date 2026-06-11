use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{Border, BorderSet, Padding, StyleDef};
use gridwell_ir::{Cell, Row, Table};
use std::fmt::Write;
use thiserror::Error;

use crate::HtmlWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Internal writer state.
struct HtmlRenderer<'a> {
    table: &'a Table,
    config: &'a HtmlWriterConfig,
    buf: String,
    indent_level: usize,
}

