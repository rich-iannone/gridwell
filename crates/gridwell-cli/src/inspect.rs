//! `gridwell inspect`: what a table IR holds, counted.
//!
//! Everything is counted from the IR as written, so an invalid table can be
//! inspected too (the count of its validation errors is one of the stats).

use std::collections::{BTreeMap, BTreeSet};

use gridwell_ir::{ContentNode, Row, RowRole, Table};
use serde_json::{json, Value};

/// A table's stats.
pub struct Stats {
    ir_version: String,
    errors: usize,
    columns: u32,
    stub_columns: u32,
    /// The ids of the hidden columns.
    hidden: Vec<String>,
    title: bool,
    subtitle: bool,
    extra_lines: usize,
    head_rows: usize,
    groups: usize,
    labeled_groups: usize,
    body_rows: usize,
    summary_rows: usize,
    grand_summary_rows: usize,
    /// Cells (not placeholders) with a colspan or rowspan other than 1.
    spanning_cells: usize,
    placeholders: usize,
    definitions: usize,
    compositions: usize,
    conditionals: usize,
    /// Every style id referenced, defined or not.
    used: BTreeSet<String>,
    /// The definitions and compositions nothing uses (a definition is used when a
    /// used composition extends it).
    unused: Vec<String>,
    footnotes: usize,
    source_notes: usize,
    /// Content nodes by `type`, unknown types included.
    content: BTreeMap<String, usize>,
    /// Typed values by type.
    typed_values: BTreeMap<String, usize>,
    /// The keys of `extensions`.
    extensions: Vec<String>,
}

impl Stats {
    pub fn of(table: &Table) -> Self {
        let mut used = BTreeSet::new();
        let mut content = BTreeMap::new();
        let mut uses = |style_id: Option<&String>, nodes: &[ContentNode]| {
            used.extend(style_id.cloned());
            for node in nodes {
                *content.entry(node_type(node)).or_insert(0) += 1;
                if let ContentNode::StyledText {
                    style_id: Some(id), ..
                } = node
                {
                    used.insert(id.clone());
                }
            }
        };

        let header = table.header.as_ref();
        if let Some(h) = header {
            for line in [&h.title, &h.subtitle].into_iter().flatten() {
                uses(line.style_id.as_ref(), &line.content);
            }
            for line in &h.extra_lines {
                uses(line.style_id.as_ref(), &line.content);
            }
        }
        for col in &table.column_spec {
            uses(col.style_id.as_ref(), &[]);
        }
        let mut sections: Vec<&[Row]> = vec![&table.table.thead.rows];
        for group in &table.table.tbody {
            if let Some(label) = &group.label {
                uses(label.style_id.as_ref(), &label.content);
            }
            sections.push(&group.rows);
            sections.push(&group.summary_rows);
        }
        let mut typed_values = BTreeMap::new();
        let (mut spanning_cells, mut placeholders) = (0, 0);
        for row in sections.into_iter().flatten() {
            uses(row.style_id.as_ref(), &[]);
            for cell in &row.cells {
                uses(cell.style_id.as_ref(), &cell.content);
                if cell.is_placeholder {
                    placeholders += 1;
                } else if cell.colspan != 1 || cell.rowspan != 1 {
                    spanning_cells += 1;
                }
                if let Some(v) = &cell.typed_value {
                    *typed_values.entry(v.value_type.to_string()).or_insert(0) += 1;
                }
            }
        }
        let footer = table.footer.as_ref();
        if let Some(f) = footer {
            for n in &f.footnotes {
                uses(n.style_id.as_ref(), &n.content);
            }
            for n in &f.source_notes {
                uses(n.style_id.as_ref(), &n.content);
            }
        }

        let styles = &table.styles;
        let bases: BTreeSet<&String> = used
            .iter()
            .filter_map(|id| styles.compositions.get(id))
            .map(|c| &c.extends)
            .collect();
        let mut unused: Vec<String> = styles
            .defs
            .keys()
            .chain(styles.compositions.keys())
            .filter(|id| !used.contains(*id) && !bases.contains(id))
            .cloned()
            .collect();
        unused.sort();
        unused.dedup();

        let body = &table.table.tbody;
        let summary = || body.iter().flat_map(|g| &g.summary_rows);
        Stats {
            ir_version: table.ir_version.clone(),
            errors: table.validate().len(),
            columns: table.config.table_cols,
            stub_columns: table.config.stub_cols,
            hidden: table
                .column_spec
                .iter()
                .filter(|c| c.hidden)
                .map(|c| c.id.clone())
                .collect(),
            title: header.is_some_and(|h| h.title.is_some()),
            subtitle: header.is_some_and(|h| h.subtitle.is_some()),
            extra_lines: header.map_or(0, |h| h.extra_lines.len()),
            head_rows: table.table.thead.rows.len(),
            groups: body.len(),
            labeled_groups: body.iter().filter(|g| g.label.is_some()).count(),
            body_rows: body.iter().map(|g| g.rows.len()).sum(),
            summary_rows: summary().count(),
            grand_summary_rows: summary()
                .filter(|r| r.role == Some(RowRole::GrandSummary))
                .count(),
            spanning_cells,
            placeholders,
            definitions: styles.defs.len(),
            compositions: styles.compositions.len(),
            conditionals: styles.conditionals.len(),
            used,
            unused,
            footnotes: footer.map_or(0, |f| f.footnotes.len()),
            source_notes: footer.map_or(0, |f| f.source_notes.len()),
            content,
            typed_values,
            extensions: match &table.extensions {
                Some(Value::Object(m)) => m.keys().cloned().collect(),
                _ => Vec::new(),
            },
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "ir_version": self.ir_version,
            "valid": self.errors == 0,
            "errors": self.errors,
            "columns": {
                "total": self.columns,
                "stub": self.stub_columns,
                "hidden": self.hidden,
            },
            "header": {
                "title": self.title,
                "subtitle": self.subtitle,
                "extra_lines": self.extra_lines,
            },
            "head_rows": self.head_rows,
            "groups": self.groups,
            "labeled_groups": self.labeled_groups,
            "body_rows": self.body_rows,
            "summary_rows": self.summary_rows,
            "grand_summary_rows": self.grand_summary_rows,
            "spanning_cells": self.spanning_cells,
            "placeholders": self.placeholders,
            "styles": {
                "definitions": self.definitions,
                "compositions": self.compositions,
                "conditionals": self.conditionals,
                "used": self.used,
                "unused": self.unused,
            },
            "footnotes": self.footnotes,
            "source_notes": self.source_notes,
            "content": self.content,
            "typed_values": self.typed_values,
            "extensions": self.extensions,
        })
    }

    pub fn to_text(&self) -> String {
        let mut columns = self.columns.to_string();
        let mut kinds = Vec::new();
        if self.stub_columns > 0 {
            kinds.push(format!("{} stub", self.stub_columns));
        }
        if !self.hidden.is_empty() {
            kinds.push(format!(
                "{} hidden: {}",
                self.hidden.len(),
                list(&self.hidden)
            ));
        }
        if !kinds.is_empty() {
            columns.push_str(&format!(" ({})", kinds.join(", ")));
        }
        let mut header: Vec<String> = [(self.title, "title"), (self.subtitle, "subtitle")]
            .into_iter()
            .filter(|(present, _)| *present)
            .map(|(_, name)| name.to_string())
            .collect();
        if self.extra_lines > 0 {
            header.push(count(self.extra_lines, "extra line"));
        }
        let mut body = format!(
            "{} in {}",
            count(self.body_rows, "row"),
            count(self.groups, "group")
        );
        if self.labeled_groups > 0 {
            body.push_str(&format!(" ({} labeled)", self.labeled_groups));
        }
        let mut summary = self.summary_rows.to_string();
        if self.grand_summary_rows > 0 {
            summary.push_str(&format!(" ({} grand summary)", self.grand_summary_rows));
        }
        let spans = if self.spanning_cells + self.placeholders == 0 {
            "none".to_string()
        } else {
            format!(
                "{}, {}",
                count(self.spanning_cells, "spanning cell"),
                count(self.placeholders, "placeholder")
            )
        };
        let mut styles = format!(
            "{}, {}, {}",
            count(self.definitions, "definition"),
            count(self.compositions, "composition"),
            count(self.conditionals, "conditional"),
        );
        styles.push_str(&format!("; {} used", self.used.len()));
        let valid = match self.errors {
            0 => "yes".to_string(),
            n => format!("no, {} (see `gridwell validate`)", count(n, "error")),
        };

        let lines = [
            ("IR version", shown(&self.ir_version)),
            ("Valid", valid),
            ("Columns", columns),
            ("Header", list_or_none(&header)),
            ("Head rows", self.head_rows.to_string()),
            ("Body", body),
            ("Summary rows", summary),
            ("Spans", spans),
            ("Styles", styles),
            ("Unused styles", list_or_none(&self.unused)),
            ("Footnotes", self.footnotes.to_string()),
            ("Source notes", self.source_notes.to_string()),
            ("Content", tally(&self.content)),
            ("Typed values", tally(&self.typed_values)),
            ("Extensions", list_or_none(&self.extensions)),
        ];
        let width = lines.iter().map(|(name, _)| name.len()).max().unwrap_or(0) + 1;
        let mut out = String::new();
        for (name, value) in lines {
            out.push_str(&format!("{:<width$} {value}\n", format!("{name}:")));
        }
        out
    }
}

/// A content node's `type`.
fn node_type(node: &ContentNode) -> String {
    match node {
        ContentNode::Text { .. } => "text",
        ContentNode::StyledText { .. } => "styled_text",
        ContentNode::LineBreak {} => "line_break",
        ContentNode::FootnoteMark { .. } => "footnote_mark",
        ContentNode::Image { .. } => "image",
        ContentNode::Raw { .. } => "raw",
        ContentNode::Unknown(m) => {
            return m
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        }
    }
    .to_string()
}

/// `1 row`, `2 rows`.
pub(crate) fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// A string from the IR as printed: quoted and escaped if it's empty or has a
/// control character (an escape sequence in an id mustn't reach the terminal) or
/// a comma (which would split it in a list).
fn shown(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_control() || c == ',') {
        format!("{s:?}")
    } else {
        s.to_string()
    }
}

/// `a, b, c`: strings from the IR, [shown](shown).
fn list<S: AsRef<str>>(items: impl IntoIterator<Item = S>) -> String {
    let items: Vec<String> = items.into_iter().map(|s| shown(s.as_ref())).collect();
    items.join(", ")
}

fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        list(items)
    }
}

/// `6 text, 2 footnote_mark`, or `none`.
fn tally(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    let items: Vec<String> = counts
        .iter()
        .map(|(k, n)| format!("{n} {}", shown(k)))
        .collect();
    items.join(", ")
}
