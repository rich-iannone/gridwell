//! What DOCX output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, find_table, Check, Unsupported, TABLE_RULES};

use crate::render::embeddable;

const CHECKS: &[Check] = &[Check {
    feature: "image",
    reason: "only a PNG, JPEG or GIF image embedded as a `data:` URI is drawn (an SVG needs a \
             bitmap fallback); others, and one linked by URL or path, show their alt text",
    uses: |_, p| p.images().any(|src| embeddable(src).is_none()),
}];

/// The features of `table` that its DOCX output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, &[TABLE_RULES]);
    found.extend(find(&rt, CHECKS));
    found
}
