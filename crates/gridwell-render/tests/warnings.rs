//! Warnings: what a format can't express of a valid table, for one format or
//! every format, as each format's writer reports it; formats and tables are
//! checked as `render` checks them.

use gridwell_ir::style::{BorderSet, StyleDef};
use gridwell_ir::Table;
use gridwell_render::{warnings, Place, RenderError, SectionKind, Warning};
use gridwell_testkit::{
    border, cell, cell_content, examples, line_break, row, text, CellExt, TableBuilder,
};

/// Cells `a` (bottom-aligned, in Georgia) and `b` (bottom-aligned, outlined
/// double), and `c` with neither, two lines high.
fn table() -> Table {
    let double = border("3px", "double", "#000000");
    TableBuilder::new(3)
        .style_def(
            "a",
            StyleDef {
                vertical_align: Some("bottom".into()),
                font_family: Some("Georgia".into()),
                ..Default::default()
            },
        )
        .style_def(
            "b",
            StyleDef {
                vertical_align: Some("bottom".into()),
                border: Some(BorderSet {
                    top: Some(double.clone()),
                    right: None,
                    bottom: Some(double),
                    left: None,
                }),
                ..Default::default()
            },
        )
        .head(row(vec![cell("A"), cell("B"), cell("C")]))
        .body(vec![row(vec![
            cell("a").style("a"),
            cell("b").style("b"),
            cell_content(vec![text("c"), line_break(), text("c")]),
        ])])
        .build()
}

fn body(c: usize) -> Place {
    Place::Cell {
        section: SectionKind::Body,
        group: Some(0),
        row: 0,
        col: c,
    }
}

fn summary(w: &[Warning]) -> Vec<(&str, &str, Vec<Place>)> {
    w.iter()
        .map(|w| (w.format, w.feature, w.places.clone()))
        .collect()
}

#[test]
fn one_format_or_every_format() {
    let t = table();
    let latex = vec![
        ("latex", "font-family", vec![body(0)]),
        ("latex", "vertical-align", vec![body(0), body(1)]),
    ];
    let typst = vec![("typst", "border-double", vec![body(1)])];
    assert_eq!(summary(&warnings(&t, Some("latex")).unwrap()), latex);
    // Looked up as `render` looks formats up.
    assert_eq!(summary(&warnings(&t, Some(" LaTeX ")).unwrap()), latex);
    assert_eq!(summary(&warnings(&t, Some("typst")).unwrap()), typst);
    for f in ["html", "rtf", "docx", "xlsx", "pptx", "svg", "ansi"] {
        assert_eq!(warnings(&t, Some(f)).unwrap(), [], "{f}");
    }
    // In registry order.
    let all: Vec<_> = latex.into_iter().chain(typst).collect();
    assert_eq!(summary(&warnings(&t, None).unwrap()), all);
}

#[test]
fn a_warning_reads_as_a_sentence() {
    let shown: Vec<String> = warnings(&table(), None)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        shown,
        [
            "[UNSUPPORTED_FEATURE] latex can't express font-family (group 1 row 1, \
             column 1): a named system font needs XeLaTeX/LuaLaTeX with fontspec; the \
             output also targets pdfLaTeX (monospace families map to `\\texttt`)",
            "[UNSUPPORTED_FEATURE] latex can't express vertical-align (group 1 row 1, \
             column 1 and 1 other place): a tabular row sets its cells on one baseline; \
             `m`/`b` columns align whole columns, not single cells",
            "[UNSUPPORTED_FEATURE] typst can't express border-double (group 1 row 1, \
             column 2): Typst strokes have no double line; a double border is drawn solid",
        ]
    );
    let mut w = warnings(&table(), Some("typst")).unwrap().remove(0);
    w.places = vec![Place::Title, Place::GroupLabel(1), Place::SourceNote(0)];
    assert!(
        w.to_string()
            .contains("(the title and 2 other places): Typst"),
        "{w}"
    );
    assert_eq!(Warning::RULE, "UNSUPPORTED_FEATURE");
}

#[test]
fn formats_and_tables_are_checked() {
    let mut invalid = table();
    invalid.config.table_cols = 4;
    let err = warnings(&invalid, Some("pdf")).unwrap_err();
    assert!(matches!(err, RenderError::UnknownFormat { .. }), "{err}");
    for format in [Some("latex"), None] {
        let err = warnings(&invalid, format).unwrap_err();
        assert!(matches!(err, RenderError::InvalidTable(_)), "{err}");
    }
}

#[test]
fn the_corpus_warns_only_of_its_borders_its_rtl_table_and_its_image() {
    for ex in examples() {
        let found: Vec<_> = warnings(&ex.table(), None)
            .unwrap()
            .iter()
            .map(|w| (w.format, w.feature))
            .collect();
        let want: &[(&str, &str)] = match ex.name {
            "borders-dashed" => &[("latex", "border-styles")],
            "borders-double" => &[("typst", "border-double")],
            "rtl-table" => &[
                ("latex", "lang"),
                ("latex", "rtl"),
                ("rtf", "lang"),
                ("ansi", "rtl"),
                ("xlsx", "lang"),
            ],
            // Formats that can't draw an image.
            "inline-image" => &[("latex", "image"), ("ansi", "image"), ("pptx", "image")],
            _ => &[],
        };
        assert_eq!(found, want, "{}", ex.name);
    }
}

#[test]
fn a_language_or_a_right_to_left_table_is_warned_of_once_for_the_table() {
    use gridwell_ir::TextDirection;
    use gridwell_testkit::examples::directed;
    let found = |t: &Table| -> Vec<(&str, &str)> {
        let w = warnings(t, None).unwrap();
        assert!(w.iter().all(|w| w.places == [Place::Table]), "{w:?}");
        w.iter().map(|w| (w.format, w.feature)).collect()
    };
    assert_eq!(
        found(&directed(Some(TextDirection::Rtl), Some("ar"))),
        [
            ("latex", "lang"),
            ("latex", "rtl"),
            ("rtf", "lang"),
            ("ansi", "rtl"),
            ("xlsx", "lang")
        ]
    );
    assert_eq!(
        found(&directed(Some(TextDirection::Rtl), None)),
        [("latex", "rtl"), ("ansi", "rtl")]
    );
    assert_eq!(
        found(&directed(Some(TextDirection::Ltr), Some("de"))),
        [("latex", "lang"), ("rtf", "lang"), ("xlsx", "lang")]
    );
    // `auto` is right to left only for right-to-left text.
    assert_eq!(found(&directed(Some(TextDirection::Auto), None)), []);
    let mut arabic = directed(Some(TextDirection::Auto), None);
    arabic
        .header
        .as_mut()
        .unwrap()
        .title
        .as_mut()
        .unwrap()
        .content = vec![text("جدول")];
    assert_eq!(found(&arabic), [("latex", "rtl"), ("ansi", "rtl")]);

    let w = warnings(&directed(Some(TextDirection::Rtl), None), Some("ansi")).unwrap();
    assert_eq!(
        w[0].to_string(),
        "[UNSUPPORTED_FEATURE] ansi can't express rtl (the table): a terminal shows columns \
         left to right, and whether it reorders right-to-left text is up to the terminal"
    );
}
