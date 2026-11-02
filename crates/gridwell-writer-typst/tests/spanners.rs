//! Spanner rules: a head cell with head rows below it is underlined, as LaTeX's
//! `\cmidrule(lr)`: across its content box, so the rule stops short of the cell's
//! ends by the inset and neighboring spanners' rules stay apart, on the cell's
//! bottom edge. Not under the last head row, blank cells, or a cell whose style
//! sets (or removes) its bottom border.
//!
//! Where Typst draws the rules is read back by compiling with show rules that
//! record positions; skipped when `typst` is missing, unless
//! `GRIDWELL_REQUIRE_TYPST` is set.

use std::process::Command;

use gridwell_ir::content::ContentNode;
use gridwell_ir::style::{BorderSet, StyleDef};
use gridwell_ir::Table;
use gridwell_testkit::images::LOGO_PNG;
use gridwell_testkit::{
    border, cell, cell_content, column, examples, image, placeholder, row, text, CellExt,
    ColumnExt, RowExt, TableBuilder,
};
use gridwell_writer_typst::render_typst;
use serde_json::Value;

/// What the writer appends to an underlined cell's content.
const RULE: &str = "#place(bottom + center, dy: 5pt, line(length: 100%, stroke: 0.5pt))";

fn typst_available() -> bool {
    if Command::new("typst").arg("--version").output().is_ok() {
        return true;
    }
    if std::env::var_os("GRIDWELL_REQUIRE_TYPST").is_some_and(|v| !v.is_empty()) {
        panic!("typst not found but GRIDWELL_REQUIRE_TYPST is set");
    }
    eprintln!("skipping: typst not found");
    false
}

/// Where Typst draws `t`: each rule's start and end (`rule-start`, `rule-end`),
/// and the start and end of each of `words` (`{word}-start`, `{word}-end`), as
/// (key, x, y) in points, in document order. Text positions are on the baseline.
fn drawn(t: &Table, words: &[&str]) -> Vec<(String, f64, f64)> {
    let dir = std::env::temp_dir().join(format!("gridwell-typst-spanners-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!(
        "{:x}.typ",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // A line is drawn from its frame's origin for its length, resolved against its
    // region (its frame may be clipped to the region; the stroke isn't).
    let doc = format!(
        "#let mark(k) = box(context [#metadata((k: k, x: here().position().x.pt(), \
         y: here().position().y.pt()))<gw>])\n\
         #show line: it => layout(size => {{\n\
         let l = it.length\n\
         let w = if type(l) == ratio {{ l * size.width }} \
         else if type(l) == length {{ l.to-absolute() }} \
         else {{ l.ratio * size.width + l.length.to-absolute() }}\n\
         stack(dir: ltr, mark(\"rule-start\"), place(dx: w, mark(\"rule-end\")), it)\n\
         }})\n\
         #show regex(\"{}\"): it => mark(it.text + \"-start\") + it + mark(it.text + \"-end\")\n\
         {}",
        words.join("|"),
        render_typst(t).unwrap()
    );
    std::fs::write(&file, doc).unwrap();
    let out = Command::new("typst")
        .arg("query")
        .arg(&file)
        .args(["<gw>", "--field", "value"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let values: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    values
        .iter()
        .map(|v| {
            (
                v["k"].as_str().unwrap().to_string(),
                v["x"].as_f64().unwrap(),
                v["y"].as_f64().unwrap(),
            )
        })
        .collect()
}

/// The (x, y) of each mark keyed `key`.
fn at(marks: &[(String, f64, f64)], key: &str) -> Vec<(f64, f64)> {
    marks
        .iter()
        .filter(|(k, _, _)| k == key)
        .map(|&(_, x, y)| (x, y))
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// The source line of the cell showing `label`.
fn cell_line<'s>(out: &'s str, label: &str) -> &'s str {
    out.lines()
        .find(|l| l.contains(&format!("[{label}]")))
        .unwrap_or_else(|| panic!("no cell {label}:\n{out}"))
}

fn underlined(out: &str, label: &str) -> bool {
    cell_line(out, label).contains(RULE)
}

/// A spanner over the middle two of four columns. The labels under it start and
/// end at the edges of its content box: the second column's is left-aligned, the
/// third's right-aligned.
fn middle_spanner() -> Table {
    TableBuilder::new(4)
        .columns(vec![
            column("a", "La"),
            column("b", "Lb"),
            column("c", "Lc").align("right"),
            column("d", "Ld"),
        ])
        .head(
            row(vec![
                cell_content(vec![]),
                cell("Over").colspan(2).scope("colgroup"),
                placeholder(),
                cell_content(vec![]),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("La"), cell("Lb"), cell("Lc"), cell("Ld")]).role("column_label"))
        .body(vec![row(vec![
            cell("1"),
            cell("a long value"),
            cell("a longer value still"),
            cell("4"),
        ])])
        .build()
}

#[test]
fn a_spanner_is_underlined_across_its_columns_content() {
    if !typst_available() {
        return;
    }
    let marks = drawn(&middle_spanner(), &["Over", "Lb", "Lc"]);
    let (start, end) = (at(&marks, "rule-start"), at(&marks, "rule-end"));
    assert_eq!((start.len(), end.len()), (1, 1), "{marks:?}");
    let ((x0, y), (x1, y1)) = (start[0], end[0]);
    assert!(close(y, y1), "a horizontal rule: {marks:?}");
    // From the second column's content start to the third's content end.
    assert!(close(x0, at(&marks, "Lb-start")[0].0), "{marks:?}");
    assert!(close(x1, at(&marks, "Lc-end")[0].0), "{marks:?}");
    // Between the spanner and its columns' labels: on the spanner cell's bottom
    // edge, the inset (5pt) below its text and above the labels' cells.
    let (over, label) = (at(&marks, "Over-start")[0].1, at(&marks, "Lb-start")[0].1);
    assert!(over + 4.9 < y && y < label - 5.0, "{marks:?}");
}

#[test]
fn neighboring_spanners_rules_stay_apart() {
    if !typst_available() {
        return;
    }
    let nested = examples()
        .into_iter()
        .find(|e| e.name == "nested-spanners")
        .unwrap()
        .table();
    let marks = drawn(&nested, &["A1", "B1"]);
    let (start, end) = (at(&marks, "rule-start"), at(&marks, "rule-end"));
    assert_eq!((start.len(), end.len()), (2, 2), "{marks:?}");
    // Each starts at its first column's content; the gap between them is the two
    // insets (5pt each) either side of the boundary between the spanners.
    assert!(close(start[0].0, at(&marks, "A1-start")[0].0), "{marks:?}");
    assert!(close(start[1].0, at(&marks, "B1-start")[0].0), "{marks:?}");
    assert!(close(start[1].0 - end[0].0, 10.0), "{marks:?}");
    assert!(close(start[0].1, start[1].1), "{marks:?}");
}

#[test]
fn only_cells_with_head_rows_below_them_are_underlined() {
    // Three head rows: a stubhead down all of them, a spanner over a sub-spanner,
    // and a cell two rows tall over the last column's label.
    let t = TableBuilder::new(4)
        .head(
            row(vec![
                cell("Stub").rowspan(3),
                cell("Top").colspan(2),
                placeholder(),
                cell("Tall").rowspan(2),
            ])
            .role("spanner_label"),
        )
        .head(
            row(vec![
                placeholder(),
                cell("Mid").colspan(2),
                placeholder(),
                placeholder(),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![placeholder(), cell("X"), cell("Y"), cell("Z")]).role("column_label"))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3"), cell("4")])])
        .build();
    let out = render_typst(&t).unwrap();
    for label in ["Top", "Mid", "Tall"] {
        assert!(underlined(&out, label), "{label}:\n{out}");
    }
    for label in ["Stub", "X", "Y", "Z"] {
        assert!(!underlined(&out, label), "{label}:\n{out}");
    }
    assert_eq!(out.matches(RULE).count(), 3, "{out}");

    // One head row: a cell spanning columns is a column label, not a spanner. And
    // never in the body.
    let flat = TableBuilder::new(3)
        .head(row(vec![cell("Wide").colspan(2), placeholder(), cell("C")]))
        .body(vec![row(vec![
            cell("Body").colspan(2),
            placeholder(),
            cell("3"),
        ])])
        .build();
    let out = render_typst(&flat).unwrap();
    assert!(!out.contains("#place("), "{out}");
}

#[test]
fn blank_spanner_cells_are_not_underlined() {
    let no_alt = match image(LOGO_PNG, "") {
        ContentNode::Image {
            src, width, height, ..
        } => ContentNode::Image {
            src,
            alt: None,
            width,
            height,
        },
        _ => unreachable!(),
    };
    let t = TableBuilder::new(5)
        .head(
            row(vec![
                cell_content(vec![]),
                cell("  "),
                cell_content(vec![text(" "), ContentNode::LineBreak {}]),
                // An image is something to underline, alt text or not.
                cell_content(vec![no_alt]),
                cell("Named"),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![
            cell("A"),
            cell("B"),
            cell("C"),
            cell("D"),
            cell("E"),
        ]))
        .body(vec![row(vec![
            cell("1"),
            cell("2"),
            cell("3"),
            cell("4"),
            cell("5"),
        ])])
        .build();
    let out = render_typst(&t).unwrap();
    assert_eq!(out.matches(RULE).count(), 2, "{out}");
    assert!(underlined(&out, "Named"), "{out}");
    let image_cell = out
        .lines()
        .find(|l| l.contains("image.decode"))
        .unwrap_or_else(|| panic!("no image:\n{out}"));
    assert!(image_cell.contains(RULE), "{out}");
}

#[test]
fn a_spanners_own_bottom_border_stands_instead() {
    let bottom = |style: &str, width: &str| StyleDef {
        border: Some(BorderSet {
            top: None,
            right: None,
            bottom: Some(border(width, style, "#FF0000")),
            left: None,
        }),
        ..Default::default()
    };
    let t = TableBuilder::new(4)
        .style_def("red", bottom("solid", "2px"))
        .style_def("none", bottom("none", "2px"))
        .style_def("zero", bottom("solid", "0px"))
        // A top border leaves the bottom to the rule.
        .style_def(
            "top",
            StyleDef {
                border: Some(BorderSet {
                    top: Some(border("1px", "solid", "#0000FF")),
                    right: None,
                    bottom: None,
                    left: None,
                }),
                ..Default::default()
            },
        )
        .head(
            row(vec![
                cell("Red").style("red"),
                cell("None").style("none"),
                cell("Zero").style("zero"),
                cell("Top").style("top"),
            ])
            .role("spanner_label"),
        )
        .head(row(vec![cell("A"), cell("B"), cell("C"), cell("D")]))
        .body(vec![row(vec![cell("1"), cell("2"), cell("3"), cell("4")])])
        .build();
    let out = render_typst(&t).unwrap();
    for label in ["Red", "None", "Zero"] {
        assert!(!underlined(&out, label), "{label}:\n{out}");
    }
    assert!(
        cell_line(&out, "Red").contains("bottom: (thickness: 1.5pt, paint: rgb(\"#FF0000\"))"),
        "{out}"
    );
    assert!(underlined(&out, "Top"), "{out}");
}
