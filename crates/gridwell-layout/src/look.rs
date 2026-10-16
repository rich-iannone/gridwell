//! The default look: gridwell's base styling for formats that draw their own rules
//! and spacing (HTML's base stylesheet, SVG), modeled on gt's defaults. One set of
//! numbers, so the formats can't drift apart.
//!
//! It is chosen by table structure, never by data, and always yields to the table's
//! own styles. Where each rule goes:
//!
//! - [`Look::outer_rule`]: above the first heading line (title, subtitle or extra
//!   line) and below the footer, when they exist.
//! - [`Look::section_rule`]: the table's own top and bottom (the outermost rules when
//!   there is no heading or footer, as in gt), under the head, above and below each
//!   group label, right of the stub, and above a group's first summary row.
//! - [`Look::row_rule`]: between rows (above every cell outside the head).
//! - [`Look::spanner_rule`]: under each spanner label, inset
//!   [`Look::spanner_rule_inset`] at each end except at the table's left and right
//!   edges, where the spanner also has no outer padding.
//! - [`Look::grand_summary_rule`]: above the first grand summary row.
//!
//! Column labels are normal weight (not bold), heading lines are centered, and the
//! table is as wide as its widest heading or footer line.

use gridwell_core::Color;

/// A horizontal or vertical line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    pub width_px: f64,
    pub color: Color,
    /// Two lines with a gap between (CSS `double`); otherwise solid.
    pub double: bool,
}

/// Padding around a box, in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pad {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Pad {
    const fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Every number in the default look.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// A CSS font list (gt's).
    pub font_family: &'static str,
    /// Body text size, in px.
    pub font_size_px: f64,
    pub text_color: Color,
    /// Sizes relative to the body text.
    pub title_scale: f64,
    pub subtitle_scale: f64,
    pub header_line_scale: f64,
    pub note_scale: f64,
    pub mark_scale: f64,
    /// How far a footnote mark is raised, in em of the surrounding text.
    pub mark_raise_em: f64,
    pub title_pad: Pad,
    pub subtitle_pad: Pad,
    pub header_line_pad: Pad,
    /// Body, group-label and summary cells.
    pub cell_pad: Pad,
    /// Column-label cells (and empty cells in spanner rows).
    pub head_cell_pad: Pad,
    /// Spanner cells; [`Pad::left`] / [`Pad::right`] drop to 0 at the table's edges.
    pub spanner_pad: Pad,
    /// Footnotes and source notes.
    pub note_pad: Pad,
    pub outer_rule: Rule,
    pub section_rule: Rule,
    pub row_rule: Rule,
    pub spanner_rule: Rule,
    /// How far a spanner's rule stops short of each end of its cell, in px.
    pub spanner_rule_inset: f64,
    pub grand_summary_rule: Rule,
}

const fn solid(width_px: f64, color: Color) -> Rule {
    Rule {
        width_px,
        color,
        double: false,
    }
}

/// gt's dark gray for the outer rules.
const OUTER: Color = Color::rgb(0xA8, 0xA8, 0xA8);
/// gt's light gray for every other rule.
const LIGHT: Color = Color::rgb(0xD3, 0xD3, 0xD3);

/// The default look.
pub const LOOK: Look = Look {
    font_family: "system-ui, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
    font_size_px: 16.0,
    text_color: Color::rgb(0x33, 0x33, 0x33),
    title_scale: 1.25,
    subtitle_scale: 0.85,
    header_line_scale: 0.85,
    note_scale: 0.9,
    mark_scale: 0.75,
    mark_raise_em: 0.4,
    title_pad: Pad::new(4.0, 5.0, 4.0, 5.0),
    subtitle_pad: Pad::new(3.0, 5.0, 5.0, 5.0),
    header_line_pad: Pad::new(0.0, 5.0, 5.0, 5.0),
    cell_pad: Pad::new(8.0, 5.0, 8.0, 5.0),
    head_cell_pad: Pad::new(5.0, 5.0, 6.0, 5.0),
    spanner_pad: Pad::new(5.0, 4.0, 7.0, 4.0),
    note_pad: Pad::new(4.0, 5.0, 4.0, 5.0),
    outer_rule: solid(2.0, OUTER),
    section_rule: solid(2.0, LIGHT),
    row_rule: solid(1.0, LIGHT),
    spanner_rule: solid(2.0, LIGHT),
    spanner_rule_inset: 4.0,
    grand_summary_rule: Rule {
        width_px: 6.0,
        color: LIGHT,
        double: true,
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_and_sizes_are_sane() {
        let l = &LOOK;
        for r in [
            l.outer_rule,
            l.section_rule,
            l.row_rule,
            l.spanner_rule,
            l.grand_summary_rule,
        ] {
            assert!(r.width_px > 0.0 && r.color.a == 255);
        }
        // A double rule needs room for two lines and a gap.
        assert!(l.grand_summary_rule.width_px >= 3.0);
        assert!(l.spanner_rule_inset * 2.0 < 40.0);
        for s in [l.title_scale, l.subtitle_scale, l.note_scale, l.mark_scale] {
            assert!(s > 0.5 && s < 2.0);
        }
        // The spanner rule sits in the spanner's bottom padding.
        assert!(l.spanner_pad.bottom >= l.spanner_rule.width_px);
    }
}
