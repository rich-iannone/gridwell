//! The example corpus. Each `pub fn` builds one valid IR table; [`all`]
//! assembles them into the ordered registry with metadata.
//!
//! Span note: a cell's index in a row equals its starting grid column, so a
//! `colspan(N)` cell is followed by `N-1` [`placeholder`]s, and a `rowspan(M)`
//! cell is followed by a [`placeholder`] at the same index in the next `M-1`
//! rows. Every row carries exactly `table_cols` cell objects.

use crate::builder::*;
use crate::{Category, Example, Feature};
use gridwell_ir::style::{ConditionalSelector, StyleDef};
use gridwell_ir::Table;
use serde_json::json;

use Category::*;
use Feature as F;

/// The full ordered corpus.
pub fn all() -> Vec<Example> {
    vec![
        // ── structure ──
        ex(
            "minimal-1x1",
            Structure,
            "Smallest valid table: one header, one body cell",
            &[F::Header],
            minimal_1x1,
        ),
        ex(
            "plain-3x3",
            Structure,
            "Baseline 3x3 table with a header row",
            &[F::Header],
            plain_3x3,
        ),
        ex(
            "single-column",
            Structure,
            "One column, several body rows",
            &[F::Header],
            single_column,
        ),
        ex(
            "single-row",
            Structure,
            "One body row across several columns",
            &[F::Header],
            single_row,
        ),
        ex(
            "no-header",
            Structure,
            "Body only — no title, subtitle, or column-label row",
            &[],
            no_header,
        ),
        ex(
            "header-only-empty-body",
            Structure,
            "Column labels but zero body rows",
            &[F::Header],
            header_only_empty_body,
        ),
        ex(
            "wide-50-cols",
            Structure,
            "50 columns to stress horizontal layout",
            &[F::Header, F::Wide],
            wide_50_cols,
        ),
        ex(
            "tall-100-rows",
            Structure,
            "100 body rows to stress vertical layout / paging",
            &[F::Header, F::Tall],
            tall_100_rows,
        ),
        // ── spans ──
        ex(
            "colspan-basic",
            Spans,
            "A spanner over two of three columns",
            &[F::Header, F::Spanner, F::Colspan],
            colspan_basic,
        ),
        ex(
            "colspan-full-width",
            Spans,
            "A spanner across every column",
            &[F::Header, F::Spanner, F::Colspan],
            colspan_full_width,
        ),
        ex(
            "rowspan-basic",
            Spans,
            "A stub cell merged across two body rows",
            &[F::Header, F::Stub, F::Rowspan],
            rowspan_basic,
        ),
        ex(
            "nested-spanners",
            Spans,
            "Two-level header: spanners over sub-spanners",
            &[F::Header, F::Spanner, F::Colspan],
            nested_spanners,
        ),
        ex(
            "mixed-spans",
            Spans,
            "A body cell spanning two rows and two columns",
            &[F::Colspan, F::Rowspan],
            mixed_spans,
        ),
        // ── groups / stub ──
        ex(
            "single-stub",
            Groups,
            "A single stub column of row labels",
            &[F::Header, F::Stub],
            single_stub,
        ),
        ex(
            "multi-col-stub",
            Groups,
            "Two stub columns",
            &[F::Header, F::Stub],
            multi_col_stub,
        ),
        ex(
            "two-row-groups",
            Groups,
            "Two labeled row groups",
            &[F::Header, F::Stub, F::RowGroup],
            two_row_groups,
        ),
        ex(
            "many-row-groups",
            Groups,
            "Five labeled row groups",
            &[F::Header, F::Stub, F::RowGroup],
            many_row_groups,
        ),
        ex(
            "group-summary",
            Groups,
            "Per-group summary rows",
            &[F::Header, F::Stub, F::RowGroup, F::Summary],
            group_summary,
        ),
        ex(
            "grand-summary",
            Groups,
            "A trailing grand-summary group",
            &[F::Header, F::Stub, F::Summary],
            grand_summary,
        ),
        // ── footnotes / notes ──
        ex(
            "footnote-single",
            Footnotes,
            "One footnote mark and definition",
            &[F::Header, F::Footnote],
            footnote_single,
        ),
        ex(
            "footnote-multiple",
            Footnotes,
            "Several marks referencing several definitions",
            &[F::Header, F::Footnote],
            footnote_multiple,
        ),
        ex(
            "source-notes",
            Footnotes,
            "Two source notes in the footer",
            &[F::Header, F::SourceNote],
            source_notes,
        ),
        ex(
            "footnote-in-stub",
            Footnotes,
            "A footnote mark inside a stub cell",
            &[F::Header, F::Stub, F::Footnote],
            footnote_in_stub,
        ),
        ex(
            "footnote-and-source",
            Footnotes,
            "Footnotes and source notes together",
            &[F::Header, F::Footnote, F::SourceNote],
            footnote_and_source,
        ),
        // ── styling ──
        ex(
            "borders-solid",
            Styling,
            "Solid borders on all cells",
            &[F::Header, F::Borders],
            borders_solid,
        ),
        ex(
            "borders-dashed",
            Styling,
            "Dashed borders",
            &[F::Header, F::Borders],
            borders_dashed,
        ),
        ex(
            "borders-double",
            Styling,
            "Double borders",
            &[F::Header, F::Borders],
            borders_double,
        ),
        ex(
            "striping-basic",
            Styling,
            "Zebra row striping",
            &[F::Header, F::Striping],
            striping_basic,
        ),
        ex(
            "cell-colors",
            Styling,
            "Foreground and background cell colors",
            &[F::Header, F::Colors],
            cell_colors,
        ),
        ex(
            "style-composition",
            Styling,
            "A composition extending a base style",
            &[F::Header, F::StyleComposition, F::Colors],
            style_composition,
        ),
        ex(
            "conditional-striping",
            Styling,
            "Row-parity conditional style",
            &[F::Header, F::Conditional],
            conditional_striping,
        ),
        // ── widths ──
        ex(
            "column-widths-mixed",
            Widths,
            "px, %, fr, and auto widths mixed",
            &[F::Header, F::ColumnWidth],
            column_widths_mixed,
        ),
        ex(
            "min-max-width",
            Widths,
            "min-width / max-width constraints",
            &[F::Header, F::ColumnWidth],
            min_max_width,
        ),
        ex(
            "fixed-table-width",
            Widths,
            "A fixed overall table width",
            &[F::Header, F::ColumnWidth],
            fixed_table_width,
        ),
        ex(
            "hidden-columns",
            Widths,
            "A hidden column between visible ones",
            &[F::Header, F::HiddenColumn],
            hidden_columns,
        ),
        ex(
            "narrow-wide-mix",
            Widths,
            "A very narrow column beside a wide one",
            &[F::Header, F::ColumnWidth],
            narrow_wide_mix,
        ),
        // ── content ──
        ex(
            "styled-text",
            Content,
            "Inline styled_text runs",
            &[F::Header, F::StyledText],
            styled_text_ex,
        ),
        ex(
            "line-breaks",
            Content,
            "Hard line breaks within cells",
            &[F::Header, F::LineBreak],
            line_breaks,
        ),
        ex(
            "inline-image",
            Content,
            "An inline image in a cell",
            &[F::Header, F::Image],
            inline_image,
        ),
        ex(
            "raw-html",
            Content,
            "Raw HTML markup content",
            &[F::Header, F::RawHtml],
            raw_html,
        ),
        ex(
            "typed-values",
            Content,
            "Cells carrying typed_value + sort_key",
            &[F::Header, F::TypedValue],
            typed_values,
        ),
        ex(
            "mixed-markup",
            Content,
            "Text, styled_text, break, and mark in one cell",
            &[F::Header, F::StyledText, F::LineBreak, F::Footnote],
            mixed_markup,
        ),
        // ── alignment ──
        ex(
            "align-left-center-right",
            Alignment,
            "Three columns, one per alignment",
            &[F::Header, F::Alignment],
            align_lcr,
        ),
        ex(
            "decimal-char-align",
            Alignment,
            "Decimal (char) alignment on a numeric column",
            &[F::Header, F::Alignment],
            decimal_char_align,
        ),
        ex(
            "per-column-align",
            Alignment,
            "Distinct alignment per column",
            &[F::Header, F::Alignment],
            per_column_align,
        ),
        ex(
            "numeric-right-align",
            Alignment,
            "Right-aligned numeric columns",
            &[F::Header, F::Alignment, F::TypedValue],
            numeric_right_align,
        ),
        // ── i18n ──
        ex(
            "unicode-cjk",
            I18n,
            "CJK characters in title and cells",
            &[F::Header, F::Cjk],
            unicode_cjk,
        ),
        ex(
            "unicode-rtl",
            I18n,
            "Right-to-left (Arabic/Hebrew) text",
            &[F::Header, F::Rtl],
            unicode_rtl,
        ),
        ex(
            "unicode-emoji",
            I18n,
            "Emoji in cells",
            &[F::Header, F::Emoji],
            unicode_emoji,
        ),
        ex(
            "combining-chars",
            I18n,
            "Combining diacritical marks",
            &[F::Header],
            combining_chars,
        ),
        ex(
            "long-word-wrap",
            I18n,
            "A very long unbreakable token",
            &[F::Header, F::ColumnWidth],
            long_word_wrap,
        ),
        // ── realism ──
        ex(
            "regional-sales",
            Realism,
            "Groups, spanners, rowspan, summary, footnote, source note",
            &[
                F::Header,
                F::Spanner,
                F::Colspan,
                F::Rowspan,
                F::Stub,
                F::RowGroup,
                F::Summary,
                F::Footnote,
                F::SourceNote,
            ],
            regional_sales,
        ),
        ex(
            "financial-report",
            Realism,
            "Formatted currency report with striping and totals",
            &[F::Header, F::Stub, F::Striping, F::Summary, F::Colors],
            financial_report,
        ),
        ex(
            "scientific-table",
            Realism,
            "Measurements with units, footnotes, and right alignment",
            &[F::Header, F::Stub, F::Footnote, F::Alignment],
            scientific_table,
        ),
    ]
}

fn ex(
    name: &'static str,
    category: Category,
    description: &'static str,
    tags: &'static [Feature],
    build: fn() -> Table,
) -> Example {
    Example {
        name,
        category,
        description,
        tags,
        build,
    }
}

// ─────────────────────────────── structure ───────────────────────────────

fn minimal_1x1() -> Table {
    TableBuilder::new(1)
        .head(row(vec![cell("Value")]).role("column_label"))
        .body(vec![row(vec![cell("Hello")])])
        .build()
}

fn plain_3x3() -> Table {
    TableBuilder::new(3)
        .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
        .body(vec![
            row(vec![cell("1"), cell("2"), cell("3")]),
            row(vec![cell("4"), cell("5"), cell("6")]),
            row(vec![cell("7"), cell("8"), cell("9")]),
        ])
        .build()
}

fn single_column() -> Table {
    TableBuilder::new(1)
        .head(row(vec![cell("Item")]).role("column_label"))
        .body(vec![
            row(vec![cell("First")]),
            row(vec![cell("Second")]),
            row(vec![cell("Third")]),
        ])
        .build()
}

fn single_row() -> Table {
    TableBuilder::new(4)
        .head(row(vec![cell("W"), cell("X"), cell("Y"), cell("Z")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3"), cell("4")])])
        .build()
}

fn no_header() -> Table {
    TableBuilder::new(2)
        .body(vec![
            row(vec![cell("a"), cell("b")]),
            row(vec![cell("c"), cell("d")]),
        ])
        .build()
}

fn header_only_empty_body() -> Table {
    TableBuilder::new(3)
        .title("Empty")
        .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
        .body(vec![])
        .build()
}

fn wide_50_cols() -> Table {
    let n = 50u32;
    let header = row((0..n).map(|i| cell(&format!("c{i}"))).collect()).role("column_label");
    let make_row = |base: u32| row((0..n).map(|i| cell(&(base + i).to_string())).collect());
    TableBuilder::new(n)
        .head(header)
        .body(vec![make_row(0), make_row(100), make_row(200)])
        .build()
}

fn tall_100_rows() -> Table {
    let mut rows = Vec::new();
    for i in 0..100u32 {
        rows.push(row(vec![
            cell(&format!("row {i}")),
            cell(&(i * 2).to_string()),
            cell(&(i * i).to_string()),
        ]));
    }
    TableBuilder::new(3)
        .title("Sequence")
        .head(row(vec![cell("n"), cell("2n"), cell("n^2")]).role("column_label"))
        .body(rows)
        .build()
}

// ───────────────────────────────── spans ─────────────────────────────────

fn colspan_basic() -> Table {
    TableBuilder::new(3)
        .columns(vec![
            column("col_0", "Name"),
            column("col_1", "Q1").align("right"),
            column("col_2", "Q2").align("right"),
        ])
        .head(
            row(vec![
                cell_content(vec![]),
                cell("Scores").colspan(2).scope("colgroup"),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("Name"), cell("Q1"), cell("Q2")]).role("column_label"))
        .body(vec![
            row(vec![cell("Alice"), cell("90"), cell("95")]),
            row(vec![cell("Bob"), cell("85"), cell("88")]),
        ])
        .build()
}

fn colspan_full_width() -> Table {
    TableBuilder::new(3)
        .head(
            row(vec![
                cell("Full-width spanner").colspan(3).scope("colgroup"),
                placeholder(),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3")])])
        .build()
}

fn rowspan_basic() -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .columns(vec![
            column("col_0", "Country"),
            column("col_1", "Value").align("right"),
        ])
        .head(row(vec![cell("Country").stub(), cell("Value")]).role("column_label"))
        .body(vec![
            row(vec![cell("Japan").rowspan(2).stub(), cell("100")]),
            row(vec![placeholder(), cell("110")]),
            row(vec![cell("Korea").stub(), cell("80")]),
        ])
        .build()
}

fn nested_spanners() -> Table {
    TableBuilder::new(4)
        .head(
            row(vec![
                cell("Group A").colspan(2).scope("colgroup"),
                placeholder(),
                cell("Group B").colspan(2).scope("colgroup"),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("A1"), cell("A2"), cell("B1"), cell("B2")]).role("column_label"))
        .body(vec![
            row(vec![cell("1"), cell("2"), cell("3"), cell("4")]),
            row(vec![cell("5"), cell("6"), cell("7"), cell("8")]),
        ])
        .build()
}

fn mixed_spans() -> Table {
    TableBuilder::new(3)
        .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
        .body(vec![
            row(vec![
                cell("2x2 block").colspan(2).rowspan(2),
                placeholder(),
                cell("top-right"),
            ]),
            row(vec![placeholder(), placeholder(), cell("mid-right")]),
            row(vec![cell("x"), cell("y"), cell("z")]),
        ])
        .build()
}

// ──────────────────────────────── groups ─────────────────────────────────

fn single_stub() -> Table {
    TableBuilder::new(3)
        .stub_cols(1)
        .columns(vec![
            column("col_0", "Region"),
            column("col_1", "Sales").align("right"),
            column("col_2", "Growth").align("right"),
        ])
        .head(row(vec![cell("Region").stub(), cell("Sales"), cell("Growth")]).role("column_label"))
        .body(vec![
            row(vec![cell("East").stub(), cell("120"), cell("4%")]),
            row(vec![cell("West").stub(), cell("98"), cell("2%")]),
        ])
        .build()
}

fn multi_col_stub() -> Table {
    TableBuilder::new(4)
        .stub_cols(2)
        .head(
            row(vec![
                cell("Region").stub(),
                cell("City").stub(),
                cell("Sales"),
                cell("Growth"),
            ])
            .role("column_label"),
        )
        .body(vec![
            row(vec![
                cell("East").stub(),
                cell("NYC").stub(),
                cell("60"),
                cell("3%"),
            ]),
            row(vec![
                cell("West").stub(),
                cell("LA").stub(),
                cell("50"),
                cell("2%"),
            ]),
        ])
        .build()
}

fn two_row_groups() -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("Country").stub(), cell("Value")]).role("column_label"))
        .group(labeled_group(
            "North America",
            vec![
                row(vec![cell("USA").stub(), cell("1250")]),
                row(vec![cell("Canada").stub(), cell("420")]),
            ],
        ))
        .group(labeled_group(
            "Europe",
            vec![
                row(vec![cell("Germany").stub(), cell("890")]),
                row(vec![cell("France").stub(), cell("610")]),
            ],
        ))
        .build()
}

fn many_row_groups() -> Table {
    let mut b = TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("Key").stub(), cell("Value")]).role("column_label"));
    for g in 1..=5 {
        b = b.group(labeled_group(
            &format!("Group {g}"),
            vec![
                row(vec![
                    cell(&format!("k{g}a")).stub(),
                    cell(&(g * 10).to_string()),
                ]),
                row(vec![
                    cell(&format!("k{g}b")).stub(),
                    cell(&(g * 20).to_string()),
                ]),
            ],
        ));
    }
    b.build()
}

fn group_summary() -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("Item").stub(), cell("Amount")]).role("column_label"))
        .group(
            labeled_group(
                "Q1",
                vec![
                    row(vec![cell("Jan").stub(), cell("10")]),
                    row(vec![cell("Feb").stub(), cell("20")]),
                ],
            )
            .summary(vec![
                row(vec![cell("Subtotal").stub(), cell("30")]).role("summary")
            ]),
        )
        .group(
            labeled_group(
                "Q2",
                vec![
                    row(vec![cell("Apr").stub(), cell("15")]),
                    row(vec![cell("May").stub(), cell("25")]),
                ],
            )
            .summary(vec![
                row(vec![cell("Subtotal").stub(), cell("40")]).role("summary")
            ]),
        )
        .build()
}

fn grand_summary() -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("Item").stub(), cell("Amount")]).role("column_label"))
        .group(group(vec![
            row(vec![cell("Alpha").stub(), cell("10")]),
            row(vec![cell("Beta").stub(), cell("20")]),
            row(vec![cell("Gamma").stub(), cell("30")]),
        ]))
        .group(group(vec![]).summary(vec![
            row(vec![cell("Total").stub(), cell("60")]).role("grand_summary"),
        ]))
        .build()
}

// ────────────────────────────── footnotes ────────────────────────────────

fn footnote_single() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Name"), cell("Score")]).role("column_label"))
        .body(vec![
            row(vec![
                cell("Alice"),
                cell_content(vec![text("90"), footnote_mark("fn1", "1")]),
            ]),
            row(vec![cell("Bob"), cell("85")]),
        ])
        .footnote("fn1", "1", "Includes bonus points.")
        .build()
}

fn footnote_multiple() -> Table {
    TableBuilder::new(2)
        .head(
            row(vec![
                cell_content(vec![text("Name"), footnote_mark("fa", "a")]),
                cell("Score"),
            ])
            .role("column_label"),
        )
        .body(vec![
            row(vec![
                cell("Alice"),
                cell_content(vec![text("90"), footnote_mark("fb", "b")]),
            ]),
            row(vec![
                cell_content(vec![text("Bob"), footnote_mark("fb", "b")]),
                cell("85"),
            ]),
        ])
        .footnote("fa", "a", "Given name only.")
        .footnote("fb", "b", "Provisional value.")
        .build()
}

fn source_notes() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Metric"), cell("Value")]).role("column_label"))
        .body(vec![row(vec![cell("Users"), cell("1,024")])])
        .source_note("Source: internal analytics.")
        .source_note("Collected June 2026.")
        .build()
}

fn footnote_in_stub() -> Table {
    TableBuilder::new(2)
        .stub_cols(1)
        .head(row(vec![cell("Country").stub(), cell("GDP")]).role("column_label"))
        .body(vec![
            row(vec![
                cell_content(vec![text("Atlantis"), footnote_mark("fn1", "*")]).stub(),
                cell("N/A"),
            ]),
            row(vec![cell("Utopia").stub(), cell("999")]),
        ])
        .footnote("fn1", "*", "Fictional; illustrative only.")
        .build()
}

fn footnote_and_source() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Test"), cell("Result")]).role("column_label"))
        .body(vec![
            row(vec![
                cell("A"),
                cell_content(vec![text("pass"), footnote_mark("fn1", "1")]),
            ]),
            row(vec![cell("B"), cell("fail")]),
        ])
        .footnote("fn1", "1", "Re-run confirmed.")
        .source_note("Source: CI pipeline.")
        .build()
}

// ─────────────────────────────── styling ─────────────────────────────────

fn borders_solid() -> Table {
    borders_with("solid")
}

fn borders_dashed() -> Table {
    borders_with("dashed")
}

fn borders_double() -> Table {
    borders_with("double")
}

fn borders_with(style: &str) -> Table {
    let def = StyleDef {
        border: Some(border_all(border("1px", style, "#333333"))),
        padding: Some(padding_all("4px")),
        ..Default::default()
    };
    TableBuilder::new(3)
        .style_def("bordered", def)
        .columns(vec![
            column("col_0", "A").style("bordered"),
            column("col_1", "B").style("bordered"),
            column("col_2", "C").style("bordered"),
        ])
        .head(
            row(vec![
                cell("A").style("bordered"),
                cell("B").style("bordered"),
                cell("C").style("bordered"),
            ])
            .role("column_label"),
        )
        .body(vec![
            row(vec![
                cell("1").style("bordered"),
                cell("2").style("bordered"),
                cell("3").style("bordered"),
            ]),
            row(vec![
                cell("4").style("bordered"),
                cell("5").style("bordered"),
                cell("6").style("bordered"),
            ]),
        ])
        .build()
}

fn striping_basic() -> Table {
    TableBuilder::new(3)
        .striping(false, true)
        .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
        .body(vec![
            row(vec![cell("1"), cell("2"), cell("3")]),
            row(vec![cell("4"), cell("5"), cell("6")]),
            row(vec![cell("7"), cell("8"), cell("9")]),
            row(vec![cell("10"), cell("11"), cell("12")]),
        ])
        .build()
}

fn cell_colors() -> Table {
    let hot = StyleDef {
        color: Some("#ffffff".into()),
        background_color: Some("#c0392b".into()),
        ..Default::default()
    };
    let cool = StyleDef {
        color: Some("#ffffff".into()),
        background_color: Some("#2980b9".into()),
        ..Default::default()
    };
    TableBuilder::new(2)
        .style_def("hot", hot)
        .style_def("cool", cool)
        .head(row(vec![cell("Label"), cell("Temp")]).role("column_label"))
        .body(vec![
            row(vec![cell("Fire"), cell("hot").style("hot")]),
            row(vec![cell("Ice"), cell("cold").style("cool")]),
        ])
        .build()
}

fn style_composition() -> Table {
    let base = StyleDef {
        font_weight: Some("bold".into()),
        color: Some("#2c3e50".into()),
        ..Default::default()
    };
    let emphasis = StyleDef {
        background_color: Some("#f1c40f".into()),
        ..Default::default()
    };
    TableBuilder::new(2)
        .style_def("base", base)
        .composition("emphasis", "base", emphasis)
        .head(row(vec![cell("Key"), cell("Value")]).role("column_label"))
        .body(vec![
            row(vec![cell("normal"), cell("42")]),
            row(vec![cell("special").style("emphasis"), cell("99")]),
        ])
        .build()
}

fn conditional_striping() -> Table {
    let odd = StyleDef {
        background_color: Some("#ecf0f1".into()),
        ..Default::default()
    };
    TableBuilder::new(2)
        .conditional(
            "odd-rows",
            ConditionalSelector {
                row_parity: Some("odd".into()),
                scope: Some("body".into()),
            },
            odd,
        )
        .head(row(vec![cell("Index"), cell("Value")]).role("column_label"))
        .body(vec![
            row(vec![cell("0"), cell("a")]),
            row(vec![cell("1"), cell("b")]),
            row(vec![cell("2"), cell("c")]),
            row(vec![cell("3"), cell("d")]),
        ])
        .build()
}

// ──────────────────────────────── widths ─────────────────────────────────

fn column_widths_mixed() -> Table {
    TableBuilder::new(4)
        .table_width("100%")
        .columns(vec![
            column("col_0", "Fixed").width("120px"),
            column("col_1", "Percent").width("25%"),
            column("col_2", "Fraction").width("1fr"),
            column("col_3", "Auto").width("auto"),
        ])
        .head(
            row(vec![
                cell("Fixed"),
                cell("Percent"),
                cell("Fraction"),
                cell("Auto"),
            ])
            .role("column_label"),
        )
        .body(vec![row(vec![
            cell("120px"),
            cell("25%"),
            cell("1fr"),
            cell("auto"),
        ])])
        .build()
}

fn min_max_width() -> Table {
    TableBuilder::new(2)
        .columns(vec![
            column("col_0", "Constrained")
                .min_width("80px")
                .max_width("200px"),
            column("col_1", "Free"),
        ])
        .head(row(vec![cell("Constrained"), cell("Free")]).role("column_label"))
        .body(vec![row(vec![
            cell("clamped between 80 and 200px"),
            cell("grows freely"),
        ])])
        .build()
}

fn fixed_table_width() -> Table {
    TableBuilder::new(3)
        .table_width("400px")
        .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3")])])
        .build()
}

fn hidden_columns() -> Table {
    TableBuilder::new(3)
        .columns(vec![
            column("col_0", "Visible 1"),
            column("col_1", "Hidden").hidden(),
            column("col_2", "Visible 2"),
        ])
        .head(row(vec![cell("Visible 1"), cell("Hidden"), cell("Visible 2")]).role("column_label"))
        .body(vec![
            row(vec![cell("a"), cell("secret"), cell("b")]),
            row(vec![cell("c"), cell("secret"), cell("d")]),
        ])
        .build()
}

fn narrow_wide_mix() -> Table {
    TableBuilder::new(2)
        .table_width("100%")
        .columns(vec![
            column("col_0", "#").width("40px").align("right"),
            column("col_1", "Description").width("auto"),
        ])
        .head(row(vec![cell("#"), cell("Description")]).role("column_label"))
        .body(vec![
            row(vec![cell("1"), cell("A long descriptive line of text.")]),
            row(vec![cell("2"), cell("Another spanning description here.")]),
        ])
        .build()
}

// ──────────────────────────────── content ────────────────────────────────

fn styled_text_ex() -> Table {
    TableBuilder::new(2)
        .style_def(
            "em",
            StyleDef {
                font_style: Some("italic".into()),
                color: Some("#8e44ad".into()),
                ..Default::default()
            },
        )
        .head(row(vec![cell("Term"), cell("Note")]).role("column_label"))
        .body(vec![row(vec![
            cell("Rust"),
            cell_content(vec![
                text("A "),
                styled("systems", "em"),
                text(" language."),
            ]),
        ])])
        .build()
}

fn line_breaks() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Name"), cell("Address")]).role("column_label"))
        .body(vec![row(vec![
            cell("HQ"),
            cell_content(vec![
                text("123 Main St"),
                line_break(),
                text("Suite 100"),
                line_break(),
                text("Anytown"),
            ]),
        ])])
        .build()
}

fn inline_image() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Icon"), cell("Label")]).role("column_label"))
        .body(vec![row(vec![
            cell_content(vec![image("https://example.com/logo.png", "logo")]),
            cell("Brand"),
        ])])
        .build()
}

fn raw_html() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Kind"), cell("Rendered")]).role("column_label"))
        .body(vec![row(vec![
            cell("bold+italic"),
            cell_content(vec![raw("html", "<b><i>rich</i></b>")]),
        ])])
        .build()
}

fn typed_values() -> Table {
    TableBuilder::new(3)
        .columns(vec![
            column("col_0", "Name"),
            column("col_1", "Score").align("right"),
            column("col_2", "Active"),
        ])
        .head(row(vec![cell("Name"), cell("Score"), cell("Active")]).role("column_label"))
        .body(vec![
            row(vec![
                cell("Alice").typed("string", json!("Alice")),
                cell("90").typed("number", json!(90)).data_type("number"),
                cell("yes").typed("boolean", json!(true)),
            ]),
            row(vec![
                cell("Bob").typed("string", json!("Bob")),
                cell("85").typed("number", json!(85)).data_type("number"),
                cell("no").typed("boolean", json!(false)),
            ]),
        ])
        .build()
}

fn mixed_markup() -> Table {
    TableBuilder::new(1)
        .style_def(
            "warn",
            StyleDef {
                color: Some("#e67e22".into()),
                font_weight: Some("bold".into()),
                ..Default::default()
            },
        )
        .head(row(vec![cell("Message")]).role("column_label"))
        .body(vec![row(vec![cell_content(vec![
            text("Status: "),
            styled("degraded", "warn"),
            line_break(),
            text("Retry advised"),
            footnote_mark("fn1", "†"),
        ])])])
        .footnote("fn1", "†", "See runbook section 4.")
        .build()
}

// ─────────────────────────────── alignment ───────────────────────────────

fn align_lcr() -> Table {
    TableBuilder::new(3)
        .columns(vec![
            column("col_0", "Left").align("left"),
            column("col_1", "Center").align("center"),
            column("col_2", "Right").align("right"),
        ])
        .head(row(vec![cell("Left"), cell("Center"), cell("Right")]).role("column_label"))
        .body(vec![
            row(vec![cell("aaa"), cell("bbb"), cell("ccc")]),
            row(vec![cell("d"), cell("e"), cell("f")]),
        ])
        .build()
}

fn decimal_char_align() -> Table {
    TableBuilder::new(2)
        .columns(vec![
            column("col_0", "Item"),
            column("col_1", "Amount").align("char").align_char("."),
        ])
        .head(row(vec![cell("Item"), cell("Amount")]).role("column_label"))
        .body(vec![
            row(vec![cell("A"), cell("1234.5")]),
            row(vec![cell("B"), cell("6.789")]),
            row(vec![cell("C"), cell("42.0")]),
        ])
        .build()
}

fn per_column_align() -> Table {
    TableBuilder::new(4)
        .columns(vec![
            column("col_0", "L").align("left"),
            column("col_1", "C").align("center"),
            column("col_2", "R").align("right"),
            column("col_3", "J").align("justify"),
        ])
        .head(row(vec![cell("L"), cell("C"), cell("R"), cell("J")]).role("column_label"))
        .body(vec![row(vec![
            cell("left"),
            cell("center"),
            cell("right"),
            cell("justified text here"),
        ])])
        .build()
}

fn numeric_right_align() -> Table {
    TableBuilder::new(3)
        .columns(vec![
            column("col_0", "Product"),
            column("col_1", "Qty").align("right"),
            column("col_2", "Price").align("right"),
        ])
        .head(row(vec![cell("Product"), cell("Qty"), cell("Price")]).role("column_label"))
        .body(vec![
            row(vec![
                cell("Widget"),
                cell("12").typed("number", json!(12)),
                cell("3.50").typed("number", json!(3.5)),
            ]),
            row(vec![
                cell("Gadget"),
                cell("4").typed("number", json!(4)),
                cell("19.99").typed("number", json!(19.99)),
            ]),
        ])
        .build()
}

// ───────────────────────────────── i18n ──────────────────────────────────

fn unicode_cjk() -> Table {
    TableBuilder::new(2)
        .title("販売実績")
        .head(row(vec![cell("地域"), cell("売上")]).role("column_label"))
        .body(vec![
            row(vec![cell("東京"), cell("1,250")]),
            row(vec![cell("大阪"), cell("980")]),
        ])
        .build()
}

fn unicode_rtl() -> Table {
    TableBuilder::new(2)
        .config(|c| c.locale = "ar".into())
        .head(row(vec![cell("الاسم"), cell("القيمة")]).role("column_label"))
        .body(vec![
            row(vec![cell("مرحبا"), cell("١٢٣")]),
            row(vec![cell("שלום"), cell("456")]),
        ])
        .build()
}

fn unicode_emoji() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Mood"), cell("Icon")]).role("column_label"))
        .body(vec![
            row(vec![cell("happy"), cell("😀")]),
            row(vec![cell("party"), cell("🎉🎊")]),
            row(vec![cell("family"), cell("👨‍👩‍👧‍👦")]),
        ])
        .build()
}

fn combining_chars() -> Table {
    TableBuilder::new(2)
        .head(row(vec![cell("Form"), cell("Text")]).role("column_label"))
        .body(vec![
            row(vec![cell("composed"), cell("café")]),
            row(vec![cell("decomposed"), cell("cafe\u{0301}")]),
            row(vec![cell("stacked"), cell("a\u{0300}\u{0301}\u{0302}")]),
        ])
        .build()
}

fn long_word_wrap() -> Table {
    TableBuilder::new(2)
        .columns(vec![
            column("col_0", "Label").width("100px"),
            column("col_1", "Token").width("120px"),
        ])
        .head(row(vec![cell("Label"), cell("Token")]).role("column_label"))
        .body(vec![row(vec![
            cell("URL"),
            cell("https://example.com/very/long/path/that/should/not/break/nicely?query=1"),
        ])])
        .build()
}

// ─────────────────────────────── realism ─────────────────────────────────

fn regional_sales() -> Table {
    let num = StyleDef {
        text_align: Some("right".into()),
        ..Default::default()
    };
    TableBuilder::new(4)
        .stub_cols(1)
        .title("Regional Sales Performance")
        .subtitle("Fiscal years 2023–2024 (in millions USD)")
        .style_def("num", num)
        .columns(vec![
            column("col_0", "Country"),
            column("col_1", "Year").align("right"),
            column("col_2", "Revenue").align("right"),
            column("col_3", "Profit").align("right"),
        ])
        .head(
            row(vec![
                cell_content(vec![]).stub(),
                cell_content(vec![]),
                cell("Financials").colspan(2).scope("colgroup"),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(
            row(vec![
                cell("Country").stub(),
                cell("Year"),
                cell("Revenue"),
                cell("Profit"),
            ])
            .role("column_label"),
        )
        .group(labeled_group(
            "North America",
            vec![
                row(vec![
                    cell("United States").rowspan(2).stub(),
                    cell("2023"),
                    cell("1,250.3"),
                    cell_content(vec![text("270.1"), footnote_mark("fn1", "1")]),
                ]),
                row(vec![
                    placeholder(),
                    cell("2024"),
                    cell("1,380.7"),
                    cell("330.3"),
                ]),
                row(vec![
                    cell("Canada").stub(),
                    cell("2024"),
                    cell("420.1"),
                    cell("109.5"),
                ]),
            ],
        ))
        .group(labeled_group(
            "Europe",
            vec![
                row(vec![
                    cell("Germany").stub(),
                    cell("2024"),
                    cell("945.2"),
                    cell("164.9"),
                ]),
                row(vec![
                    cell("France").stub(),
                    cell("2024"),
                    cell("610.8"),
                    cell("120.6"),
                ]),
            ],
        ))
        .footnote("fn1", "1", "Includes one-time restructuring charge.")
        .source_note("Source: Internal finance database, June 2026.")
        .build()
}

fn financial_report() -> Table {
    let total = StyleDef {
        font_weight: Some("bold".into()),
        background_color: Some("#f5f5f5".into()),
        ..Default::default()
    };
    TableBuilder::new(3)
        .stub_cols(1)
        .striping(false, true)
        .title("Quarterly P&L")
        .style_def("total", total)
        .columns(vec![
            column("col_0", "Line item"),
            column("col_1", "Q1").align("right"),
            column("col_2", "Q2").align("right"),
        ])
        .head(row(vec![cell("Line item").stub(), cell("Q1"), cell("Q2")]).role("column_label"))
        .group(
            group(vec![
                row(vec![cell("Revenue").stub(), cell("$1,200"), cell("$1,350")]),
                row(vec![cell("COGS").stub(), cell("$700"), cell("$760")]),
                row(vec![cell("Opex").stub(), cell("$300"), cell("$320")]),
            ])
            .summary(vec![row(vec![
                cell("Net income").stub().style("total"),
                cell("$200").style("total"),
                cell("$270").style("total"),
            ])
            .role("summary")]),
        )
        .source_note("Unaudited management figures.")
        .build()
}

fn scientific_table() -> Table {
    TableBuilder::new(4)
        .stub_cols(1)
        .title("Reaction rate constants")
        .columns(vec![
            column("col_0", "Compound"),
            column("col_1", "k (s⁻¹)").align("right"),
            column("col_2", "T (K)").align("right"),
            column("col_3", "Ea (kJ/mol)").align("right"),
        ])
        .head(
            row(vec![
                cell("Compound").stub(),
                cell_content(vec![text("k (s⁻¹)"), footnote_mark("fn1", "a")]),
                cell("T (K)"),
                cell("Ea (kJ/mol)"),
            ])
            .role("column_label"),
        )
        .body(vec![
            row(vec![
                cell("A").stub(),
                cell("1.2×10³"),
                cell("298"),
                cell("52.3"),
            ]),
            row(vec![
                cell("B").stub(),
                cell("3.4×10²"),
                cell("298"),
                cell("61.0"),
            ]),
            row(vec![
                cell("C").stub(),
                cell("8.9×10¹"),
                cell("310"),
                cell("70.8"),
            ]),
        ])
        .footnote("fn1", "a", "Measured under pseudo-first-order conditions.")
        .source_note("Source: laboratory notebook, 2026.")
        .build()
}
