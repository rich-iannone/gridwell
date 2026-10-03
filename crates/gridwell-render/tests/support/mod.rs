//! Shared test support: the grid a table *should* show, computed from the IR alone
//! (no layout crate), and readers that recover the grid a writer *did* produce,
//! each from the format's own structure.
//!
//! A grid is one string per visible position, row by row, after span expansion:
//! every position shows the text of the cell covering it. Group labels are rows
//! whose every position shows the label. Text is compared with all whitespace
//! removed, since formats legitimately differ in how they render line breaks.

#![allow(dead_code)]

pub mod ooxml;
pub mod readers;
pub mod typeset;

use std::ops::Range;

use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table, ValueType};

/// One string per position, row by row.
pub type Grid = Vec<Vec<String>>;

/// What a position should show.
#[derive(Debug, Clone, PartialEq)]
pub enum Want {
    /// This text (whitespace removed).
    Text(String),
    /// Anything: the cell contains format-specific raw markup, which only its own
    /// format renders.
    Any,
}

/// The expected grid of a table.
#[derive(Debug, Clone)]
pub struct Expected {
    pub rows: Vec<Vec<Want>>,
    /// The raw number of a cell typed `number`/`integer`, per position (written
    /// as a numeric cell by XLSX).
    pub numbers: Vec<Vec<Option<f64>>>,
    /// Header lines (title, subtitle, extra lines) above the table.
    pub header_lines: usize,
    /// Every visible cell's area (rows, visible columns), group labels included:
    /// the regions tile the grid. Readers that see merged areas rather than
    /// expanded positions (LaTeX) compare region by region.
    pub regions: Vec<Region>,
}

/// The area one cell covers, in grid rows and visible columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub rows: Range<usize>,
    pub cols: Range<usize>,
}

impl Expected {
    pub fn width(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }
}

/// Whitespace and control characters removed (formats render line breaks
/// differently, and XML-based formats must drop most control characters).
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}

/// The text a reader should recover from content: text, styled text, footnote
/// marks, image alt text. `None` if there is raw content.
fn want_of(content: &[ContentNode]) -> Want {
    let mut out = String::new();
    for n in content {
        match n {
            ContentNode::Text { value } | ContentNode::StyledText { value, .. } => {
                out.push_str(value)
            }
            ContentNode::FootnoteMark { mark_text, .. } => out.push_str(mark_text),
            ContentNode::Image { alt, .. } => out.push_str(alt.as_deref().unwrap_or("")),
            ContentNode::Raw { .. } => return Want::Any,
            ContentNode::LineBreak {} | ContentNode::Unknown => {}
        }
    }
    Want::Text(norm(&out))
}

pub fn expected(table: &Table) -> Expected {
    let hidden: Vec<bool> = table.column_spec.iter().map(|c| c.hidden).collect();
    let visible = hidden.iter().filter(|h| !**h).count();
    // The visible index of each grid column (hidden columns take the next one).
    let vis_before: Vec<usize> = (0..=hidden.len())
        .map(|c| hidden[..c].iter().filter(|h| !**h).count())
        .collect();
    type Section = (Vec<Vec<Want>>, Vec<Vec<Option<f64>>>, Vec<Region>);
    let section = |rows: &[Row]| -> Section {
        let width = hidden.len();
        let mut grid = vec![vec![Want::Text(String::new()); width]; rows.len()];
        let mut nums = vec![vec![None; width]; rows.len()];
        let mut regions = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.is_placeholder {
                    continue;
                }
                let want = want_of(&cell.content);
                let num = cell
                    .typed_value
                    .as_ref()
                    .filter(|t| matches!(t.value_type, ValueType::Number | ValueType::Integer))
                    .and_then(|t| t.value.as_f64());
                let end = (c + cell.colspan as usize).min(width);
                let cols = vis_before[c]..vis_before[end];
                if !cols.is_empty() {
                    regions.push(Region {
                        rows: r..(r + cell.rowspan as usize).min(rows.len()),
                        cols,
                    });
                }
                for (grid_row, num_row) in grid
                    .iter_mut()
                    .zip(nums.iter_mut())
                    .skip(r)
                    .take(cell.rowspan as usize)
                {
                    for cc in c..end {
                        grid_row[cc] = want.clone();
                        num_row[cc] = num;
                    }
                }
            }
        }
        fn keep<T>(row: Vec<T>, hidden: &[bool]) -> Vec<T> {
            row.into_iter()
                .zip(hidden)
                .filter(|(_, h)| !**h)
                .map(|(v, _)| v)
                .collect()
        }
        (
            grid.into_iter().map(|r| keep(r, &hidden)).collect(),
            nums.into_iter().map(|r| keep(r, &hidden)).collect(),
            regions,
        )
    };
    let mut rows = Vec::new();
    let mut numbers = Vec::new();
    let mut regions = Vec::new();
    let mut push = |(g, n, reg): Section| {
        let offset = rows.len();
        regions.extend(reg.into_iter().map(|Region { rows, cols }| Region {
            rows: rows.start + offset..rows.end + offset,
            cols,
        }));
        rows.extend(g);
        numbers.extend(n);
    };
    if !table.config.column_labels_hidden {
        push(section(&table.table.thead.rows));
    }
    for g in &table.table.tbody {
        if let Some(label) = &g.label {
            push((
                vec![vec![want_of(&label.content); visible]],
                vec![vec![None; visible]],
                vec![Region {
                    rows: 0..1,
                    cols: 0..visible,
                }],
            ));
        }
        push(section(&g.rows));
        push(section(&g.summary_rows));
    }
    let header_lines = table.header.as_ref().map_or(0, |h| {
        usize::from(h.title.is_some()) + usize::from(h.subtitle.is_some()) + h.extra_lines.len()
    });
    if visible == 0 {
        rows.clear();
        numbers.clear();
        regions.clear();
    }
    Expected {
        rows,
        numbers,
        header_lines,
        regions,
    }
}

/// Compare a recovered grid with the expected one. `numbers`: also accept a
/// cell's raw number in place of its text (XLSX numeric cells).
pub fn compare(got: &Grid, want: &Expected, numbers: bool) -> Result<(), String> {
    if got.len() != want.rows.len() {
        return Err(format!(
            "{} rows, expected {}\n got: {got:?}\nwant: {:?}",
            got.len(),
            want.rows.len(),
            want.rows
        ));
    }
    for (r, (g, w)) in got.iter().zip(&want.rows).enumerate() {
        if g.len() != w.len() {
            return Err(format!(
                "row {r}: {} cells, expected {}\n got: {g:?}\nwant: {w:?}",
                g.len(),
                w.len()
            ));
        }
        for (c, (gt, wt)) in g.iter().zip(w).enumerate() {
            let ok = match wt {
                Want::Any => true,
                Want::Text(t) => {
                    norm(gt) == *t
                        || (numbers
                            && want.numbers[r][c]
                                .is_some_and(|n| gt.parse::<f64>().is_ok_and(|v| v == n)))
                }
            };
            if !ok {
                return Err(format!(
                    "({r},{c}): got {gt:?}, want {wt:?}\n got: {got:?}\nwant: {:?}",
                    want.rows
                ));
            }
        }
    }
    Ok(())
}
