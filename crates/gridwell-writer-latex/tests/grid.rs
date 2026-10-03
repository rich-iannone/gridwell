//! Structural oracle on the LaTeX source: in every tabular row, the entries
//! (counting `\multicolumn{n}` as n) add up to the number of visible columns, and
//! each cell's text sits at the visible column where its span starts. LaTeX
//! itself accepts short rows, so a cell shifted left (e.g. by a dropped empty cell
//! under a `\multirow`) compiles fine and only this check catches it.

use gridwell_testkit::spans::{label, tilings};
use gridwell_testkit::{column, group, ColumnExt, TableBuilder};
use gridwell_writer_latex::render_latex;

/// Split a row on top-level `&` (not inside braces).
fn entries(row: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    let bytes = row.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'&' if depth == 0 && (i == 0 || bytes[i - 1] != b'\\') => {
                out.push(row[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(row[start..].trim());
    out
}

fn span(entry: &str) -> usize {
    entry
        .strip_prefix("\\multicolumn{")
        .and_then(|r| r.split('}').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
}

#[test]
fn rows_cover_the_visible_grid_and_cells_sit_in_their_columns() {
    let mut checked = 0;
    for (rows, cols) in [(1, 4), (2, 3), (3, 3), (2, 4), (3, 4)] {
        for t in tilings(rows, cols) {
            for mask in 0u32..(1 << cols) - 1 {
                let hidden: Vec<bool> = (0..cols).map(|i| mask & (1 << i) != 0).collect();
                let columns = (0..cols)
                    .map(|i| {
                        let c = column(&format!("c{i}"), "");
                        if hidden[i] {
                            c.hidden()
                        } else {
                            c
                        }
                    })
                    .collect();
                let table = TableBuilder::new(cols as u32)
                    .columns(columns)
                    .group(group(t.to_rows()))
                    .build();
                let src = render_latex(&table).unwrap();
                let visible = hidden.iter().filter(|h| !**h).count();
                let vis = |c: usize| (0..c).filter(|&i| !hidden[i]).count();

                // Expected visible start column of each origin label.
                let mut want = std::collections::HashMap::new();
                for rect in &t.rects {
                    if let Some(first) = (rect.col..rect.col + rect.colspan).find(|&c| !hidden[c]) {
                        want.insert(label(rect.row, rect.col), (rect.row, vis(first)));
                    }
                }

                let body: Vec<&str> = src
                    .lines()
                    .filter(|l| l.ends_with("\\\\") && !l.starts_with('{'))
                    .collect();
                assert_eq!(body.len(), rows, "{} {hidden:?}\n{src}", t.ascii());
                for (r, line) in body.iter().enumerate() {
                    let row = line.trim_end_matches("\\\\").trim();
                    let mut col = 0;
                    for e in entries(row) {
                        if let Some((lab, (wr, wc))) =
                            want.iter().find(|(lab, _)| e.contains(lab.as_str()))
                        {
                            assert_eq!(
                                (r, col),
                                (*wr, *wc),
                                "{lab} misplaced in {} {hidden:?}\n{src}",
                                t.ascii()
                            );
                        }
                        col += span(e);
                    }
                    assert_eq!(col, visible, "row {r} of {} {hidden:?}\n{src}", t.ascii());
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 40_000, "{checked}");
}
