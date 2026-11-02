//! Column widths as declared, and in points for formats that give every column a
//! width: `%` divides the table's width, `fr` what the other columns leave, and a
//! table with a width but no `fr` column spreads it over the columns without one.
//! Columns without a width can be sized for their content, within the line.

use gridwell_core::Length;
use gridwell_ir::Table;
use gridwell_layout::{resolve, ColumnWidth, ContentWidth};
use gridwell_testkit::{cell, column, row, ColumnExt, TableBuilder};

const LINE: f64 = 432.0;
const AUTO: f64 = 108.0;
const EM: f64 = 10.0;

/// One row, a column per width (`""` for none), in a table `table_width` wide.
fn table(table_width: Option<&str>, widths: &[&str]) -> Table {
    let mut b = TableBuilder::new(widths.len() as u32).columns(
        widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let c = column(&format!("c{i}"), "C");
                if w.is_empty() {
                    c
                } else {
                    c.width(w)
                }
            })
            .collect(),
    );
    if let Some(w) = table_width {
        b = b.table_width(w);
    }
    b.body(vec![row(widths.iter().map(|_| cell("x")).collect())])
        .build()
}

fn declared(table_width: Option<&str>, widths: &[&str]) -> Vec<ColumnWidth> {
    resolve(&table(table_width, widths)).column_widths()
}

fn points(table_width: Option<&str>, widths: &[&str]) -> Vec<f64> {
    resolve(&table(table_width, widths)).column_widths_pt(LINE, AUTO, EM)
}

fn assert_close(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-9, "{got:?} vs {want:?}");
    }
}

#[test]
fn the_table_width_is_an_absolute_length_or_a_percentage() {
    let width = |w: &str| resolve(&table(Some(w), &[""])).table_width();
    assert_eq!(width("400px"), Some(Length::Px(400.0)));
    assert_eq!(width("20em"), Some(Length::Em(20.0)));
    assert_eq!(width("50%"), Some(Length::Percent(50.0)));
    for w in ["auto", "0", "0%", "2fr", "-5px", "bogus"] {
        assert_eq!(width(w), None, "{w}");
    }
    assert_eq!(resolve(&table(None, &[""])).table_width(), None);
}

#[test]
fn columns_without_a_width_fill_a_table_that_has_one() {
    use ColumnWidth::*;
    // No table width: nothing to fill.
    assert_eq!(
        declared(None, &["", "30pt"]),
        [Auto, Fixed(Length::Pt(30.0))]
    );
    assert_eq!(declared(Some("auto"), &[""]), [Auto]);
    // A table width and no `fr` column: the columns without a width are `1fr`.
    assert_eq!(
        declared(Some("100%"), &["", "30pt", "", "10%"]),
        [Fr(1.0), Fixed(Length::Pt(30.0)), Fr(1.0), Percent(10.0)]
    );
    // An `fr` column takes what is left, so the others keep no width.
    assert_eq!(declared(Some("100%"), &["", "2fr"]), [Auto, Fr(2.0)]);
    // A width that isn't above zero is none.
    assert_eq!(declared(Some("300pt"), &["0px", "0fr"]), [Fr(1.0), Fr(1.0)]);
}

#[test]
fn hidden_columns_take_no_share() {
    let mut t = table(Some("300pt"), &["", "", ""]);
    t.column_spec[1].hidden = true;
    let rt = resolve(&t);
    assert_close(&rt.column_widths_pt(LINE, AUTO, EM), &[150.0, 150.0]);
}

#[test]
fn widths_in_points() {
    // Without a table width, auto columns get the default and `fr` fills the line.
    assert_close(&points(None, &["", ""]), &[AUTO, AUTO]);
    assert_close(
        &points(None, &["90pt", "25%", "1fr", ""]),
        &[90.0, 108.0, LINE - 90.0 - 108.0 - AUTO, AUTO],
    );
    // `%` and `fr` divide the table's width; `fr` in proportion.
    assert_close(
        &points(Some("400pt"), &["100pt", "25%", "1fr", "3fr"]),
        &[100.0, 100.0, 50.0, 150.0],
    );
    // A `%` table width is that share of the line; `em` is `EM`.
    assert_close(&points(Some("50%"), &["50%", "2em"]), &[108.0, 20.0]);
    assert_close(&points(Some("50%"), &["25%", ""]), &[54.0, 162.0]);
    // Columns without a width share a table width equally.
    assert_close(&points(Some("300pt"), &["", "", ""]), &[100.0; 3]);
    assert_close(&points(Some("400px"), &["", "30pt"]), &[270.0, 30.0]);
    // A table of fixed and `%` columns is as wide as they add up to.
    assert_close(&points(Some("400pt"), &["100pt", "10%"]), &[100.0, 40.0]);
}

fn units(table_width: Option<&str>, widths: &[&str], max: u32) -> Vec<u32> {
    resolve(&table(table_width, widths)).column_widths_in(LINE, AUTO, EM, 20.0, max)
}

#[test]
fn widths_in_a_formats_unit_are_rounded_and_at_least_one() {
    assert_eq!(units(Some("100pt"), &["", "", ""], 10_000), [667, 667, 667]);
    assert_eq!(units(None, &["1.5pt", ""], 10_000), [30, 2160]);
    // Too thin to round to a unit: one unit.
    assert_eq!(units(None, &["0.01pt", "1e-300px"], 10_000), [1, 1]);
}

#[test]
fn widths_in_a_formats_unit_add_up_to_at_most_its_maximum() {
    // Under the maximum: as they are.
    assert_eq!(units(None, &["40pt", "10pt"], 1000), [800, 200]);
    // Over: a unit each, and the rest in proportion.
    assert_eq!(units(None, &["40pt", "10pt"], 500), [399, 100]);
    assert_eq!(units(None, &["40pt", "20pt", "20pt"], 999), [499, 250, 250]);
    // One column over the maximum on its own, and an infinite sum.
    for cols in [&["1e300pt", "10pt"][..], &["1e300pt", "1e300pt", "1e300pt"]] {
        let got = units(None, cols, u32::MAX);
        let total: u64 = got.iter().map(|&u| u64::from(u)).sum();
        assert!(total <= u64::from(u32::MAX), "{cols:?}: {got:?}");
        assert!(got.iter().all(|&u| u >= 1), "{cols:?}: {got:?}");
    }
    assert_eq!(units(None, &["1e300pt", "1e300pt"], 1000), [500, 500]);
    // Too wide for an `f64` in the unit (an infinite sum).
    assert_eq!(units(None, &["1e307pt", "1e308pt"], 1000), [500, 500]);
    // The thin column keeps its unit.
    assert_eq!(units(None, &["1e300pt", "1pt"], 1000), [999, 1]);
    assert_eq!(units(None, &["1e300pt", "1e300pt", "1pt"], 9), [4, 4, 1]);
}

#[test]
fn fr_columns_with_no_room_get_the_default() {
    assert_close(
        &points(Some("100pt"), &["80pt", "30pt", "1fr"]),
        &[80.0, 30.0, AUTO],
    );
    assert_close(&points(Some("100pt"), &["100pt", ""]), &[100.0, AUTO]);
    // Every column's share sums to the table's width when the `fr` columns fit.
    let got = points(Some("7in"), &["1in", "15%", "2fr", "", "1fr"]);
    assert!(
        (got.iter().sum::<f64>() - 7.0 * 72.0).abs() < 1e-9,
        "{got:?}"
    );
}

fn fit(table_width: Option<&str>, widths: &[&str], content: &[(f64, f64)]) -> Vec<f64> {
    let content: Vec<ContentWidth> = content
        .iter()
        .map(|&(min, max)| ContentWidth { min, max })
        .collect();
    resolve(&table(table_width, widths)).column_widths_fit_pt(LINE, AUTO, EM, &content)
}

#[test]
fn columns_without_a_width_fit_their_content_on_the_line() {
    // Room for every line: each as wide as its widest.
    assert_close(
        &fit(None, &["", ""], &[(20.0, 50.0), (30.0, 300.0)]),
        &[50.0, 300.0],
    );
    // Not enough (an absolute and a `%` column take 100pt and 43.2pt): from
    // their `max` toward their `min`, in proportion to how far apart they are
    // (they need 60pt and 360pt, the line leaves 288.8pt of 410pt: 228.8pt
    // over the `min`s, of 350pt between them).
    let got = fit(
        None,
        &["", "100pt", "", "10%"],
        &[(20.0, 50.0), (0.0, 0.0), (40.0, 360.0), (0.0, 0.0)],
    );
    let k = (LINE - 100.0 - 43.2 - 60.0) / 350.0;
    assert_close(&got, &[20.0 + 30.0 * k, 100.0, 40.0 + 320.0 * k, 43.2]);
    assert!((got.iter().sum::<f64>() - LINE).abs() < 1e-9, "{got:?}");
    // Not even for their `min`: their `min`, past the line.
    assert_close(
        &fit(None, &["", ""], &[(300.0, 400.0), (200.0, 200.0)]),
        &[300.0, 200.0],
    );
    // `fr` columns share what they leave.
    assert_close(
        &fit(
            None,
            &["", "1fr", "3fr"],
            &[(10.0, 32.0), (5.0, 5.0), (5.0, 5.0)],
        ),
        &[32.0, 100.0, 300.0],
    );
    // A table with a width and no `fr` column: shared, whatever the content.
    assert_close(
        &fit(Some("300pt"), &["", ""], &[(10.0, 10.0), (20.0, 290.0)]),
        &[150.0, 150.0],
    );
}

#[test]
fn content_widths_are_taken_with_care() {
    // A column left out gets the default; a `max` that isn't finite is the
    // default, a `min` that isn't is the `max`, and a `min` over the `max`
    // (or under zero) is clamped to it.
    assert_close(
        &fit(None, &["", "", ""], &[(10.0, 20.0)]),
        &[20.0, AUTO, AUTO],
    );
    assert_close(
        &fit(None, &["", ""], &[(1.0, f64::NAN), (f64::INFINITY, 30.0)]),
        &[AUTO, 30.0],
    );
    let got = fit(None, &["", ""], &[(500.0, 300.0), (-50.0, 400.0)]);
    assert_close(&got, &[300.0, LINE - 300.0]);
    // Content for a column with a width is ignored.
    assert_close(&fit(None, &["30pt"], &[(100.0, 200.0)]), &[30.0]);
    // Without content, as `column_widths_pt`.
    for cols in [&["", ""][..], &["90pt", "25%", "1fr", ""], &["100pt", ""]] {
        assert_close(&fit(None, cols, &[]), &points(None, cols));
    }
}
