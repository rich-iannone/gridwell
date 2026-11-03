//! The base stylesheet: gridwell's default HTML look ([`gridwell_layout::look`],
//! shared with SVG), modeled on gt's (fonts, the rules above and below the table and
//! around the column labels, row-group labels, the stub, footnotes and source notes,
//! padding). It is the HTML counterpart of LaTeX's default booktabs rules: chosen by
//! table structure, never by data.
//!
//! Every selector is wrapped in `:where()`, which has zero specificity, so the
//! table's own styles (class rules) and any host-page CSS always win over it.
//! Rules target the part classes (`{prefix}_title`, `{prefix}_stub`, …).
//!
//! Cells never set an inherited property (font weight, alignment, color): a value
//! set directly on a cell beats one it would inherit from a row style on its
//! `<tr>`, whatever the specificity. Such properties go on the container or the
//! row instead, and `th` inherits its weight rather than being bold.
//!
//! The few one-sided rules (the stub's rule, the spanner rules' ends) are mirrored
//! for a right-to-left table (`dir: rtl`), whose first column is on the right.

use gridwell_layout::look::{Pad, Rule, LOOK};

/// A length in CSS: `0` without a unit, otherwise px.
fn px(v: f64) -> String {
    if v == 0.0 {
        "0".to_string()
    } else {
        format!("{v}px")
    }
}

/// A scale as a CSS percentage.
fn pct(scale: f64) -> String {
    format!("{}%", (scale * 100.0).round())
}

/// CSS `padding`, in its shortest form.
fn pad(p: &Pad) -> String {
    let (t, r, b, l) = (px(p.top), px(p.right), px(p.bottom), px(p.left));
    if r == l {
        if t == b {
            if t == r {
                t
            } else {
                format!("{t} {r}")
            }
        } else {
            format!("{t} {r} {b}")
        }
    } else {
        format!("{t} {r} {b} {l}")
    }
}

/// A CSS border value.
fn rule(r: &Rule) -> String {
    let style = if r.double { "double" } else { "solid" };
    format!("{} {style} {}", px(r.width_px), r.color.to_css())
}

/// Selector list `selector` scoped to the table's container `scope` (its id
/// selector, or nothing): each selector then matches only within it, and the
/// container's own only it.
pub fn scoped(scope: &str, p: &str, selector: &str) -> String {
    if scope.is_empty() {
        return selector.to_string();
    }
    let container = format!(".{p}_container");
    selector
        .split(", ")
        .map(|s| {
            if s == container {
                format!("{scope}{s}")
            } else {
                format!("{scope} {s}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The base rules for class prefix `p`, one per line, in order, scoped to the
/// container `scope` (see [`scoped`]); `rtl` mirrors them.
pub fn rules(p: &str, rtl: bool, scope: &str) -> Vec<String> {
    // The table's start and end sides.
    let (start, end) = if rtl {
        ("right", "left")
    } else {
        ("left", "right")
    };
    let l = &LOOK;
    let w = |selector: &str, decls: &str| {
        format!(":where({}) {{ {decls} }}", scoped(scope, p, selector))
    };
    let outer = rule(&l.outer_rule);
    let section = rule(&l.section_rule);
    let spanner_color = l.spanner_rule.color.to_css();
    let spanner_width = px(l.spanner_rule.width_px);
    let inset = l.spanner_rule_inset;
    vec![
        // The container shrinks to the table and is centered, so the heading and
        // footer are as wide as the table (gt puts both inside its table).
        w(
            &format!(".{p}_container"),
            &format!(
                "font-family: {}; color: {}; font-size: {}; line-height: normal; \
                 -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; \
                 width: fit-content; max-width: 100%; margin-left: auto; margin-right: auto",
                l.font_family,
                l.text_color.to_css(),
                px(l.font_size_px)
            ),
        ),
        // The first heading line (the heading is the container's first divs)
        // carries the top rule.
        w(
            &format!(
                ".{p}_title:first-of-type, .{p}_subtitle:first-of-type, \
                 .{p}_header_line:first-of-type"
            ),
            &format!("border-top: {outer}"),
        ),
        w(
            &format!(".{p}_title"),
            &format!(
                "font-size: {}; text-align: center; padding: {}",
                pct(l.title_scale),
                pad(&l.title_pad)
            ),
        ),
        w(
            &format!(".{p}_subtitle"),
            &format!(
                "font-size: {}; text-align: center; padding: {}",
                pct(l.subtitle_scale),
                pad(&l.subtitle_pad)
            ),
        ),
        w(
            &format!(".{p}_header_line"),
            &format!(
                "font-size: {}; text-align: center; padding: {}",
                pct(l.header_line_scale),
                pad(&l.header_line_pad)
            ),
        ),
        w(
            &format!(".{p}_table"),
            &format!(
                "border-collapse: collapse; margin: 0; width: 100%; \
                 border-top: {section}; border-bottom: {section}"
            ),
        ),
        // An edge drawn by its cells ([`gridwell_layout::look::EdgeRules`]): the
        // table's own rule is off, and the cells carrying it draw it instead.
        w(&format!(".{p}_cells_top"), "border-top: none"),
        w(&format!(".{p}_cells_bottom"), "border-bottom: none"),
        w(
            &format!(".{p}_table td, .{p}_table th"),
            &format!(
                "padding: {}; border-top: {}",
                pad(&l.cell_pad),
                rule(&l.row_rule)
            ),
        ),
        // Column labels are normal weight (not the browser's bold), unless a row
        // style says otherwise.
        w(&format!(".{p}_table th"), "font-weight: inherit"),
        // Column labels and spanners: no hairlines between head rows, bottom
        // aligned (on the row, so a row style can change it), a rule under the head.
        w(
            &format!(".{p}_table thead th"),
            &format!("border-top: none; padding: {}", pad(&l.head_cell_pad)),
        ),
        w(&format!(".{p}_table thead tr"), "vertical-align: bottom"),
        w(
            &format!(".{p}_table thead"),
            &format!("border-bottom: {section}"),
        ),
        w(&format!(".{p}_cells_head thead"), "border-bottom: none"),
        // Under such a head, the first row has no hairline (it would beat a thinner
        // border of the cell above).
        w(
            &format!(".{p}_cells_head thead + tbody > tr:first-child > *"),
            "border-top: none",
        ),
        w(&format!(".{p}_rule_top"), &format!("border-top: {section}")),
        w(
            &format!(".{p}_rule_bottom"),
            &format!("border-bottom: {section}"),
        ),
        // A spanner's rule is inset at each end, so neighboring spanners' rules
        // don't run together; in the first and last column the spanner has no
        // outer padding and its rule runs to the table's edge, as in gt (a
        // background line: a border can't be inset).
        w(
            &format!(".{p}_spanner"),
            &format!(
                "padding: {}; background-image: linear-gradient({spanner_color}, \
                 {spanner_color}); background-size: calc(100% - {}) {spanner_width}; \
                 background-position: bottom center; background-repeat: no-repeat",
                pad(&l.spanner_pad),
                px(2.0 * inset)
            ),
        ),
        w(
            &format!(".{p}_spanner:first-child"),
            &format!(
                "padding-{start}: 0; background-size: calc(100% - {}) {spanner_width}; \
                 background-position: bottom {start}",
                px(inset)
            ),
        ),
        w(
            &format!(".{p}_spanner:last-child"),
            &format!(
                "padding-{end}: 0; background-size: calc(100% - {}) {spanner_width}; \
                 background-position: bottom {end}",
                px(inset)
            ),
        ),
        w(
            &format!(".{p}_spanner:first-child:last-child"),
            &format!("background-size: 100% {spanner_width}"),
        ),
        w(
            &format!(".{p}_group_label"),
            &format!("border-top: {section}; border-bottom: {section}"),
        ),
        w(&format!(".{p}_stub"), &format!("border-{end}: {section}")),
        // A group's summary rows start under a section rule.
        w(
            &format!(
                "tbody > tr.{p}_summary:first-child > *, \
                 tr:not(.{p}_summary) + tr.{p}_summary > *"
            ),
            &format!("border-top: {section}"),
        ),
        // The first grand summary row gets gt's double rule.
        w(
            &format!(
                "tbody > tr.{p}_grand_summary:first-child > *, \
                 tr:not(.{p}_grand_summary) + tr.{p}_grand_summary > *"
            ),
            &format!("border-top: {}", rule(&l.grand_summary_rule)),
        ),
        // Numbers line up.
        w(
            &format!(".{p}_table tbody .{p}__al_right"),
            "font-variant-numeric: tabular-nums",
        ),
        w(&format!(".{p}_footer"), &format!("border-bottom: {outer}")),
        w(
            &format!(".{p}_footnote, .{p}_source_note"),
            &format!(
                "margin: 0; font-size: {}; padding: {}",
                pct(l.note_scale),
                pad(&l.note_pad)
            ),
        ),
        // Marks: small and raised (not a full superscript), italic, and not drawn
        // as links.
        w(
            &format!(".{p}_footnote_mark"),
            &format!(
                "font-size: {}; vertical-align: {}em; line-height: 0; font-style: italic",
                pct(l.mark_scale),
                l.mark_raise_em
            ),
        ),
        w(
            &format!(".{p}_footnote_mark a"),
            "color: inherit; text-decoration: none",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_uses_the_shortest_form() {
        let p = |t, r, b, l| {
            pad(&Pad {
                top: t,
                right: r,
                bottom: b,
                left: l,
            })
        };
        assert_eq!(p(4.0, 4.0, 4.0, 4.0), "4px");
        assert_eq!(p(4.0, 5.0, 4.0, 5.0), "4px 5px");
        assert_eq!(p(0.0, 5.0, 5.0, 5.0), "0 5px 5px");
        assert_eq!(p(1.0, 2.0, 3.0, 4.0), "1px 2px 3px 4px");
        assert_eq!(pct(0.85), "85%");
        assert_eq!(pct(1.25), "125%");
    }

    #[test]
    fn rtl_mirrors_exactly_the_one_sided_rules() {
        let (ltr, rtl) = (rules("gw", false, ""), rules("gw", true, ""));
        assert_eq!(ltr.len(), rtl.len());
        let mirror = |s: &str| {
            s.replace("left", "\0")
                .replace("right", "left")
                .replace('\0', "right")
        };
        let mut changed = 0;
        for (l, r) in ltr.iter().zip(&rtl) {
            if l != r {
                assert_eq!(&mirror(l), r);
                changed += 1;
            }
        }
        // The stub's rule and the first and last spanners'.
        assert_eq!(changed, 3);
        assert!(
            rtl.iter().any(|r| r.contains("_stub) { border-left:")),
            "{rtl:?}"
        );
    }

    #[test]
    fn edge_rules_come_after_the_rules_they_override() {
        // Every rule has zero specificity: the later of two wins.
        let rules = rules("gw", false, "");
        let at = |selector: &str| {
            rules
                .iter()
                .position(|r| r.starts_with(&format!(":where({selector})")))
                .unwrap_or_else(|| panic!("no {selector}"))
        };
        assert!(at(".gw_cells_top") > at(".gw_table"));
        assert!(at(".gw_cells_bottom") > at(".gw_table"));
        assert!(at(".gw_cells_head thead") > at(".gw_table thead"));
        let first_row = at(".gw_cells_head thead + tbody > tr:first-child > *");
        assert!(first_row > at(".gw_table td, .gw_table th"));
        for carried in [at(".gw_rule_top"), at(".gw_rule_bottom")] {
            assert!(carried > at(".gw_table thead th") && carried > first_row);
            // Group labels' and summaries' own rules still win.
            assert!(carried < at(".gw_group_label"));
            assert!(
                carried
                    < at("tbody > tr.gw_summary:first-child > *, \
                          tr:not(.gw_summary) + tr.gw_summary > *")
            );
        }
    }

    #[test]
    fn a_scope_prefixes_every_selector() {
        let (bare, scoped) = (rules("gw", false, ""), rules("gw", false, "#t"));
        assert_eq!(bare.len(), scoped.len());
        for (b, s) in bare.iter().zip(&scoped) {
            let selectors = |r: &str| {
                let inner = r.strip_prefix(":where(").unwrap().split(") {").next();
                inner
                    .unwrap()
                    .split(", ")
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            };
            let (b, s) = (selectors(b), selectors(s));
            assert_eq!(b.len(), s.len());
            for (b, s) in b.iter().zip(&s) {
                if b == ".gw_container" {
                    assert_eq!(s, "#t.gw_container");
                } else {
                    assert_eq!(s, &format!("#t {b}"));
                }
            }
        }
        assert!(scoped[0].starts_with(":where(#t.gw_container) {"));
        assert!(scoped[1].starts_with(
            ":where(#t .gw_title:first-of-type, #t .gw_subtitle:first-of-type, \
             #t .gw_header_line:first-of-type) {"
        ));
    }

    #[test]
    fn no_rule_misses_the_row_headers() {
        // A stub is a `<th>` in the body: a rule naming `td` must name `th` too.
        for line in rules("gw", false, "") {
            let selector = line.split('{').next().unwrap();
            let names = |tag: &str| {
                selector
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|w| w == tag)
            };
            assert!(!names("td") || names("th"), "{line}");
        }
    }
}
