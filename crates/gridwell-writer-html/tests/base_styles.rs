//! The base stylesheet (gridwell's default look): on by default, off with
//! `base_styles: false`, never with inline styles, and nothing else. The default
//! output is exactly the bare output plus the base rules, and every base rule has
//! zero specificity, so the table's own styles and the host page's CSS win.

use gridwell_ir::Table;
use gridwell_testkit::examples;
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};

fn render(t: &Table, base_styles: bool, inline_styles: bool, prefix: &str) -> String {
    HtmlWriter::with_config(HtmlWriterConfig {
        base_styles,
        inline_styles,
        class_prefix: prefix.into(),
        ..HtmlWriterConfig::default()
    })
    .render(t)
    .unwrap()
}

fn is_base_rule(line: &str) -> bool {
    line.trim_start().starts_with(":where(")
}

/// The default output with the base rules taken out (and the `<style>` block too,
/// if the base rules were all it held).
fn without_base(html: &str) -> String {
    let lines: Vec<&str> = html.lines().filter(|l| !is_base_rule(l)).collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "<style>" && lines.get(i + 1).map(|l| l.trim()) == Some("</style>") {
            i += 2;
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }
    out.join("\n") + if html.ends_with('\n') { "\n" } else { "" }
}

#[test]
fn default_output_is_the_bare_output_plus_the_base_rules() {
    for ex in examples() {
        let t = ex.table();
        for prefix in ["gw", "zz"] {
            let default = render(&t, true, false, prefix);
            let bare = render(&t, false, false, prefix);
            assert!(!bare.lines().any(is_base_rule), "{}", ex.name);
            assert_eq!(without_base(&default), bare, "{} ({prefix})", ex.name);
            // The base rules come first in the style block, before any table rule.
            let style: Vec<&str> = default
                .lines()
                .skip_while(|l| l.trim() != "<style>")
                .skip(1)
                .take_while(|l| l.trim() != "</style>")
                .collect();
            let first_table_rule = style.iter().position(|l| !is_base_rule(l));
            let last_base_rule = style.iter().rposition(|l| is_base_rule(l)).unwrap();
            if let Some(i) = first_table_rule {
                assert!(i > last_base_rule, "{}: base rules not first", ex.name);
            }
        }
    }
}

#[test]
fn every_base_rule_has_zero_specificity_and_uses_the_prefix() {
    let t = examples()[0].table();
    let html = render(&t, true, false, "zz");
    let bare = render(&t, false, false, "zz");
    // Every line the base stylesheet adds, whatever it looks like.
    let rules: Vec<&str> = html
        .lines()
        .filter(|l| !bare.lines().any(|b| b == *l))
        .filter(|l| !matches!(l.trim(), "<style>" | "</style>"))
        .collect();
    assert!(rules.len() >= 15, "{}", rules.len());
    for rule in &rules {
        let rule = rule.trim();
        let (selector, body) = rule.split_once(" { ").unwrap();
        // The whole selector is one or more `:where(…)` lists: no part outside one
        // (which would add specificity).
        let mut rest = selector;
        while !rest.is_empty() {
            let inner = rest
                .strip_prefix(":where(")
                .unwrap_or_else(|| panic!("{rule}"));
            let mut depth = 1;
            let end = inner
                .char_indices()
                .find(|&(_, c)| {
                    depth += match c {
                        '(' => 1,
                        ')' => -1,
                        _ => 0,
                    };
                    depth == 0
                })
                .map(|(i, _)| i)
                .unwrap_or_else(|| panic!("unbalanced: {rule}"));
            rest = inner[end + 1..].trim_start_matches([' ', '~', '+', '>']);
        }
        assert!(body.ends_with(" }"), "{rule}");
        assert!(!selector.contains(".gw_"), "default prefix in {rule}");
        assert!(selector.contains(".zz_"), "{rule}");
    }
}

#[test]
fn inline_styles_never_get_base_styles() {
    for ex in examples() {
        let t = ex.table();
        let a = render(&t, true, true, "gw");
        let b = render(&t, false, true, "gw");
        assert_eq!(a, b, "{}", ex.name);
        assert!(!a.contains("<style>"), "{}", ex.name);
    }
}

#[test]
fn a_table_without_styles_gets_a_style_block_only_for_the_base_rules() {
    let t = gridwell_testkit::TableBuilder::new(1)
        .body(vec![gridwell_testkit::row(vec![gridwell_testkit::cell(
            "x",
        )])])
        .build();
    assert!(!render(&t, false, false, "gw").contains("<style>"));
    let html = render(&t, true, false, "gw");
    assert_eq!(html.matches("<style>").count(), 1, "{html}");
}

#[test]
fn the_option_is_on_by_default_and_parses_from_json() {
    assert!(HtmlWriterConfig::default().base_styles);
    let c: HtmlWriterConfig = serde_json::from_str(r#"{"base_styles": false}"#).unwrap();
    assert!(!c.base_styles);
    let c: HtmlWriterConfig = serde_json::from_str("{}").unwrap();
    assert!(c.base_styles);
}

fn row_border_table(border: serde_json::Value) -> Table {
    let style: gridwell_ir::style::StyleDef =
        serde_json::from_value(serde_json::json!({ "border": border })).unwrap();
    gridwell_testkit::TableBuilder::new(2)
        .style_def("rb", style)
        .head(gridwell_testkit::row(vec![
            gridwell_testkit::cell("A"),
            gridwell_testkit::cell("B"),
        ]))
        .body(vec![
            gridwell_testkit::row(vec![
                gridwell_testkit::cell("1"),
                gridwell_testkit::cell("2"),
            ]),
            gridwell_testkit::RowExt::style(
                gridwell_testkit::row(vec![
                    gridwell_testkit::cell("3"),
                    gridwell_testkit::cell("4"),
                ]),
                "rb",
            ),
        ])
        .build()
}

#[test]
fn a_removed_border_removes_the_base_hairline_too() {
    // `none` on a row: the cells carry `border-top: none` (a `<tr>` border would
    // lose to the base stylesheet's hairline on the cells).
    let t = row_border_table(serde_json::json!({ "top": { "style": "none" } }));
    let html = render(&t, true, false, "gw");
    assert!(
        html.contains(".gw_table .gw__c0 { border-top: none }"),
        "{html}"
    );
    assert!(
        html.contains("<td class=\"gw__c0\" headers=\"col_0\">3</td>"),
        "{html}"
    );
    assert!(
        html.contains("<td class=\"gw__c0\" headers=\"col_1\">4</td>"),
        "{html}"
    );
    assert!(html.contains("<td headers=\"col_0\">1</td>"), "{html}");
    // Inline: on the cells' own `style`.
    let inline = render(&t, true, true, "gw");
    assert!(
        inline.contains("<td style=\"border-top: none\" headers=\"col_0\">3</td>"),
        "{inline}"
    );
    // `hidden` counts as removed; an unset side doesn't.
    let t = row_border_table(serde_json::json!({ "bottom": { "style": "hidden" } }));
    let html = render(&t, true, false, "gw");
    assert!(
        html.contains(".gw_table .gw__c0 { border-bottom: none }"),
        "{html}"
    );
}

#[test]
fn a_drawn_row_border_is_carried_by_its_cells() {
    // A thin dashed rule on a row would lose the collapsed-border conflict to the
    // base 1px solid hairline if it were only on the `<tr>`.
    let t = row_border_table(
        serde_json::json!({ "top": { "style": "dashed", "width": "1px", "color": "#FF0000" } }),
    );
    let html = render(&t, true, false, "gw");
    let rule = html
        .lines()
        .find(|l| l.contains(".gw__c0 {"))
        .unwrap_or_else(|| panic!("no generated class:\n{html}"));
    assert!(rule.contains("border-top: 1px dashed #FF0000"), "{rule}");
    assert!(
        html.contains("<td class=\"gw__c0\" headers=\"col_0\">3</td>"),
        "{html}"
    );
}

/// Two columns; head cell "A" (and, with `body_top`, body cell "1") sets its
/// own 1px border on the head's edge.
fn head_edge_table(body_top: bool) -> Table {
    use gridwell_testkit::{cell, row, CellExt, TableBuilder};
    let side = |s: &str| -> gridwell_ir::style::StyleDef {
        serde_json::from_value(serde_json::json!({
            "border": { s: { "style": "solid", "width": "1px", "color": "#333333" } }
        }))
        .unwrap()
    };
    let one = if body_top {
        cell("1").style("t")
    } else {
        cell("1")
    };
    TableBuilder::new(2)
        .style_def("b", side("bottom"))
        .style_def("t", side("top"))
        .head(row(vec![cell("A").style("b"), cell("B")]))
        .body(vec![row(vec![one, cell("2")])])
        .build()
}

#[test]
fn an_edge_with_a_cell_border_is_drawn_by_its_cells() {
    // In the collapsed-border model the thead's 2px rule would beat "A"'s 1px
    // border: the edge is the cells', and the others carry the rule.
    let html = render(&head_edge_table(false), true, false, "gw");
    assert!(
        html.contains("<table class=\"gw_table gw_cells_head\">"),
        "{html}"
    );
    let line = |text: &str| {
        html.lines()
            .find(|l| l.contains(&format!(">{text}</")))
            .unwrap_or_else(|| panic!("no {text}:\n{html}"))
    };
    assert!(!line("A").contains("_rule_"), "{}", line("A"));
    assert!(line("B").contains("gw_rule_head"), "{}", line("B"));
    // "1" faces "A"'s border; "2" carries the rule too.
    assert!(!line("1").contains("_rule_"), "{}", line("1"));
    assert!(
        line("2").contains("class=\"gw_rule_head\""),
        "{}",
        line("2")
    );
    // The markup doesn't depend on the base stylesheet.
    let bare = render(&head_edge_table(false), false, false, "gw");
    assert!(bare.contains("gw_cells_head") && bare.contains("gw_rule_head"));

    let html = without_base(&render(&head_edge_table(true), true, false, "zz"));
    assert!(html.contains("zz_cells_head") && html.contains("zz_rule_head"));
    assert!(html.contains("<td class=\"zz_style-t\" headers=\"col_0\">1</td>"));

    // Inline styles have no base stylesheet to switch off.
    let html = render(&head_edge_table(false), false, true, "gw");
    assert!(
        !html.contains("_cells_") && !html.contains("_rule_"),
        "{html}"
    );

    // Without cell borders, no edge is the cells'.
    for ex in examples()
        .iter()
        .filter(|e| !e.name.starts_with("borders-"))
    {
        let html = without_base(&render(&ex.table(), true, false, "gw"));
        assert!(!html.contains("gw_cells_"), "{}", ex.name);
    }
}

#[test]
fn summaries_at_the_top_come_first_and_mark_their_end() {
    use gridwell_testkit::{cell, group, labeled_group, row, CellExt, RowExt, TableBuilder};
    let sum = |label: &str, role: &str| row(vec![cell(label).stub(), cell("9")]).role(role);
    let t = TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell(""), cell("A")]))
        .group(
            group(vec![])
                .summary(vec![sum("All", "grand_summary")])
                .summary_top(),
        )
        .group(
            labeled_group("Gee", vec![row(vec![cell("x").stub(), cell("1")])])
                .summary(vec![sum("Sum", "summary"), sum("Max", "summary")])
                .summary_top(),
        )
        .build();
    let html = render(&t, true, false, "gw");
    let at = |text: &str| {
        html.find(&format!(">{text}</"))
            .unwrap_or_else(|| panic!("no {text}:\n{html}"))
    };
    let label = html.find("Gee").unwrap();
    assert!(at("All") < label && label < at("Sum") && at("Sum") < at("Max"));
    assert!(at("Max") < at("x"), "{html}");
    for (class, n) in [
        (
            "<tr class=\"gw_grand_summary gw_summary_top gw_summary_end\">",
            1,
        ),
        ("<tr class=\"gw_summary gw_summary_top\">", 1),
        ("<tr class=\"gw_summary gw_summary_top gw_summary_end\">", 1),
    ] {
        assert_eq!(html.matches(class).count(), n, "{class}\n{html}");
    }
    // At the bottom, summary rows have no marks.
    let ex = examples().into_iter().find(|e| e.name == "group-summary");
    let bottom = without_base(&render(&ex.unwrap().table(), true, false, "gw"));
    assert!(bottom.contains("gw_summary") && !bottom.contains("_summary_top"));
    assert!(!bottom.contains("_summary_end"), "{bottom}");
}

#[test]
fn the_tables_rules_are_its_base_rules() {
    use gridwell_testkit::{cell, labeled_group, row, TableBuilder};
    let t = TableBuilder::new(1)
        .head(row(vec![cell("A")]))
        .group(labeled_group("G", vec![row(vec![cell("1")])]))
        .config(|c| {
            c.rules = Some(
                serde_json::from_value(serde_json::json!({
                    "group": { "width": "3px", "style": "dotted", "color": "#123456" },
                    "rows": { "style": "none" },
                }))
                .unwrap(),
            );
        })
        .build();
    let html = render(&t, true, false, "gw");
    assert!(
        html.contains(
            ":where(.gw_group_label) { border-top: 3px dotted #123456; \
             border-bottom: 3px dotted #123456 }"
        ),
        "{html}"
    );
    assert!(
        html.contains(":where(.gw_table td, .gw_table th) { padding: 8px 5px; border-top: none }")
    );
    // Only the base stylesheet changes.
    let plain = TableBuilder::new(1)
        .head(row(vec![cell("A")]))
        .group(labeled_group("G", vec![row(vec![cell("1")])]))
        .build();
    assert_eq!(
        without_base(&html),
        without_base(&render(&plain, true, false, "gw"))
    );
}
