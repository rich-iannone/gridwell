//! What terminal output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, find_table, Check, TableCheck, Unsupported, TABLE_RULES};

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "rtl",
    reason: "a terminal shows columns left to right, and whether it reorders right-to-left \
             text is up to the terminal",
    uses: |rt| rt.is_rtl(),
}];

const CHECKS: &[Check] = &[
    Check {
        feature: "image",
        reason: "a terminal shows text: an image shows its alt text",
        uses: |_, p| p.images().next().is_some(),
    },
    Check {
        feature: "link",
        reason: "a cell or line links as a whole, to its first link, as an OSC 8 hyperlink \
                 (with `color`; a terminal without them shows the text): other links in it \
                 link nowhere",
        uses: |_, p| {
            let mut links = p.links();
            links.next().is_some_and(|first| links.any(|l| l != first))
        },
    },
];

/// The features of `table` that its terminal output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, TABLE_CHECKS);
    found.extend(find_table(&rt, &[TABLE_RULES]));
    found.extend(find(&rt, CHECKS));
    found
}
