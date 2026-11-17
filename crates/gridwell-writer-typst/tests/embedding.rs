//! In a document: the table is one block, so it can be included or pasted
//! anywhere. The text after it starts a new paragraph, its set rules stay inside
//! it, a figure doesn't center its notes, the heading stays on the page where the
//! table starts, and on a page of its own it is laid out as before it was a block.
//! Each is compiled with a document that `#include`s the table (as a Quarto raw
//! Typst block can) or holds its source, and read back with `pdftotext -bbox`.
//!
//! Skipped when the tools are missing, unless `GRIDWELL_REQUIRE_TYPST` is set. The
//! page-break test needs Typst 0.13 or later (0.12 ignores a sticky block before a
//! table) and is skipped on older versions.

use std::path::{Path, PathBuf};
use std::process::Command;

use gridwell_ir::style::StyleDef;
use gridwell_ir::Table;
use gridwell_testkit::{cell, row, TableBuilder};
use gridwell_writer_typst::render_typst;

fn require(tool: &str) -> bool {
    if Command::new(tool).arg("-v").output().is_ok() {
        return true;
    }
    if std::env::var_os("GRIDWELL_REQUIRE_TYPST").is_some_and(|v| !v.is_empty()) {
        panic!("{tool} not found but GRIDWELL_REQUIRE_TYPST is set");
    }
    eprintln!("skipping: {tool} not found");
    false
}

fn tools() -> bool {
    require("typst") && require("pdftotext")
}

/// Typst's (major, minor) version.
fn typst_version() -> (u32, u32) {
    let out = Command::new("typst").arg("--version").output().unwrap();
    let out = String::from_utf8_lossy(&out.stdout);
    let version = out.split_whitespace().nth(1).unwrap_or_default();
    let mut parts = version.split('.').map(|p| p.parse().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gridwell-typst-embedding-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A word of the PDF: its page (from 0), its box, and its text.
#[derive(Debug, Clone)]
struct Word {
    page: usize,
    x_min: f64,
    y_min: f64,
    y_max: f64,
    text: String,
}

/// Compile `document` beside `table.typ` (the table) and read back its words.
fn words(dir: &Path, table: &Table, document: &str) -> Vec<Word> {
    words_of(dir, &render_typst(table).unwrap(), document)
}

/// Compile `document` beside `table.typ` (`source`) and read back its words.
fn words_of(dir: &Path, source: &str, document: &str) -> Vec<Word> {
    std::fs::write(dir.join("table.typ"), source).unwrap();
    let typ = dir.join("doc.typ");
    let pdf = dir.join("doc.pdf");
    std::fs::write(&typ, document).unwrap();
    let out = Command::new("typst")
        .arg("compile")
        .arg(&typ)
        .arg(&pdf)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{document}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new("pdftotext")
        .arg("-bbox")
        .arg(&pdf)
        .arg("-")
        .output()
        .unwrap();
    assert!(out.status.success());
    let attr = |line: &str, name: &str| -> f64 {
        let start = line.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        let len = line[start..].find('"').unwrap();
        line[start..start + len].parse().unwrap()
    };
    let mut page = None;
    let mut words = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if line.starts_with("<page ") {
            page = Some(page.map_or(0, |p| p + 1));
        } else if let Some(rest) = line.strip_prefix("<word ") {
            let text = &rest[rest.find('>').unwrap() + 1..rest.rfind("</word>").unwrap()];
            words.push(Word {
                page: page.unwrap(),
                x_min: attr(line, "xMin"),
                y_min: attr(line, "yMin"),
                y_max: attr(line, "yMax"),
                text: text.to_string(),
            });
        }
    }
    words
}

fn word<'w>(words: &'w [Word], text: &str) -> &'w Word {
    words
        .iter()
        .find(|w| w.text == text)
        .unwrap_or_else(|| panic!("no word {text:?} in {words:#?}"))
}

/// A titled table, wider than its notes, with `rows` body rows.
fn table(rows: usize) -> Table {
    TableBuilder::new(3)
        .title("Heading")
        .subtitle("Subheading")
        .head(row(vec![cell("Name"), cell("Value"), cell("Date")]))
        .body(
            (0..rows)
                .map(|r| {
                    row(vec![
                        cell(&format!("r{r}")),
                        cell("1,234.56"),
                        cell("September 15, 2015"),
                    ])
                })
                .collect(),
        )
        .footnote("f1", "1", "Footnote")
        .source_note("Source")
        .build()
}

#[test]
fn the_output_is_one_block_with_a_sticky_heading() {
    let src = render_typst(&table(2)).unwrap();
    assert!(
        src.starts_with("#block[\n#set align(left)\n#block(sticky: true, below: 1.2 * 12pt)[\n"),
        "{src}"
    );
    assert!(src.ends_with("\n]\n"), "{src}");
    // No heading, no heading block.
    let bare = TableBuilder::new(1)
        .head(row(vec![cell("H")]))
        .body(vec![row(vec![cell("x")])])
        .build();
    let src = render_typst(&bare).unwrap();
    assert!(
        src.starts_with("#block[\n#set align(left)\n#table(") && !src.contains("sticky"),
        "{src}"
    );
}

#[test]
fn text_after_the_table_is_a_paragraph_of_its_own() {
    if !tools() {
        return;
    }
    // As Pandoc writes a raw block: no blank line before the next paragraph.
    let words = words(
        &work_dir("paragraph"),
        &table(2),
        "Before.\n#include \"table.typ\"\nAfter.\n",
    );
    let (before, heading) = (word(&words, "Before."), word(&words, "Heading"));
    let (source, after) = (word(&words, "Source"), word(&words, "After."));
    assert!(before.y_max <= heading.y_min, "{before:?} {heading:?}");
    assert!(source.y_max <= after.y_min, "{source:?} {after:?}");
}

#[test]
fn in_a_figure_the_notes_line_up_with_the_heading() {
    if !tools() {
        return;
    }
    let words = words(
        &work_dir("figure"),
        &table(2),
        "#figure(include \"table.typ\", caption: [Caption])\n",
    );
    // The figure centers the table: the heading and the notes are at its left
    // edge, not each centered.
    let left = word(&words, "Heading").x_min;
    assert!(left > 100.0, "the table isn't centered: {words:#?}");
    for text in ["Subheading", "Source"] {
        let w = word(&words, text);
        assert!((w.x_min - left).abs() < 0.5, "{w:?} vs {left}");
    }
    // The footnote starts with its mark (Typst 0.12 draws `¹`, later `1`).
    let note = words.iter().position(|w| w.text == "Footnote").unwrap();
    let mark = &words[note - 1];
    assert!((mark.x_min - left).abs() < 0.5, "{mark:?} vs {left}");
}

#[test]
fn the_heading_stays_on_the_page_where_the_table_starts() {
    if !tools() {
        return;
    }
    if typst_version() < (0, 13) {
        eprintln!("skipping: Typst 0.12 ignores a sticky block before a table");
        return;
    }
    let dir = work_dir("sticky");
    // A short table and one longer than a page: the heading goes with the first
    // rows, and the long table still breaks.
    for rows in [3, 30] {
        let mut moved = false;
        for step in 0..40 {
            let space = f64::from(step) * 0.25;
            let doc = format!(
                "#set page(width: 14cm, height: 14cm, margin: 1cm)\n#v({space}cm)\n\
                 #include \"table.typ\"\nAfter.\n"
            );
            let words = words(&dir, &table(rows), &doc);
            let pages: Vec<usize> = ["Heading", "Subheading", "Name", "r0"]
                .iter()
                .map(|t| word(&words, t).page)
                .collect();
            assert!(
                pages.iter().all(|&p| p == pages[0]),
                "{rows} rows after {space}cm: the heading, labels and first row are on pages {pages:?}"
            );
            moved |= pages[0] == 1;
            if rows == 30 {
                let last = word(&words, &format!("r{}", rows - 1)).page;
                assert!(last > pages[0], "the long table didn't break");
            }
        }
        assert!(
            moved,
            "{rows} rows: no space put the table on the next page"
        );
    }
}

#[test]
fn on_a_page_of_its_own_the_table_is_laid_out_as_without_the_blocks() {
    if !tools() {
        return;
    }
    // The blocks' lines removed: the heading lines' paragraphs, then the table.
    let unwrapped = |src: &str| -> String {
        src.lines()
            .filter(|l| !l.starts_with("#block") && *l != "]")
            .map(|l| format!("{l}\n"))
            .collect()
    };
    let titled = || {
        TableBuilder::new(1)
            .title("Heading")
            .head(row(vec![cell("Name")]))
            .body(vec![row(vec![cell("x")])])
            .source_note("Source")
    };
    let sized = |size: &str| StyleDef {
        font_size: Some(size.into()),
        ..StyleDef::default()
    };
    // A styled size, absolute or relative to the text's, is the last line's.
    let mut em = titled().style_def("s", sized("150%")).build();
    em.header.as_mut().unwrap().title.as_mut().unwrap().style_id = Some("s".into());
    let mut px = titled()
        .subtitle("Subheading")
        .style_def("s", sized("20px"))
        .build();
    px.header
        .as_mut()
        .unwrap()
        .subtitle
        .as_mut()
        .unwrap()
        .style_id = Some("s".into());
    let dir = work_dir("spacing");
    for (name, table) in [
        ("title", titled().build()),
        ("title and subtitle", table(2)),
        ("em title", em),
        ("px subtitle", px),
    ] {
        let src = render_typst(&table).unwrap();
        let doc = "#include \"table.typ\"\n";
        let wrapped = words_of(&dir, &src, doc);
        let bare = words_of(&dir, &unwrapped(&src), doc);
        assert_eq!(wrapped.len(), bare.len(), "{name}");
        for (w, b) in wrapped.iter().zip(&bare) {
            assert!(
                w.text == b.text
                    && (w.x_min - b.x_min).abs() < 0.01
                    && (w.y_min - b.y_min).abs() < 0.01,
                "{name}: {w:?} vs {b:?}\n{src}"
            );
        }
    }
}

#[test]
fn pasted_into_a_document_its_set_rules_stay_inside_it() {
    if !tools() {
        return;
    }
    // A raw Typst block can hold the table itself, not an include of it.
    let src = render_typst(&table(2)).unwrap();
    let doc = format!("#set align(center)\nBefore.\n{src}After.\n");
    let words = words_of(&work_dir("pasted"), "", &doc);
    let (source, after) = (word(&words, "Source"), word(&words, "After."));
    assert!(source.y_max <= after.y_min, "{source:?} {after:?}");
    // The document's alignment, not the table's.
    assert!(after.x_min > 250.0, "{after:?}");
}
