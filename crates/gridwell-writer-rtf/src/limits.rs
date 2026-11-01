//! What RTF output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find_table, TableCheck, Unsupported};

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "lang",
    reason: "RTF names a language by its Windows language id (`\\lang`), which the writer \
             doesn't yet map language tags to",
    uses: |rt| rt.table.config.lang.is_some(),
}];

/// The features of `table` that its RTF output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    find_table(&resolve(table), TABLE_CHECKS)
}
