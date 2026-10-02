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
    /// The table exceeds a configured size limit (see [`Limits`]). When this fires,
    /// the remaining checks are skipped: they could be arbitrarily expensive.
    LimitExceeded,
}

impl ValidationRule {
    /// The rule's documented identifier, e.g. `SPAN_OVERFLOW_RIGHT` (the same string
    /// it serializes to).
    pub fn id(self) -> &'static str {
        match self {
            ValidationRule::ColCount => "COL_COUNT",
            ValidationRule::RowCount => "ROW_COUNT",
            ValidationRule::ColspecLength => "COLSPEC_LENGTH",
            ValidationRule::StubContiguous => "STUB_CONTIGUOUS",
            ValidationRule::StyleRefsValid => "STYLE_REFS_VALID",
            ValidationRule::FootnoteRefsValid => "FOOTNOTE_REFS_VALID",
            ValidationRule::SpanOverflowRight => "SPAN_OVERFLOW_RIGHT",
            ValidationRule::SpanOverflowBottom => "SPAN_OVERFLOW_BOTTOM",
            ValidationRule::SpanOverlap => "SPAN_OVERLAP",
            ValidationRule::SpanGap => "SPAN_GAP",
            ValidationRule::SpanPlaceholderHasContent => "SPAN_PLACEHOLDER_HAS_CONTENT",
            ValidationRule::SpanPlaceholderMismatch => "SPAN_PLACEHOLDER_MISMATCH",
            ValidationRule::SpanZeroValue => "SPAN_ZERO_VALUE",
            ValidationRule::SummaryRequiresStub => "SUMMARY_REQUIRES_STUB",
            ValidationRule::LimitExceeded => "LIMIT_EXCEEDED",
        }
    }
}

impl fmt::Display for ValidationRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// Returned by [`Table::ensure_valid`](crate::Table::ensure_valid) when the IR fails
/// validation. Render entry points (CLI, FFI, Python, R) refuse such tables rather
/// than handing malformed IR to a writer.
#[derive(Debug, Clone)]
pub struct InvalidTable {
    pub errors: Vec<ValidationError>,
}

impl InvalidTable {
    /// How many errors `Display` lists before summarizing the rest.
    pub const DISPLAY_LIMIT: usize = 10;
}

impl fmt::Display for InvalidTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.errors.len();
        write!(
            f,
            "table IR failed validation with {n} error{}",
            if n == 1 { "" } else { "s" }
        )?;
        for e in self.errors.iter().take(Self::DISPLAY_LIMIT) {
            write!(f, "\n  - {e}")?;
        }
        if n > Self::DISPLAY_LIMIT {
            write!(f, "\n  … and {} more", n - Self::DISPLAY_LIMIT)?;
        }
        Ok(())
    }
}

impl std::error::Error for InvalidTable {}

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

/// Size limits enforced before any other validation work.
///
/// IR may come from untrusted producers (including across the FFI boundary), and
/// declared dimensions such as `config.table_cols` drive allocation in the validator
/// and in writers. These limits bound that work. The defaults are generous for real
/// tables; `max_table_cols` and `max_rows_per_section` match Excel's sheet limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum `config.table_cols`.
    pub max_table_cols: u32,
    /// Maximum rows in any one section (thead, a group's data rows, or a group's
    /// summary rows).
    pub max_rows_per_section: u32,
    /// Maximum total number of cell objects across the whole table.
    pub max_total_cells: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_table_cols: 16_384,
            max_rows_per_section: 1_048_576,
            max_total_cells: 50_000_000,
        }
    }
}

/// Validate a table IR with the default [`Limits`], returning all errors found.
pub fn validate(table: &Table) -> Vec<ValidationError> {
    validate_with_limits(table, &Limits::default())
}

/// Validate a table IR with explicit [`Limits`], returning all errors found.
///
/// If any limit is exceeded, only the `LIMIT_EXCEEDED` errors are returned.
pub fn validate_with_limits(table: &Table, limits: &Limits) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    validate_limits(table, limits, &mut errors);
    if !errors.is_empty() {
        return errors;
    }

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

/// LIMIT_EXCEEDED: declared and actual dimensions within [`Limits`].
fn validate_limits(table: &Table, limits: &Limits, errors: &mut Vec<ValidationError>) {
    let limit_error = |section: &str, row_group: Option<u32>, message: String| ValidationError {
        rule: ValidationRule::LimitExceeded,
        section: section.to_string(),
        row_group,
        row: None,
        col: None,
        message,
    };

    if table.config.table_cols > limits.max_table_cols {
        errors.push(limit_error(
            "config",
            None,
            format!(
                "config.table_cols is {} but the limit is {}",
                table.config.table_cols, limits.max_table_cols
            ),
        ));
    }

    let mut total_cells: u64 = 0;
    let mut check_section = |rows: &[crate::cell::Row], section: &str, row_group: Option<u32>| {
        if rows.len() as u64 > limits.max_rows_per_section as u64 {
            errors.push(limit_error(
                section,
                row_group,
                format!(
                    "{section} has {} rows but the per-section limit is {}",
                    rows.len(),
                    limits.max_rows_per_section
                ),
            ));
        }
        total_cells += rows.iter().map(|r| r.cells.len() as u64).sum::<u64>();
    };

    check_section(&table.table.thead.rows, "thead", None);
    for (g, group) in table.table.tbody.iter().enumerate() {
        check_section(&group.rows, "tbody", Some(g as u32));
        check_section(&group.summary_rows, "tbody_summary", Some(g as u32));
    }

    if total_cells > limits.max_total_cells {
        errors.push(limit_error(
            "table",
            None,
            format!(
                "table has {total_cells} cells but the limit is {}",
                limits.max_total_cells
            ),
        ));
    }
}

/// COLSPEC_LENGTH: column_spec array length == config.table_cols
fn validate_colspec_length(table: &Table, errors: &mut Vec<ValidationError>) {
    if table.column_spec.len() as u32 != table.config.table_cols {
        errors.push(ValidationError {
            rule: ValidationRule::ColspecLength,
            section: "column_spec".to_string(),
            row_group: None,
            row: None,
            col: None,
            message: format!(
                "column_spec has {} entries but config.table_cols is {}",
                table.column_spec.len(),
                table.config.table_cols
            ),
        });
    }
}

/// ROW_COUNT: thead row count == config.header_rows
fn validate_header_row_count(table: &Table, errors: &mut Vec<ValidationError>) {
    let actual = table.table.thead.rows.len() as u32;
    if actual != table.config.header_rows {
        errors.push(ValidationError {
            rule: ValidationRule::RowCount,
            section: "thead".to_string(),
            row_group: None,
            row: None,
            col: None,
            message: format!(
                "thead has {} rows but config.header_rows is {}",
                actual, table.config.header_rows
            ),
        });
    }
}

/// ROW_COUNT: total body data rows == config.body_rows (summary rows not counted)
fn validate_body_row_count(table: &Table, errors: &mut Vec<ValidationError>) {
    let actual: u32 = table.table.tbody.iter().map(|g| g.rows.len() as u32).sum();
    if actual != table.config.body_rows {
        errors.push(ValidationError {
            rule: ValidationRule::RowCount,
            section: "tbody".to_string(),
            row_group: None,
            row: None,
            col: None,
            message: format!(
                "tbody has {} data rows but config.body_rows is {}",
                actual, table.config.body_rows
            ),
        });
    }
}

/// COL_COUNT: every row has exactly `table_cols` cell objects
/// (placeholders fill positions covered by colspans/rowspans from other cells)
fn validate_col_counts(table: &Table, errors: &mut Vec<ValidationError>) {
    let expected = table.config.table_cols;

    // Check thead rows
    for (r, row) in table.table.thead.rows.iter().enumerate() {
        let count = row.cells.len() as u32;
        if count != expected {
            errors.push(ValidationError {
                rule: ValidationRule::ColCount,
                section: "thead".to_string(),
                row_group: None,
                row: Some(r as u32),
                col: None,
                message: format!(
                    "thead row {r} has {count} cells but config.table_cols is {expected}"
                ),
            });
        }
    }

    // Check tbody rows
    for (g, group) in table.table.tbody.iter().enumerate() {
        for (r, row) in group.rows.iter().enumerate() {
            let count = row.cells.len() as u32;
            if count != expected {
                errors.push(ValidationError {
                    rule: ValidationRule::ColCount,
                    section: "tbody".to_string(),
                    row_group: Some(g as u32),
                    row: Some(r as u32),
                    col: None,
                    message: format!(
                        "tbody group {g} row {r} has {count} cells but config.table_cols is {expected}"
                    ),
                });
            }
        }
        for (r, row) in group.summary_rows.iter().enumerate() {
            let count = row.cells.len() as u32;
            if count != expected {
                errors.push(ValidationError {
                    rule: ValidationRule::ColCount,
                    section: "tbody_summary".to_string(),
                    row_group: Some(g as u32),
                    row: Some(r as u32),
                    col: None,
                    message: format!(
                        "tbody group {g} summary row {r} has {count} cells but config.table_cols is {expected}"
                    ),
                });
            }
        }
    }
}

/// STYLE_REFS_VALID: all style_id values reference a defined style or composition
fn validate_style_refs(table: &Table, errors: &mut Vec<ValidationError>) {
    let valid_ids: std::collections::HashSet<&str> = table
        .styles
        .defs
        .keys()
        .chain(table.styles.compositions.keys())
        .map(|s| s.as_str())
        .collect();

    let mut check_style_ref = |style_id: &Option<String>,
                               section: &str,
                               row_group: Option<u32>,
                               row: Option<u32>,
                               col: Option<u32>| {
        if let Some(ref id) = style_id {
            if !valid_ids.contains(id.as_str()) {
                errors.push(ValidationError {
                    rule: ValidationRule::StyleRefsValid,
                    section: section.to_string(),
                    row_group,
                    row,
                    col,
                    message: format!(
                        "style_id \"{id}\" is not defined in styles.defs or styles.compositions"
                    ),
                });
            }
        }
    };

    // Check header styles
    if let Some(ref header) = table.header {
        if let Some(ref title) = header.title {
            check_style_ref(&title.style_id, "header.title", None, None, None);
        }
        if let Some(ref subtitle) = header.subtitle {
            check_style_ref(&subtitle.style_id, "header.subtitle", None, None, None);
        }
    }

    // Check thead cells
    for (r, row) in table.table.thead.rows.iter().enumerate() {
        check_style_ref(&row.style_id, "thead", None, Some(r as u32), None);
        for (c, cell) in row.cells.iter().enumerate() {
            check_style_ref(
                &cell.style_id,
                "thead",
                None,
                Some(r as u32),
                Some(c as u32),
            );
        }
    }

    // Check tbody cells
    for (g, group) in table.table.tbody.iter().enumerate() {
        if let Some(ref label) = group.label {
            check_style_ref(&label.style_id, "tbody_label", Some(g as u32), None, None);
        }
        for (r, row) in group.rows.iter().enumerate() {
            check_style_ref(&row.style_id, "tbody", Some(g as u32), Some(r as u32), None);
            for (c, cell) in row.cells.iter().enumerate() {
                check_style_ref(
                    &cell.style_id,
                    "tbody",
                    Some(g as u32),
                    Some(r as u32),
                    Some(c as u32),
                );
            }
        }
        for (r, row) in group.summary_rows.iter().enumerate() {
            check_style_ref(
                &row.style_id,
                "tbody_summary",
                Some(g as u32),
                Some(r as u32),
                None,
            );
            for (c, cell) in row.cells.iter().enumerate() {
                check_style_ref(
                    &cell.style_id,
                    "tbody_summary",
                    Some(g as u32),
                    Some(r as u32),
                    Some(c as u32),
                );
            }
        }
    }
}

/// FOOTNOTE_REFS_VALID: all footnote_mark refs have a matching footer.footnotes[].id
fn validate_footnote_refs(table: &Table, errors: &mut Vec<ValidationError>) {
    let footnote_ids: std::collections::HashSet<&str> = table
        .footer
        .as_ref()
        .map(|f| f.footnotes.iter().map(|fn_| fn_.id.as_str()).collect())
        .unwrap_or_default();

    let check_content = |content: &[ContentNode],
                         section: &str,
                         row_group: Option<u32>,
                         row: Option<u32>,
                         col: Option<u32>,
                         errors: &mut Vec<ValidationError>| {
        for node in content {
            if let ContentNode::FootnoteMark { reference, .. } = node {
                if !footnote_ids.contains(reference.as_str()) {
                    errors.push(ValidationError {
                        rule: ValidationRule::FootnoteRefsValid,
                        section: section.to_string(),
                        row_group,
                        row,
                        col,
                        message: format!(
                            "footnote_mark references \"{reference}\" but no footnote with that id exists"
                        ),
                    });
                }
            }
        }
    };

    // Check thead cells
    for (r, row) in table.table.thead.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            check_content(
                &cell.content,
                "thead",
                None,
                Some(r as u32),
                Some(c as u32),
                errors,
            );
        }
    }

    // Check tbody cells
    for (g, group) in table.table.tbody.iter().enumerate() {
        for (r, row) in group.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                check_content(
                    &cell.content,
                    "tbody",
                    Some(g as u32),
                    Some(r as u32),
                    Some(c as u32),
                    errors,
                );
            }
        }
    }
}

/// SUMMARY_REQUIRES_STUB: rows with summary role require stub_cols >= 1
fn validate_summary_requires_stub(table: &Table, errors: &mut Vec<ValidationError>) {
    if table.config.stub_cols > 0 {
        return;
    }

    for (g, group) in table.table.tbody.iter().enumerate() {
        if !group.summary_rows.is_empty() {
            errors.push(ValidationError {
                rule: ValidationRule::SummaryRequiresStub,
                section: "tbody".to_string(),
                row_group: Some(g as u32),
                row: None,
                col: None,
                message: format!("Row group {g} has summary rows but config.stub_cols is 0"),
            });
        }
    }
}

/// Validate spans using grid materialization.
///
/// Each section (thead, each group's data rows, each group's summary rows) gets its own
/// grid. A section is only materialized when every row has exactly `table_cols` cells:
/// otherwise COL_COUNT has already reported it, span errors would be noise, and — more
/// importantly — the grid size stays bounded by the number of cells actually present in
/// the input rather than by the declared `table_cols`.
fn validate_spans(table: &Table, errors: &mut Vec<ValidationError>) {
    let table_cols = table.config.table_cols;

    let mut check_section = |rows: &[crate::cell::Row], section: &str, row_group: Option<u32>| {
        if rows.is_empty()
            || rows
                .iter()
                .any(|r| r.cells.len() as u64 != table_cols as u64)
        {
            return;
        }
        let (_grid, span_errors) = OccupancyGrid::materialize(rows, table_cols, section, row_group);
        errors.extend(span_errors);
    };

    check_section(&table.table.thead.rows, "thead", None);
    for (g, group) in table.table.tbody.iter().enumerate() {
        check_section(&group.rows, "tbody", Some(g as u32));
        check_section(&group.summary_rows, "tbody_summary", Some(g as u32));
    }
}

/// SPAN_PLACEHOLDER_HAS_CONTENT: placeholder cells must have empty content
fn validate_placeholder_content(table: &Table, errors: &mut Vec<ValidationError>) {
    // Check thead
    for (r, row) in table.table.thead.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.is_placeholder && !cell.content.is_empty() {
                errors.push(ValidationError {
                    rule: ValidationRule::SpanPlaceholderHasContent,
                    section: "thead".to_string(),
                    row_group: None,
                    row: Some(r as u32),
                    col: Some(c as u32),
                    message: format!(
                        "Placeholder cell at thead (row={r}, col={c}) has non-empty content"
                    ),
                });
            }
        }
    }

    // Check tbody
    for (g, group) in table.table.tbody.iter().enumerate() {
        for (r, row) in group.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.is_placeholder && !cell.content.is_empty() {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanPlaceholderHasContent,
                        section: "tbody".to_string(),
                        row_group: Some(g as u32),
                        row: Some(r as u32),
                        col: Some(c as u32),
                        message: format!(
                            "Placeholder cell at tbody group {g} (row={r}, col={c}) has non-empty content"
                        ),
                    });
                }
            }
        }
        for (r, row) in group.summary_rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.is_placeholder && !cell.content.is_empty() {
                    errors.push(ValidationError {
                        rule: ValidationRule::SpanPlaceholderHasContent,
                        section: "tbody_summary".to_string(),
                        row_group: Some(g as u32),
                        row: Some(r as u32),
                        col: Some(c as u32),
                        message: format!(
                            "Placeholder cell at tbody group {g} summary (row={r}, col={c}) has non-empty content"
                        ),
                    });
                }
            }
        }
    }
}
