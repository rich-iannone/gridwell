//! What PPTX output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, Check, Unsupported};

const CHECKS: &[Check] = &[Check {
    feature: "image",
    reason: "PowerPoint text (a table cell, or the text box of a title or note) holds no \
             pictures: an image shows its alt text",
    uses: |_, p| p.images().next().is_some(),
}];

/// The features of `table` that its PPTX output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    find(&resolve(table), CHECKS)
}
