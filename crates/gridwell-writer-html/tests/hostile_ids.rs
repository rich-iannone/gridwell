//! Style ids never reach markup unless they are plain names. Valid IR can't hold
//! any other (`INVALID_STYLE_ID`), but the writer is safe even when handed
//! unvalidated IR: a hostile id gets a generated class, in every place a style id
//! can appear and in both style modes. Also: style classes have their own
//! namespace (`gw_style-<id>`), so a style named like a part (`title`) can't style
//! that part, and a bad `class_prefix` is refused.

use gridwell_ir::{is_valid_style_id, Table};
use gridwell_testkit::{cell, labeled_group, row, styled, CellExt, RowExt, TableBuilder};
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig, RenderError};
use scraper::{Html, Selector};

const HOSTILE: &[&str] = &[
    "x\"><script>alert(1)</script>",
    "x\" onmouseover=\"alert(1)",
    "</style><script>alert(1)</script><style>",
    "a{color:red}",
    "a,body",
    "a b",
    "",
    " ",
    "é",
    "x'y",
    "a<b",
];

fn bold() -> gridwell_ir::style::StyleDef {
    serde_json::from_value(serde_json::json!({ "font_weight": "bold" })).unwrap()
}

/// A table that uses `id` everywhere a style id can go: a def, a composition
/// extending it (also hostile), and references from a cell, a row, the title, a
/// footnote, a source note, a group label and a styled run.
fn table_using(id: &str) -> Table {
    let comp = format!("{id}-c");
    let mut t = TableBuilder::new(2)
        .stub_cols(1)
        .title("T")
        .style_def(id, bold())
        .composition(&comp, id, Default::default())
        .head(row(vec![cell("S"), cell("A")]))
        .group(
            labeled_group(
                "G",
                vec![
                    row(vec![cell("r").stub(), cell("1").style(id)]).style(&comp),
                    row(vec![
                        cell("q").stub(),
                        gridwell_testkit::cell_content(vec![styled("run", id)]),
                    ]),
                ],
            )
            .label_style(id),
        )
        .footnote("f", "1", "note")
        .source_note("src")
        .build();
    let header = t.header.as_mut().unwrap();
    header.title.as_mut().unwrap().style_id = Some(comp.clone());
    let footer = t.footer.as_mut().unwrap();
    footer.footnotes[0].style_id = Some(id.to_string());
    footer.source_notes[0].style_id = Some(comp);
    t
}

fn render(t: &Table, inline_styles: bool) -> String {
    HtmlWriter::with_config(HtmlWriterConfig {
        inline_styles,
        ..HtmlWriterConfig::default()
    })
    .render(t)
    .unwrap()
}

fn is_plain(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The base stylesheet's rules (the same for every table with the default prefix),
/// from a harmless table.
fn base_rules() -> Vec<String> {
    render(&table_using("ok"), false)
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with(":where("))
        .map(str::to_string)
        .collect()
}

/// The structure an attacker would need to change: elements, attributes, class
/// tokens, and the `<style>` block's rules (exactly the base rules, then plain
/// class selectors).
fn assert_clean(html: &str, what: &str) {
    let base = base_rules();
    let doc = Html::parse_fragment(html);
    let all = Selector::parse("*").unwrap();
    let allowed_tags = [
        "html", "div", "style", "table", "thead", "tbody", "tr", "th", "td", "p", "sup", "a",
        "span",
    ];
    let allowed_attrs = [
        "class",
        "style",
        "role",
        "aria-level",
        "scope",
        "colspan",
        "rowspan",
        "id",
        "href",
    ];
    let mut styles = 0;
    for el in doc.select(&all) {
        let v = el.value();
        assert!(
            allowed_tags.contains(&v.name()),
            "{what}: element <{}>:\n{html}",
            v.name()
        );
        for (name, value) in v.attrs() {
            assert!(
                allowed_attrs.contains(&name),
                "{what}: attribute {name}:\n{html}"
            );
            if name == "class" {
                for token in value.split_whitespace() {
                    assert!(is_plain(token), "{what}: class token {token:?}:\n{html}");
                }
            }
        }
        if v.name() == "style" {
            styles += 1;
            for line in el.text().collect::<String>().lines().map(str::trim) {
                if line.is_empty() || base.iter().any(|b| b == line) {
                    continue;
                }
                // `.a { … }` or `.a .b { … }`, plain class names only.
                let (selector, body) = line
                    .split_once(" { ")
                    .unwrap_or_else(|| panic!("{what}: style line {line:?}:\n{html}"));
                assert!(
                    body.ends_with(" }") && !body.contains('{'),
                    "{what}: {line:?}"
                );
                for part in selector.split(' ') {
                    let class = part
                        .strip_prefix('.')
                        .unwrap_or_else(|| panic!("{what}: selector {selector:?}"));
                    assert!(is_plain(class), "{what}: selector {selector:?}");
                }
            }
        }
    }
    assert!(styles <= 1, "{what}: {styles} <style> elements:\n{html}");
}

#[test]
fn hostile_style_ids_never_reach_markup() {
    for id in HOSTILE {
        assert!(!is_valid_style_id(id), "{id:?} is meant to be invalid");
        let t = table_using(id);
        // Valid IR can't hold them.
        assert!(
            t.validate()
                .iter()
                .any(|e| e.rule == gridwell_ir::ValidationRule::InvalidStyleId),
            "{id:?}"
        );
        for inline in [false, true] {
            let html = render(&t, inline);
            assert_clean(&html, &format!("{id:?} inline={inline}"));
            // The raw id is nowhere in the output (escaped text aside, ids never
            // appear as text).
            if id.trim().len() > 1 {
                assert!(!html.contains(id), "{id:?} inline={inline}:\n{html}");
            }
        }
    }
}

#[test]
fn hostile_ids_still_style_their_cells() {
    // The def is the first unsafe id in sorted order (its composition sorts
    // after it), so it is `gw__s0`; the cell that uses it has that class, and the
    // rule applies bold.
    let t = table_using("a b");
    let html = render(&t, false);
    assert!(html.contains(".gw__s0 { font-weight: bold }"), "{html}");
    assert!(html.contains(".gw__s1 { font-weight: bold }"), "{html}");
    let doc = Html::parse_fragment(&html);
    let n = |s: &str| doc.select(&Selector::parse(s).unwrap()).count();
    assert_eq!(n("td.gw__s0"), 2, "{html}"); // the cell and the group label
    assert_eq!(n("td.gw_group_label.gw__s0"), 1, "{html}");
    assert_eq!(n("p.gw_footnote.gw__s0"), 1, "{html}");
    assert_eq!(n("p.gw_source_note.gw__s1"), 1, "{html}");
    assert_eq!(n("span.gw__s0"), 1, "{html}"); // the styled run
    assert_eq!(n("tr.gw__s1"), 1, "{html}"); // the row, via the composition
    assert_eq!(n("div.gw_title.gw__s1"), 1, "{html}");
    // With inline styles, the style is applied inline as usual.
    let inline = render(&t, true);
    assert!(
        inline.contains("<span style=\"font-weight: bold\">run</span>"),
        "{inline}"
    );
}

#[test]
fn an_undefined_hostile_reference_gets_no_class() {
    let mut t = table_using("ok");
    t.table.tbody[0].rows[0].cells[1].style_id = Some("x\" onclick=\"y".into());
    let html = render(&t, false);
    assert_clean(&html, "undefined ref");
    assert!(html.contains("<td>1</td>"), "{html}");
}

#[test]
fn style_classes_have_their_own_namespace() {
    // A style named like a part gets `gw_style-title`; the part keeps `gw_title`,
    // and no rule targets the part class.
    for id in ["title", "stub", "table", "footer"] {
        let t = table_using(id);
        assert!(t.validate().is_empty(), "{id}: {:?}", t.validate());
        let html = render(&t, false);
        assert!(
            html.contains(&format!(".gw_style-{id} {{ font-weight: bold }}")),
            "{html}"
        );
        assert!(
            !html.contains(&format!(".gw_{id} {{")),
            "{id}: rule on a part class:\n{html}"
        );
        assert!(html.contains("class=\"gw_title gw_style-"), "{html}");
        assert_clean(&html, id);
    }
}

#[test]
fn a_bad_class_prefix_is_refused() {
    let t = table_using("ok");
    for prefix in ["x\"><script>", "", "1abc", "a b", "a.b"] {
        let err = HtmlWriter::with_config(HtmlWriterConfig {
            class_prefix: prefix.into(),
            ..HtmlWriterConfig::default()
        })
        .render(&t)
        .err()
        .unwrap_or_else(|| panic!("{prefix:?} accepted"));
        assert!(matches!(err, RenderError::Config(_)), "{prefix:?}: {err}");
        assert!(err.to_string().contains("class_prefix"), "{err}");
    }
    for prefix in ["gw", "_x", "my-table_2"] {
        HtmlWriter::with_config(HtmlWriterConfig {
            class_prefix: prefix.into(),
            ..HtmlWriterConfig::default()
        })
        .render(&t)
        .unwrap();
    }
}
