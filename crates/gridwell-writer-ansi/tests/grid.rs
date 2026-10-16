//! Structural oracle for terminal output: every line of the grid has the same
//! display width, a row's vertical bars only appear on a column boundary (spans
//! merge columns; they never invent new ones), and each horizontal rule's
//! junctions meet exactly the bars of the rows above and below it. Also:
//! table text can't smuggle terminal escape sequences into the output.

use gridwell_ir::Table;
use gridwell_testkit::spans::tilings;
use gridwell_testkit::{cell, column, examples, group, row, CellExt, ColumnExt, TableBuilder};
use gridwell_writer_ansi::render_ansi;
use unicode_width::UnicodeWidthStr;

/// Remove the SGR sequences (`ESC [ … m`) the writer emits.
fn strip_sgr(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            for d in chars.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Column positions (in display columns) of `chars` in `line`.
fn positions(line: &str, chars: &[char]) -> Vec<usize> {
    // Measured as strings, not summed per char: unicode-width treats some
    // sequences (e.g. Arabic lam-alef) as narrower than their parts.
    line.char_indices()
        .filter(|(_, c)| chars.contains(c))
        .map(|(i, _)| UnicodeWidthStr::width(&line[..i]))
        .collect()
}

fn check(name: &str, table: &Table) {
    let out = strip_sgr(&render_ansi(table).unwrap());
    let lines: Vec<&str> = out.lines().collect();
    let Some(top) = lines.iter().position(|l| l.starts_with('┌')) else {
        // No grid (every column hidden).
        return;
    };
    let bottom = lines.iter().rposition(|l| l.starts_with('└')).unwrap();
    let width = UnicodeWidthStr::width(lines[top]);
    let is_rule = |l: &str| l.starts_with(['┌', '├', '└']);
    // Border characters with a stroke going up, and going down.
    const UP: [char; 6] = ['├', '┼', '┤', '└', '┴', '┘'];
    const DOWN: [char; 6] = ['┌', '┬', '┐', '├', '┼', '┤'];
    for (i, line) in lines.iter().enumerate().take(bottom + 1).skip(top) {
        assert_eq!(
            UnicodeWidthStr::width(*line),
            width,
            "{name}: ragged line {line:?}\n{out}"
        );
        if !is_rule(line) {
            continue;
        }
        // A rule's junctions meet exactly the bars of the rows it separates: a
        // span above or below has no boundary there, so the rule runs straight.
        let bars = |l: Option<&&str>| match l {
            Some(l) if !is_rule(l) => positions(l, &['│']),
            _ => vec![],
        };
        let above = if i > top {
            bars(lines.get(i - 1))
        } else {
            vec![]
        };
        let below = if i < bottom {
            bars(lines.get(i + 1))
        } else {
            vec![]
        };
        assert_eq!(
            positions(line, &UP),
            above,
            "{name}: rule {line:?} doesn't meet the bars above\n{out}"
        );
        assert_eq!(
            positions(line, &DOWN),
            below,
            "{name}: rule {line:?} doesn't meet the bars below\n{out}"
        );
    }
    // Spans merge columns; they never invent new ones: every bar sits on a
    // boundary of the same grid drawn without spans.
    let unspanned = strip_sgr(&render_ansi(&without_spans(table)).unwrap());
    let top_rule = unspanned.lines().find(|l| l.starts_with('┌')).unwrap();
    let boundaries = positions(top_rule, &['┌', '┬', '┐']);
    for line in &lines[top..=bottom] {
        for b in positions(line, &['│']) {
            assert!(
                boundaries.contains(&b),
                "{name}: bar at {b} in {line:?}\n{out}"
            );
        }
    }
}

/// `table` with every span split into single cells. Column widths come from
/// single-column cells only, so this draws the same columns with every boundary.
fn without_spans(table: &Table) -> Table {
    let mut t = table.clone();
    let block = &mut t.table;
    let rows = block.thead.rows.iter_mut().chain(
        block
            .tbody
            .iter_mut()
            .flat_map(|g| g.rows.iter_mut().chain(g.summary_rows.iter_mut())),
    );
    for c in rows.flat_map(|r| r.cells.iter_mut()) {
        if c.colspan > 1 || c.is_placeholder {
            c.content.clear();
        }
        c.colspan = 1;
        c.rowspan = 1;
        c.is_placeholder = false;
    }
    t
}

#[test]
fn corpus_grids_are_rectangular() {
    for ex in examples() {
        let t = ex.table();
        check(ex.name, &t);
        for hidden in 0..t.column_spec.len() {
            let mut h = t.clone();
            h.column_spec[hidden].hidden = true;
            check(&format!("{} hide {hidden}", ex.name), &h);
        }
    }
}

#[test]
fn every_span_layout_and_hidden_mask_is_rectangular() {
    for (rows, cols) in [(2, 3), (3, 3), (2, 4)] {
        for t in tilings(rows, cols) {
            for mask in 0u32..(1 << cols) {
                let columns = (0..cols)
                    .map(|i| {
                        let c = column(&format!("c{i}"), "");
                        if mask & (1 << i) != 0 {
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
                check(&format!("{} mask {mask:b}", t.ascii()), &table);
            }
        }
    }
}

#[test]
fn spanning_text_too_long_for_its_columns_is_truncated() {
    // Columns are sized by single-column cells; a spanning cell's text can still
    // be wider than its columns, and must be cut to fit with an ellipsis.
    for text in [
        "a spanning cell much wider than both columns put together",
        "東京大阪名古屋札幌福岡神戸京都横浜川崎仙台",
    ] {
        let table = TableBuilder::new(2)
            .body(vec![
                row(vec![cell(text).colspan(2), gridwell_testkit::placeholder()]),
                row(vec![cell("a"), cell("b")]),
            ])
            .build();
        let out = strip_sgr(&render_ansi(&table).unwrap());
        assert!(out.contains('…'), "{out}");
        check(text, &table);
    }
}

#[test]
fn control_characters_never_reach_the_terminal() {
    let evil = "a\u{1b}]0;pwned\u{7}b\u{1b}[2Jc\u{9b}31md\te\nf";
    let table = TableBuilder::new(1)
        .title(evil)
        .head(row(vec![cell(evil)]))
        .body(vec![row(vec![cell(evil)])])
        .footnote("f", evil, evil)
        .source_note(evil)
        .build();
    let out = render_ansi(&table).unwrap();
    // The only escapes left are the writer's own SGR sequences.
    let stripped = strip_sgr(&out);
    assert!(
        !stripped.chars().any(|c| c.is_control() && c != '\n'),
        "control characters leaked: {stripped:?}"
    );
    assert!(stripped.contains("a]0;pwnedb[2Jc31md e f"), "{stripped}");
    check("evil", &table);
}

#[test]
fn background_colours_are_opt_in() {
    use gridwell_writer_ansi::{AnsiConfig, AnsiWriter};
    let table = TableBuilder::new(1)
        .style_def(
            "f",
            gridwell_ir::StyleDef {
                background_color: Some("#336699".into()),
                ..Default::default()
            },
        )
        .striping(true, true)
        .body(vec![row(vec![cell("a").style("f")]), row(vec![cell("b")])])
        .build();
    let render = |background_colors| {
        AnsiWriter::with_config(AnsiConfig {
            background_colors,
            ..Default::default()
        })
        .render(&table)
        .unwrap()
    };
    let off = render(false);
    assert!(!off.contains("\u{1b}[48;"), "{off:?}");
    let on = render(true);
    assert!(
        on.contains("\u{1b}[48;2;51;102;153m"),
        "fill missing: {on:?}"
    );
    // The stripe (row 2) as it looks on white.
    assert!(
        on.contains("\u{1b}[48;2;249;249;249m"),
        "stripe missing: {on:?}"
    );
    check("backgrounds", &table);
    // Needs true color.
    let no_tc = AnsiWriter::with_config(AnsiConfig {
        background_colors: true,
        true_color: false,
        ..Default::default()
    })
    .render(&table)
    .unwrap();
    assert!(!no_tc.contains("\u{1b}[48;"));
}

#[test]
fn footer_marks_match_the_marks_in_cells() {
    // With box drawing, a mark is superscript in the cell and must be the same in
    // the footer line that explains it; plain ASCII output keeps both plain.
    use gridwell_testkit::{cell_content, footnote_mark, text};
    use gridwell_writer_ansi::{AnsiConfig, AnsiWriter};
    let table = TableBuilder::new(1)
        .body(vec![row(vec![cell_content(vec![
            text("Mean"),
            footnote_mark("fn1", "1"),
        ])])])
        .footnote("fn1", "1", "SD = standard deviation.")
        .build();
    let out = strip_sgr(&render_ansi(&table).unwrap());
    assert!(out.contains("Mean¹"), "{out}");
    assert!(out.contains("  ¹ SD = standard deviation."), "{out}");
    let ascii = AnsiWriter::with_config(AnsiConfig {
        box_drawing: false,
        ..Default::default()
    })
    .render(&table)
    .unwrap();
    let ascii = strip_sgr(&ascii);
    assert!(ascii.contains("Mean1"), "{ascii}");
    assert!(ascii.contains("  1 SD = standard deviation."), "{ascii}");
}
