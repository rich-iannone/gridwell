//! Column widths as declared, and in points for formats that give every column a
//! width: `%` divides the table's width, `fr` what the other columns leave, and a
//! table with a width but no `fr` column spreads it over the columns without one.

use gridwell_core::Length;
use gridwell_ir::Table;
use gridwell_layout::{resolve, ColumnWidth};
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
