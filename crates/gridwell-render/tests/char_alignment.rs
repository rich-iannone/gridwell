//! Alignment on a character (`align: char`): every writer ends a cell with the
//! padding the layout computes, in its own way (hidden or unfilled text, room,
//! figure and punctuation spaces), and where the format is laid out here (SVG, and Typst
//! and LaTeX typeset), the characters really line up.

mod support;

use gridwell_ir::Table;
use gridwell_testkit::{cell, column, row, CellExt, ColumnExt, RowExt, TableBuilder};

use support::typeset;

/// One column aligned on the period: one integer digit each, so lined up on
/// the period, the numbers start at one place (right-aligned, they would end
/// at one).
const NUMBERS: [&str; 4] = ["1.5", "2.25", "3.125", "4"];

fn table() -> Table {
    TableBuilder::new(1)
        .columns(vec![column("v", "Value").align("char")])
        .head(row(vec![cell("Value")]).role("column_label"))
        .body(NUMBERS.iter().map(|v| row(vec![cell(v)])).collect())
        .build()
}

fn text(t: &Table, format: &str) -> String {
    match format {
        "docx" => gridwell_writer_docx::DocxWriter::new()
            .render_document_xml(t)
            .unwrap(),
        "pptx" => gridwell_writer_pptx::PptxWriter::new()
            .render_slide_xml(t)
            .unwrap(),
        "xlsx" => {
            let w = gridwell_writer_xlsx::XlsxWriter::new();
            let parts = [
                w.render_sheet_xml(t),
                w.render_styles_xml(t),
                w.render_shared_strings_xml(t),
            ];
            parts.map(Result::unwrap).concat()
        }
        f => gridwell_render::render(t, f, None)
            .unwrap()
            .as_text()
            .unwrap()
            .to_string(),
    }
}

/// U+2007 FIGURE SPACE, U+2008 PUNCTUATION SPACE.
const FIG: &str = "\u{2007}";
const PUNCT: &str = "\u{2008}";

#[test]
fn every_writer_pads_the_cells() {
    let fig2 = FIG.repeat(2);
    let pad4 = format!("{PUNCT}{}", FIG.repeat(3));
    // (format, `1.5`'s padding of two digits, `4`'s of a period and three)
    let cases: &[(&str, String, String)] = &[
        (
            "html",
            "1.5<span class=\"gw__pad\" aria-hidden=\"true\">00</span>".into(),
            "4<span class=\"gw__pad\" aria-hidden=\"true\">.000</span>".into(),
        ),
        (
            "latex",
            "1.5\\phantom{00}".into(),
            "4\\phantom{.000}".into(),
        ),
        (
            "typst",
            "[1\\.5#hide[00]]".into(),
            "[4#hide[\\.000]]".into(),
        ),
        (
            "rtf",
            "\\qr 1.5{\\ulnone\\strike0 \\u8199?\\u8199?}\\cell".into(),
            "\\qr 4{\\ulnone\\strike0 \\u8200?\\u8199?\\u8199?\\u8199?}\\cell".into(),
        ),
        (
            "docx",
            format!("1.5</w:t></w:r><w:r><w:t xml:space=\"preserve\">{fig2}</w:t>"),
            format!("4</w:t></w:r><w:r><w:t xml:space=\"preserve\">{pad4}</w:t>"),
        ),
        (
            "pptx",
            "<a:t>1.5</a:t></a:r><a:r><a:rPr lang=\"en-US\"><a:noFill/></a:rPr><a:t>00</a:t>"
                .into(),
            "<a:t>4</a:t></a:r><a:r><a:rPr lang=\"en-US\"><a:noFill/></a:rPr><a:t>.000</a:t>"
                .into(),
        ),
        ("xlsx", format!("1.5{fig2}"), format!("4{pad4}")),
        ("ansi", " 1.5   │".into(), " 4     │".into()),
    ];
    let t = table();
    for (format, short, bare) in cases {
        let out = text(&t, format);
        assert!(out.contains(short), "{format}: {short:?} missing:\n{out}");
        assert!(out.contains(bare), "{format}: {bare:?} missing:\n{out}");
    }
    // Lined up already: no padding.
    let t = TableBuilder::new(1)
        .columns(vec![column("v", "Value").align("char")])
        .body(vec![row(vec![cell("1.5")]), row(vec![cell("22.5")])])
        .build();
    for format in ["html", "latex", "typst", "rtf", "docx", "pptx", "xlsx"] {
        let out = text(&t, format);
        for pad in [
            "gw__pad", "phantom", "#hide", "noFill", "\\u8199?", FIG, PUNCT,
        ] {
            assert!(!out.contains(pad), "{format}: {pad:?} in:\n{out}");
        }
    }
}

#[test]
fn xlsx_numbers_keep_their_value_and_leave_the_room() {
    let t = TableBuilder::new(1)
        .columns(vec![column("v", "Value").align("char")])
        .body(vec![
            row(vec![cell("1.5").typed("number", 1.5.into())]),
            row(vec![cell("2.125").typed("number", 2.125.into())]),
            row(vec![cell("3").typed("integer", 3.into())]),
            row(vec![cell("−4.25").typed("number", (-4.25).into())]),
        ])
        .build();
    let out = text(&t, "xlsx");
    for code in [
        "formatCode=\"0.0_0_0\"",
        "formatCode=\"0_._0_0_0\"",
        "formatCode=\"0.00_0;&quot;\u{2212}&quot;0.00_0\"",
    ] {
        assert!(out.contains(code), "{code:?} missing:\n{out}");
    }
    // `2.125` needs none, and the values stay numbers.
    assert!(
        out.contains("<v>1.5</v>") && out.contains("<v>-4.25</v>"),
        "{out}"
    );
    assert!(!out.contains(FIG), "{out}");
}

#[test]
fn pandoc_and_quarto_right_align_without_padding() {
    for format in ["pandoc", "quarto"] {
        // The column right-aligned; its cells take that.
        let out = text(&table(), format);
        let compact: String = out.split_whitespace().collect();
        assert!(compact.contains(r#""t":"AlignRight""#), "{format}: {out}");
        assert!(
            !out.contains(FIG) && !out.contains("\\u2007"),
            "{format}: {out}"
        );
    }
}

/// The left edges of `words` that are `NUMBERS`, in their order.
fn starts(words: &[(String, f64, f64)]) -> (Vec<f64>, Vec<f64>) {
    let numbers: Vec<_> = NUMBERS
        .iter()
        .map(|n| {
            let w = words.iter().find(|w| w.0 == *n);
            w.unwrap_or_else(|| panic!("{n} not among {words:?}"))
        })
        .collect();
    (
        numbers.iter().map(|w| w.1).collect(),
        numbers.iter().map(|w| w.2).collect(),
    )
}

/// The numbers start at one place, and (not merely right-aligned) end apart.
fn assert_lined_up(format: &str, starts: &[f64], ends: &[f64], tol: f64) {
    let spread = |v: &[f64]| {
        let (lo, hi) = v
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        hi - lo
    };
    assert!(spread(starts) < tol, "{format}: starts {starts:?}");
    assert!(spread(ends) > 5.0, "{format}: ends {ends:?}");
}

#[test]
fn svg_lines_the_characters_up() {
    let svg = text(&table(), "svg");
    // A right-aligned line ends at `x` and is drawn `textLength` wide.
    let mut words = Vec::new();
    for line in svg.lines().filter(|l| l.starts_with("<text ")) {
        let attr = |name: &str| -> f64 {
            let key = format!(" {name}=\"");
            let i = line.find(&key).unwrap() + key.len();
            line[i..i + line[i..].find('"').unwrap()].parse().unwrap()
        };
        assert!(line.contains("text-anchor=\"end\""), "{line}");
        let text = &line[line.find('>').unwrap() + 1..line.rfind("</text>").unwrap()];
        let (x, len) = (attr("x"), attr("textLength"));
        words.push((text.to_string(), x - len, x));
    }
    let (starts, ends) = starts(&words);
    assert_lined_up("svg", &starts, &ends, 0.05);
}

#[test]
fn typst_lines_the_characters_up() {
    if !typeset::require("typst", "GRIDWELL_REQUIRE_TYPST")
        || !typeset::require("pdftotext", "GRIDWELL_REQUIRE_TYPST")
    {
        return;
    }
    let src = text(&table(), "typst");
    let words = typeset::typst_words(&src).unwrap();
    let (starts, ends) = starts(&words);
    assert_lined_up("typst", &starts, &ends, 0.05);
}

#[test]
fn latex_lines_the_characters_up() {
    if !typeset::require("lualatex", "GRIDWELL_REQUIRE_LATEX")
        || !typeset::require("pdftotext", "GRIDWELL_REQUIRE_LATEX")
    {
        return;
    }
    let src = text(&table(), "latex");
    let tabular = typeset::latex_tabular(&src).unwrap();
    let words = typeset::latex_words(&tabular).unwrap();
    let (starts, ends) = starts(&words);
    assert_lined_up("latex", &starts, &ends, 0.05);
}
