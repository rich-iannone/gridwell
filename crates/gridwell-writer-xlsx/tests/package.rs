//! The package as a whole: the same table always makes the same bytes, text over
//! a cell's 32,767 characters is cut (and warned of), and a table larger than a
//! sheet is an error.

#[allow(dead_code)]
mod support;

use std::io::Read;

use gridwell_ir::StyleDef;
use gridwell_layout::unsupported::Place;
use gridwell_testkit::{cell, cell_content, footnote_mark, row, styled, text, CellExt};
use gridwell_testkit::{examples, TableBuilder};
use gridwell_writer_xlsx::{render_xlsx, unsupported, RenderError, XlsxWriter};
use support::{one_row, render, string_item, text_of};

/// A package part's text.
fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn the_same_table_makes_the_same_bytes() {
    let tables: Vec<_> = examples().iter().map(|ex| ex.table()).collect();
    let first: Vec<_> = tables.iter().map(|t| render_xlsx(t).unwrap()).collect();
    // Past a second boundary: a timestamp taken from the clock would differ.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    for (t, a) in tables.iter().zip(&first) {
        assert!(render_xlsx(t).unwrap() == *a, "{:?}", t.header);
    }
    let core = part(&first[0], "docProps/core.xml");
    assert!(
        core.contains(">1980-01-01T00:00:00Z</dcterms:created>"),
        "{core}"
    );
}

#[test]
fn the_sheet_is_named_table() {
    let bytes = render_xlsx(&examples()[0].table()).unwrap();
    assert!(part(&bytes, "xl/workbook.xml").contains("<sheet name=\"Table\""));
}

#[test]
fn a_sheet_without_text_has_no_shared_strings() {
    let t = one_row(vec![cell("5").typed("number", serde_json::json!(5))]);
    let strings = XlsxWriter::new().render_shared_strings_xml(&t).unwrap();
    assert_eq!(strings, "");
}

/// Text of `n` characters, not all one byte long.
fn long(n: usize) -> String {
    "aé😀".chars().cycle().take(n).collect()
}

#[test]
fn text_is_cut_at_a_cells_limit_and_warned_of() {
    let t = TableBuilder::new(2)
        .body(vec![row(vec![cell(&long(32_767)), cell(&long(40_000))])])
        .source_note(&long(32_768))
        .build();
    let (sheet, _, sst) = render(&t);
    assert_eq!(text_of(&sheet, &sst, "A1"), long(32_767));
    assert_eq!(text_of(&sheet, &sst, "B1"), long(32_767));
    assert_eq!(text_of(&sheet, &sst, "A3"), long(32_767));
    let found = unsupported(&t);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].feature, "text-length");
    assert_eq!(found[0].places.len(), 2, "{found:?}");
    assert_eq!(found[0].places[1], Place::SourceNote(0));
}

#[test]
fn a_footnotes_mark_counts_toward_the_limit() {
    // The note's row is its mark, a space and its text. (So long a note is
    // written plain: its superscript mark's markup takes it over.)
    let cut = |t| {
        unsupported(t)
            .into_iter()
            .find(|u| u.feature == "text-length")
            .map(|u| u.places)
    };
    let t = TableBuilder::new(1)
        .body(vec![row(vec![cell_content(vec![
            text("x"),
            footnote_mark("fn1", "1"),
        ])])])
        .footnote("fn1", "1", &long(32_765))
        .build();
    assert_eq!(cut(&t), None);
    let (sheet, _, sst) = render(&t);
    assert_eq!(text_of(&sheet, &sst, "A3").chars().count(), 32_767);

    let t = TableBuilder::new(1)
        .body(vec![row(vec![cell("x")])])
        .footnote("fn1", "1", &long(32_766))
        .build();
    assert_eq!(cut(&t), Some(vec![Place::Footnote(0)]));
    let (sheet, _, sst) = render(&t);
    assert_eq!(text_of(&sheet, &sst, "A3"), format!("1 {}", &long(32_765)));
}

#[test]
fn rich_text_over_the_limit_with_its_markup_is_written_plain() {
    // Each run's formatting is markup: 2,000 bold runs of 10 characters are
    // within Excel's 32,767 characters, but not with their markup.
    let bold = StyleDef {
        font_weight: Some("bold".into()),
        ..Default::default()
    };
    let runs = (0..2000)
        .map(|i| {
            if i % 2 == 0 {
                styled("abcdefghij", "b")
            } else {
                text("0123456789")
            }
        })
        .collect();
    let t = TableBuilder::new(2)
        .style_def("b", bold)
        .body(vec![row(vec![
            cell_content(runs),
            cell_content(vec![text("a "), styled("bold", "b")]),
        ])])
        .build();
    let (sheet, _, sst) = render(&t);
    let plain = string_item(&sheet, &sst, "A1");
    assert!(!plain.contains("<r>"), "{}", &plain[..200]);
    assert_eq!(text_of(&sheet, &sst, "A1").len(), 20_000);
    // A short rich cell keeps its runs.
    assert!(string_item(&sheet, &sst, "B1").contains("<b/>"), "{sst}");
    let found = unsupported(&t);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].feature, "rich-text-length");
    assert_eq!(found[0].places.len(), 1);
}

#[test]
fn a_table_larger_than_a_sheet_is_an_error() {
    // Excel has 16,384 columns.
    let wide = |n: usize| {
        TableBuilder::new(n as u32)
            .body(vec![row((0..n).map(|_| cell("x")).collect())])
            .build()
    };
    assert!(render_xlsx(&wide(16_384)).is_ok());
    let err = render_xlsx(&wide(16_385)).unwrap_err();
    assert!(matches!(err, RenderError::Xlsx(_)), "{err}");
    assert!(err.to_string().contains("1,048,576"), "{err}");
    // A table that can't be written has nothing to warn of.
    assert!(unsupported(&wide(16_385)).is_empty());
}
