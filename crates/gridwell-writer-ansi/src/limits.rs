//! What terminal output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find_table, TableCheck, Unsupported};

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "rtl",
    reason: "a terminal shows columns left to right, and whether it reorders right-to-left \
             text is up to the terminal",
    uses: |rt| rt.is_rtl(),
}];

/// The features of `table` that its terminal output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    find_table(&resolve(table), TABLE_CHECKS)
}
