//! With a `table_id`, every rule of the style block is scoped to the table's
//! container, so tables on one page can't restyle each other: each rule matches
//! what it matched unscoped, and nothing outside the table, whatever the id.

use std::collections::HashSet;

use gridwell_ir::Table;
use gridwell_testkit::examples;
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};
use scraper::{Html, Selector};

fn render(t: &Table, table_id: Option<&str>) -> String {
    HtmlWriter::with_config(HtmlWriterConfig {
        table_id: table_id.map(str::to_string),
        ..HtmlWriterConfig::default()
    })
    .render(t)
    .unwrap()
}

/// The style block's rules, as (selector, declarations), `:where()` taken off.
fn rules(html: &str) -> Vec<(String, String)> {
    html.lines()
        .skip_while(|l| l.trim() != "<style>")
        .skip(1)
        .take_while(|l| l.trim() != "</style>")
        .map(|l| {
            let (selector, decls) = l.trim().split_once(" { ").unwrap();
            let selector = selector
                .strip_prefix(":where(")
                .map(|s| s.strip_suffix(')').unwrap())
                .unwrap_or(selector);
            (selector.to_string(), decls.to_string())
        })
        .collect()
}

/// The elements `selector` matches, by node id.
fn matched(doc: &Html, selector: &str) -> HashSet<String> {
    let selector =
        Selector::parse(selector).unwrap_or_else(|e| panic!("{selector:?} doesn't parse: {e:?}"));
    doc.select(&selector)
        .map(|e| format!("{:?}", e.id()))
        .collect()
}

#[test]
fn scoped_rules_match_what_they_matched_unscoped() {
    for ex in examples() {
        let t = ex.table();
        let bare = rules(&render(&t, None));
        let html = render(&t, Some("t1"));
        let scoped = rules(&html);
        assert_eq!(bare.len(), scoped.len(), "{}", ex.name);
        let doc = Html::parse_fragment(&html);
        for ((b, bd), (s, sd)) in bare.iter().zip(&scoped) {
            assert_eq!(bd, sd, "{}", ex.name);
            for part in s.split(", ") {
                assert!(
                    part.starts_with("#t1 ") || part == "#t1.gw_container",
                    "{}: {s}",
                    ex.name
                );
            }
            assert_eq!(matched(&doc, b), matched(&doc, s), "{}: {s}", ex.name);
        }
    }
}

#[test]
fn any_table_id_scopes_to_its_own_container() {
    let t = examples()
        .into_iter()
        .find(|e| e.name == "style-composition")
        .map(|e| e.table())
        .unwrap_or_else(|| examples()[0].table());
    for id in [
        "t1",
        "1a",
        "-1",
        "-",
        "a.b:c",
        "é€",
        "x\"y",
        "</style><b>",
        "a\\b",
        "#t",
    ] {
        let html = render(&t, Some(id));
        // The id can't end the style block.
        let style = html.split("<style>").nth(1).unwrap();
        let style = style.split("</style>").next().unwrap();
        assert!(!style.contains('<'), "{id:?}: {style}");
        let doc = Html::parse_fragment(&html);
        let container = matched(&doc, "div.gw_container");
        let (first, _) = &rules(&html)[0];
        assert_eq!(matched(&doc, first), container, "{id:?}: {first}");
        // Every rule parses and matches as much as with a plain id.
        let plain = render(&t, Some("t"));
        let plain_doc = Html::parse_fragment(&plain);
        for ((s, _), (p, _)) in rules(&html).iter().zip(&rules(&plain)) {
            let (n, m) = (matched(&doc, s).len(), matched(&plain_doc, p).len());
            assert_eq!(n, m, "{id:?}: {s}");
        }
    }
}

#[test]
fn tables_on_one_page_dont_restyle_each_other() {
    for ex in examples() {
        let t = ex.table();
        let page = format!("{}{}", render(&t, Some("a")), render(&t, Some("b")));
        let doc = Html::parse_fragment(&page);
        let inside = |id: &str| {
            let mut all = matched(&doc, &format!("#{id}"));
            all.extend(matched(&doc, &format!("#{id} *")));
            all
        };
        for (s, _) in rules(&render(&t, Some("a"))) {
            let hit = matched(&doc, &s);
            assert!(hit.is_subset(&inside("a")), "{}: {s}", ex.name);
        }
    }
}

/// The table's own id (`config.id`) is the `table_id` when the option is unset:
/// the same output, scoped the same way. The option wins over it.
#[test]
fn the_tables_own_id_is_the_default_table_id() {
    for ex in examples() {
        let mut t = ex.table();
        t.config.id = Some("own".into());
        assert_eq!(
            render(&t, None),
            render(&ex.table(), Some("own")),
            "{}",
            ex.name
        );
        assert_eq!(
            render(&t, Some("opt")),
            render(&ex.table(), Some("opt")),
            "{}",
            ex.name
        );
    }
    // One that isn't an HTML id (in IR that wasn't validated) is left out.
    let mut t = examples()[0].table();
    for bad in ["", "a b"] {
        t.config.id = Some(bad.into());
        assert_eq!(render(&t, None), render(&examples()[0].table(), None));
    }
}
