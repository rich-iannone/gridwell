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
