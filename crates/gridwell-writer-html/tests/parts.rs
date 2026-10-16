//! Structural part classes (`gw_title`, `gw_col_label`, `gw_stub`, …): every part
//! the IR describes is tagged, with the right element and count, in both style
//! modes and with any class prefix, so stylesheets (gridwell's own or a host
//! page's) can target them.

use gridwell_ir::{RowRole, Table};
use gridwell_layout::{resolve, SectionKind};
use gridwell_testkit::{cell, examples, labeled_group, row, CellExt, RowExt, TableBuilder};
use gridwell_writer_html::{HtmlWriter, HtmlWriterConfig};
use scraper::{Html, Selector};

fn render(table: &Table, prefix: &str, inline_styles: bool) -> Html {
    let config = HtmlWriterConfig {
        class_prefix: prefix.to_string(),
        inline_styles,
        ..HtmlWriterConfig::default()
    };
    Html::parse_fragment(&HtmlWriter::with_config(config).render(table).unwrap())
}

fn count(doc: &Html, selector: &str) -> usize {
    doc.select(&Selector::parse(selector).unwrap()).count()
}

/// What the layout says each part's count should be.
#[derive(Debug, Default, PartialEq)]
struct Expected {
    title: usize,
    subtitle: usize,
    header_line: usize,
    col_labels: usize,
    spanners: usize,
    col_label: usize,
    spanner: usize,
    stubhead: usize,
    stub: usize,
    group_label: usize,
    summary: usize,
    grand_summary: usize,
    footnote: usize,
    source_note: usize,
}

fn expected(table: &Table) -> Expected {
    let rt = resolve(table);
    let mut e = Expected {
        title: rt.header.title.is_some() as usize,
        subtitle: rt.header.subtitle.is_some() as usize,
        header_line: rt.header.extra_lines.len(),
        footnote: rt.footer.footnotes.len(),
        source_note: rt.footer.source_notes.len(),
        ..Expected::default()
    };
    for section in rt.sections() {
        for r in &section.rows {
            let spanner_row = r.role == Some(RowRole::SpannerLabel);
            match section.kind {
                SectionKind::Head if spanner_row => e.spanners += 1,
                SectionKind::Head => e.col_labels += 1,
                SectionKind::Summary if r.role == Some(RowRole::GrandSummary) => {
                    e.grand_summary += 1
                }
                SectionKind::Summary => e.summary += 1,
                SectionKind::Body => {}
            }
            for c in r.cells() {
                match section.kind {
                    SectionKind::Head if c.is_stub => e.stubhead += 1,
                    SectionKind::Head if spanner_row => e.spanner += !c.content.is_empty() as usize,
                    SectionKind::Head => e.col_label += 1,
                    _ => e.stub += c.is_stub as usize,
                }
            }
        }
    }
    e.group_label = rt.groups.iter().filter(|g| g.label.is_some()).count();
    e
}

fn found(doc: &Html, p: &str) -> Expected {
    // Each part is counted on its element; the totals below check it appears on no
    // other element.
    Expected {
        title: count(doc, &format!("div.{p}_title")),
        subtitle: count(doc, &format!("div.{p}_subtitle")),
        header_line: count(doc, &format!("div.{p}_header_line")),
        col_labels: count(doc, &format!("thead > tr.{p}_col_labels")),
        spanners: count(doc, &format!("thead > tr.{p}_spanners")),
        col_label: count(doc, &format!("thead th.{p}_col_label")),
        spanner: count(doc, &format!("thead th.{p}_spanner")),
        stubhead: count(doc, &format!("thead th.{p}_stubhead")),
        stub: count(doc, &format!("tbody td.{p}_stub")),
        group_label: count(doc, &format!("tbody td.{p}_group_label")),
        summary: count(doc, &format!("tbody > tr.{p}_summary")),
        grand_summary: count(doc, &format!("tbody > tr.{p}_grand_summary")),
        footnote: count(doc, &format!("div.{p}_footnotes > p.{p}_footnote")),
        source_note: count(doc, &format!("div.{p}_source_notes > p.{p}_source_note")),
    }
}

const PARTS: &[&str] = &[
    "title",
    "subtitle",
    "header_line",
    "col_labels",
    "spanners",
    "col_label",
    "spanner",
    "stubhead",
    "stub",
    "group_label",
    "summary",
    "grand_summary",
    "footnote",
    "source_note",
];

fn total(doc: &Html, p: &str, part: &str) -> usize {
    count(doc, &format!(".{p}_{part}"))
}

#[test]
fn every_part_is_tagged_where_the_ir_puts_it() {
    let mut seen = std::collections::BTreeSet::new();
    for ex in examples() {
        let t = ex.table();
        let want = expected(&t);
        for (prefix, inline) in [("gw", false), ("gw", true), ("zz", false)] {
            let doc = render(&t, prefix, inline);
            let got = found(&doc, prefix);
            assert_eq!(got, want, "{} (prefix {prefix}, inline {inline})", ex.name);
            // Nowhere else: the total of each class equals its count on its element.
            let sum: usize = PARTS.iter().map(|part| total(&doc, prefix, part)).sum();
            let on_element = [
                got.title,
                got.subtitle,
                got.header_line,
                got.col_labels,
                got.spanners,
                got.col_label,
                got.spanner,
                got.stubhead,
                got.stub,
                got.group_label,
                got.summary,
                got.grand_summary,
                got.footnote,
                got.source_note,
            ]
            .iter()
            .sum::<usize>();
            assert_eq!(
                sum, on_element,
                "{}: a part class on the wrong element",
                ex.name
            );
        }
        for part in PARTS {
            if total(&render(&t, "gw", false), "gw", part) > 0 {
                seen.insert(*part);
            }
        }
    }
    // The corpus exercises every part but the extra header line.
    let missing: Vec<_> = PARTS.iter().filter(|p| !seen.contains(*p)).collect();
    assert_eq!(missing, [&"header_line"], "parts never exercised");
}

#[test]
fn every_footnote_mark_is_tagged() {
    for ex in examples() {
        let doc = render(&ex.table(), "gw", false);
        let sups = count(&doc, "sup");
        assert_eq!(count(&doc, "sup.gw_footnote_mark"), sups, "{}", ex.name);
        // One in the footer per footnote; the rest point at a footnote.
        let footer = count(&doc, "p.gw_footnote > sup.gw_footnote_mark");
        assert_eq!(footer, count(&doc, "p.gw_footnote"), "{}", ex.name);
        assert_eq!(
            count(&doc, "sup.gw_footnote_mark > a[href^='#']"),
            sups - footer,
            "{}",
            ex.name
        );
    }
}

fn spanner_table() -> Table {
    TableBuilder::new(3)
        .stub_cols(1)
        .title("T")
        .style_def(
            "hl",
            serde_json::from_value(serde_json::json!({ "font_weight": "bold" })).unwrap(),
        )
        .head(
            row(vec![
                cell(""),
                cell("Group").colspan(2),
                gridwell_testkit::placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("Stub"), cell("A"), cell("B")]))
        .group(labeled_group(
            "G",
            vec![row(vec![
                cell("r1").stub(),
                cell("1"),
                cell("2").style("hl"),
            ])],
        ))
        .build()
}

#[test]
fn spanner_filler_cells_get_no_part_and_roleless_head_rows_are_column_labels() {
    let doc = render(&spanner_table(), "gw", false);
    // The stub column's head cells are the stubhead in both rows; the spanner row
    // has one spanner; the role-less second row is column labels.
    assert_eq!(count(&doc, "tr.gw_spanners th.gw_stubhead"), 1);
    assert_eq!(count(&doc, "tr.gw_spanners th.gw_spanner"), 1);
    assert_eq!(count(&doc, "tr.gw_col_labels th.gw_col_label"), 2);
    assert_eq!(count(&doc, "tr.gw_col_labels th.gw_stubhead"), 1);
}

#[test]
fn part_classes_combine_with_style_classes() {
    let mut t = spanner_table();
    t.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("hl".into());
    let html = HtmlWriter::new().render(&t).unwrap();
    assert!(html.contains("class=\"gw_title gw_style-hl\""), "{html}");
    // A data cell has no part, so it only has its style class.
    assert!(html.contains("<td class=\"gw_style-hl\">2</td>"), "{html}");

    // Inline styles: the part class stays, the style moves into `style`.
    let inline = HtmlWriter::with_config(HtmlWriterConfig {
        inline_styles: true,
        ..HtmlWriterConfig::default()
    })
    .render(&t)
    .unwrap();
    assert!(
        inline.contains("class=\"gw_title\" style=\"font-weight: bold\""),
        "{inline}"
    );
    assert!(!inline.contains("gw_style-hl"), "{inline}");
}
