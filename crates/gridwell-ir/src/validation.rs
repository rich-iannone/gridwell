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
    /// A keyword field (alignment, role, border style, …) holds a value that is not
    /// one of its allowed values. The value is kept verbatim; see [`crate::keywords`].
    UnknownValue,
    /// A colour field holds something that is not a CSS colour (see
    /// [`gridwell_core::color`] for the accepted forms).
    InvalidColor,
    /// A length field does not parse, or uses a form its field does not allow
    /// (e.g. a negative width or `fr` padding).
    InvalidLength,
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
            ValidationRule::UnknownValue => "UNKNOWN_VALUE",
            ValidationRule::InvalidColor => "INVALID_COLOR",
            ValidationRule::InvalidLength => "INVALID_LENGTH",
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
    validate_stub_contiguous(table, &mut errors);
    validate_spans(table, &mut errors);
    validate_placeholder_content(table, &mut errors);
    validate_keywords(table, &mut errors);
    crate::value_checks::validate_values(table, &mut errors);

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

/// FOOTNOTE_REFS_VALID: every footnote_mark ref (in any cell, header line, group
/// label or note) has a matching footer.footnotes[].id
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

    // Header lines
    if let Some(header) = &table.header {
        let lines = header
            .title
            .iter()
            .chain(&header.subtitle)
            .chain(&header.extra_lines);
        for line in lines {
            check_content(&line.content, "header", None, None, None, errors);
        }
    }

    // Body: group labels, data rows, summary rows
    for (g, group) in table.table.tbody.iter().enumerate() {
        let g = Some(g as u32);
        if let Some(label) = &group.label {
            check_content(&label.content, "tbody_label", g, None, None, errors);
        }
        for (section, rows) in [
            ("tbody", &group.rows),
            ("tbody_summary", &group.summary_rows),
        ] {
            for (r, row) in rows.iter().enumerate() {
                for (c, cell) in row.cells.iter().enumerate() {
                    check_content(
                        &cell.content,
                        section,
                        g,
                        Some(r as u32),
                        Some(c as u32),
                        errors,
                    );
                }
            }
        }
    }

    // Footnote and source-note text (a note may refer to another note)
    if let Some(footer) = &table.footer {
        for n in &footer.footnotes {
            check_content(&n.content, "footer", None, None, None, errors);
        }
        for n in &footer.source_notes {
            check_content(&n.content, "footer", None, None, None, errors);
        }
    }
}

/// STUB_CONTIGUOUS: the stub is a block of `config.stub_cols` columns at the left:
/// it fits in the table, and no cell flagged `is_stub` starts to its right.
fn validate_stub_contiguous(table: &Table, errors: &mut Vec<ValidationError>) {
    let stub = table.config.stub_cols;
    if stub > table.config.table_cols {
        errors.push(ValidationError {
            rule: ValidationRule::StubContiguous,
            section: "config".to_string(),
            row_group: None,
            row: None,
            col: None,
            message: format!(
                "config.stub_cols is {stub} but the table has only {} columns",
                table.config.table_cols
            ),
        });
        return;
    }
    let mut check = |rows: &[crate::cell::Row], section: &str, row_group: Option<u32>| {
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.is_stub && !cell.is_placeholder && c as u32 >= stub {
                    errors.push(ValidationError {
                        rule: ValidationRule::StubContiguous,
                        section: section.to_string(),
                        row_group,
                        row: Some(r as u32),
                        col: Some(c as u32),
                        message: format!(
                            "cell is flagged is_stub but column {c} is outside the stub columns (config.stub_cols is {stub})"
                        ),
                    });
                }
            }
        }
    };
    check(&table.table.thead.rows, "thead", None);
    for (g, group) in table.table.tbody.iter().enumerate() {
        check(&group.rows, "tbody", Some(g as u32));
        check(&group.summary_rows, "tbody_summary", Some(g as u32));
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
        // A row with the wrong number of cells is reported once, as COL_COUNT;
        // checking the section's spans too would only add a SPAN_GAP per missing
        // position. So with valid cell counts, an uncovered position is always a
        // placeholder (SPAN_PLACEHOLDER_MISMATCH), and SPAN_GAP comes only from
        // `OccupancyGrid::materialize` called on short rows directly.
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

/// UNKNOWN_VALUE: every keyword field holds a known value.
fn validate_keywords(table: &Table, errors: &mut Vec<ValidationError>) {
    use crate::cell::Row;
    use crate::keywords::Keyword;
    use crate::style::StyleDef;

    fn check<K: Keyword>(
        value: Option<&K>,
        field: &str,
        section: &str,
        row_group: Option<u32>,
        row: Option<u32>,
        col: Option<u32>,
        errors: &mut Vec<ValidationError>,
    ) {
        if let Some(v) = value.filter(|v| !v.is_known()) {
            errors.push(ValidationError {
                rule: ValidationRule::UnknownValue,
                section: section.to_string(),
                row_group,
                row,
                col,
                message: format!(
                    "{field} is \"{}\", which is not one of: {}",
                    v.as_str(),
                    K::ALLOWED.join(", ")
                ),
            });
        }
    }

    fn check_style(def: &StyleDef, at: &str, errors: &mut Vec<ValidationError>) {
        let f = |name: &str| format!("{at}.{name}");
        check(
            def.font_weight.as_ref(),
            &f("font_weight"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.font_style.as_ref(),
            &f("font_style"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.text_align.as_ref(),
            &f("text_align"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.vertical_align.as_ref(),
            &f("vertical_align"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.text_transform.as_ref(),
            &f("text_transform"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.text_decoration.as_ref(),
            &f("text_decoration"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.white_space.as_ref(),
            &f("white_space"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.word_break.as_ref(),
            &f("word_break"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.overflow.as_ref(),
            &f("overflow"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            def.text_overflow.as_ref(),
            &f("text_overflow"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        if let Some(border) = &def.border {
            for (side, b) in [
                ("top", &border.top),
                ("right", &border.right),
                ("bottom", &border.bottom),
                ("left", &border.left),
            ] {
                if let Some(b) = b {
                    check(
                        b.style.as_ref(),
                        &f(&format!("border.{side}.style")),
                        "styles",
                        None,
                        None,
                        None,
                        errors,
                    );
                }
            }
        }
    }

    fn check_rows(
        rows: &[Row],
        section: &str,
        row_group: Option<u32>,
        errors: &mut Vec<ValidationError>,
    ) {
        for (r, row) in rows.iter().enumerate() {
            let r = Some(r as u32);
            check(
                row.role.as_ref(),
                "row role",
                section,
                row_group,
                r,
                None,
                errors,
            );
            for (c, cell) in row.cells.iter().enumerate() {
                let c = Some(c as u32);
                check(
                    cell.scope.as_ref(),
                    "cell scope",
                    section,
                    row_group,
                    r,
                    c,
                    errors,
                );
                check(
                    cell.data_type.as_ref(),
                    "cell data_type",
                    section,
                    row_group,
                    r,
                    c,
                    errors,
                );
                if let Some(tv) = &cell.typed_value {
                    check(
                        Some(&tv.value_type),
                        "typed_value.type",
                        section,
                        row_group,
                        r,
                        c,
                        errors,
                    );
                }
            }
        }
    }

    check(
        Some(&table.config.page_break_mode),
        "config.page_break_mode",
        "config",
        None,
        None,
        None,
        errors,
    );
    check(
        table.config.container_overflow.as_ref(),
        "config.container_overflow",
        "config",
        None,
        None,
        None,
        errors,
    );

    for (i, col) in table.column_spec.iter().enumerate() {
        check(
            Some(&col.align),
            &format!("column_spec[{i}].align"),
            "column_spec",
            None,
            None,
            Some(i as u32),
            errors,
        );
    }

    // Sorted ids so error order is deterministic (the palette is a HashMap).
    let mut ids: Vec<&String> = table.styles.defs.keys().collect();
    ids.sort();
    for id in ids {
        check_style(&table.styles.defs[id], &format!("styles.defs.{id}"), errors);
    }
    let mut ids: Vec<&String> = table.styles.compositions.keys().collect();
    ids.sort();
    for id in ids {
        check_style(
            &table.styles.compositions[id].overrides,
            &format!("styles.compositions.{id}.overrides"),
            errors,
        );
    }
    for (i, cond) in table.styles.conditionals.iter().enumerate() {
        let at = format!("styles.conditionals[{i}]");
        check(
            cond.selector.row_parity.as_ref(),
            &format!("{at}.selector.row_parity"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check(
            cond.selector.scope.as_ref(),
            &format!("{at}.selector.scope"),
            "styles",
            None,
            None,
            None,
            errors,
        );
        check_style(&cond.style, &format!("{at}.style"), errors);
    }

    check_rows(&table.table.thead.rows, "thead", None, errors);
    for (g, group) in table.table.tbody.iter().enumerate() {
        check_rows(&group.rows, "tbody", Some(g as u32), errors);
        check_rows(&group.summary_rows, "tbody_summary", Some(g as u32), errors);
    }
}
