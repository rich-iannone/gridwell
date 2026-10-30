//! Cell borders: a horizontal edge is a line of its own between two rows (an
//! `\omit` / `\multispan` cell per run of columns, filled with leaders), a
//! vertical one a `!{\vrule …}` in the cell's `\multicolumn` spec. Where two
//! cells' borders meet, one edge is drawn: the wider, then the more prominent
//! style, then the first.

use gridwell_ir::style::{Border, BorderSet};
use gridwell_ir::{StyleDef, Table};
use gridwell_testkit::examples::styled_merge;
use gridwell_testkit::{border, border_all, cell, examples, row, CellExt, TableBuilder};
use gridwell_writer_latex::{render_latex, LatexWriter, LatexWriterConfig};

/// The tabular's lines.
fn grid_lines(src: &str) -> Vec<&str> {
    src.lines()
        .skip_while(|l| !l.starts_with("\\begin{"))
        .skip(1)
        .take_while(|l| !l.starts_with("\\end{"))
        .collect()
}

fn is_edge_line(line: &str) -> bool {
    line.starts_with("\\omit") || line.starts_with("\\multispan")
}

fn sides(top: bool, right: bool, bottom: bool, left: bool, b: Border) -> BorderSet {
    let on = |set: bool| set.then(|| b.clone());
    BorderSet {
        top: on(top),
        right: on(right),
        bottom: on(bottom),
        left: on(left),
    }
}

fn styled(border: BorderSet) -> StyleDef {
    StyleDef {
        border: Some(border),
        ..Default::default()
    }
}

/// Two rows of two cells, `a b` over `c d`, with the given styles.
fn two_by_two(styles: Vec<(&str, StyleDef)>, cells: [Option<&str>; 4]) -> Table {
    let mut tb = TableBuilder::new(2);
    for (id, def) in styles {
        tb = tb.style_def(id, def);
    }
    let mk = |text: &str, style: Option<&str>| match style {
        Some(s) => cell(text).style(s),
        None => cell(text),
    };
    tb.head(row(vec![cell("H1"), cell("H2")]))
        .body(vec![
            row(vec![mk("a", cells[0]), mk("b", cells[1])]),
            row(vec![mk("c", cells[2]), mk("d", cells[3])]),
        ])
        .build()
}

#[test]
fn a_boxed_merge_and_a_dashed_label() {
    let src = render_latex(&styled_merge()).unwrap();
    let green = "\\color[HTML]{00AA00}";
    let blue = "\\color[HTML]{0000BB}";
    let dashes = "\\xleaders\\hbox to 4.5pt{\\hss\\vrule height 0.75pt width 2.25pt\\hss}\\hfill";
    let gbar = format!("!{{{{{green}\\vrule width 4.5pt}}}}");
    let bbar = format!("!{{{{{blue}\\vrule width 0.75pt}}}}");
    let expected = [
        "\\toprule".to_string(),
        "H1 & H2 & H3 \\\\".into(),
        "\\midrule".into(),
        // The label's top and bottom, each one line over all three columns.
        format!("\\multispan{{3}}{{{blue}{dashes}}}\\cr"),
        format!("\\multicolumn{{3}}{{{bbar}l{bbar}}}{{\\bfseries Group}} \\\\"),
        format!("\\multispan{{3}}{{{blue}{dashes}}}\\cr"),
        "\\midrule".into(),
        // The merge's top and bottom over its two columns, sides on both rows;
        // `c` and `d` have none, so their column is left out of the lines.
        format!("\\multispan{{2}}{{{green}\\leaders\\hrule height 4.5pt\\hfill}}\\cr"),
        format!("\\multicolumn{{2}}{{{gbar}l{gbar}}}{{\\multirow{{2}}{{*}}{{M}}}} & c \\\\"),
        format!("\\multicolumn{{2}}{{{gbar}l{gbar}}}{{}} & d \\\\"),
        format!("\\multispan{{2}}{{{green}\\leaders\\hrule height 4.5pt\\hfill}}\\cr"),
        "\\bottomrule".into(),
    ];
    assert_eq!(grid_lines(&src), expected, "{src}");
}

#[test]
fn edges_meeting_without_a_rule_are_one_line() {
    // No midrule between the label and its rows: the label's bottom and the
    // merge's top meet. The wider green wins over the merge's columns; the
    // label's thinner dashes, alone over `c`, are centered in the line.
    let cfg = LatexWriterConfig {
        booktabs: false,
        longtable: true,
        ..Default::default()
    };
    let src = LatexWriter::with_config(cfg)
        .render(&styled_merge())
        .unwrap();
    let lines = grid_lines(&src);
    let label = lines.iter().position(|l| l.contains("Group")).unwrap();
    assert_eq!(
        lines[label + 1],
        "\\multispan{2}{\\color[HTML]{00AA00}\\leaders\\hrule height 4.5pt\\hfill}\
         &\\omit{\\color[HTML]{0000BB}\\xleaders\\hbox to 4.5pt\
         {\\hss\\vrule height 2.625pt depth -1.875pt width 2.25pt\\hss}\\hfill}\\cr",
        "{src}"
    );
    assert!(lines[label + 2].contains("{M}"), "{src}");
    // Before `\endhead` and the closing rule, nothing is left over.
    assert_eq!(lines.iter().filter(|l| is_edge_line(l)).count(), 3, "{src}");
}

#[test]
fn the_wider_edge_wins_then_the_more_prominent_style() {
    let thin = styled(sides(
        false,
        true,
        true,
        false,
        border("1px", "solid", "#FF0000"),
    ));
    let wide = styled(sides(
        true,
        false,
        false,
        true,
        border("2px", "dashed", "#0000FF"),
    ));
    // `a`'s bottom meets `c`'s top; `a`'s right meets `b`'s left.
    let src = render_latex(&two_by_two(
        vec![("thin", thin), ("wide", wide)],
        [Some("thin"), Some("wide"), Some("wide"), None],
    ))
    .unwrap();
    let lines = grid_lines(&src);
    let edge_lines: Vec<&&str> = lines.iter().filter(|l| is_edge_line(l)).collect();
    // Above `a b`: only `b`'s top. Between the rows: `c`'s wider top only.
    assert_eq!(edge_lines.len(), 2, "{src}");
    assert!(
        edge_lines[0].starts_with("\\omit&\\omit{\\color[HTML]{0000FF}"),
        "{src}"
    );
    assert!(
        edge_lines[1].starts_with("\\omit{\\color[HTML]{0000FF}\\xleaders")
            && !edge_lines[1].contains("FF0000"),
        "{src}"
    );
    // The boundary between `a` and `b` is drawn once, by `a`, in blue.
    let first = lines.iter().find(|l| l.contains("{a}")).unwrap();
    assert_eq!(
        *first, "\\multicolumn{1}{l!{{\\color[HTML]{0000FF}\\vrule width 1.5pt}}}{a} & b \\\\",
        "{src}"
    );

    // Equal widths: solid beats dashed, whichever comes first.
    for (upper, lower) in [("dashed", "solid"), ("solid", "dashed")] {
        let up = styled(sides(
            false,
            false,
            true,
            false,
            border("1px", upper, "#111111"),
        ));
        let down = styled(sides(
            true,
            false,
            false,
            false,
            border("1px", lower, "#222222"),
        ));
        let src = render_latex(&two_by_two(
            vec![("up", up), ("down", down)],
            [Some("up"), None, Some("down"), None],
        ))
        .unwrap();
        let solid = if upper == "solid" { "111111" } else { "222222" };
        let edge = grid_lines(&src)
            .into_iter()
            .find(|l| is_edge_line(l))
            .unwrap();
        assert!(
            edge.contains(solid) && edge.contains("\\leaders\\hrule"),
            "{src}"
        );
    }
    // Fully equal: the first (above) wins.
    let up = styled(sides(
        false,
        false,
        true,
        false,
        border("1px", "solid", "#111111"),
    ));
    let down = styled(sides(
        true,
        false,
        false,
        false,
        border("1px", "solid", "#222222"),
    ));
    let src = render_latex(&two_by_two(
        vec![("up", up), ("down", down)],
        [Some("up"), None, Some("down"), None],
    ))
    .unwrap();
    assert!(src.contains("111111") && !src.contains("222222"), "{src}");
}

#[test]
fn dashes_dots_and_double_lines() {
    let src = |style: &str| {
        let def = styled(sides(
            false,
            true,
            true,
            false,
            border("4px", style, "#000000"),
        ));
        render_latex(&two_by_two(vec![("s", def)], [Some("s"), None, None, None])).unwrap()
    };
    // 4px is 3pt: dashes 9pt long every 18pt, dots 3pt every 6pt.
    let dashed = src("dashed");
    assert!(
        dashed.contains("\\xleaders\\hbox to 18pt{\\hss\\vrule height 3pt width 9pt\\hss}"),
        "{dashed}"
    );
    let dotted = src("dotted");
    assert!(
        dotted.contains("\\xleaders\\hbox to 6pt{\\hss\\vrule height 3pt width 3pt\\hss}"),
        "{dotted}"
    );
    // A rule can't be dashed: the right side is solid in both.
    for s in [&dashed, &dotted] {
        assert!(
            s.contains("l!{{\\color[HTML]{000000}\\vrule width 3pt}}}{a}"),
            "{s}"
        );
    }
    // Double: two 1pt lines 1pt apart, both ways.
    let double = src("double");
    let lines = grid_lines(&double);
    let i = lines.iter().position(|l| is_edge_line(l)).unwrap();
    let line = "\\omit{\\color[HTML]{000000}\\leaders\\hrule height 1pt\\hfill}\\cr";
    assert_eq!(
        lines[i..i + 3],
        [line, "\\noalign{\\vskip 1pt}", line],
        "{double}"
    );
    assert!(
        double.contains("!{{\\color[HTML]{000000}\\vrule width 1pt\\hskip 1pt\\vrule width 1pt}}"),
        "{double}"
    );
}

#[test]
fn bottoms_are_drawn_before_the_closing_rule() {
    for ex in ["borders-solid", "borders-dashed", "borders-double"] {
        let t = examples()
            .into_iter()
            .find(|e| e.name == ex)
            .unwrap()
            .table();
        for booktabs in [true, false] {
            let cfg = LatexWriterConfig {
                booktabs,
                ..Default::default()
            };
            let src = LatexWriter::with_config(cfg).render(&t).unwrap();
            let lines = grid_lines(&src);
            let n = lines.len();
            assert!(is_edge_line(lines[n - 2]), "{ex}: {src}");
            // Every row of cells has a line above and below it.
            for (i, l) in lines.iter().enumerate() {
                if l.ends_with("\\\\") {
                    let edge =
                        |j: usize| is_edge_line(lines[j]) || lines[j].starts_with("\\noalign");
                    assert!(edge(i - 1) && edge(i + 1), "{ex}: row {i}\n{src}");
                }
            }
        }
    }
}

#[test]
fn tables_without_borders_draw_none() {
    for ex in examples() {
        let mut t = ex.table();
        for def in t.styles.defs.values_mut() {
            def.border = None;
        }
        for c in &mut t.styles.conditionals {
            c.style.border = None;
        }
        for c in t.styles.compositions.values_mut() {
            c.overrides.border = None;
        }
        let src = render_latex(&t).unwrap();
        assert!(
            !src.contains("\\omit") && !src.contains("\\multispan") && !src.contains("!{"),
            "{}: {src}",
            ex.name
        );
    }
}

#[test]
fn every_side_is_read_from_its_own_border() {
    // `a`'s four sides each in their own color: top and bottom lines, left and
    // right rules, each in the right place.
    let def = StyleDef {
        border: Some(BorderSet {
            top: Some(border("1px", "solid", "#000001")),
            right: Some(border("1px", "solid", "#000002")),
            bottom: Some(border("1px", "solid", "#000003")),
            left: Some(border("1px", "solid", "#000004")),
        }),
        ..Default::default()
    };
    let src = render_latex(&two_by_two(vec![("s", def)], [Some("s"), None, None, None])).unwrap();
    let lines = grid_lines(&src);
    let a = lines.iter().position(|l| l.contains("{a}")).unwrap();
    assert!(lines[a - 1].contains("000001"), "{src}");
    assert!(lines[a + 1].contains("000003"), "{src}");
    assert_eq!(
        lines[a],
        "\\multicolumn{1}{!{{\\color[HTML]{000004}\\vrule width 0.75pt}}l\
         !{{\\color[HTML]{000002}\\vrule width 0.75pt}}}{a} & b \\\\",
        "{src}"
    );
    // All four sides of `border_all` together: closed box.
    let def = styled(border_all(border("1px", "solid", "#000000")));
    let src = render_latex(&two_by_two(vec![("s", def)], [Some("s"); 4])).unwrap();
    assert_eq!(
        grid_lines(&src).iter().filter(|l| is_edge_line(l)).count(),
        3,
        "{src}"
    );
}
