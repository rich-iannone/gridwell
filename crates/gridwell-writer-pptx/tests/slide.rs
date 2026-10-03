//! Slide geometry: the title box, the table and the notes box are stacked top to
//! bottom without overlapping, text boxes stay on the slide, and the header and
//! notes text is all there.

use gridwell_testkit::examples;
use gridwell_writer_pptx::PptxWriter;

const SLIDE_H: u64 = 6_858_000;

/// (name, y, height) for each shape, in document order.
fn shapes(xml: &str) -> Vec<(String, u64, u64)> {
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
        out.push((name, num("y=\""), num("cy=\"")));
    }
    out
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
