//! What RTF output can't express (see `gridwell_layout::unsupported`).

use gridwell_core::{Image, ImageFormat};
use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, find_table, Check, TableCheck, Unsupported, TABLE_RULES};

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "lang",
    reason: "RTF names a language by its Windows language id (`\\lang`), which the writer \
             doesn't yet map language tags to",
    uses: |rt| rt.table.config.lang.is_some(),
}];

const CHECKS: &[Check] = &[Check {
    feature: "image",
    reason: "only a PNG or JPEG image embedded as a `data:` URI is drawn (RTF has no GIF or \
             SVG picture); others, and one linked by URL or path, show their alt text",
    uses: |_, p| {
        p.images().any(|src| {
            !Image::from_src(src)
                .is_some_and(|i| matches!(i.format, ImageFormat::Png | ImageFormat::Jpeg))
        })
    },
}];

/// The features of `table` that its RTF output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, TABLE_CHECKS);
    found.extend(find_table(&rt, &[TABLE_RULES]));
    found.extend(find(&rt, CHECKS));
    found
}
