//! No writer panics on a valid table whose lengths are extreme: every length field
//! (column widths and their bounds, the table's width, font sizes, padding,
//! indents, borders, images, styled runs), from tiny to near `f64::MAX`, in every
//! unit kind, rendered to every format. Debug builds check integer overflow, so a
//! sum that would wrap in release panics here.

use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{Border, BorderSet, Padding, StyleDef};
use gridwell_ir::{Row, Table};
use gridwell_testkit::{
    cell, cell_content, column, row, styled, text, CellExt, ColumnExt, TableBuilder,
};

/// Magnitudes either side of where each format's integer units run out (twips and
/// dxa near 1e8pt, EMU near 3e5pt), and beyond.
fn values() -> Vec<String> {
    let mut v = Vec::new();
    for e in [0, 4, 5, 6, 8, 12, 21, 300] {
        for unit in ["pt", "px", "em", "in", "%"] {
            for m in ["1", "3"] {
                v.push(format!("{m}e{e}{unit}"));
            }
        }
    }
    v.push("1e-300px".into());
    v
}

fn base(style: Option<StyleDef>) -> TableBuilder {
    let b = TableBuilder::new(3)
        .title("Title")
        .subtitle("Sub")
        .footnote("a", "1", "Note")
        .source_note("Source");
    match style {
        Some(s) => b.style_def("s", s),
        None => b,
    }
}

/// Two rows of three cells, styled `s` or not.
fn body(styled_cells: bool) -> Vec<Row> {
    let c = |s: &str| {
        if styled_cells {
            cell(s).style("s")
        } else {
            cell(s)
        }
    };
    vec![
        row(vec![c("a"), c("b"), c("c")]),
        row(vec![c("d"), c("e"), c("f")]),
    ]
}

/// One table per length field, each set to `v`.
fn tables(v: &str) -> Vec<(String, Table)> {
    let mut out = Vec::new();
    let cols = ["a", "b", "c"].map(|id| column(id, id).width(v)).to_vec();
    out.push((
        "column width",
        base(None).columns(cols).body(body(false)).build(),
    ));
    out.push((
        "table width",
        base(None).table_width(v).body(body(false)).build(),
    ));
    let bounded = vec![
        column("a", "A").min_width(v).max_width(v),
        column("b", "B"),
        column("c", "C"),
    ];
    out.push((
        "column bounds",
        base(None).columns(bounded).body(body(false)).build(),
    ));
    let side = || Some(v.to_string());
    let border = || {
        Some(Border {
            width: side(),
            style: None,
            color: Some("red".into()),
        })
    };
    let styles = [
        (
            "font size",
            StyleDef {
                font_size: side(),
                ..Default::default()
            },
        ),
        (
            "indent",
            StyleDef {
                indent: side(),
                ..Default::default()
            },
        ),
        (
            "style bounds",
            StyleDef {
                min_width: side(),
                max_width: side(),
                ..Default::default()
            },
        ),
        (
            "padding",
            StyleDef {
                padding: Some(Padding {
                    top: side(),
                    right: side(),
                    bottom: side(),
                    left: side(),
                }),
                ..Default::default()
            },
        ),
        (
            "border",
            StyleDef {
                border: Some(BorderSet {
                    top: border(),
                    right: border(),
                    bottom: border(),
                    left: border(),
                }),
                ..Default::default()
            },
        ),
    ];
    for (name, s) in styles {
        out.push((name, base(Some(s)).body(body(true)).build()));
    }
    let image = ContentNode::Image {
        src: "x.png".into(),
        alt: Some("x".into()),
        width: side(),
        height: side(),
    };
    let run = StyleDef {
        font_size: side(),
        ..Default::default()
    };
    out.push((
        "image and run",
        base(Some(run))
            .body(vec![row(vec![
                cell_content(vec![image]),
                cell_content(vec![styled("r", "s")]),
                cell_content(vec![text("t")]),
            ])])
            .build(),
    ));
    out.into_iter()
        .map(|(name, t)| (format!("{name} {v}"), t))
        .collect()
}

#[test]
fn no_writer_panics_on_extreme_lengths() {
    let mut rendered = 0;
    let mut failures = Vec::new();
    for v in values() {
        for (name, t) in tables(&v) {
            // Fields that don't take a unit kind (a `%` border) are invalid: skip.
            if !t.validate().is_empty() {
                continue;
            }
            for f in gridwell_render::names() {
                match std::panic::catch_unwind(|| gridwell_render::render(&t, f, None)) {
                    Ok(Ok(_)) => rendered += 1,
                    Ok(Err(e)) => failures.push(format!("{f}, {name}: {e}")),
                    Err(e) => {
                        let msg = e
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| e.downcast_ref::<&str>().copied())
                            .unwrap_or("");
                        failures.push(format!("{f}, {name}: panicked: {msg}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Most tables are valid (only some fields reject some units).
    assert!(rendered > 3000, "{rendered} renders");
}
