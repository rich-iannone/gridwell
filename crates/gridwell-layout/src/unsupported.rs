//! What a format can't express of a particular table, found before it's
//! written. A writer that drops or approximates a feature (LaTeX's vertical
//! alignment, Typst's double borders) lists [`Check`]s for it; [`find`] runs
//! them over every part of the table and says where it uses each feature, so a
//! caller can be warned rather than surprised by the output.

use std::fmt;

use gridwell_ir::content::ContentNode;

use crate::{Line, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable, Section, SectionKind};

/// Where in a table something is. Indices count from 0, as in the IR: a cell's
/// `row` within its part of the IR (`thead.rows`, or a group's `rows` or
/// `summary_rows`), its `col` in `column_spec`, a `group` in `tbody`. They are
/// shown counting from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The table as a whole: its language or direction.
    Table,
    Title,
    Subtitle,
    HeaderLine(usize),
    Cell {
        section: SectionKind,
        /// `None` in the head.
        group: Option<usize>,
        row: usize,
        col: usize,
    },
    GroupLabel(usize),
    Footnote(usize),
    SourceNote(usize),
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Place::Table => write!(f, "the table"),
            Place::Title => write!(f, "the title"),
            Place::Subtitle => write!(f, "the subtitle"),
            Place::HeaderLine(i) => write!(f, "header line {}", i + 1),
            Place::Cell {
                section,
                group,
                row,
                col,
            } => {
                let group = group.map_or(0, |g| g + 1);
                match section {
                    SectionKind::Head => write!(f, "head row {}", row + 1),
                    SectionKind::Body => write!(f, "group {group} row {}", row + 1),
                    SectionKind::Summary => write!(f, "group {group} summary row {}", row + 1),
                }?;
                write!(f, ", column {}", col + 1)
            }
            Place::GroupLabel(g) => write!(f, "the label of group {}", g + 1),
            Place::Footnote(i) => write!(f, "footnote {}", i + 1),
            Place::SourceNote(i) => write!(f, "source note {}", i + 1),
        }
    }
}

/// A feature a format can't express, and where a table uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// The feature, as the feature matrix names it (`vertical-align`).
    pub feature: &'static str,
    /// Why the format can't express it, and what the writer does instead.
    pub reason: &'static str,
    /// Where the table uses it, in document order; never empty.
    pub places: Vec<Place>,
}

/// One piece of a table's content: a cell (visible ones only), a header line,
/// a group label or a note.
#[derive(Debug, Clone, Copy)]
pub struct Part<'r> {
    pub place: Place,
    /// A cell's style after the cascade; a line's or note's own.
    pub style: &'r ResolvedStyle,
    pub content: &'r [ContentNode],
    /// For a cell: the cell, and the row it starts in.
    pub cell: Option<(&'r ResolvedCell<'r>, &'r ResolvedRow<'r>)>,
}

impl Part<'_> {
    /// A cell, or a group label: the parts drawn as cells of the grid.
    pub fn in_grid(&self) -> bool {
        matches!(self.place, Place::Cell { .. } | Place::GroupLabel(_))
    }

    /// The styles of the content's `styled_text` runs: each its own style, with
    /// no cascade (as writers draw them).
    pub fn runs<'s>(&'s self, rt: &'s ResolvedTable) -> impl Iterator<Item = ResolvedStyle> + 's {
        self.content.iter().filter_map(move |n| match n {
            ContentNode::StyledText {
                style_id: Some(id), ..
            } => Some(rt.style(id)),
            _ => None,
        })
    }

    /// The URL each linked run links to, as writers write it
    /// ([`gridwell_core::link::link_url`]): runs whose `href` isn't a link are
    /// left out.
    pub fn links(&self) -> impl Iterator<Item = String> + '_ {
        self.content.iter().filter_map(|n| match n {
            ContentNode::Text { href, .. } | ContentNode::StyledText { href, .. } => {
                href.as_deref().and_then(gridwell_core::link::link_url)
            }
            _ => None,
        })
    }

    /// The `src` of each image in the content.
    pub fn images(&self) -> impl Iterator<Item = &str> + '_ {
        self.content.iter().filter_map(|n| match n {
            ContentNode::Image { src, .. } => Some(src.as_str()),
            _ => None,
        })
    }
}

impl ResolvedTable<'_> {
    /// Every piece of content with its place and style, in document order (as
    /// [`contents`](Self::contents)).
    pub fn parts(&self) -> Vec<Part<'_>> {
        let h = &self.header;
        let mut out: Vec<Part<'_>> = Vec::new();
        out.extend(h.title.iter().map(|l| line(Place::Title, l)));
        out.extend(h.subtitle.iter().map(|l| line(Place::Subtitle, l)));
        out.extend(
            h.extra_lines
                .iter()
                .enumerate()
                .map(|(i, l)| line(Place::HeaderLine(i), l)),
        );
        cells(&mut out, &self.head, None);
        for (g, group) in self.groups.iter().enumerate() {
            out.extend(group.label.iter().map(|l| line(Place::GroupLabel(g), l)));
            for s in group.sections() {
                cells(&mut out, s, Some(g));
            }
        }
        out.extend(self.footer.footnotes.iter().enumerate().map(|(i, n)| Part {
            place: Place::Footnote(i),
            style: &n.style,
            content: &n.content,
            cell: None,
        }));
        out.extend(
            self.footer
                .source_notes
                .iter()
                .enumerate()
                .map(|(i, l)| line(Place::SourceNote(i), l)),
        );
        out
    }
}

fn line<'r>(place: Place, l: &'r Line<'_>) -> Part<'r> {
    Part {
        place,
        style: &l.style,
        content: &l.content,
        cell: None,
    }
}

fn cells<'r>(out: &mut Vec<Part<'r>>, section: &'r Section<'_>, group: Option<usize>) {
    for (r, row) in section.rows.iter().enumerate() {
        out.extend(row.cells().map(|c| Part {
            place: Place::Cell {
                section: section.kind,
                group,
                row: r,
                col: c.grid_col,
            },
            style: &c.style,
            content: &c.content,
            cell: Some((c, row)),
        }));
    }
}

/// A feature a format can't express, and how to tell a part uses it.
pub struct Check {
    pub feature: &'static str,
    pub reason: &'static str,
    pub uses: fn(&ResolvedTable, &Part) -> bool,
}

/// `config.rules` changing the default look's rules ([`crate::look`]), for a
/// format whose writer draws rules of its own (all but HTML, SVG, PPTX and XLSX).
pub const TABLE_RULES: TableCheck = TableCheck {
    feature: "table-rules",
    reason: "`config.rules` changes the rules of gridwell's default look, which this format \
             doesn't draw: its rules are its own",
    uses: |rt| rt.rules != crate::look::DEFAULT_RULES,
};

/// A feature of the table as a whole (its language, its direction), and how to
/// tell the table uses it.
pub struct TableCheck {
    pub feature: &'static str,
    pub reason: &'static str,
    pub uses: fn(&ResolvedTable) -> bool,
}

/// Run table-wide `checks`: one [`Unsupported`] at [`Place::Table`] for each
/// the table uses, in the order of `checks`.
pub fn find_table(rt: &ResolvedTable, checks: &[TableCheck]) -> Vec<Unsupported> {
    checks
        .iter()
        .filter(|c| (c.uses)(rt))
        .map(|c| Unsupported {
            feature: c.feature,
            reason: c.reason,
            places: vec![Place::Table],
        })
        .collect()
}

/// Run `checks` over every part of the table: one [`Unsupported`] for each
/// check that finds the feature in some part, in the order of `checks`.
pub fn find(rt: &ResolvedTable, checks: &[Check]) -> Vec<Unsupported> {
    let parts = rt.parts();
    checks
        .iter()
        .filter_map(|c| {
            let places: Vec<Place> = parts
                .iter()
                .filter(|p| (c.uses)(rt, p))
                .map(|p| p.place)
                .collect();
            (!places.is_empty()).then_some(Unsupported {
                feature: c.feature,
                reason: c.reason,
                places,
            })
        })
        .collect()
}
