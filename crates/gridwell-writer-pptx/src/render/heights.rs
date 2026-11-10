//! Row and text box heights. A viewer grows a row to fit its text, but never
//! draws it shorter than its `h`, so a row asked for at least as tall as its
//! text is drawn as tall as asked. The rows' heights are estimated here (lines
//! wrapped at their column's width, at the height of their tallest run), so the
//! table's frame is as tall as the table and the notes below it start where it
//! ends.

use std::convert::Infallible;

use gridwell_layout::measure::clusters;
use gridwell_layout::{ResolvedCell, ResolvedTable, Section, SectionKind, Slot};

use super::widths::{breaks, run_size, EMU_PER_PT, INSET_PT};
use super::{cell_fmt, for_each_run, label_fmt, pad_fmt, Para, Run, BASE_PT};

/// A line's height, in em of its tallest run: Calibri's (with its line gap,
/// as PowerPoint spaces single lines), taller than the DejaVu Sans LibreOffice
/// draws in its place (1.164).
const LINE_HEIGHT: f64 = 1.22;

/// A cell's and a text box's inset above and below its text: DrawingML's
/// default (`marT`, `marB`, `tIns`, `bIns`: 45720 EMU, 0.05in).
const INSET_V_PT: f64 = 3.6;

/// Each row's height in EMU, in the order the rows are written: the head's,
/// then each group's label row, rows and summary rows. A cell spanning rows
/// that its rows don't have room for asks them for the rest in equal shares.
pub(super) fn row_heights(rt: &ResolvedTable, col_widths: &[u32]) -> Vec<u32> {
    let cols: Vec<f64> = col_widths
        .iter()
        .map(|&w| f64::from(w) / EMU_PER_PT)
        .collect();
    let mut rows = Vec::new();
    section_heights(rt, &rt.head, true, &cols, &mut rows);
    for g in &rt.groups {
        if let Some(label) = &g.label {
            let mut w = Wrap::new(cols.iter().sum::<f64>() - 2.0 * INSET_PT);
            let _ = for_each_run(rt, &label.content, label_fmt(label), &mut |r| w.add(r));
            rows.push(w.height() + 2.0 * INSET_V_PT);
        }
        for s in g.sections() {
            section_heights(rt, s, s.kind != SectionKind::Body, &cols, &mut rows);
        }
    }
    rows.into_iter().map(emu).collect()
}

/// A text box's height in EMU for `paras` at `width_emu`: its lines and insets
/// (none for no paragraphs).
pub(super) fn box_height(rt: &ResolvedTable, paras: &[Para], width_emu: u32) -> u32 {
    if paras.is_empty() {
        return 0;
    }
    let width = f64::from(width_emu) / EMU_PER_PT - 2.0 * INSET_PT;
    let lines: f64 = paras
        .iter()
        .map(|p| {
            let mut w = Wrap::new(width);
            let _ = p.for_each_run(rt, &mut |r| w.add(r));
            w.height()
        })
        .sum();
    emu(lines + 2.0 * INSET_V_PT)
}

/// Points to EMU, at least one, and within what a frame's height holds.
fn emu(pt: f64) -> u32 {
    (pt * EMU_PER_PT).round().clamp(1.0, f64::from(i32::MAX)) as u32
}

/// Appends a section's row heights, in points, to `rows`.
fn section_heights(
    rt: &ResolvedTable,
    section: &Section,
    strong: bool,
    cols: &[f64],
    rows: &mut Vec<f64>,
) {
    let start = rows.len();
    let width = |first: usize, span: usize| -> f64 {
        let n = cols.len();
        cols[first.min(n)..first.saturating_add(span).min(n)]
            .iter()
            .sum()
    };
    // (first row, rows spanned, height)
    let mut spans: Vec<(usize, usize, f64)> = Vec::new();
    for (r, row) in section.rows.iter().enumerate() {
        let mut h = 0.0_f64;
        let mut covered = false;
        for slot in &row.slots {
            let Slot::Origin(cell) = slot else {
                covered = true;
                continue;
            };
            let need = cell_height(rt, cell, strong, width(cell.col, cell.colspan));
            if cell.rowspan > 1 {
                spans.push((start + r, cell.rowspan, need));
            } else {
                h = h.max(need);
            }
        }
        // A covered position holds an empty paragraph at the table's size, as
        // tall as a line; a row of spans' first rows only is one too.
        if covered || h == 0.0 {
            h = h.max(BASE_PT * LINE_HEIGHT + 2.0 * INSET_V_PT);
        }
        rows.push(h);
    }
    // Narrow spans first, so wider ones see what they asked for.
    spans.sort_by_key(|&(_, span, _)| span);
    for (first, span, need) in spans {
        let spanned = &mut rows[first..first.saturating_add(span).min(start + section.rows.len())];
        let share = (need - spanned.iter().sum::<f64>()).max(0.0) / spanned.len() as f64;
        for h in spanned {
            *h += share;
        }
    }
}

/// A cell's height, in points: its content and alignment padding, as written,
/// wrapped at `width` less its insets.
fn cell_height(rt: &ResolvedTable, cell: &ResolvedCell, strong: bool, width: f64) -> f64 {
    let fmt = cell_fmt(cell, strong);
    let mut w = Wrap::new(width - 2.0 * INSET_PT);
    let _ = for_each_run(rt, &cell.content, fmt, &mut |r| w.add(r));
    if let Some(pad) = cell.align_pad {
        let _ = w.add(Some((&pad.text(), pad_fmt(fmt))));
    }
    w.height() + 2.0 * INSET_V_PT
}

/// Runs wrapped as a viewer wraps them: at spaces and after a hyphen, slash,
/// `?` or `!` ([`breaks_after`]), a word that doesn't fit on a line going to
/// the next, one too wide for any line breaking where it overflows. Widths are
/// estimates ([`clusters`]), wider than the text drawn, so lines break no later
/// than they do.
#[derive(Debug)]
struct Wrap {
    /// The room for text on a line.
    width: f64,
    /// The width of the words on the line so far.
    line: f64,
    /// The size of its tallest run (0: none).
    line_size: f64,
    /// The spaces after its last word: their width and largest size.
    space: (f64, f64),
    /// The word being read: its grapheme clusters' widths and sizes.
    word: Vec<(f64, f64)>,
    /// Whether the word ends with a character a line may break after.
    soft: bool,
    /// The sizes of the lines done.
    lines: Vec<f64>,
}

impl Wrap {
    fn new(width: f64) -> Self {
        Self {
            width: width.max(0.0),
            line: 0.0,
            line_size: 0.0,
            space: (0.0, 0.0),
            word: Vec::new(),
            soft: false,
            lines: Vec::new(),
        }
    }

    fn add(&mut self, run: Run) -> Result<(), Infallible> {
        let Some((text, fmt)) = run else {
            self.end_word();
            self.end_line();
            return Ok(());
        };
        let (size, pt) = run_size(&fmt);
        for (c, em) in clusters(text) {
            let w = em * pt;
            if breaks(c) {
                self.end_word();
                if self.line > 0.0 {
                    self.space = (self.space.0 + w, self.space.1.max(size));
                }
            } else {
                if self.soft && !c.is_ascii_digit() && !breaks_after(c) {
                    self.end_word();
                }
                self.word.push((w, size));
                self.soft = breaks_after(c);
            }
        }
        Ok(())
    }

    /// The word read so far onto the line, or the next.
    fn end_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let word: f64 = self.word.iter().map(|c| c.0).sum();
        let fits = |line: f64, w: f64, width: f64| line + w <= width + 1e-6;
        if self.line > 0.0 {
            if fits(self.line + self.space.0, word, self.width) {
                self.line += self.space.0;
                self.line_size = self.line_size.max(self.space.1);
            } else {
                self.end_line();
            }
        }
        self.space = (0.0, 0.0);
        for (w, size) in std::mem::take(&mut self.word) {
            // On a line of its own (it didn't fit after the last), a word
            // wider than the line breaks where it overflows.
            if self.line > 0.0 && !fits(self.line, w, self.width) {
                self.end_line();
            }
            self.line += w;
            self.line_size = self.line_size.max(size);
        }
    }

    fn end_line(&mut self) {
        // An empty line is as tall as the table's text.
        let size = if self.line_size > 0.0 {
            self.line_size
        } else {
            BASE_PT
        };
        self.lines.push(size);
        self.line = 0.0;
        self.line_size = 0.0;
        self.space = (0.0, 0.0);
    }

    /// The lines' height, in points.
    fn height(mut self) -> f64 {
        self.end_word();
        self.end_line();
        self.lines.iter().map(|s| s * LINE_HEIGHT).sum()
    }
}

/// A line may break after `c` (Unicode line breaking's hyphens, `/`, `?` and
/// `!`), but not before a digit (`1/2`, `-5`) or another of these (`://`).
fn breaks_after(c: char) -> bool {
    matches!(
        c,
        '-' | '/' | '?' | '!' | '\u{2010}' | '\u{2013}' | '\u{2014}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::widths::column_widths;
    use crate::render::Fmt;
    use gridwell_ir::{ContentNode, StyleDef, Table};
    use gridwell_layout::measure::{char_em, text_em};
    use gridwell_layout::resolve;
    use gridwell_testkit::*;

    /// A line of `size` points.
    fn line(size: f64) -> f64 {
        size * LINE_HEIGHT
    }

    /// A cell or box of `lines` (their heights).
    fn with_insets(lines: f64) -> f64 {
        lines + 2.0 * INSET_V_PT
    }

    /// Each row's height in points.
    fn heights(t: &Table) -> Vec<f64> {
        let rt = resolve(t);
        row_heights(&rt, &column_widths(&rt))
            .into_iter()
            .map(|h| f64::from(h) / EMU_PER_PT)
            .collect()
    }

    /// Within the rounding to EMU.
    fn assert_close(got: &[f64], want: &[f64]) {
        assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() <= 0.5 / EMU_PER_PT, "{got:?} vs {want:?}");
        }
    }

    fn one_column(width: &str, cells: &[&str]) -> Table {
        TableBuilder::new(1)
            .columns(vec![column("a", "A").width(width)])
            .body(cells.iter().map(|c| row(vec![cell(c)])).collect())
            .build()
    }

    #[test]
    fn a_line_of_text_is_a_row_of_29_points() {
        let t = TableBuilder::new(2)
            .columns(vec![column("a", "A"), column("b", "B")])
            .head(row(vec![cell("A"), cell("B")]).role("column_label"))
            .body(vec![
                row(vec![cell("x"), cell("y")]),
                row(vec![cell(""), cell("")]),
            ])
            .build();
        let one = with_insets(line(BASE_PT));
        assert!((one - 29.16).abs() < 1e-9);
        // The head is bold: as tall. An empty cell is a line.
        assert_close(&heights(&t), &[one, one, one]);
    }

    #[test]
    fn each_line_break_adds_a_line() {
        let t = TableBuilder::new(1)
            .columns(vec![column("a", "A")])
            .body(vec![
                row(vec![cell_content(vec![
                    text("one"),
                    line_break(),
                    text("two"),
                ])]),
                row(vec![cell_content(vec![text("one"), line_break()])]),
            ])
            .build();
        let two = with_insets(2.0 * line(BASE_PT));
        assert_close(&heights(&t), &[two, two]);
    }

    #[test]
    fn text_wraps_at_its_columns_width() {
        // 45.6pt for text: "aaa" is 34.56pt, "aaa bbb" 74.88pt.
        let t = one_column("60pt", &["aaa bbb ccc", "aaa", "aaa   "]);
        assert!(text_em("aaa", false) * BASE_PT <= 45.6);
        assert!(text_em("aaa b", false) * BASE_PT > 45.6);
        let lines = |n: f64| with_insets(n * line(BASE_PT));
        // Spaces ending a line hang past it.
        assert_close(&heights(&t), &[lines(3.0), lines(1.0), lines(1.0)]);
        // As many words to a line as fit.
        let t = one_column("100pt", &["aa aa aa aa aa"]);
        let (word, space) = (text_em("aa", false) * BASE_PT, char_em(' ') * BASE_PT);
        let per_line = ((85.6 + space) / (word + space)).floor();
        assert_eq!(per_line, 3.0);
        assert_close(&heights(&t), &[lines(2.0)]);
    }

    #[test]
    fn an_emoji_sequence_wraps_as_one_emoji() {
        // 2 emoji (22.5pt each) to 45.6pt: four families joined with ZWJ are two
        // lines, not the eight their sixteen people would be.
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let t = one_column("60pt", &[&family.repeat(4)]);
        assert_close(&heights(&t), &[with_insets(2.0 * line(BASE_PT))]);
    }

    #[test]
    fn a_word_too_wide_for_any_line_breaks_where_it_overflows() {
        // 2 of the 18pt `W`s to 45.6pt.
        let t = one_column("60pt", &["WWWWWWWWWW", "a WWWWW"]);
        let lines = |n: f64| with_insets(n * line(BASE_PT));
        // `a`, then the word from a line of its own.
        assert_close(&heights(&t), &[lines(5.0), lines(4.0)]);
        // A line breaks after a slash or hyphen too, where a word too wide would
        // fill its line: "aa/a", "a/aa"; but not before a digit, or another.
        let t = one_column(
            "60pt",
            &["aa/aa/aa", "aa-aa-aa", "aa/11/11", "aa-11-11", "a/W//W"],
        );
        assert_close(
            &heights(&t),
            &[lines(3.0), lines(3.0), lines(2.0), lines(2.0), lines(3.0)],
        );
        // A no-break space joins words: into one too wide, here.
        let t = one_column("60pt", &["aaa aaa", "aaa\u{A0}aaa"]);
        assert_close(&heights(&t), &[lines(2.0), lines(3.0)]);
    }

    #[test]
    fn a_line_is_as_tall_as_its_tallest_run() {
        let big = StyleDef {
            font_size: Some("36pt".into()),
            ..StyleDef::default()
        };
        let t = TableBuilder::new(2)
            .columns(vec![column("a", "A"), column("b", "B")])
            .style_def("big", big)
            .body(vec![
                row(vec![
                    cell_content(vec![text("a"), styled("B", "big")]),
                    cell("c"),
                ]),
                row(vec![
                    cell_content(vec![styled("B", "big"), line_break(), text("a")]),
                    cell("c"),
                ]),
                row(vec![cell("c").style("big"), cell("c")]),
            ])
            .build();
        assert_close(
            &heights(&t),
            &[
                with_insets(line(36.0)),
                with_insets(line(36.0) + line(BASE_PT)),
                with_insets(line(36.0)),
            ],
        );
    }

    #[test]
    fn a_cell_spanning_rows_shares_what_they_lack() {
        let tall = cell_content(vec![
            text("1"),
            line_break(),
            text("2"),
            line_break(),
            text("3"),
            line_break(),
            text("4"),
        ]);
        let t = TableBuilder::new(2)
            .columns(vec![column("a", "A"), column("b", "B")])
            .body(vec![
                row(vec![tall.rowspan(2), cell("x")]),
                row(vec![placeholder(), cell("y")]),
                row(vec![cell("short").rowspan(2), cell("z")]),
                row(vec![placeholder(), cell("w")]),
            ])
            .build();
        let one = with_insets(line(BASE_PT));
        let share = (with_insets(4.0 * line(BASE_PT)) - 2.0 * one) / 2.0;
        // A span its rows have room for asks nothing more.
        assert_close(&heights(&t), &[one + share, one + share, one, one]);
        // A row of spans' first rows, and one of covered positions, is a line.
        let t = TableBuilder::new(2)
            .columns(vec![column("a", "A"), column("b", "B")])
            .body(vec![
                row(vec![cell("a").rowspan(2), cell("b").rowspan(2)]),
                row(vec![placeholder(), placeholder()]),
            ])
            .build();
        assert_close(&heights(&t), &[one, one]);
        // A row of small text is shorter than that, unless a position in it is
        // covered.
        let small = StyleDef {
            font_size: Some("8pt".into()),
            ..StyleDef::default()
        };
        let t = TableBuilder::new(2)
            .columns(vec![column("a", "A"), column("b", "B")])
            .style_def("small", small)
            .body(vec![
                row(vec![cell("s").style("small"), cell("s").style("small")]),
                row(vec![
                    cell("s").style("small").rowspan(2),
                    cell("s").style("small"),
                ]),
                row(vec![placeholder(), cell("s").style("small")]),
            ])
            .build();
        let small = with_insets(line(8.0));
        assert!(small < one);
        assert_close(&heights(&t), &[small, small, one]);
    }

    #[test]
    fn the_head_wraps_as_bold() {
        // "aaar" fits 45.6pt (42.84pt), but not bold (47.98pt).
        assert!(text_em("aaar", false) * BASE_PT <= 45.6);
        assert!(text_em("aaar", true) * BASE_PT > 45.6);
        let t = TableBuilder::new(1)
            .columns(vec![column("a", "A").width("60pt")])
            .head(row(vec![cell("aaar")]).role("column_label"))
            .body(vec![row(vec![cell("aaar")])])
            .build();
        let lines = |n: f64| with_insets(n * line(BASE_PT));
        assert_close(&heights(&t), &[lines(2.0), lines(1.0)]);
    }

    #[test]
    fn a_group_label_wraps_across_the_table() {
        // 120pt less the insets: 105.6pt for the bold label.
        let label = "aaaa aaaa aaaa";
        let t = TableBuilder::new(2)
            .columns(vec![
                column("a", "A").width("60pt"),
                column("b", "B").width("60pt"),
            ])
            .group(labeled_group(label, vec![row(vec![cell("x"), cell("y")])]))
            .build();
        assert!(text_em("aaaa aaaa", true) * BASE_PT > 105.6);
        assert!(text_em("aaaa", true) * BASE_PT <= 105.6);
        let one = with_insets(line(BASE_PT));
        assert_close(&heights(&t), &[with_insets(3.0 * line(BASE_PT)), one]);
    }

    #[test]
    fn a_text_box_is_as_tall_as_its_lines() {
        let t = TableBuilder::new(1)
            .columns(vec![column("a", "A")])
            .body(vec![row(vec![cell("x")])])
            .build();
        let rt = resolve(&t);
        let empty: [ContentNode; 0] = [];
        let words = [text("aa aa aa aa aa")];
        let para = |content, size| Para {
            mark: None,
            content,
            fmt: Fmt {
                size: Some(size),
                ..Fmt::default()
            },
        };
        assert_eq!(box_height(&rt, &[], 1_000_000), 0);
        let h = |paras: &[Para], width_pt: f64| {
            f64::from(box_height(&rt, paras, (width_pt * EMU_PER_PT) as u32)) / EMU_PER_PT
        };
        // An empty paragraph is a line.
        let paras = [para(&words[..], 2400), para(&empty[..], 1400)];
        assert_close(&[h(&paras, 600.0)], &[with_insets(line(24.0) + line(18.0))]);
        // Wrapped at the box's width less its insets: 3 words to 107.6pt.
        let (word, space) = (text_em("aa", false) * 24.0, char_em(' ') * 24.0);
        assert!(3.0 * word + 2.0 * space <= 107.6 && 4.0 * word + 3.0 * space > 107.6);
        assert_close(&[h(&paras[..1], 122.0)], &[with_insets(2.0 * line(24.0))]);
        // Four words fit 150pt, but not 135.6pt.
        let four = [text("aa aa aa aa")];
        assert!(4.0 * word + 3.0 * space <= 150.0 && 4.0 * word + 3.0 * space > 135.6);
        assert_close(
            &[h(&[para(&four[..], 2400)], 150.0)],
            &[with_insets(2.0 * line(24.0))],
        );
        // A footnote's mark and its space count.
        let mark = Para {
            mark: Some("WWWWWWWWWWWWWW"),
            ..para(&words[..], 1000)
        };
        assert!(h(&[mark], 114.4) > h(&[para(&words[..], 1000)], 114.4));
    }

    /// In every example, each row is at least as tall as each cell in it, and
    /// the rows a cell spans together.
    #[test]
    fn every_cell_in_the_corpus_has_its_height() {
        for ex in gridwell_testkit::examples() {
            let t = ex.table();
            let rt = resolve(&t);
            if rt.is_empty() {
                continue;
            }
            let cols: Vec<f64> = column_widths(&rt)
                .into_iter()
                .map(|w| f64::from(w) / EMU_PER_PT)
                .collect();
            let rows: Vec<f64> = row_heights(&rt, &column_widths(&rt))
                .into_iter()
                .map(|h| f64::from(h) / EMU_PER_PT)
                .collect();
            let mut at = 0;
            let check = |section: &Section, strong: bool, at: &mut usize| {
                for (r, row) in section.rows.iter().enumerate() {
                    for slot in &row.slots {
                        let Slot::Origin(cell) = slot else { continue };
                        let width: f64 = cols[cell.col..cell.col + cell.colspan].iter().sum();
                        let need = cell_height(&rt, cell, strong, width);
                        let span = cell.rowspan.min(section.rows.len() - r);
                        let have: f64 = rows[*at + r..*at + r + span].iter().sum();
                        assert!(
                            have >= need - 1e-3,
                            "{}: {:?} wants {need}, has {have}",
                            ex.name,
                            cell.content
                        );
                    }
                }
                *at += section.rows.len();
            };
            check(&rt.head, true, &mut at);
            for g in &rt.groups {
                if g.label.is_some() {
                    assert!(rows[at] >= with_insets(line(BASE_PT)) - 1e-3);
                    at += 1;
                }
                for s in g.sections() {
                    check(s, s.kind != SectionKind::Body, &mut at);
                }
            }
            assert_eq!(at, rows.len(), "{}", ex.name);
        }
    }
}
