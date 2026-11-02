//! `text_transform` is applied to the content everywhere text appears (cells,
//! header lines, group labels, footnotes, source notes), from the cascaded style
//! and each run's own, unless the writer asks for the content as written.

use std::borrow::Cow;

use gridwell_ir::content::ContentNode;
use gridwell_ir::style::StyleDef;
use gridwell_ir::Table;
use gridwell_layout::{plain_text, resolve, resolve_with, ResolveOptions};
use gridwell_testkit::{
    cell, cell_content, footnote_mark, labeled_group, row, styled, text, CellExt, RowExt,
    TableBuilder,
};

fn transform(t: &str) -> StyleDef {
    StyleDef {
        text_transform: Some(t.into()),
        ..Default::default()
    }
}

fn table() -> Table {
    let mut t = TableBuilder::new(2)
        .style_def("up", transform("uppercase"))
        .style_def("cap", transform("capitalize"))
        .style_def("keep", transform("none"))
        .title("the title")
        .head(row(vec![cell("one"), cell("two")]))
        .group(
            labeled_group(
                "the group",
                vec![
                    row(vec![
                        cell_content(vec![
                            text("a "),
                            styled("kept", "keep"),
                            footnote_mark("f", "x"),
                        ])
                        .style("up"),
                        cell("as written"),
                    ]),
                    // A row's transform reaches its cells; a cell's own wins.
                    row(vec![cell("by the row"), cell("SHOUT").style("keep")]).style("cap"),
                ],
            )
            .label_style("up"),
        )
        .footnote("f", "x", "a note")
        .source_note("source: ")
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("cap".into());
    let footer = t.footer.as_mut().unwrap();
    footer.footnotes[0].style_id = Some("up".into());
    footer.source_notes[0]
        .content
        .push(styled("the agency", "cap"));
    t
}

fn texts(content: &[ContentNode]) -> String {
    plain_text(content, "/")
}

#[test]
fn every_kind_of_content_is_transformed() {
    let t = table();
    let rt = resolve(&t);
    assert_eq!(
        texts(&rt.header.title.as_ref().unwrap().content),
        "The Title"
    );
    let g = &rt.groups[0];
    assert_eq!(texts(&g.label.as_ref().unwrap().content), "THE GROUP");
    let rows: Vec<Vec<String>> = g
        .rows
        .rows
        .iter()
        .map(|r| r.cells().map(|c| texts(&c.content)).collect())
        .collect();
    assert_eq!(rows, [["A keptx", "as written"], ["By The Row", "SHOUT"]]);
    // The footnote mark is kept as written.
    let first = rt.groups[0].rows.rows[0].cells().next().unwrap();
    assert!(matches!(
        first.content.last(),
        Some(ContentNode::FootnoteMark { mark_text, .. }) if mark_text == "x"
    ));
    assert_eq!(texts(&rt.footer.footnotes[0].content), "A NOTE");
    assert_eq!(
        texts(&rt.footer.source_notes[0].content),
        "source: The Agency"
    );
    // The head cells have no transform: borrowed from the IR, not copied.
    for c in rt.head.rows[0].cells() {
        assert!(matches!(c.content, Cow::Borrowed(_)));
    }
    // `contents()` lists the transformed content.
    let all: Vec<String> = rt.contents().into_iter().map(texts).collect();
    assert!(all.contains(&"THE GROUP".to_string()), "{all:?}");
}

#[test]
fn opting_out_keeps_the_content_as_written() {
    let t = table();
    let rt = resolve_with(
        &t,
        ResolveOptions {
            transform_text: false,
            ..ResolveOptions::default()
        },
    );
    assert_eq!(
        texts(&rt.header.title.as_ref().unwrap().content),
        "the title"
    );
    let first = rt.groups[0].rows.rows[0].cells().next().unwrap();
    assert_eq!(texts(&first.content), "a keptx");
    // The style still says what the transform is, for the writer to express.
    assert!(first.style.text_transform.is_some());
    assert_eq!(texts(&rt.footer.footnotes[0].content), "a note");
    for c in rt.contents() {
        assert!(!c.is_empty());
    }
    let borrowed = rt
        .groups
        .iter()
        .flat_map(|g| &g.rows.rows)
        .flat_map(|r| r.cells())
        .all(|c| matches!(c.content, Cow::Borrowed(_)));
    assert!(borrowed);
}
