//! What terminal output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, find_table, Check, TableCheck, Unsupported};

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "rtl",
    reason: "a terminal shows columns left to right, and whether it reorders right-to-left \
             text is up to the terminal",
    uses: |rt| rt.is_rtl(),
}];

const CHECKS: &[Check] = &[Check {
    feature: "image",
    reason: "a terminal shows text: an image shows its alt text",
    uses: |_, p| p.images().next().is_some(),
}];

/// The features of `table` that its terminal output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, TABLE_CHECKS);
    found.extend(find(&rt, CHECKS));
    found
}
