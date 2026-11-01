//! The table's direction: `config.dir`, with `auto` resolved by the first strongly
//! directional character in document order, and never taken from `config.lang`.

use gridwell_ir::{Table, TextDirection};
use gridwell_layout::resolve;
use gridwell_testkit::{cell, column, row, TableBuilder};

fn table(dir: Option<TextDirection>, lang: Option<&str>, title: Option<&str>, body: &str) -> Table {
    let mut b = TableBuilder::new(2)
        .columns(vec![column("a", "123"), column("b", "B")])
        .head(row(vec![cell("123"), cell("(1)")]))
        .body(vec![row(vec![cell(body), cell("x")])])
        .config(|c| {
            c.dir = dir;
            c.lang = lang.map(str::to_string);
        });
    if let Some(t) = title {
        b = b.title(t);
    }
    b.build()
}

fn rtl(t: &Table) -> bool {
    resolve(t).is_rtl()
}

#[test]
fn rtl_and_ltr_are_as_set() {
    assert!(rtl(&table(Some(TextDirection::Rtl), None, None, "abc")));
    assert!(!rtl(&table(
        Some(TextDirection::Ltr),
        Some("ar"),
        None,
        "مرحبا"
    )));
}

#[test]
fn auto_takes_the_first_strong_character_in_document_order() {
    let auto = || Some(TextDirection::Auto);
    // The header has no strong character, so the body decides.
    assert!(rtl(&table(auto(), None, None, "١٢ مرحبا")));
    assert!(!rtl(&table(auto(), None, None, "abc مرحبا")));
    // The title comes first.
    assert!(!rtl(&table(auto(), None, Some("Sales"), "مرحبا")));
    assert!(rtl(&table(auto(), None, Some("שלום"), "abc")));
    // No strong character at all: left to right.
    assert!(!rtl(&table(auto(), None, None, "42")));
}

#[test]
fn the_language_does_not_set_the_direction() {
    assert!(!rtl(&table(None, Some("ar"), None, "مرحبا")));
    assert!(!rtl(&table(None, Some("he-IL"), None, "שלום")));
}
