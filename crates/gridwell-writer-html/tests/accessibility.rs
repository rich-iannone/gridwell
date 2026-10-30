//! Header semantics for assistive technology: the stub is a row header
//! (`<th scope="row">`), a cell's own `scope` is kept (and makes it a header cell
//! wherever it is), and the table carries `aria-label` / `aria-describedby`.

use gridwell_ir::Table;
use gridwell_testkit::{
    cell, column, labeled_group, row, CellExt, ColumnExt, RowExt, TableBuilder,
};
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};
use scraper::{ElementRef, Html, Selector};

fn render(table: &Table, inline_styles: bool) -> Html {
    let config = HtmlWriterConfig {
        inline_styles,
        ..HtmlWriterConfig::default()
    };
    Html::parse_fragment(&HtmlWriter::with_config(config).render(table).unwrap())
}

fn select<'a>(doc: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    doc.select(&Selector::parse(selector).unwrap()).collect()
}

fn text(e: &ElementRef) -> String {
    e.text().collect::<String>().trim().to_string()
}

/// Every cell below the head, as (tag, scope, text), in document order.
fn body_cells(doc: &Html) -> Vec<(String, Option<String>, String)> {
    select(doc, "tbody td, tbody th")
        .iter()
        .map(|e| {
            let v = e.value();
            (
                v.name().to_string(),
                v.attr("scope").map(str::to_string),
                text(e),
            )
        })
        .collect()
}

fn stub_table() -> Table {
    TableBuilder::new(3)
        .stub_cols(1)
        .head(row(vec![cell(""), cell("Q1"), cell("Q2")]))
        .group(
            labeled_group(
                "North",
                vec![
                    row(vec![cell("Oslo"), cell("1"), cell("2")]),
                    row(vec![cell("Bergen"), cell("3"), cell("4")]),
                ],
            )
            .summary(vec![
                row(vec![cell("Total"), cell("4"), cell("6")]).role("summary_row")
            ]),
        )
        .build()
}

#[test]
fn stubs_are_row_headers() {
    for inline in [false, true] {
        let doc = render(&stub_table(), inline);
        let th = |s: &str| ("th".to_string(), Some("row".to_string()), s.to_string());
        let td = |s: &str| ("td".to_string(), None, s.to_string());
        assert_eq!(
            body_cells(&doc),
            vec![
                td("North"),
                th("Oslo"),
                td("1"),
                td("2"),
                th("Bergen"),
                td("3"),
                td("4"),
                // A summary row's label is its row's header too.
                th("Total"),
                td("4"),
                td("6"),
            ],
            "inline {inline}"
        );
        // The stubhead is a header cell already; it labels a column, not a row.
        let head = select(&doc, "thead th");
        assert_eq!(head.len(), 3);
        assert!(head.iter().all(|e| e.value().attr("scope").is_none()));
    }
}

#[test]
fn a_cells_own_scope_wins_and_makes_it_a_header() {
    let table = TableBuilder::new(3)
        .stub_cols(1)
        .head(row(vec![
            cell("").scope("col"),
            cell("A").scope("col"),
            cell("B").scope("colgroup"),
        ]))
        .body(vec![
            row(vec![
                cell("s").scope("rowgroup"),
                cell("x").scope("row"),
                cell("y"),
            ]),
            // Not in the stub column, but flagged as a stub.
            row(vec![cell("t"), cell("u").stub(), cell("v")]),
        ])
        .build();
    let doc = render(&table, false);
    let cell = |tag: &str, scope: Option<&str>, s: &str| {
        (tag.to_string(), scope.map(str::to_string), s.to_string())
    };
    assert_eq!(
        body_cells(&doc),
        vec![
            cell("th", Some("rowgroup"), "s"),
            cell("th", Some("row"), "x"),
            cell("td", None, "y"),
            cell("th", Some("row"), "t"),
            cell("th", Some("row"), "u"),
            cell("td", None, "v"),
        ]
    );
    let scopes: Vec<_> = select(&doc, "thead th")
        .iter()
        .map(|e| e.value().attr("scope").unwrap_or("-").to_string())
        .collect();
    assert_eq!(scopes, ["col", "col", "colgroup"]);
}

/// A browser centers `<th>`: every header cell below the head must say how it
/// is aligned, so a stub keeps its column's (left) alignment.
#[test]
fn row_headers_keep_their_alignment() {
    let doc = render(&stub_table(), false);
    let stubs = select(&doc, "tbody th");
    assert_eq!(stubs.len(), 3);
    for s in &stubs {
        assert!(
            s.value()
                .has_class("gw__al_left", scraper::CaseSensitivity::CaseSensitive),
            "{}",
            s.html()
        );
    }
    let style = text(&select(&doc, "style")[0]);
    assert!(
        style.contains(".gw_table .gw__al_left { text-align: left }"),
        "{style}"
    );

    let doc = render(&stub_table(), true);
    for s in select(&doc, "tbody th") {
        let css = s.value().attr("style").unwrap_or_default();
        assert!(css.contains("text-align: left"), "{}", s.html());
    }

    // With no head, only the stub asks for the left-alignment rule.
    let headless = TableBuilder::new(2)
        .stub_cols(1)
        .columns(vec![column("s", "S"), column("n", "N").align("right")])
        .body(vec![row(vec![cell("Oslo"), cell("1")])])
        .build();
    let doc = render(&headless, false);
    let style = text(&select(&doc, "style")[0]);
    assert!(
        style.contains(".gw_table .gw__al_left { text-align: left }"),
        "{style}"
    );
}

#[test]
fn aria_label_and_describedby_are_on_the_table() {
    let table = TableBuilder::new(1)
        .config(|c| {
            c.aria_label = Some("Sales \"by\" <region>".into());
            c.aria_describedby = Some("notes-1 \"><script>".into());
        })
        .body(vec![row(vec![cell("x")])])
        .build();
    for inline in [false, true] {
        let html = HtmlWriter::with_config(HtmlWriterConfig {
            inline_styles: inline,
            ..HtmlWriterConfig::default()
        })
        .render(&table)
        .unwrap();
        assert!(!html.contains("<script>"), "{html}");
        let doc = Html::parse_fragment(&html);
        let t = &select(&doc, "table")[0];
        assert_eq!(t.value().attr("aria-label"), Some("Sales \"by\" <region>"));
        assert_eq!(
            t.value().attr("aria-describedby"),
            Some("notes-1 \"><script>")
        );
    }

    let plain = TableBuilder::new(1)
        .body(vec![row(vec![cell("x")])])
        .build();
    let doc = render(&plain, false);
    let t = &select(&doc, "table")[0];
    assert_eq!(t.value().attr("aria-label"), None);
    assert_eq!(t.value().attr("aria-describedby"), None);
}
