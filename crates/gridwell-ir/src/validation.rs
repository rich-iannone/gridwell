use crate::content::ContentNode;
use crate::span::OccupancyGrid;
use crate::Table;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Validation rule identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationRule {
    ColCount,
    RowCount,
    ColspecLength,
    StubContiguous,
    StyleRefsValid,
    FootnoteRefsValid,
    SpanOverflowRight,
    SpanOverflowBottom,
    SpanOverlap,
    SpanGap,
    SpanPlaceholderHasContent,
    SpanPlaceholderMismatch,
    SpanZeroValue,
    SummaryRequiresStub,
}

impl fmt::Display for ValidationRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A validation error with location context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub rule: ValidationRule,
    pub section: String,
    pub row_group: Option<u32>,
    pub row: Option<u32>,
    pub col: Option<u32>,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.rule, self.message)
    }
}

/// Validate a table IR, returning all errors found.
pub fn validate(table: &Table) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    validate_colspec_length(table, &mut errors);
    validate_header_row_count(table, &mut errors);
    validate_body_row_count(table, &mut errors);
    validate_col_counts(table, &mut errors);
    validate_style_refs(table, &mut errors);
    validate_footnote_refs(table, &mut errors);
    validate_summary_requires_stub(table, &mut errors);
    validate_spans(table, &mut errors);
    validate_placeholder_content(table, &mut errors);

    errors
}

