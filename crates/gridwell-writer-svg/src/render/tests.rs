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
        // Each line is as tall as its largest text (computed here, independently
        // of the layout's own height function).
        let need: f64 = b
            .lines
            .iter()
            .map(|line| {
                let size = line
                    .runs
                    .iter()
                    .map(|r| r.style.size.unwrap_or(b.font_size))
                    .fold(f64::NAN, f64::max);
                (if size.is_nan() { b.font_size } else { size }) * LINE_HEIGHT
            })
            .sum();
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
        if need > b.height + EPS {
            fail(format!(
                "{what}: {} lines need {:.1}px, box is {:.1}px",
                b.lines.len(),
                need,
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
        // 75% of 16px, raised 0.4em of its own size (as HTML), italic.
        svg.contains(
            "pass<tspan font-style=\"italic\" font-size=\"12px\" baseline-shift=\"4.8px\">1</tspan>"
        ),
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
        svg.lines()
            .any(|l| l.contains("text-anchor=\"middle\"") && l.ends_with(">mid</text>")),
        "{svg}"
    );
    assert!(
        svg.lines()
            .any(|l| l.contains("text-anchor=\"end\"") && l.ends_with(">right</text>")),
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
    assert_eq!(l.table_height, 0.0);
    // The (empty) table is as wide as its note, as in HTML.
    assert!(l.table_width > 0.0 && l.footer.len() == 1);
    assert_ok(check("all hidden", &table));
}

#[test]
fn decorations_and_sizes_reach_the_runs_and_the_layout() {
    use gridwell_ir::style::StyleDef;
    use gridwell_testkit::{cell_content, styled, text, CellExt};
    let def = |f: fn(&mut StyleDef)| {
        let mut d = StyleDef::default();
        f(&mut d);
        d
    };
    let table = TableBuilder::new(1)
        .style_def("u", def(|d| d.text_decoration = Some("underline".into())))
        .style_def(
            "s",
            def(|d| d.text_decoration = Some("line-through".into())),
        )
        .style_def("big", def(|d| d.font_size = Some("28px".into())))
        .body(vec![
            row(vec![cell("normal")]),
            row(vec![cell("BIG").style("big")]),
            row(vec![cell_content(vec![
                styled("under", "u"),
                text(" "),
                styled("struck", "s"),
                text(" tail"),
            ])]),
        ])
        .build();
    let svg = render(&table, &SvgConfig::default()).unwrap();
    assert!(
        svg.contains("<tspan font-size=\"28px\">BIG</tspan>"),
        "{svg}"
    );
    assert!(
        svg.contains("<tspan text-decoration=\"underline\">under</tspan>"),
        "{svg}"
    );
    // The gap after a decorated word is not decorated.
    assert!(
        svg.contains("<tspan text-decoration=\"line-through\">struck</tspan> tail"),
        "{svg}"
    );

    let l = layout(&table, &SvgConfig::default());
    let height = |text: &str| {
        l.cells
            .iter()
            .find(|c| c.text.lines.iter().any(|line| line.text() == text))
            .map(|c| c.height)
            .unwrap()
    };
    // 28px text needs a taller row than 14px text, and is measured at its size.
    assert!(
        height("BIG") > height("normal") + 10.0,
        "{} vs {}",
        height("BIG"),
        height("normal")
    );
    let big = &l
        .cells
        .iter()
        .find(|c| c.text.lines[0].text() == "BIG")
        .unwrap()
        .text
        .lines[0];
    assert!(
        big.width > crate::measure::text_width("BIG", 27.0, false, false),
        "{}",
        big.width
    );
    assert_ok(check("decorations", &table));
}

// ─── The default look (shared with HTML's base stylesheet) ───

const OUTER: &str = "#A8A8A8";
const LIGHT: &str = "#D3D3D3";

fn hrules<'a>(l: &'a Layout, color: &str, width: f64) -> Vec<&'a RuleLine> {
    l.rules
        .iter()
        .filter(|r| r.y1 == r.y2 && r.color == color && (r.width - width).abs() < EPS)
        .collect()
}

fn full_width(l: &Layout, r: &RuleLine) -> bool {
    r.x1.abs() < EPS && (r.x2 - l.table_width).abs() < EPS
}

#[test]
fn outer_rules_frame_the_heading_and_footer_section_rules_the_table() {
    let t = TableBuilder::new(2)
        .title("T")
        .head(row(vec![cell("A"), cell("B")]))
        .body(vec![
            row(vec![cell("1"), cell("2")]),
            row(vec![cell("3"), cell("4")]),
        ])
        .source_note("src")
        .build();
    let l = layout(&t, &SvgConfig::default());
    // Dark rules: one above the title, one below the note, full width.
    let outer = hrules(&l, OUTER, 2.0);
    assert_eq!(outer.len(), 2, "{:?}", l.rules);
    assert!(outer.iter().all(|r| full_width(&l, r)));
    assert!((outer[0].y1 - 1.0).abs() < EPS, "top rule at the very top");
    assert!(
        (outer[1].y1 - (l.height - 1.0)).abs() < 1.0,
        "bottom rule at the bottom"
    );
    // Light 2px: the table's top, under the head and the table's bottom (per cell:
    // two cells per boundary; head bottom and body top coincide).
    let ys: std::collections::BTreeSet<i64> = hrules(&l, LIGHT, 2.0)
        .iter()
        .map(|r| (r.y1 * 100.0).round() as i64)
        .collect();
    assert_eq!(ys.len(), 3, "{ys:?}");
    // One hairline boundary, between the two body rows: one segment per cell, each
    // drawn once.
    assert_eq!(hrules(&l, LIGHT, 1.0).len(), 2);
    // The title is centered over the table.
    assert_eq!(l.header[0].align, Align::Center);
    assert!((l.header[0].width - (l.table_width - 10.0)).abs() < EPS);
}

#[test]
fn group_labels_stub_summaries_and_spanners_get_their_rules() {
    use gridwell_testkit::{placeholder, CellExt, RowExt};
    let t = TableBuilder::new(3)
        .stub_cols(1)
        .head(row(vec![cell(""), cell("Group").colspan(2), placeholder()]).role("spanner_label"))
        .head(row(vec![cell("S"), cell("A"), cell("B")]))
        .group(
            gridwell_testkit::labeled_group(
                "G",
                vec![row(vec![cell("r").stub(), cell("1"), cell("2")])],
            )
            .summary(vec![
                row(vec![cell("Total").stub(), cell("1"), cell("2")]).role("summary_row")
            ]),
        )
        .group(group(vec![]).summary(vec![
            row(vec![cell("All").stub(), cell("1"), cell("2")]).role("grand_summary_row"),
        ]))
        .build();
    let l = layout(&t, &SvgConfig::default());
    // The group label: full-width 2px rules above and below.
    let label = l
        .cells
        .iter()
        .find(|c| c.text.lines[0].text() == "G")
        .unwrap();
    let full: Vec<_> = hrules(&l, LIGHT, 2.0)
        .into_iter()
        .filter(|r| full_width(&l, r))
        .collect();
    assert!(full.iter().any(|r| (r.y1 - label.y).abs() < EPS));
    assert!(full
        .iter()
        .any(|r| (r.y1 - (label.y + label.height)).abs() < EPS));
    // The stub's right border beside body cells only (not the head or label).
    let stub_rules: Vec<_> = l.rules.iter().filter(|r| r.x1 == r.x2).collect();
    assert_eq!(stub_rules.len(), 3, "r, Total and All: {stub_rules:?}");
    assert!(stub_rules
        .iter()
        .all(|r| r.y1 >= label.y + label.height - EPS));
    // The spanner's rule: flush at the right edge (last column), inset on the left.
    let spanner = l
        .cells
        .iter()
        .find(|c| c.text.lines.first().is_some_and(|x| x.text() == "Group"))
        .unwrap();
    let sr = hrules(&l, LIGHT, 2.0)
        .into_iter()
        .find(|r| r.y1 > spanner.y + spanner.height / 2.0 && r.y1 < spanner.y + spanner.height)
        .expect("spanner rule inside the spanner cell");
    assert!((sr.x1 - (spanner.x + 4.0)).abs() < EPS && (sr.x2 - l.table_width).abs() < EPS);
    // The grand summary's double rule.
    let double: Vec<_> = l.rules.iter().filter(|r| r.double).collect();
    assert!(!double.is_empty() && double.iter().all(|r| (r.width - 6.0).abs() < EPS));
    assert_ok(check("parts", &t));
}

#[test]
fn the_table_widens_to_its_heading_and_to_a_fixed_width() {
    let t = TableBuilder::new(1)
        .title("A heading much wider than the column")
        .body(vec![row(vec![cell("x")])])
        .build();
    let l = layout(&t, &SvgConfig::default());
    assert_eq!(l.header[0].lines.len(), 1, "no wrap below the cap");
    assert!(
        l.cells[0].width > 200.0,
        "the column takes the heading's width"
    );
    assert!((l.cells[0].width - l.table_width).abs() < EPS);
    assert_ok(check("wide heading", &t));

    let mut t = TableBuilder::new(2)
        .body(vec![row(vec![cell("a"), cell("b")])])
        .build();
    t.config.table_width = Some("400px".into());
    let l = layout(&t, &SvgConfig::default());
    assert!((l.table_width - 400.0).abs() < EPS, "{}", l.table_width);
    assert_ok(check("fixed width", &t));
}

#[test]
fn cell_borders_and_padding_from_styles_win() {
    use gridwell_testkit::CellExt;
    let bordered: gridwell_ir::style::StyleDef = serde_json::from_value(serde_json::json!({
        "padding": { "top": "1px", "right": "1px", "bottom": "1px", "left": "1px" },
        "border": { "bottom": { "width": "3px", "style": "dashed", "color": "#FF0000" },
                    "top": { "style": "none" } }
    }))
    .unwrap();
    let t = TableBuilder::new(1)
        .style_def("b", bordered)
        .body(vec![
            row(vec![cell("w")]),
            row(vec![cell("x").style("b")]),
            row(vec![cell("y")]),
        ])
        .build();
    let l = layout(&t, &SvgConfig::default());
    let find = |text: &str| {
        l.cells
            .iter()
            .find(|c| c.text.lines[0].text() == text)
            .unwrap()
    };
    let x = find("x");
    // Its own padding (1px) on the left: text starts at 1px.
    assert!((x.text.x - 1.0).abs() < EPS, "{}", x.text.x);
    // Its dashed red bottom border.
    let red: Vec<_> = l.rules.iter().filter(|r| r.color == "#FF0000").collect();
    assert_eq!(red.len(), 1);
    assert_eq!(red[0].dash, Some(9.0));
    assert!((red[0].y1 - (x.y + x.height)).abs() < EPS);
    // The hairline above it (its own top border) is gone: removed by its style.
    assert!(
        !l.rules
            .iter()
            .any(|r| r.y1 == r.y2 && r.color == LIGHT && (r.y1 - x.y).abs() < EPS),
        "{:?}",
        l.rules
    );
    // The boundary below is as thick as the widest border on it (3px, not the 1px
    // hairline): half of it is in `y`'s box.
    let y = find("y");
    assert!(
        (y.text.y - (y.y + 1.5 + 8.0)).abs() < EPS,
        "{} {}",
        y.text.y,
        y.y
    );
    assert_ok(check("borders", &t));
}

#[test]
fn base_styles_off_draws_no_rules_and_no_color() {
    let t = examples()
        .into_iter()
        .find(|e| e.name == "regional-sales")
        .unwrap()
        .table();
    let config = SvgConfig {
        base_styles: false,
        ..SvgConfig::default()
    };
    let l = layout(&t, &config);
    assert!(l.rules.is_empty(), "{:?}", &l.rules[..1]);
    let svg = render(&t, &config).unwrap();
    assert!(!svg.contains("fill: #333333"), "{svg}");
    assert_ok(check("plain", &t));
    // The default draws them.
    assert!(!layout(&t, &SvgConfig::default()).rules.is_empty());
}

#[test]
fn a_removed_top_border_drops_hairlines_but_not_section_rules() {
    use gridwell_testkit::RowExt;
    // gt's `table_body.hlines.style = "none"` arrives as a row style removing the
    // top border.
    let none: gridwell_ir::style::StyleDef =
        serde_json::from_value(serde_json::json!({ "border": { "top": { "style": "none" } } }))
            .unwrap();
    let t = TableBuilder::new(1)
        .style_def("nohl", none)
        .head(row(vec![cell("H")]))
        .body(vec![
            row(vec![cell("a")]).style("nohl"),
            row(vec![cell("b")]).style("nohl"),
            row(vec![cell("c")]).style("nohl"),
        ])
        .build();
    let l = layout(&t, &SvgConfig::default());
    assert!(
        hrules(&l, LIGHT, 1.0).is_empty(),
        "hairlines drawn: {:?}",
        l.rules
    );
    // Table top, under the head, table bottom: still there.
    assert_eq!(hrules(&l, LIGHT, 2.0).len(), 3, "{:?}", l.rules);
    // Without the style, the two hairlines are back.
    let mut plain = t.clone();
    for r in &mut plain.table.tbody[0].rows {
        r.style_id = None;
    }
    assert_eq!(
        hrules(&layout(&plain, &SvgConfig::default()), LIGHT, 1.0).len(),
        2
    );
}
