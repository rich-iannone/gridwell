//! What SVG output can't express (see `gridwell_layout::unsupported`).

use gridwell_core::Image;
use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, Check, Unsupported};

const CHECKS: &[Check] = &[Check {
    feature: "image",
    reason: "only an image embedded as a `data:` URI (PNG, JPEG, GIF or SVG) is drawn, so the \
             SVG stays self-contained; one linked by URL or path shows its alt text",
    uses: |_, p| p.images().any(|src| Image::from_src(src).is_none()),
}];

/// The features of `table` that its SVG output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    find(&resolve(table), CHECKS)
}
