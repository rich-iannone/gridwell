//! What XLSX output can't express (see `gridwell_layout::unsupported`).

use gridwell_ir::Table;
use gridwell_layout::resolve;
use gridwell_layout::unsupported::{
    find, find_table, Check, Place, TableCheck, Unsupported, TABLE_RULES,
};

use crate::render::{font_pt, losses, picture, url_fits};

const TABLE_CHECKS: &[TableCheck] = &[TableCheck {
    feature: "lang",
    reason: "a spreadsheet's cells have no language",
    uses: |rt| rt.table.config.lang.is_some(),
}];

const CHECKS: &[Check] = &[
    Check {
        feature: "image",
        reason: "only an image alone in its cell or line, embedded as a PNG, JPEG or GIF `data:` \
                 URI, is drawn (over the cell); one beside text or in a footnote, an SVG, and \
                 one linked by URL or path show their alt text",
        uses: |_, p| {
            p.images().next().is_some()
                && (matches!(p.place, Place::Footnote(_))
                    || picture(p.content, font_pt(p.style)).is_none())
        },
    },
    Check {
        feature: "link",
        reason: "a cell links as a whole, to its first link: other links in it, and a URL \
                 over Excel's 2,080 characters (before or after its `#`), link nowhere",
        uses: |_, p| {
            let mut links = p.links();
            let first = links.next();
            first
                .as_deref()
                .is_some_and(|f| !url_fits(f) || links.any(|l| l != f))
        },
    },
];

/// The features of `table` that its XLSX output can't express, and where.
/// Text over Excel's limit is found by laying out the workbook, which knows
/// exactly what it cuts.
pub fn unsupported(table: &Table) -> Vec<Unsupported> {
    let rt = resolve(table);
    let mut found = find_table(&rt, TABLE_CHECKS);
    found.extend(find_table(&rt, &[TABLE_RULES]));
    found.extend(find(&rt, CHECKS));
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
