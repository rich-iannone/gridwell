//! What XLSX output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find_table, TableCheck, Unsupported};

use crate::render::losses;

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "lang",
    reason: "a spreadsheet's cells have no language",
    uses: |rt| rt.table.config.lang.is_some(),
}];

/// The features of `table` that its XLSX output can't express, and where.
/// Text over Excel's limit is found by laying out the workbook, which knows
/// exactly what it cuts.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, TABLE_CHECKS);
    let lost = losses(&rt);
    for (feature, reason, places) in [
        (
            "text-length",
            "a cell holds at most 32,767 characters: the rest is cut",
            lost.cut,
        ),
        (
            "rich-text-length",
            "rich text whose formatting markup takes it over a cell's 32,767 characters \
             is written without its formatting",
            lost.plain,
        ),
    ] {
        if !places.is_empty() {
            found.push(Unsupported {
                feature,
                reason,
                places,
            });
        }
    }
    found
}
