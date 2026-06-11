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

