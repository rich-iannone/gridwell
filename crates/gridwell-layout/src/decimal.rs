//! Alignment on a character (`align: char`): a column's numbers lined up on
//! their decimal points (or any `align_char`).
//!
//! No format but HTML 4 ever had this natively (and no browser implements
//! that), so the layout does it the way typesetters do by hand: every cell
//! aligned on a character is right-aligned and ends with invisible padding — the
//! character itself, for a cell without one, then as many digits as it is short
//! of the column's longest run after the character ([`AlignPad`]). Each writer
//! draws the padding invisibly in its own way: hidden text where the format has
//! it (exact, with tabular figures), else figure and punctuation spaces.
//!
//! Which cells: those of the body and summary rows whose alignment resolves to
//! `char` (the column's, or a style's `text_align`) and that span one column; a
//! column label or a cell spanning columns is right-aligned. Each cell's last
//! line is measured, by the text after the first occurrence of the character
//! (trailing white space aside), in characters: digits, as a column of numbers
//! has. A cell without the character ends where the character would be; a blank
//! cell is left as it is. With no cell holding the character, nothing is padded.

use gridwell_ir::content::ContentNode;
use gridwell_ir::ColumnSpec;

use crate::{plain_text, ResolvedTable, Slot};

/// The character a column aligns on when it names none.
pub const DEFAULT_CHAR: &str = ".";

/// The character a column aligns on: its `align_char`, else [`DEFAULT_CHAR`].
pub(crate) fn char_of(spec: &ColumnSpec) -> &str {
    spec.align_char
        .as_deref()
        .filter(|c| !c.is_empty())
        .unwrap_or(DEFAULT_CHAR)
}

/// The invisible padding a cell aligned on a character ends with, so that the
/// character lines up with the others in its column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignPad<'a> {
    /// The character, for a cell that has none: its text then ends where the
    /// character would be.
    pub mark: Option<&'a str>,
    /// How many digits the text after the character is short of the column's
    /// longest.
    pub digits: usize,
}

impl AlignPad<'_> {
    /// The padding as text to draw invisibly in the cell's font (`\phantom`,
    /// `hide`, `visibility: hidden`): the character, then zeros. Exact with
    /// tabular figures.
    pub fn text(&self) -> String {
        let mut s = self.mark.unwrap_or("").to_string();
        s.extend(std::iter::repeat_n('0', self.digits));
        s
    }

    /// The padding as spaces, for a format that can't hide text: a punctuation
    /// space (U+2008, as wide as a period) for each character of the mark and a
    /// figure space (U+2007, as wide as a digit) for each digit. As exact as the
    /// font's spaces are.
    pub fn spaces(&self) -> String {
        let marks = self.mark.map_or(0, |m| m.chars().count());
        let mut s: String = std::iter::repeat_n('\u{2008}', marks).collect();
        s.extend(std::iter::repeat_n('\u{2007}', self.digits));
        s
    }
}

/// How many characters follow the first `mark` on the last line of `content`
/// (trailing white space aside): `Some(None)` for a line without the mark,
/// `None` for a blank one.
fn after_mark(content: &[ContentNode], mark: &str) -> Option<Option<usize>> {
    let start = content
        .iter()
        .rposition(|n| matches!(n, ContentNode::LineBreak {}))
        .map_or(0, |i| i + 1);
    let line = plain_text(&content[start..], "");
    let line = line.trim_end();
    if line.trim_start().is_empty() {
        return None;
    }
    Some(
        line.find(mark)
            .map(|i| line[i + mark.len()..].chars().count()),
    )
}

/// Set [`crate::ResolvedCell::align_pad`] on every cell aligned on a character,
/// column by column (and character by character, should styles name several).
pub(crate) fn pad(rt: &mut ResolvedTable<'_>) {
    for col in 0..rt.columns.len() {
        let mut marks: Vec<&str> = Vec::new();
        for (_, mark) in cells(rt, col) {
            if !marks.contains(&mark) {
                marks.push(mark);
            }
        }
        for mark in marks {
            let longest = cells(rt, col)
                .filter(|&(_, m)| m == mark)
                .filter_map(|(c, _)| after_mark(c, mark).flatten())
                .max();
            let Some(longest) = longest else {
                continue;
            };
            for group in &mut rt.groups {
                for section in [&mut group.rows, &mut group.summary_rows] {
                    for row in &mut section.rows {
                        let Some(Slot::Origin(cell)) = row.slots.get_mut(col) else {
                            continue;
                        };
                        if cell.align_char != Some(mark) {
                            continue;
                        }
                        cell.align_pad = match after_mark(&cell.content, mark) {
                            None => None,
                            Some(None) => Some(AlignPad {
                                mark: Some(mark),
                                digits: longest,
                            }),
                            Some(Some(n)) => (n < longest).then_some(AlignPad {
                                mark: None,
                                digits: longest - n,
                            }),
                        };
                    }
                }
            }
        }
    }
}

/// The content and character of each cell starting in column `col` that aligns
/// on one.
fn cells<'r, 'a>(
    rt: &'r ResolvedTable<'a>,
    col: usize,
) -> impl Iterator<Item = (&'r [ContentNode], &'a str)> + 'r {
    rt.groups
        .iter()
        .flat_map(|g| g.rows.rows.iter().chain(&g.summary_rows.rows))
        .filter_map(move |r| r.slots.get(col).and_then(Slot::origin))
        .filter_map(|c| Some((&*c.content, c.align_char?)))
}

#[cfg(test)]
mod tests;
