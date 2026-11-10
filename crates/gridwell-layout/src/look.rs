//! The default look: gridwell's base styling for formats that draw their own rules
//! and spacing (HTML's base stylesheet, SVG, PPTX), modeled on gt's defaults. One set of
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
//! A table's `config.rules` change these rules ([`TableRules`]).
//!
//! A cell's own border replaces the rule on its edge. On the table's top and
//! bottom and under the head, once a cell sets its own border, the edge is drawn by
//! its cells ([`EdgeRules`]). [`BaseRules`] puts the rules on the row boundaries
//! and says which cells draw them.
//!
//! Column labels are normal weight (not bold), heading lines are centered, and the
//! table is as wide as its widest heading or footer line.

pub mod edges;
pub mod rules;

pub use edges::{Edge, EdgePart, EdgeRules};
use gridwell_core::{Color, Length};
use gridwell_ir::{BorderStyle, Table};
pub use rules::{BaseRules, OwnEdges};

/// A horizontal or vertical line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// Always positive and finite.
    pub width_px: f64,
    pub color: Color,
    pub style: LineStyle,
}

impl Rule {
    /// Two lines with a gap between (CSS `double`).
    pub fn is_double(&self) -> bool {
        self.style == LineStyle::Double
    }
}

/// How a rule is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    /// Two lines with a gap between; only on rules at least 3px wide (CSS draws a
    /// narrower one solid, and so does [`TableRules`]).
    Double,
}

impl LineStyle {
    /// The style's CSS name.
    pub fn css(self) -> &'static str {
        match self {
            LineStyle::Solid => "solid",
            LineStyle::Dashed => "dashed",
            LineStyle::Dotted => "dotted",
            LineStyle::Double => "double",
        }
    }

    /// Where two rules meet, CSS's collapsed borders draw the stronger style of
    /// two equally wide ones: double, then solid, dashed and dotted.
    fn strength(self) -> u8 {
        match self {
            LineStyle::Double => 3,
            LineStyle::Solid => 2,
            LineStyle::Dashed => 1,
            LineStyle::Dotted => 0,
        }
    }
}

/// The rules of one table: the default look's, each changed by the table's
/// `config.rules`. A field a rule there sets replaces that field of the default
/// (an invalid value changes nothing), and a `style` of `none` or `hidden`, or a
/// width of 0, removes the rule (`None`). The names are `config.rules`'.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableRules {
    /// [`Look::outer_rule`] above the heading.
    pub title: Option<Rule>,
    /// [`Look::section_rule`] on the table's top.
    pub top: Option<Rule>,
    /// [`Look::section_rule`] under the column labels.
    pub column_labels: Option<Rule>,
    /// [`Look::spanner_rule`].
    pub spanners: Option<Rule>,
    /// [`Look::section_rule`] above and below each group label.
    pub group: Option<Rule>,
    /// [`Look::row_rule`].
    pub rows: Option<Rule>,
    /// [`Look::section_rule`] after the stub.
    pub stub: Option<Rule>,
    /// [`Look::section_rule`] between a group's summary rows and its data rows.
    pub summary: Option<Rule>,
    /// [`Look::grand_summary_rule`].
    pub grand_summary: Option<Rule>,
    /// [`Look::section_rule`] on the table's bottom.
    pub bottom: Option<Rule>,
    /// [`Look::outer_rule`] below the footer.
    pub notes: Option<Rule>,
}

/// The default look's rules, which `config.rules` change.
pub const DEFAULT_RULES: TableRules = TableRules {
    title: Some(LOOK.outer_rule),
    top: Some(LOOK.section_rule),
    column_labels: Some(LOOK.section_rule),
    spanners: Some(LOOK.spanner_rule),
    group: Some(LOOK.section_rule),
    rows: Some(LOOK.row_rule),
    stub: Some(LOOK.section_rule),
    summary: Some(LOOK.section_rule),
    grand_summary: Some(LOOK.grand_summary_rule),
    bottom: Some(LOOK.section_rule),
    notes: Some(LOOK.outer_rule),
};

impl TableRules {
    /// The table's rules. See the type's docs.
    pub fn new(table: &Table) -> Self {
        let mut out = DEFAULT_RULES;
        let Some(set) = &table.config.rules else {
            return out;
        };
        let pairs = [
            (&mut out.title, &set.title),
            (&mut out.top, &set.top),
            (&mut out.column_labels, &set.column_labels),
            (&mut out.spanners, &set.spanners),
            (&mut out.group, &set.group),
            (&mut out.rows, &set.rows),
            (&mut out.stub, &set.stub),
            (&mut out.summary, &set.summary),
            (&mut out.grand_summary, &set.grand_summary),
            (&mut out.bottom, &set.bottom),
            (&mut out.notes, &set.notes),
        ];
        for (rule, border) in pairs {
            if let (Some(r), Some(b)) = (rule.as_mut(), border) {
                *rule = changed(*r, b);
            }
        }
        out
    }
}

/// `rule` with what `b` sets changed. See [`TableRules`].
fn changed(mut rule: Rule, b: &gridwell_ir::style::Border) -> Option<Rule> {
    match &b.style {
        Some(BorderStyle::None | BorderStyle::Hidden) => return None,
        Some(BorderStyle::Solid) => rule.style = LineStyle::Solid,
        Some(BorderStyle::Dashed) => rule.style = LineStyle::Dashed,
        Some(BorderStyle::Dotted) => rule.style = LineStyle::Dotted,
        Some(BorderStyle::Double) => rule.style = LineStyle::Double,
        Some(BorderStyle::Unknown(_)) | None => {}
    }
    let width = b.width.as_deref().and_then(|w| w.parse::<Length>().ok());
    // In px, with em (and rem) of the look's font size.
    let pt = LOOK.font_size_px * 0.75;
    match width.and_then(|w| w.to_pt(pt, pt)).map(|pt| pt / 0.75) {
        Some(0.0) => return None,
        Some(px) if px > 0.0 && px.is_finite() => rule.width_px = px,
        _ => {}
    }
    if let Some(c) = b.color.as_deref().and_then(|c| c.parse::<Color>().ok()) {
        rule.color = c;
    }
    if rule.style == LineStyle::Double && rule.width_px < 3.0 {
        rule.style = LineStyle::Solid;
    }
    Some(rule)
}

/// Where two rules meet, whether `a` is drawn over `b`, as CSS's collapsed borders
/// decide: the wider wins, then the stronger style ([`LineStyle`]); `None` if
/// neither does.
pub fn stronger(a: &Rule, b: &Rule) -> Option<bool> {
    if a.width_px != b.width_px {
        Some(a.width_px > b.width_px)
    } else if a.style != b.style {
        Some(a.style.strength() > b.style.strength())
    } else {
        None
    }
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
        style: LineStyle::Solid,
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
        style: LineStyle::Double,
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
