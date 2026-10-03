//! Every CSS colour form means the same colour in every writer. Before
//! `gridwell-core` was used everywhere, most writers understood only `#RRGGBB`
//! (silently dropping `red`, `#abc` or `hsl()`), and the ANSI and RTF writers
//! byte-sliced the hex digits, panicking on non-ASCII input such as `#ééé`.

use gridwell_ir::{StyleDef, Table};
use gridwell_testkit::{cell, row, CellExt, TableBuilder};

fn table_with(color: Option<&str>, background: Option<&str>) -> Table {
    TableBuilder::new(2)
        .style_def(
            "c",
            StyleDef {
                color: color.map(Into::into),
                background_color: background.map(Into::into),
                ..Default::default()
            },
        )
        .body(vec![row(vec![cell("COLORED").style("c"), cell("plain")])])
        .build()
}

/// (writer, render, marker for text #9932CC, marker for background #123456)
type WriterCase = (
    &'static str,
    fn(&Table) -> String,
    Option<&'static str>,
    Option<&'static str>,
);

fn writers() -> Vec<WriterCase> {
    vec![
        (
            "html",
            |t| gridwell_writer_html::render_html(t).unwrap(),
            Some("color: #9932CC"),
            Some("background-color: #123456"),
        ),
        (
            "svg",
            |t| gridwell_writer_svg::render_svg(t).unwrap(),
            Some("fill=\"#9932CC\""),
            Some("fill=\"#123456\""),
        ),
        (
            "typst",
            |t| gridwell_writer_typst::render_typst(t).unwrap(),
            Some("rgb(\"#9932CC\")"),
            Some("rgb(\"#123456\")"),
        ),
        (
            "latex",
            |t| gridwell_writer_latex::render_latex(t).unwrap(),
            Some("\\textcolor{[HTML]{9932CC}}"),
            Some("\\cellcolor{[HTML]{123456}}"),
        ),
        (
            "rtf",
            |t| gridwell_writer_rtf::render_rtf(t).unwrap(),
            Some("\\red153\\green50\\blue204;"),
            Some("\\red18\\green52\\blue86;"),
        ),
        (
            "docx",
            |t| {
                gridwell_writer_docx::DocxWriter::new()
                    .render_document_xml(t)
                    .unwrap()
            },
            Some("w:val=\"9932CC\""),
            Some("w:fill=\"123456\""),
        ),
        (
            "pptx",
            |t| {
                gridwell_writer_pptx::PptxWriter::new()
                    .render_slide_xml(t)
                    .unwrap()
            },
            None,
            Some("val=\"123456\""),
        ),
        (
            "ansi",
            |t| gridwell_writer_ansi::render_ansi(t).unwrap(),
            Some("\u{1b}[38;2;153;50;204m"),
            None,
        ),
    ]
}

/// Spellings of #9932CC (text) and #123456 (background).
const TEXT_FORMS: &[&str] = &[
    "#9932CC",
    "#9932cc",
    "darkorchid",
    "DarkOrchid",
    "rgb(153, 50, 204)",
    "rgb(153 50 204)",
    "rgba(153, 50, 204, 1)",
    "hsl(280.13 60.6% 49.8%)",
];
const BACKGROUND_FORMS: &[&str] = &[
    "#123456",
    "rgb(18 52 86)",
    "rgb(18, 52, 86, 100%)",
    "#123456FF",
];

#[test]
fn every_colour_form_reaches_every_writer() {
    for (&text, &bg) in TEXT_FORMS.iter().zip(BACKGROUND_FORMS.iter().cycle()) {
        let t = table_with(Some(text), Some(bg));
        assert!(t.validate().is_empty(), "{text} / {bg}: {:?}", t.validate());
        for (name, render, text_marker, bg_marker) in writers() {
            let out = render(&t);
            if let Some(m) = text_marker {
                assert!(
                    out.contains(m),
                    "{name}: text {text:?} missing {m:?}:\n{out}"
                );
            }
            if let Some(m) = bg_marker {
                assert!(
                    out.contains(m),
                    "{name}: background {bg:?} missing {m:?}:\n{out}"
                );
            }
        }
    }
}

#[test]
fn short_hex_expands() {
    let t = table_with(Some("#abc"), None);
    let html = gridwell_writer_html::render_html(&t).unwrap();
    assert!(html.contains("color: #AABBCC"), "{html}");
    let latex = gridwell_writer_latex::render_latex(&t).unwrap();
    assert!(latex.contains("[HTML]{AABBCC}"), "{latex}");
}

#[test]
fn invalid_and_transparent_colours_paint_nothing_and_never_panic() {
    // `#a€bc` and `#0é000` are six bytes with a code point straddling a two-byte
    // boundary: exactly what the old `&hex[0..2]` slicing panicked on.
    for bad in [
        "#a€bc",
        "#0é000",
        "#ééé",
        "#éé",
        "#😀😀",
        "#€00",
        "nope",
        "transparent",
        "rgba(1, 2, 3, 0)",
        "",
    ] {
        let t = table_with(Some(bad), Some(bad));
        for (name, render, ..) in writers() {
            let out = std::panic::catch_unwind(|| render(&t))
                .unwrap_or_else(|_| panic!("{name} panicked on colour {bad:?}"));
            // No writer may paint the cell black (RTF's old fallback for an
            // unparseable background).
            for black in [
                "\\clcbpat1 ",
                "\\clcbpat1\\",
                "w:fill=\"000000\"",
                "[HTML]{000000}",
            ] {
                assert!(
                    !out.contains(black),
                    "{name}: {bad:?} painted black:\n{out}"
                );
            }
        }
    }
}

#[test]
fn translucent_colours_keep_alpha_where_the_format_has_it() {
    let t = table_with(Some("rgb(153 50 204 / 50%)"), None);
    let html = gridwell_writer_html::render_html(&t).unwrap();
    assert!(html.contains("color: rgba(153, 50, 204, 0.502)"), "{html}");
    let typst = gridwell_writer_typst::render_typst(&t).unwrap();
    assert!(typst.contains("rgb(\"#9932CC80\")"), "{typst}");
    // Formats without alpha keep the colour.
    let latex = gridwell_writer_latex::render_latex(&t).unwrap();
    assert!(latex.contains("[HTML]{9932CC}"), "{latex}");
}
