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

/// The base rules for class prefix `p`, one per line, in order.
pub fn rules(p: &str) -> Vec<String> {
    let l = &LOOK;
    let w = |selector: &str, decls: &str| format!(":where({selector}) {{ {decls} }}");
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
                "padding-left: 0; background-size: calc(100% - {}) {spanner_width}; \
                 background-position: bottom left",
                px(inset)
            ),
        ),
        w(
            &format!(".{p}_spanner:last-child"),
            &format!(
                "padding-right: 0; background-size: calc(100% - {}) {spanner_width}; \
                 background-position: bottom right",
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
        w(&format!(".{p}_stub"), &format!("border-right: {section}")),
        // A group's summary rows start under a section rule.
        w(
            &format!(
                "tbody > tr.{p}_summary:first-child td, \
                 tr:not(.{p}_summary) + tr.{p}_summary td"
            ),
            &format!("border-top: {section}"),
        ),
        // The first grand summary row gets gt's double rule.
        w(
            &format!(
                "tbody > tr.{p}_grand_summary:first-child td, \
                 tr:not(.{p}_grand_summary) + tr.{p}_grand_summary td"
            ),
            &format!("border-top: {}", rule(&l.grand_summary_rule)),
        ),
        // Numbers line up.
        w(
            &format!(".{p}_table td.{p}__al_right"),
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
}
