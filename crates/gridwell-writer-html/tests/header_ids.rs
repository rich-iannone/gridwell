//! Header ids, as Great Tables makes them: the stubhead, column labels, spanners,
//! row-group labels, stub cells, summary-row labels, title and subtitle get ids
//! (unique, valid, and prefixed by `table_id`), each body cell names its headers
//! in `headers`, and the table is labeled by the title and described by the
//! subtitle.

use gridwell_ir::Table;
use gridwell_testkit::arb::arb_valid_ir;
use gridwell_testkit::{
    cell, cell_content, column, examples, group, labeled_group, placeholder, raw, row, CellExt,
    RowExt, TableBuilder,
};
use gridwell_writer_html::{check_config, HtmlWriter, HtmlWriterConfig};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, TestRunner};
use scraper::{ElementRef, Html, Selector};
use std::collections::{BTreeSet, HashSet};

fn html_with(table: &Table, config: HtmlWriterConfig) -> String {
    HtmlWriter::with_config(config).render(table).unwrap()
}

fn render(table: &Table) -> Html {
    Html::parse_fragment(&html_with(table, HtmlWriterConfig::default()))
}

fn render_id(table: &Table, table_id: &str) -> Html {
    let config = HtmlWriterConfig {
        table_id: Some(table_id.into()),
        ..HtmlWriterConfig::default()
    };
    Html::parse_fragment(&html_with(table, config))
}

fn select<'a>(doc: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    doc.select(&Selector::parse(selector).unwrap()).collect()
}

fn attr<'a>(e: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    e.value().attr(name)
}

/// Each element of `selector` as `text#id` or `text[headers]` (or just its text).
fn cells(doc: &Html, selector: &str) -> Vec<String> {
    select(doc, selector)
        .iter()
        .map(|e| {
            let text = e.text().collect::<String>().trim().to_string();
            match (attr(e, "id"), attr(e, "headers")) {
                (Some(id), None) => format!("{text}#{id}"),
                (None, Some(h)) => format!("{text}[{h}]"),
                (None, None) => text,
                (Some(id), Some(h)) => format!("{text}#{id}[{h}]"),
            }
        })
        .collect()
}

fn table_attr(doc: &Html, name: &str) -> Option<String> {
    attr(&select(doc, "table")[0], name).map(str::to_string)
}

/// Title, subtitle, a stubhead, two levels of spanners (with a blank cell beside
/// the lower one), two labeled groups with a summary, and a grand summary.
fn full_table() -> Table {
    TableBuilder::new(4)
        .stub_cols(1)
        .title("Sales")
        .subtitle("By region")
        .columns(vec![
            column("region", "Region"),
            column("a", "A"),
            column("b", "B"),
            column("c", "C"),
        ])
        .head(
            row(vec![
                cell(""),
                cell("Top").colspan(3),
                placeholder(),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(
            row(vec![
                cell(""),
                cell("Mid").colspan(2),
                placeholder(),
                cell(" "),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("Region"), cell("A"), cell("B"), cell("C")]))
        .group(
            labeled_group(
                "North East",
                vec![
                    row(vec![cell("Oslo"), cell("1"), cell("2"), cell("3")]),
                    row(vec![cell("Bergen"), cell("4"), cell("5"), cell("6")]),
                ],
            )
            .summary(vec![row(vec![
                cell("Total"),
                cell("5"),
                cell("7"),
                cell("9"),
            ])
            .role("summary_row")]),
        )
        .group(labeled_group(
            "South",
            vec![row(vec![cell("Rome"), cell("7"), cell("8"), cell("9")])],
        ))
        .group(group(vec![]).summary(vec![
            row(vec![cell("Total"), cell("12"), cell("15"), cell("18")]).role("grand_summary"),
        ]))
        .build()
}

#[test]
fn every_header_has_an_id_and_every_cell_names_its_headers() {
    let doc = render(&full_table());
    assert_eq!(
        cells(&doc, "div[role=heading]"),
        ["Sales#title", "By region#subtitle"]
    );
    assert_eq!(
        table_attr(&doc, "aria-labelledby").as_deref(),
        Some("title")
    );
    assert_eq!(
        table_attr(&doc, "aria-describedby").as_deref(),
        Some("subtitle")
    );
    // A blank cell heads nothing, so it gets no id.
    assert_eq!(
        cells(&doc, "thead th"),
        [
            "",
            "Top#Top",
            "",
            "Mid#Mid",
            "",
            "Region#Region",
            "A#a",
            "B#b",
            "C#c"
        ]
    );
    assert_eq!(
        cells(&doc, "tbody th, tbody td"),
        [
            "North East#north_east",
            "Oslo#stub-1",
            "1[north_east stub-1 Top Mid a]",
            "2[north_east stub-1 Top Mid b]",
            "3[north_east stub-1 Top c]",
            "Bergen#stub-2",
            "4[north_east stub-2 Top Mid a]",
            "5[north_east stub-2 Top Mid b]",
            "6[north_east stub-2 Top c]",
            "Total#summary-north_east-Total",
            "5[north_east summary-north_east-Total Top Mid a]",
            "7[north_east summary-north_east-Total Top Mid b]",
            "9[north_east summary-north_east-Total Top c]",
            "South#south",
            // Stubs number the rows in display order, across groups.
            "Rome#stub-3",
            "7[south stub-3 Top Mid a]",
            "8[south stub-3 Top Mid b]",
            "9[south stub-3 Top c]",
            // A grand summary belongs to no group.
            "Total#grand-summary-Total",
            "12[grand-summary-Total Top Mid a]",
            "15[grand-summary-Total Top Mid b]",
            "18[grand-summary-Total Top c]",
        ]
    );
    // The group label spans the row as a column-group header.
    let label = &select(&doc, "tbody th[scope=colgroup]")[0];
    assert_eq!(attr(label, "colspan"), Some("4"));

    // The same ids in inline-style mode.
    let inline = Html::parse_fragment(&html_with(
        &full_table(),
        HtmlWriterConfig {
            inline_styles: true,
            ..HtmlWriterConfig::default()
        },
    ));
    for selector in ["thead th", "tbody th, tbody td", "div[role=heading]"] {
        assert_eq!(
            cells(&inline, selector),
            cells(&doc, selector),
            "{selector}"
        );
    }
}

/// A grand summary after a labeled group's rows (in its summary rows) sums the
/// whole table, so the group's label doesn't head it; the group's own summary is
/// headed by it.
#[test]
fn a_grand_summary_is_headed_by_no_group() {
    let table = TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("S"), cell("V")]))
        .group(
            labeled_group("G", vec![row(vec![cell("r"), cell("1")])]).summary(vec![
                row(vec![cell("Sum"), cell("1")]).role("summary_row"),
                row(vec![cell("All"), cell("1")]).role("grand_summary"),
            ]),
        )
        .build();
    assert_eq!(
        cells(&render(&table), "tbody th, tbody td"),
        [
            "G#g",
            "r#stub-1",
            "1[g stub-1 col_1]",
            "Sum#summary-g-Sum",
            "1[g summary-g-Sum col_1]",
            "All#grand-summary-All",
            "1[grand-summary-All col_1]",
        ]
    );
}

#[test]
fn a_table_id_prefixes_every_id_and_names_the_container() {
    let doc = render_id(&full_table(), "sales-2024");
    let container = &select(&doc, "div.gw_container")[0];
    assert_eq!(attr(container, "id"), Some("sales-2024"));
    assert_eq!(
        cells(&doc, "div[role=heading]"),
        ["Sales#sales-2024-title", "By region#sales-2024-subtitle"]
    );
    assert_eq!(
        table_attr(&doc, "aria-labelledby").as_deref(),
        Some("sales-2024-title")
    );
    assert_eq!(cells(&doc, "thead th")[1], "Top#sales-2024-Top");
    assert_eq!(
        cells(&doc, "tbody td")[0],
        "1[sales-2024-north_east sales-2024-stub-1 sales-2024-Top sales-2024-Mid sales-2024-a]"
    );
    // Every id but the container's has the prefix.
    for e in select(&doc, "[id]").iter().skip(1) {
        assert!(
            attr(e, "id").unwrap().starts_with("sales-2024-"),
            "{}",
            e.html()
        );
    }

    // An id with characters that can't be in an id is still the container's id,
    // escaped; the prefix is sanitized.
    let html = html_with(
        &full_table(),
        HtmlWriterConfig {
            table_id: Some("t&\"x\"".into()),
            ..HtmlWriterConfig::default()
        },
    );
    assert!(html.contains("<div id=\"t&amp;&quot;x&quot;\""), "{html}");
    let doc = Html::parse_fragment(&html);
    assert_eq!(
        attr(&select(&doc, "div.gw_container")[0], "id"),
        Some("t&\"x\"")
    );
    assert_eq!(
        table_attr(&doc, "aria-labelledby").as_deref(),
        Some("t-x-title")
    );
}

#[test]
fn an_empty_or_spaced_table_id_is_rejected() {
    for bad in ["", "a b", " a", "a\tb", "a\u{a0}b"] {
        let config = HtmlWriterConfig {
            table_id: Some(bad.into()),
            ..HtmlWriterConfig::default()
        };
        let err = check_config(&config).unwrap_err();
        assert!(err.contains("table_id"), "{bad:?}: {err}");
        let err = HtmlWriter::with_config(config)
            .render(&full_table())
            .unwrap_err()
            .to_string();
        assert!(err.contains("table_id"), "{bad:?}: {err}");
    }
    for good in [None, Some("a"), Some("sales-2024"), Some("é:1")] {
        let config = HtmlWriterConfig {
            table_id: good.map(str::to_string),
            ..HtmlWriterConfig::default()
        };
        assert!(check_config(&config).is_ok(), "{good:?}");
    }
    // The class prefix is still checked.
    let config = HtmlWriterConfig {
        class_prefix: "1x".into(),
        table_id: Some("ok".into()),
        ..HtmlWriterConfig::default()
    };
    assert!(check_config(&config).unwrap_err().contains("class_prefix"));
}

/// A two-column stub where a cell of the outer column spans two rows: the second
/// row is headed by it too, and its scope is the row group. A cell spanning two
/// columns has both columns' labels.
#[test]
fn a_spanning_stub_cell_heads_every_row_it_covers() {
    let table = TableBuilder::new(4)
        .stub_cols(2)
        .head(row(vec![
            cell("Region"),
            cell("City"),
            cell("X"),
            cell("Y"),
        ]))
        .body(vec![
            row(vec![
                cell("North").rowspan(2),
                cell("Oslo"),
                cell("1"),
                cell("2"),
            ]),
            row(vec![
                placeholder(),
                cell("Bergen"),
                cell("3").colspan(2),
                placeholder(),
            ]),
            row(vec![cell("South"), cell("Rome"), cell("5"), cell("6")]),
        ])
        .build();
    let doc = render(&table);
    assert_eq!(
        cells(&doc, "thead th"),
        ["Region#Region", "City#City", "X#col_2", "Y#col_3"]
    );
    assert_eq!(
        cells(&doc, "tbody th, tbody td"),
        [
            "North#stub-1-1",
            "Oslo#stub-1-2",
            "1[stub-1-1 stub-1-2 col_2]",
            "2[stub-1-1 stub-1-2 col_3]",
            "Bergen#stub-2-2",
            "3[stub-1-1 stub-2-2 col_2 col_3]",
            "South#stub-3-1",
            "Rome#stub-3-2",
            "5[stub-3-1 stub-3-2 col_2]",
            "6[stub-3-1 stub-3-2 col_3]",
        ]
    );
    let scopes: Vec<_> = select(&doc, "tbody th")
        .iter()
        .map(|e| attr(e, "scope").unwrap_or("-"))
        .collect();
    assert_eq!(scopes, ["rowgroup", "row", "row", "row", "row"]);
    // A cell's own scope still wins.
    let mut table = table;
    table.table.tbody[0].rows[0].cells[0].scope = Some("row".into());
    let doc = render(&table);
    assert_eq!(attr(&select(&doc, "tbody th")[0], "scope"), Some("row"));
}

#[test]
fn repeated_and_reserved_ids_get_a_suffix() {
    // Two column ids that sanitize alike, one that is the title's id, one that is
    // a footnote's, and one that is nothing but punctuation.
    let table = TableBuilder::new(5)
        .title("T")
        .columns(vec![
            column("a b", "1"),
            column("a-b", "2"),
            column("title", "3"),
            column("fn1", "4"),
            column("?!", "5"),
        ])
        .head(row(vec![
            cell("1"),
            cell("2"),
            cell("3"),
            cell("4"),
            cell("5"),
        ]))
        .body(vec![row(vec![
            cell("x"),
            cell("x"),
            cell("x"),
            cell("x"),
            cell("x"),
        ])])
        .footnote("fn1", "1", "A note")
        .build();
    let doc = render(&table);
    assert_eq!(
        cells(&doc, "thead th"),
        ["1#a-b", "2#a-b-2", "3#title", "4#fn1-2", "5#column"]
    );
    // The title is made after the column labels, as in Great Tables.
    assert_eq!(cells(&doc, "div[role=heading]"), ["T#title-2"]);
    assert_eq!(
        table_attr(&doc, "aria-labelledby").as_deref(),
        Some("title-2")
    );
    assert_eq!(select(&doc, "p#fn1").len(), 1);
    assert_unique_ids(&doc, "collisions");
}

#[test]
fn label_text_is_sanitized_into_ids() {
    let table = TableBuilder::new(4)
        .stub_cols(1)
        .columns(vec![
            column("s", "s"),
            column("\"><script>alert(1)</script>", "x"),
            column("b", "b"),
            column("c", "c"),
        ])
        .head(
            row(vec![
                cell(""),
                cell_content(vec![raw("html", "<b>Name</b> &amp; place")]),
                cell_content(vec![raw("html", "<em>Größe</em> (m²)")]).colspan(2),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![
            cell_content(vec![raw("html", "<i>Stub</i><br>label")]),
            cell("X"),
            cell("B"),
            cell("C"),
        ]))
        .group(labeled_group(
            "Where?  Here!",
            vec![row(vec![cell("r"), cell("1"), cell("2"), cell("3")])],
        ))
        .build();
    let doc = render(&table);
    let ids: Vec<_> = select(&doc, "[id]")
        .iter()
        .map(|e| attr(e, "id").unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        [
            "Name-place",
            "Größe-m²",
            "Stublabel",
            "script-alert-1-script",
            "b",
            "c",
            // The group's id (`where?__here!`), as Great Tables uses it.
            "where-__here",
            "stub-1",
        ]
    );
    assert_unique_ids(&doc, "sanitized");
}

#[test]
fn hidden_column_labels_leave_only_the_row_headers() {
    let table = TableBuilder::new(2)
        .stub_cols(1)
        .hide_column_labels()
        .head(row(vec![cell("S"), cell("V")]))
        .group(labeled_group("G", vec![row(vec![cell("r"), cell("1")])]))
        .build();
    let doc = render(&table);
    assert!(select(&doc, "thead").is_empty());
    assert_eq!(
        cells(&doc, "tbody th, tbody td"),
        ["G#g", "r#stub-1", "1[g stub-1]"]
    );

    // With no headers at all, a cell has no `headers` attribute.
    let bare = TableBuilder::new(1)
        .hide_column_labels()
        .head(row(vec![cell("V")]))
        .body(vec![row(vec![cell("1")])])
        .build();
    let doc = render(&bare);
    assert_eq!(cells(&doc, "tbody td"), ["1"]);
    assert!(select(&doc, "[id]").is_empty());
}

#[test]
fn the_tables_own_aria_settings_win() {
    let with = |f: fn(&mut gridwell_ir::Config)| {
        let mut t = full_table();
        f(&mut t.config);
        render(&t)
    };
    let doc = with(|c| c.aria_label = Some("Sales".into()));
    assert_eq!(table_attr(&doc, "aria-label").as_deref(), Some("Sales"));
    assert_eq!(table_attr(&doc, "aria-labelledby"), None);
    assert_eq!(
        table_attr(&doc, "aria-describedby").as_deref(),
        Some("subtitle")
    );

    // A caption names the table already.
    let doc = with(|c| c.summary = Some("About sales".into()));
    assert_eq!(table_attr(&doc, "aria-labelledby"), None);

    let doc = with(|c| c.aria_describedby = Some("notes".into()));
    assert_eq!(
        table_attr(&doc, "aria-labelledby").as_deref(),
        Some("title")
    );
    assert_eq!(
        table_attr(&doc, "aria-describedby").as_deref(),
        Some("notes")
    );

    // A subtitle alone still describes the table.
    let mut t = full_table();
    t.header.as_mut().unwrap().title = None;
    let doc = render(&t);
    assert_eq!(table_attr(&doc, "aria-labelledby"), None);
    assert_eq!(
        table_attr(&doc, "aria-describedby").as_deref(),
        Some("subtitle")
    );
}

const FLAG: &str = r##"<svg viewBox="0 0 2 1"><defs><clipPath id="c"><rect width="2" height="1"/></clipPath></defs><rect clip-path="url(#c)" fill="red"/><use href="#c"/></svg>"##;

#[test]
fn svg_ids_in_body_cells_are_made_unique() {
    let flag = || cell_content(vec![raw("html", FLAG)]);
    let table = TableBuilder::new(2)
        .stub_cols(1)
        .columns(vec![column("name", "Name"), column("flag", "Flag")])
        .head(row(vec![cell("Name"), flag()]))
        .group(
            labeled_group(
                "G",
                vec![row(vec![cell("a"), flag()]), row(vec![cell("b"), flag()])],
            )
            .summary(vec![row(vec![cell("Total"), flag()]).role("summary_row")]),
        )
        .build();
    let svg_ids = |doc: &Html| -> Vec<String> {
        select(doc, "svg clipPath")
            .iter()
            .map(|e| attr(e, "id").unwrap().to_string())
            .collect()
    };
    let doc = render(&table);
    // The head's SVG is left as it is; each body cell's is prefixed by its column
    // and row, and its references follow.
    assert_eq!(
        svg_ids(&doc),
        ["c", "flag-0-c", "flag-1-c", "flag-summary-1-c"]
    );
    let html = html_with(&table, HtmlWriterConfig::default());
    assert!(html.contains("clip-path=\"url(#flag-1-c)\""), "{html}");
    assert!(html.contains("<use href=\"#flag-summary-1-c\"/>"), "{html}");

    let doc = render_id(&table, "t");
    assert_eq!(
        svg_ids(&doc),
        ["c", "t-flag-0-c", "t-flag-1-c", "t-flag-summary-1-c"]
    );
}

/// Every id is unique.
fn assert_unique_ids(doc: &Html, name: &str) {
    let mut seen = HashSet::new();
    for e in select(doc, "[id]") {
        let id = attr(&e, "id").unwrap();
        assert!(seen.insert(id.to_string()), "{name}: id {id:?} repeated");
    }
}

/// The invariants of any table's ids: they are unique, `headers` names only
/// header cells, every header below the head and every labeled head cell has an
/// id, the table's `aria-labelledby` names an element, and with a `table_id` every
/// id the writer makes has its prefix.
fn check_invariants(t: &Table, name: &str) {
    for table_id in [None, Some("t1")] {
        let config = HtmlWriterConfig {
            table_id: table_id.map(str::to_string),
            ..HtmlWriterConfig::default()
        };
        let doc = Html::parse_fragment(&html_with(t, config));
        let name = format!("{name} (table_id {table_id:?})");
        assert_unique_ids(&doc, &name);
        let header_ids: BTreeSet<_> = select(&doc, "th[id]")
            .iter()
            .map(|e| attr(e, "id").unwrap().to_string())
            .collect();
        for e in select(&doc, "[headers]") {
            assert_eq!(e.value().name(), "td", "{name}: {}", e.html());
            for token in attr(&e, "headers").unwrap().split(' ') {
                assert!(
                    header_ids.contains(token),
                    "{name}: {token:?} in {}",
                    e.html()
                );
            }
        }
        for e in select(&doc, "tbody th") {
            assert!(attr(&e, "id").is_some(), "{name}: {}", e.html());
        }
        for e in select(&doc, "thead th") {
            let labeled = !e.text().collect::<String>().trim().is_empty();
            assert!(!labeled || attr(&e, "id").is_some(), "{name}: {}", e.html());
        }
        let all_ids: HashSet<_> = select(&doc, "[id]")
            .iter()
            .map(|e| attr(e, "id").unwrap().to_string())
            .collect();
        if let Some(id) = table_attr(&doc, "aria-labelledby") {
            assert!(all_ids.contains(&id), "{name}: aria-labelledby {id:?}");
        }
        if t.config.aria_describedby.is_none() {
            if let Some(id) = table_attr(&doc, "aria-describedby") {
                assert!(all_ids.contains(&id), "{name}: aria-describedby {id:?}");
            }
        }
        if let Some(prefix) = table_id {
            let footnotes: HashSet<_> = t
                .footer
                .as_ref()
                .map(|f| f.footnotes.iter().map(|n| n.id.as_str()).collect())
                .unwrap_or_default();
            for id in &all_ids {
                assert!(
                    id == prefix
                        || id.starts_with(&format!("{prefix}-"))
                        || footnotes.contains(id.as_str()),
                    "{name}: id {id:?} without the prefix"
                );
            }
        }
    }
}

#[test]
fn every_examples_ids_are_consistent() {
    for ex in examples() {
        check_invariants(&ex.table(), ex.name);
    }
    check_invariants(&full_table(), "full_table");
}

#[test]
fn random_tables_ids_are_consistent() {
    let mut runner = TestRunner::new(Config::default());
    for _ in 0..256 {
        let t = arb_valid_ir().new_tree(&mut runner).unwrap().current();
        check_invariants(&t, &t.to_json().unwrap());
    }
}
