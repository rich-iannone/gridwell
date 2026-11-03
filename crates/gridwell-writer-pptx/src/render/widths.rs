//! Column widths. A slide's table doesn't fit its columns to their content
//! (PowerPoint, Keynote and Impress take each `gridCol` as written, wrapping
//! what doesn't fit), so a column without a width is sized here for its widest
//! cell, from estimated text widths ([`gridwell_layout::measure`]): wider than
//! the theme's Calibri, and than the fonts a viewer without it substitutes, so
//! nothing wraps that has room not to.

use std::convert::Infallible;

use gridwell_core::Length;
use gridwell_layout::measure::{char_em, BOLD_FACTOR};
use gridwell_layout::{ColumnWidth, ContentWidth, ResolvedCell, ResolvedTable, Slot};

use super::{cell_fmt, for_each_run, label_fmt, pad_fmt, Fmt, Para, Run, BASE_PT, TEXT_W};
use crate::xml;

pub(super) const EMU_PER_PT: f64 = 12700.0;

/// A cell's and a text box's inset left and right of its text: DrawingML's
/// default (`marL`, `marR`, `lIns`, `rIns`: 91440 EMU, 0.1in).
pub(super) const INSET_PT: f64 = 7.2;

/// Room beyond the estimate: in proportion, for the letters DejaVu Sans Bold
/// draws wider than the bold estimate (`i`, `l`, `t`: a few percent), and a
/// point for rounding, so text about as wide as its estimate doesn't wrap.
const SLACK: f64 = 0.04;
const SLACK_PT: f64 = 1.0;

/// Column widths in EMU: absolute lengths convert exactly, `%` and `fr` resolve
/// against the table's width (else the slide's text width, as the text boxes),
/// `auto` fits the column's content ([`content_widths`]; when the table has a
/// width, such columns share it instead), and `em` is the table text's size (see
/// [`ResolvedTable::column_widths_fit_pt`]). They add up to at most `i32::MAX`
/// EMU (about 60m), so the frame's width holds them.
pub(super) fn column_widths(rt: &ResolvedTable) -> Vec<u32> {
    let line_pt = f64::from(TEXT_W) / EMU_PER_PT;
    let auto_pt = f64::from(xml::DEFAULT_COL_WIDTH_EMU) / EMU_PER_PT;
    let content = content_widths(rt, line_pt, auto_pt);
    rt.column_widths_fit_in(
        line_pt,
        auto_pt,
        BASE_PT,
        &content,
        EMU_PER_PT,
        i32::MAX as u32,
    )
}

/// The room each column's content asks for, in points (empty when no column is
/// `auto`). A cell spanning one column asks it for its own, within the column's
/// `min_width` and `max_width`; a cell spanning several (a spanner, a group
/// label) asks the `auto` columns it spans that are under their `max_width`, in
/// equal shares, for what the others it spans don't give it (an `fr` column
/// counts for nothing: it gets what is left). As the SVG writer sizes them.
fn content_widths(rt: &ResolvedTable, line_pt: f64, auto_pt: f64) -> Vec<ContentWidth> {
    let widths = rt.column_widths();
    if !widths.contains(&ColumnWidth::Auto) {
        return Vec::new();
    }
    let n = widths.len();
    let given: Vec<f64> = rt
        .column_widths_pt(line_pt, auto_pt, BASE_PT)
        .into_iter()
        .zip(&widths)
        .map(|(pt, w)| match w {
            ColumnWidth::Fixed(_) | ColumnWidth::Percent(_) => pt,
            ColumnWidth::Auto | ColumnWidth::Fr(_) => 0.0,
        })
        .collect();
    let mut cols = vec![ContentWidth::fixed(room(0.0)); n];
    // (first column, columns spanned, room)
    let mut spans: Vec<(usize, usize, ContentWidth)> = Vec::new();
    let strong = std::iter::once((&rt.head, true)).chain(
        rt.groups
            .iter()
            .flat_map(|g| [(&g.rows, false), (&g.summary_rows, true)]),
    );
    for (section, strong) in strong {
        for slot in section.rows.iter().flat_map(|r| &r.slots) {
            let Slot::Origin(cell) = slot else { continue };
            let need = cell_room(rt, cell, strong);
            match cols.get_mut(cell.col) {
                Some(c) if cell.colspan == 1 => {
                    c.min = c.min.max(need.min);
                    c.max = c.max.max(need.max);
                }
                _ => spans.push((cell.col, cell.colspan, need)),
            }
        }
    }
    for label in rt.groups.iter().filter_map(|g| g.label.as_ref()) {
        let mut m = Measure::default();
        let _ = for_each_run(rt, &label.content, label_fmt(label), &mut |r| m.add(r));
        spans.push((0, n, m.room()));
    }
    let pt = |l: Option<&Length>| {
        l.and_then(|l| l.to_pt(BASE_PT, BASE_PT))
            .filter(|pt| pt.is_finite() && *pt > 0.0)
    };
    let mut capped = vec![false; n];
    for (i, col) in rt.columns.iter().enumerate() {
        let lo = pt(col.min_width.as_ref()).unwrap_or(0.0);
        let hi = pt(col.max_width.as_ref()).map(|hi| hi.max(lo));
        let c = &mut cols[i];
        c.max = c.max.max(lo).min(hi.unwrap_or(f64::INFINITY));
        c.min = c.min.max(lo).min(c.max);
        capped[i] = hi.is_some_and(|hi| c.max >= hi);
    }
    // Narrow spans first, so wider ones see what they asked for.
    spans.sort_by_key(|&(_, span, _)| span);
    for (first, span, need) in spans {
        let spanned = first.min(n)..first.saturating_add(span).min(n);
        let auto: Vec<usize> = spanned
            .clone()
            .filter(|&i| widths[i] == ColumnWidth::Auto && !capped[i])
            .collect();
        if auto.is_empty() {
            continue;
        }
        // What the spanned columns give it so far.
        let have = |of: fn(&ContentWidth) -> f64| -> f64 {
            spanned
                .clone()
                .map(|i| match widths[i] {
                    ColumnWidth::Auto => of(&cols[i]),
                    _ => given[i],
                })
                .sum()
        };
        let share = |want: f64, have: f64| (want - have).max(0.0) / auto.len() as f64;
        let min = share(need.min, have(|c| c.min));
        let max = share(need.max, have(|c| c.max));
        for &i in &auto {
            cols[i].min += min;
            cols[i].max = (cols[i].max + max).max(cols[i].min);
        }
    }
    cols
}

/// The room the widest line of text box paragraphs asks for, in points.
pub(super) fn text_room<'a>(
    rt: &ResolvedTable,
    paras: impl IntoIterator<Item = &'a Para<'a>>,
) -> f64 {
    paras
        .into_iter()
        .map(|p| {
            let mut m = Measure::default();
            let _ = p.for_each_run(rt, &mut |r| m.add(r));
            m.room().max
        })
        .fold(0.0, f64::max)
}

/// A cell's room: its content, then its alignment padding, as written.
fn cell_room(rt: &ResolvedTable, cell: &ResolvedCell, strong: bool) -> ContentWidth {
    let fmt = cell_fmt(cell, strong);
    let mut m = Measure::default();
    let _ = for_each_run(rt, &cell.content, fmt, &mut |r| m.add(r));
    if let Some(pad) = cell.align_pad {
        let _ = m.add(Some((&pad.text(), pad_fmt(fmt))));
    }
    m.room()
}

/// A width of text in a cell: with its slack and insets.
fn room(text_pt: f64) -> f64 {
    text_pt * (1.0 + SLACK) + SLACK_PT + 2.0 * INSET_PT
}

/// The widest line and word of runs, in points. A footnote mark counts at the
/// size of its text (a viewer draws it smaller).
#[derive(Debug, Default)]
struct Measure {
    line: f64,
    word: f64,
    widest_line: f64,
    widest_word: f64,
}

impl Measure {
    fn add(&mut self, run: Run) -> Result<(), Infallible> {
        let Some((text, fmt)) = run else {
            self.line = 0.0;
            self.word = 0.0;
            return Ok(());
        };
        let (_, pt) = run_size(&fmt);
        for c in text.chars() {
            let w = char_em(c) * pt;
            self.line += w;
            if breaks(c) {
                self.word = 0.0;
            } else {
                self.word += w;
                // Spaces ending a line hang past it.
                self.widest_line = self.widest_line.max(self.line);
                self.widest_word = self.widest_word.max(self.word);
            }
        }
        Ok(())
    }

    fn room(&self) -> ContentWidth {
        ContentWidth {
            min: room(self.widest_word),
            max: room(self.widest_line),
        }
    }
}

/// A run's size, and the size its characters are measured at (wider when
/// bold), in points.
pub(super) fn run_size(fmt: &Fmt) -> (f64, f64) {
    let size = fmt.size.map_or(BASE_PT, |s| f64::from(s) / 100.0);
    (size, if fmt.bold { size * BOLD_FACTOR } else { size })
}

/// A line may break at `c`: whitespace but the no-break spaces.
pub(super) fn breaks(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{A0}' | '\u{2007}' | '\u{202F}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridwell_ir::{StyleDef, Table};
    use gridwell_layout::measure::text_em;
    use gridwell_layout::resolve;
    use gridwell_testkit::*;

    /// Each column's width in points.
    fn widths(t: &Table) -> Vec<f64> {
        column_widths(&resolve(t))
            .into_iter()
            .map(|w| f64::from(w) / EMU_PER_PT)
            .collect()
    }

    /// The room `text` asks for at `size` points.
    fn need(text: &str, bold: bool, size: f64) -> f64 {
        room(text_em(text, bold) * size)
    }

    /// Within the rounding to EMU.
    fn assert_close(got: &[f64], want: &[f64]) {
        assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() <= 0.5 / EMU_PER_PT, "{got:?} vs {want:?}");
        }
    }

    fn two_columns() -> Vec<gridwell_ir::ColumnSpec> {
        vec![column("a", "A"), column("b", "B")]
    }

    #[test]
    fn a_column_is_as_wide_as_its_widest_line() {
        let t = TableBuilder::new(2)
            .columns(two_columns())
            .head(row(vec![cell("Name"), cell("Amount")]).role("column_label"))
            .body(vec![
                row(vec![cell("Al"), cell("1")]),
                row(vec![
                    cell_content(vec![text("short"), line_break(), text("the second line")]),
                    cell("2 3"),
                ]),
            ])
            .build();
        // The head is bold; lines are apart.
        let want = [
            need("the second line", false, BASE_PT),
            need("Amount", true, BASE_PT),
        ];
        assert_close(&widths(&t), &want);
        assert!(need("Amount", true, BASE_PT) > need("Amount", false, BASE_PT));
    }

    #[test]
    fn runs_count_at_their_own_size_and_weight() {
        let big = StyleDef {
            font_size: Some("36pt".into()),
            ..StyleDef::default()
        };
        let bold = StyleDef {
            font_weight: Some("bold".into()),
            ..StyleDef::default()
        };
        let t = TableBuilder::new(2)
            .columns(two_columns())
            .style_def("big", big)
            .style_def("bold", bold)
            .body(vec![row(vec![
                cell("Big").style("big"),
                cell_content(vec![
                    text("ab"),
                    styled("cd", "bold"),
                    footnote_mark("n", "1"),
                    image("x.png", "pic"),
                    raw("latex", "\\textbf{wide raw markup}"),
                ]),
            ])])
            .build();
        let mixed = (text_em("ab", false) + text_em("cd", true) + text_em("1pic", false)) * BASE_PT;
        assert_close(&widths(&t), &[need("Big", false, 36.0), room(mixed)]);
    }

    #[test]
    fn the_alignment_padding_counts() {
        let t = TableBuilder::new(1)
            .columns(vec![column("v", "V").align("char")])
            .body(vec![row(vec![cell("1234.5")]), row(vec![cell("6.789")])])
            .build();
        // `1234.5` ends with `00` (drawn without a fill).
        assert_close(&widths(&t), &[need("1234.500", false, BASE_PT)]);
    }

    #[test]
    fn a_spanning_cell_widens_the_columns_it_spans() {
        let wide = "A spanner wider than its columns";
        let t = TableBuilder::new(3)
            .columns(vec![
                column("a", "A"),
                column("b", "B"),
                column("c", "C").width("50pt"),
            ])
            .head(row(vec![cell(wide).colspan(2), placeholder(), cell("C")]).role("column_spanner"))
            .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
            .body(vec![row(vec![cell("x"), cell("yyy"), cell("z")])])
            .build();
        // Each auto column gets half of what the spanner needs beyond them.
        let (a, b) = (need("A", true, BASE_PT), need("yyy", false, BASE_PT));
        let extra = (need(wide, true, BASE_PT) - a - b) / 2.0;
        assert_close(&widths(&t), &[a + extra, b + extra, 50.0]);
        // Over an absolute column too: that column gives what it is.
        let t = TableBuilder::new(2)
            .columns(vec![column("a", "A"), column("b", "B").width("80pt")])
            .head(row(vec![cell(wide).colspan(2), placeholder()]).role("column_spanner"))
            .body(vec![row(vec![cell("x"), cell("y")])])
            .build();
        assert_close(&widths(&t), &[need(wide, true, BASE_PT) - 80.0, 80.0]);
    }

    #[test]
    fn a_group_label_widens_the_columns() {
        let label = "A long group label, wider than the table";
        let t = TableBuilder::new(2)
            .columns(two_columns())
            .group(labeled_group(label, vec![row(vec![cell("x"), cell("yy")])]))
            .build();
        let (a, b) = (need("x", false, BASE_PT), need("yy", false, BASE_PT));
        let extra = (need(label, true, BASE_PT) - a - b) / 2.0;
        assert_close(&widths(&t), &[a + extra, b + extra]);
    }

    #[test]
    fn too_wide_for_the_slide_they_narrow_to_fit_it() {
        let long = "word ".repeat(40);
        let t = TableBuilder::new(2)
            .columns(two_columns())
            .body(vec![row(vec![cell(&long), cell(&long.repeat(2))])])
            .build();
        let got = widths(&t);
        let line = f64::from(TEXT_W) / EMU_PER_PT;
        assert!(
            (got.iter().sum::<f64>() - line).abs() <= 1.0 / EMU_PER_PT,
            "{got:?}"
        );
        // In proportion to how much they narrow.
        let (min, max) = (
            need("word", false, BASE_PT),
            need(long.trim_end(), false, BASE_PT),
        );
        let max2 = need(long.repeat(2).trim_end(), false, BASE_PT);
        let k = (line - 2.0 * min) / (max + max2 - 2.0 * min);
        assert_close(&got, &[min + (max - min) * k, min + (max2 - min) * k]);
        // Words too wide to fit: the words' width, past the slide.
        let word = "W".repeat(40);
        let t = TableBuilder::new(2)
            .columns(two_columns())
            .body(vec![row(vec![cell(&word), cell(&word)])])
            .build();
        assert_close(&widths(&t), &[need(&word, false, BASE_PT); 2]);
    }

    #[test]
    fn min_and_max_width_bound_the_content() {
        let long = "a line of text much wider than its maximum";
        let t = TableBuilder::new(3)
            .columns(vec![
                column("a", "A").max_width("60pt"),
                column("b", "B").min_width("200pt"),
                column("c", "C"),
            ])
            .head(row(vec![cell("A"), cell("B"), cell("C")]).role("column_label"))
            .head(
                row(vec![cell(long).colspan(3), placeholder(), placeholder()])
                    .role("column_spanner"),
            )
            .body(vec![row(vec![cell(long), cell("x"), cell("y")])])
            .build();
        let got = widths(&t);
        // A capped column doesn't grow for a spanner either: the rest do.
        let c = need("C", true, BASE_PT);
        let extra = (need(long, true, BASE_PT) - 60.0 - 200.0 - c) / 2.0;
        assert!(extra > 0.0);
        assert_close(&got, &[60.0, 200.0 + extra, c + extra]);
    }

    #[test]
    fn fixed_percent_and_fr_columns_keep_their_widths() {
        let t = TableBuilder::new(4)
            .columns(vec![
                column("a", "A").width("1in"),
                column("b", "B").width("25%"),
                column("c", "C"),
                column("d", "D").width("1fr"),
            ])
            .body(vec![row(vec![
                cell("x"),
                cell("y"),
                cell("Content"),
                cell("z"),
            ])])
            .build();
        let line = f64::from(TEXT_W) / EMU_PER_PT;
        let c = need("Content", false, BASE_PT);
        assert_close(
            &widths(&t),
            &[72.0, line / 4.0, c, line - 72.0 - line / 4.0 - c],
        );
        // A table with a width spreads it over the columns without one.
        let t = TableBuilder::new(2)
            .columns(two_columns())
            .table_width("300pt")
            .body(vec![row(vec![cell("x"), cell("much wider content")])])
            .build();
        assert_close(&widths(&t), &[150.0, 150.0]);
    }

    /// In every example sized for its content, each cell has its room: a line
    /// of its own when the table fits the slide, else its widest word.
    #[test]
    fn every_cell_in_the_corpus_has_its_room() {
        let line = f64::from(TEXT_W) / EMU_PER_PT;
        for ex in gridwell_testkit::examples() {
            let t = ex.table();
            let sized = t.config.table_width.is_none()
                && t.column_spec
                    .iter()
                    .all(|c| c.width == "auto" && c.min_width.is_none() && c.max_width.is_none());
            if !sized {
                continue;
            }
            let rt = resolve(&t);
            let got = widths(&t);
            let fits = got.iter().sum::<f64>() < line;
            let check = |first: usize, span: usize, need: ContentWidth, what: &str| {
                let have: f64 = got[first..first + span].iter().sum();
                let want = if fits { need.max } else { need.min };
                assert!(
                    have >= want - 1e-3,
                    "{}: {what} wants {want}, has {have}",
                    ex.name
                );
            };
            for (section, strong) in std::iter::once((&rt.head, true)).chain(
                rt.groups
                    .iter()
                    .flat_map(|g| [(&g.rows, false), (&g.summary_rows, true)]),
            ) {
                for slot in section.rows.iter().flat_map(|r| &r.slots) {
                    if let Slot::Origin(cell) = slot {
                        let need = cell_room(&rt, cell, strong);
                        check(cell.col, cell.colspan, need, &format!("{:?}", cell.content));
                    }
                }
            }
            for label in rt.groups.iter().filter_map(|g| g.label.as_ref()) {
                let mut m = Measure::default();
                let _ = for_each_run(&rt, &label.content, label_fmt(label), &mut |r| m.add(r));
                check(0, got.len(), m.room(), "a group label");
            }
        }
    }
}
