//! Layout invariants, checked on the geometry rather than on rendered pixels:
//! text stays inside its cell, rows are tall enough, cells tile the table exactly,
//! and everything fits the canvas. Run over the whole corpus, every small span
//! layout, every example with each column hidden in turn, and narrow fixed columns
//! full of long text.

use super::*;
use gridwell_testkit::spans::tilings;
use gridwell_testkit::{cell, column, examples, group, row, ColumnExt, TableBuilder};

const EPS: f64 = 1e-6;

fn check(name: &str, table: &Table) -> Vec<String> {
    let config = SvgConfig::default();
    let l = layout(table, &config);
    let mut problems = Vec::new();
    let mut fail = |msg: String| problems.push(format!("{name}: {msg}"));

    let check_block = |b: &TextBlock, what: &str, fail: &mut dyn FnMut(String)| {
        let line_h = b.font_size * LINE_HEIGHT;
        for line in &b.lines {
            // A single character can't be broken further; allow it to exceed a
            // pathologically narrow box.
            let single = line.text().chars().count() <= 1;
            if line.width > b.width + EPS && !single {
                fail(format!(
                    "{what}: line {:?} is {:.1}px wide in a {:.1}px box",
                    line.text(),
                    line.width,
                    b.width
                ));
            }
        }
        if b.lines.len() as f64 * line_h > b.height + EPS {
            fail(format!(
                "{what}: {} lines need {:.1}px, box is {:.1}px",
                b.lines.len(),
                b.lines.len() as f64 * line_h,
                b.height
            ));
        }
    };

    for (i, c) in l.cells.iter().enumerate() {
        check_block(&c.text, &format!("cell {i}"), &mut fail);
        if c.x < l.table_x - EPS
            || c.y < l.table_y - EPS
            || c.x + c.width > l.table_x + l.table_width + EPS
            || c.y + c.height > l.table_y + l.table_height + EPS
        {
            fail(format!("cell {i} outside the table rect"));
        }
    }
    for (i, b) in l.header.iter().chain(&l.footer).enumerate() {
        check_block(b, &format!("note/header {i}"), &mut fail);
        if b.x + max_width(&b.lines) > l.width + EPS {
            fail(format!("note/header {i} wider than the canvas"));
        }
        if b.y + b.height > l.height + EPS {
            fail(format!("note/header {i} below the canvas"));
        }
    }
    if l.table_width > l.width + EPS || l.table_y + l.table_height > l.height + EPS {
        fail("table larger than the canvas".into());
    }

    // Exact tiling: no overlaps, and areas sum to the table area.
    for (i, a) in l.cells.iter().enumerate() {
        for b in &l.cells[i + 1..] {
            let ox = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
            let oy = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
            if ox > EPS && oy > EPS {
                fail(format!(
                    "cells overlap at ({:.1},{:.1})",
                    a.x.max(b.x),
                    a.y.max(b.y)
                ));
            }
        }
    }
    let area: f64 = l.cells.iter().map(|c| c.width * c.height).sum();
    let table_area = l.table_width * l.table_height;
    if (area - table_area).abs() > 1e-3 * table_area.max(1.0) {
        fail(format!(
            "cells cover {area:.1}px² of a {table_area:.1}px² table (gap)"
        ));
    }
    problems
}

fn assert_ok(problems: Vec<String>) {
    assert!(
        problems.is_empty(),
        "{} layout problem(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

#[test]
fn corpus_layouts_hold_invariants() {
    let mut problems = Vec::new();
    for ex in examples() {
        problems.extend(check(ex.name, &ex.table()));
    }
    assert_ok(problems);
}

#[test]
fn every_span_layout_holds_invariants() {
    let mut problems = Vec::new();
    for (r, c) in [(1, 4), (2, 3), (3, 3)] {
        for t in tilings(r, c) {
            let table = TableBuilder::new(c as u32)
                .group(group(t.to_rows()))
                .build();
            problems.extend(check(&t.ascii(), &table));
        }
    }
    assert_ok(problems);
}

#[test]
fn hiding_any_column_holds_invariants_and_leaks_nothing() {
    let mut problems = Vec::new();
    for ex in examples() {
        let base = ex.table();
        for hidden in 0..base.column_spec.len() {
            let mut table = base.clone();
            table.column_spec[hidden].hidden = true;
            // Mark the hidden column's own cells so a leak is detectable.
            for row in table.table.thead.rows.iter_mut().chain(
                table
                    .table
                    .tbody
                    .iter_mut()
                    .flat_map(|g| g.rows.iter_mut().chain(g.summary_rows.iter_mut())),
            ) {
                let c = &mut row.cells[hidden];
                if !c.is_placeholder && c.colspan == 1 {
                    c.content = vec![ContentNode::Text {
                        value: "HIDDEN_LEAK".into(),
                    }];
                }
            }
            let name = format!("{} hide col {hidden}", ex.name);
            problems.extend(check(&name, &table));
            let svg = render(&table, &SvgConfig::default()).unwrap();
            if svg.contains("HIDDEN_LEAK") {
                problems.push(format!("{name}: hidden column content rendered"));
            }
        }
    }
    assert_ok(problems);
}

#[test]
fn long_text_in_narrow_fixed_columns_wraps_inside_cells() {
    let words = "the quick brown fox jumps over the lazy dog";
    let url = "https://example.com/a/very/long/path/without/any/spaces/at/all";
    let mut problems = Vec::new();
    for w in ["30px", "60px", "90px", "150px"] {
        let table = TableBuilder::new(3)
            .columns(vec![
                column("a", "A").width(w),
                column("b", "B").width(w),
                column("c", "C"),
            ])
            .title("A title long enough that it has to wrap over the narrow table")
            .head(row(vec![cell(words), cell(url), cell("auto")]))
            .body(vec![row(vec![
                cell(url),
                cell(words),
                cell("東京 大阪 販売実績 😀😀"),
            ])])
            .footnote(
                "f",
                "1",
                "A footnote that is also quite long and must wrap within the canvas width.",
            )
            .build();
        problems.extend(check(&format!("width {w}"), &table));
    }
    assert_ok(problems);
}

#[test]
fn max_width_wraps_and_min_width_pads() {
    let table = TableBuilder::new(2)
        .columns(vec![
            column("a", "A").min_width("80px").max_width("200px"),
            column("b", "B").min_width("300px"),
        ])
        .body(vec![row(vec![
            cell("clamped between 80 and 200px"),
            cell("x"),
        ])])
        .build();
    let l = layout(&table, &SvgConfig::default());
    let a = l.cells.iter().find(|c| c.x == 0.0).unwrap();
    let b = l.cells.iter().find(|c| c.x > 0.0).unwrap();
    assert!(a.width <= 200.0 + EPS, "max_width ignored: {}", a.width);
    assert!(a.text.lines.len() >= 2, "text should wrap at max_width");
    assert!(b.width >= 300.0 - EPS, "min_width ignored: {}", b.width);
    assert_ok(check("min/max", &table));
}

#[test]
fn line_breaks_start_new_lines() {
    let table = TableBuilder::new(1)
        .body(vec![row(vec![gridwell_testkit::cell_content(vec![
            gridwell_testkit::text("123 Main St"),
            gridwell_testkit::line_break(),
            gridwell_testkit::text("Suite 100"),
        ])])])
        .build();
    let l = layout(&table, &SvgConfig::default());
    let lines: Vec<String> = l.cells[0].text.lines.iter().map(Line::text).collect();
    assert_eq!(lines, vec!["123 Main St", "Suite 100"]);
}

#[test]
fn footnote_marks_are_superscript_runs() {
    let table = TableBuilder::new(1)
        .body(vec![row(vec![gridwell_testkit::cell_content(vec![
            gridwell_testkit::text("pass"),
            gridwell_testkit::footnote_mark("f", "1"),
        ])])])
        .footnote("f", "1", "note")
        .build();
    let svg = render(&table, &SvgConfig::default()).unwrap();
    assert!(
        svg.contains("pass<tspan font-size=\"9.8px\" baseline-shift=\"super\">1</tspan>"),
        "{svg}"
    );
}

#[test]
fn column_alignment_sets_text_anchor() {
    let table = TableBuilder::new(3)
        .columns(vec![
            column("l", "L"),
            column("c", "C").align("center"),
            column("r", "R").align("right"),
        ])
        .body(vec![row(vec![cell("left"), cell("mid"), cell("right")])])
        .build();
    let svg = render(&table, &SvgConfig::default()).unwrap();
    assert!(
        svg.contains("text-anchor=\"middle\" xml:space=\"preserve\">mid<"),
        "{svg}"
    );
    assert!(
        svg.contains("text-anchor=\"end\" xml:space=\"preserve\">right<"),
        "{svg}"
    );
}

#[test]
fn every_column_hidden_is_an_empty_table() {
    let table = TableBuilder::new(2)
        .columns(vec![column("a", "A").hidden(), column("b", "B").hidden()])
        .body(vec![row(vec![cell("x"), cell("y")])])
        .footnote("f", "1", "note")
        .build();
    let l = layout(&table, &SvgConfig::default());
    assert!(l.cells.is_empty());
    assert_eq!(l.table_width, 0.0);
    assert_ok(check("all hidden", &table));
}
