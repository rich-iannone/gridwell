//! Slide geometry: the title box, the table and the notes box are stacked top to
//! bottom without overlapping, the table as tall as its rows, text boxes stay on
//! the slide, lined up with the table, and the header and notes text is all
//! there.

use gridwell_ir::{Table, TextDirection};
use gridwell_testkit::*;
use gridwell_writer_pptx::PptxWriter;

const SLIDE_H: u64 = 6_858_000;
/// The slide less half-inch margins.
const TEXT_X: u64 = 457_200;
const TEXT_END: u64 = 457_200 + 8_229_600;

/// (name, y, height) for each shape, in document order.
fn shapes(xml: &str) -> Vec<(String, u64, u64)> {
    boxes(xml)
        .into_iter()
        .map(|b| (b.name, b.y, b.cy))
        .collect()
}

#[derive(Debug)]
struct Shape {
    name: String,
    x: u64,
    y: u64,
    cx: u64,
    cy: u64,
}

/// Each shape's name and place, in document order.
fn boxes(xml: &str) -> Vec<Shape> {
    let mut out = Vec::new();
    for chunk in xml.split("<p:cNvPr id=\"").skip(2) {
        let name = chunk
            .split("name=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string();
        let num = |key: &str| -> u64 {
            chunk
                .split(key)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        out.push(Shape {
            name,
            x: num("off x=\""),
            y: num(" y=\""),
            cx: num("cx=\""),
            cy: num("cy=\""),
        });
    }
    out
}

/// The `key="…"` numbers in `xml`, in order.
fn numbers(xml: &str, key: &str) -> Vec<u64> {
    xml.split(key)
        .skip(1)
        .map(|s| s.split('"').next().unwrap().parse().unwrap())
        .collect()
}

fn slide(t: &Table) -> String {
    PptxWriter::new().render_slide_xml(t).unwrap()
}

/// The shape named `name`.
fn shape<'s>(shapes: &'s [Shape], name: &str) -> &'s Shape {
    shapes.iter().find(|s| s.name == name).unwrap()
}

#[test]
fn shapes_stack_without_overlap() {
    let mut with_header = 0;
    let mut with_notes = 0;
    for ex in examples() {
        let t = ex.table();
        let xml = PptxWriter::new().render_slide_xml(&t).unwrap();
        let s = shapes(&xml);
        let names: Vec<&str> = s.iter().map(|(n, ..)| n.as_str()).collect();
        let header = t.header.as_ref().is_some_and(|h| {
            h.title.is_some() || h.subtitle.is_some() || !h.extra_lines.is_empty()
        });
        let notes = t
            .footer
            .as_ref()
            .is_some_and(|f| !f.footnotes.is_empty() || !f.source_notes.is_empty());
        let mut want = Vec::new();
        if header {
            want.push("Title");
            with_header += 1;
        }
        want.push("Table");
        if notes {
            want.push("Notes");
            with_notes += 1;
        }
        assert_eq!(names, want, "{}", ex.name);
        for pair in s.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            assert!(a.1 + a.2 <= b.1, "{}: {} overlaps {}", ex.name, a.0, b.0);
        }
        // Text boxes on the slide whenever the whole stack fits.
        let total: u64 = s.iter().map(|(_, _, h)| h).sum();
        if total < SLIDE_H - 600_000 {
            let (_, y, h) = s.last().unwrap();
            assert!(y + h <= SLIDE_H, "{}: past the bottom", ex.name);
        }
    }
    assert!(
        with_header > 3 && with_notes > 3,
        "{with_header} {with_notes}"
    );
}

#[test]
fn title_subtitle_and_notes_text_is_present() {
    for ex in examples() {
        let t = ex.table();
        let xml = PptxWriter::new().render_slide_xml(&t).unwrap();
        let mut expect = Vec::new();
        if let Some(h) = &t.header {
            for line in h.title.iter().chain(&h.subtitle).chain(&h.extra_lines) {
                expect.push(gridwell_layout::plain_text(&line.content, ""));
            }
        }
        if let Some(f) = &t.footer {
            for n in &f.footnotes {
                expect.push(gridwell_layout::plain_text(&n.content, ""));
            }
            for n in &f.source_notes {
                expect.push(gridwell_layout::plain_text(&n.content, ""));
            }
        }
        let text: String = xml
            .split("<a:t>")
            .skip(1)
            .map(|s| s.split("</a:t>").next().unwrap())
            .collect();
        let text = text
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#39;", "'")
            .replace("&quot;", "\"");
        for e in expect {
            assert!(text.contains(&e), "{}: {e:?} missing", ex.name);
        }
    }
}

#[test]
fn the_table_is_as_big_as_its_rows_and_columns() {
    for ex in examples() {
        let xml = slide(&ex.table());
        let Some(table) = boxes(&xml).into_iter().find(|s| s.name == "Table") else {
            continue;
        };
        let rows = numbers(&xml, "<a:tr h=\"");
        let cols = numbers(&xml, "<a:gridCol w=\"");
        assert_eq!(table.cy, rows.iter().sum::<u64>(), "{}", ex.name);
        assert_eq!(table.cx, cols.iter().sum::<u64>(), "{}", ex.name);
        // Each row at least a line of 18pt text and its insets (29.16pt).
        assert!(rows.iter().all(|&h| h >= 370_332), "{}: {rows:?}", ex.name);
    }
}

#[test]
fn text_boxes_line_up_with_the_table() {
    for ex in examples() {
        let t = ex.table();
        let xml = slide(&t);
        let s = boxes(&xml);
        let Some(table) = s.iter().find(|s| s.name == "Table") else {
            continue;
        };
        let rtl = t.config.dir == Some(TextDirection::Rtl);
        for b in s.iter().filter(|b| b.name != "Table") {
            let what = format!("{}: {}", ex.name, b.name);
            assert!(b.x >= TEXT_X && b.x + b.cx <= TEXT_END, "{what}: {b:?}");
            // From the table's start edge to the far margin, or wider.
            if rtl {
                assert_eq!(b.x, TEXT_X, "{what}");
                assert!(b.x + b.cx >= (table.x + table.cx).min(TEXT_END), "{what}");
            } else {
                assert_eq!(b.x + b.cx, TEXT_END, "{what}");
                assert!(b.x <= table.x.max(TEXT_X), "{what}");
            }
        }
    }
}

fn small_table() -> TableBuilder {
    TableBuilder::new(1)
        .columns(vec![column("a", "A")])
        .body(vec![row(vec![cell("x")])])
}

#[test]
fn a_text_box_starts_at_the_table_unless_its_text_needs_more_room() {
    let t = small_table().title("T").source_note("Note").build();
    let s = boxes(&slide(&t));
    let table = shape(&s, "Table");
    for name in ["Title", "Notes"] {
        let b = shape(&s, name);
        assert_eq!((b.x, b.x + b.cx), (table.x, TEXT_END), "{name}");
    }
    // A title wider than the slide right of the table: the box widens leftward
    // as far as it needs, and both boxes keep to one span, a line tall.
    let title = |t: &str| small_table().title(t).source_note("Note").build();
    let s = boxes(&slide(&title("A title wider than its table")));
    let (b, n) = (shape(&s, "Title"), shape(&s, "Notes"));
    assert!(TEXT_X < b.x && b.x < shape(&s, "Table").x, "{b:?}");
    assert_eq!(b.x + b.cx, TEXT_END);
    assert_eq!((n.x, n.cx), (b.x, b.cx));
    let one = shape(&boxes(&slide(&title("T"))), "Title").cy;
    assert_eq!(b.cy, one);
    // Wider than the slide: the box spans it, and its lines wrap.
    let s = boxes(&slide(&title(&"A title wider than its table ".repeat(4))));
    let b = shape(&s, "Title");
    assert_eq!((b.x, b.x + b.cx), (TEXT_X, TEXT_END));
    assert!(b.cy > 2 * one, "{b:?} vs {one}");
}

#[test]
fn right_to_left_a_text_box_ends_at_the_table() {
    let t = small_table()
        .config(|c| c.dir = Some(TextDirection::Rtl))
        .title("T")
        .footnote("n", "1", "A note")
        .build();
    let s = boxes(&slide(&t));
    let table = shape(&s, "Table");
    for name in ["Title", "Notes"] {
        let b = shape(&s, name);
        assert_eq!((b.x, b.x + b.cx), (TEXT_X, table.x + table.cx), "{name}");
    }
    // A title wider than the slide left of the table: the box widens rightward.
    let t = small_table()
        .config(|c| c.dir = Some(TextDirection::Rtl))
        .title("A title wider than its table")
        .build();
    let s = boxes(&slide(&t));
    let (b, table) = (shape(&s, "Title"), shape(&s, "Table"));
    assert_eq!(b.x, TEXT_X);
    assert!(
        table.x + table.cx < b.x + b.cx && b.x + b.cx < TEXT_END,
        "{b:?}"
    );
}

#[test]
fn a_group_label_row_is_as_tall_as_its_lines() {
    let t = TableBuilder::new(2)
        .columns(vec![
            column("a", "A").width("60pt"),
            column("b", "B").width("60pt"),
        ])
        .head(row(vec![cell("A"), cell("B")]).role("column_label"))
        .group(labeled_group(
            "A group label wider than its table",
            vec![row(vec![cell("x"), cell("y")])],
        ))
        .build();
    let rows = numbers(&slide(&t), "<a:tr h=\"");
    // Head, label, row: the label's row wraps, the others are a line.
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[0], rows[2]), (370_332, 370_332));
    assert!(rows[1] > 2 * rows[0], "{rows:?}");
}

#[test]
fn every_note_line_has_room_in_the_notes_box() {
    let note = |n: usize| {
        let mut b = small_table();
        for _ in 0..n {
            b = b.source_note("A source note");
        }
        shape(&boxes(&slide(&b.build())), "Notes").cy
    };
    // 10pt lines: 12.2pt each, and 7.2pt of insets.
    assert_eq!(note(1), (19.4_f64 * 12700.0).round() as u64);
    assert_eq!(note(3) - note(1), (24.4_f64 * 12700.0).round() as u64);
    // A note too long for a line wraps.
    let t = small_table().source_note(&"word ".repeat(200)).build();
    assert!(shape(&boxes(&slide(&t)), "Notes").cy > 5 * note(1));
}
