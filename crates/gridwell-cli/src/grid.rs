//! `gridwell grid`: each section's occupancy grid, drawn as text.
//!
//! Each section (the head, each group's rows, each group's summary rows) has a grid
//! of its own, as the validator materializes it. Every cell that isn't a
//! placeholder gets a code (A, B, …, Z, AA, …), and each position of the grid shows
//! the code of the cell that covers it, so a span is a run of one code. A position
//! nothing covers is `.`. The spanning cells are listed under the grid, and the
//! section's span errors after them. An invalid table is drawn too: the grid is
//! for finding out why.

use std::collections::HashMap;

use gridwell_ir::span::OccupancyGrid;
use gridwell_ir::{Cell, ContentNode, Row, Table};

use crate::inspect::count;

pub fn draw(table: &Table) -> String {
    let cols = table.config.table_cols;
    let mut out = String::new();
    let stub: Vec<usize> = (0..table.config.stub_cols.min(cols) as usize).collect();
    let hidden: Vec<usize> = (table.column_spec.iter().enumerate())
        .filter(|(_, c)| c.hidden)
        .map(|(c, _)| c)
        .collect();
    for (name, list) in [("Stub", stub), ("Hidden", hidden)] {
        if !list.is_empty() {
            let list: Vec<String> = list.iter().map(|c| format!("c{c}")).collect();
            out.push_str(&format!("{name} columns: {}\n", list.join(", ")));
        }
    }

    // Each with the validator's name for it, for its messages.
    let mut sections = vec![("thead".to_string(), &table.table.thead.rows, "thead", None)];
    for (g, group) in table.table.tbody.iter().enumerate() {
        let mut name = format!("tbody group {g}");
        if let Some(label) = &group.label {
            name.push_str(&format!(" {}", excerpt(&label.content)));
        }
        let summary = format!("tbody group {g} summary");
        let g = Some(g as u32);
        sections.push((name, &group.rows, "tbody", g));
        if !group.summary_rows.is_empty() {
            sections.push((summary, &group.summary_rows, "tbody_summary", g));
        }
    }
    for (name, rows, section, row_group) in sections {
        if !out.is_empty() {
            out.push('\n');
        }
        section_grid(&mut out, &name, rows, cols, section, row_group);
    }
    out
}

fn section_grid(
    out: &mut String,
    name: &str,
    rows: &[Row],
    cols: u32,
    section: &str,
    row_group: Option<u32>,
) {
    if rows.is_empty() {
        out.push_str(&format!("{name}: no rows\n"));
        return;
    }
    // As in validation: a row with the wrong number of cells is a COL_COUNT error,
    // and its section isn't drawn (which also keeps the grid no bigger than the
    // cells in the input, whatever `table_cols` says).
    if let Some((r, row)) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.cells.len() as u64 != cols as u64)
    {
        out.push_str(&format!(
            "{name}: not drawn, row {r} has {} cells but config.table_cols is {cols}\n",
            row.cells.len()
        ));
        return;
    }
    out.push_str(&format!(
        "{name}: {}, {}\n",
        count(rows.len(), "row"),
        count(cols as usize, "column")
    ));

    let (grid, errors) = OccupancyGrid::materialize(rows, cols, section, row_group);
    // A code for every cell that claims positions, in document order.
    let mut codes: HashMap<(u32, u32), String> = HashMap::new();
    let mut spanning: Vec<(String, u32, u32, &Cell)> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.is_placeholder {
                continue;
            }
            let code = code(codes.len());
            if cell.colspan != 1 || cell.rowspan != 1 {
                spanning.push((code.clone(), r as u32, c as u32, cell));
            }
            codes.insert((r as u32, c as u32), code);
        }
    }

    let label_width = format!("r{}", rows.len() - 1).len();
    let width = codes
        .values()
        .map(String::len)
        .chain([format!("c{}", cols.saturating_sub(1)).len()])
        .max()
        .unwrap_or(1);
    let mut line = " ".repeat(label_width + 1);
    for c in 0..cols {
        line.push_str(&format!(" {:<width$} ", format!("c{c}")));
    }
    push_line(out, &line);
    for (r, positions) in grid.grid.iter().enumerate() {
        let mut line = format!("{:<label_width$} ", format!("r{r}"));
        for owner in positions {
            let code = owner
                .and_then(|o| codes.get(&o))
                .map_or(".", String::as_str);
            line.push_str(&format!("[{code:<width$}]"));
        }
        push_line(out, &line);
    }

    for (code, r, c, cell) in spanning {
        let mut line = format!("{code} = cell(r{r},c{c}");
        if cell.rowspan != 1 {
            line.push_str(&format!(", rowspan={}", cell.rowspan));
        }
        if cell.colspan != 1 {
            line.push_str(&format!(", colspan={}", cell.colspan));
        }
        line.push_str(&format!(") {}", excerpt(&cell.content)));
        push_line(out, &line);
    }
    for e in errors {
        out.push_str(&format!("! {e}\n"));
    }
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line.trim_end());
    out.push('\n');
}

/// The code of the `n`th cell: A … Z, AA … AZ, BA, ….
fn code(mut n: usize) -> String {
    let mut code = Vec::new();
    loop {
        code.push(b'A' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    code.reverse();
    String::from_utf8(code).unwrap()
}

/// The start of some content's text, quoted (and escaped, so nothing in the IR
/// reaches the terminal as a control sequence).
fn excerpt(content: &[ContentNode]) -> String {
    const MAX: usize = 24;
    let mut text = String::new();
    for node in content {
        match node {
            ContentNode::Text { value } | ContentNode::StyledText { value, .. } => {
                text.push_str(value)
            }
            ContentNode::FootnoteMark { mark_text, .. } => text.push_str(mark_text),
            ContentNode::LineBreak {} => text.push(' '),
            ContentNode::Image { alt, .. } => text.push_str(alt.as_deref().unwrap_or("[image]")),
            ContentNode::Raw { .. } | ContentNode::Unknown(_) => {}
        }
    }
    if text.chars().count() > MAX {
        text = text.chars().take(MAX - 3).collect();
        text.push_str("...");
    }
    format!("{text:?}")
}
