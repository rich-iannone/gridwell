//! What Typst output can't express (see `gridwell_layout::unsupported`).

use gridwell_core::Image;
use gridwell_ir::{BorderStyle, Table};
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{find, Check, Unsupported};

use crate::render::BASE_PT;

const CHECKS: &[Check] = &[
    Check {
        feature: "border-double",
        reason: "Typst strokes have no double line; a double border is drawn solid",
        uses: |_, p| {
            p.in_grid()
                && p.style
                    .strokes(BASE_PT)
                    .iter()
                    .any(|(_, k)| k.as_ref().is_some_and(|k| k.style == BorderStyle::Double))
        },
    },
    Check {
        feature: "image",
        reason: "only an image embedded as a `data:` URI (PNG, JPEG, GIF or SVG) is drawn; one \
                 linked by URL or path shows its alt text",
        uses: |_, p| p.images().any(|src| Image::from_src(src).is_none()),
    },
];

/// The features of `table` that its Typst output can't express, and where.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    find(&resolve(table), CHECKS)
}
