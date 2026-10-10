use gridwell_core::{Color, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table};
use gridwell_layout::{resolve, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable, Slot};
use std::fmt::Write;
use thiserror::Error;

use crate::LatexWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

struct LatexRenderer<'r, 'a> {
    rt: &'r ResolvedTable<'a>,
    config: &'r LatexWriterConfig,
    buf: String,
}

impl<'r, 'a> LatexRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>, config: &'r LatexWriterConfig) -> Self {
        Self {
            rt,
            config,
            buf: String::with_capacity(4096),
        }
    }

    fn use_longtable(&self) -> bool {
        if self.config.longtable {
            return true;
        }
        let body_rows: usize = self.rt.groups.iter().map(|g| g.rows.rows.len()).sum();
        self.config
            .longtable_threshold
            .is_some_and(|t| body_rows > t as usize)
    }

    fn width(&self) -> usize {
        self.rt.columns.len()
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.render_title();
        // A tabular needs at least one column: with every column hidden only the
        // title and notes remain.
        if !self.rt.is_empty() {
            self.render_begin();
            self.render_toprule();
            self.render_thead();
            self.render_tbody();
            self.render_bottomrule();
            self.render_end();
        }
        self.render_footnotes();
        Ok(self.buf)
    }

    // ─── Title / Subtitle ───

    fn render_title(&mut self) {
        let header = &self.rt.header;
        // Lines outside the tabular take inline styling only (`\cellcolor` is
        // table-only).
        if let Some(title) = &header.title {
            let text = self.content(title.content, &HAlign::Left, &title.style);
            if !text.is_empty() {
                let text = apply_block_style(&text, &title.style);
                writeln!(self.buf, "{{\\large\\bfseries {text}}}\\\\").unwrap();
            }
        }
        let lines: Vec<_> = header.subtitle.iter().chain(&header.extra_lines).collect();
        for (i, line) in lines.iter().enumerate() {
            let text = self.content(line.content, &HAlign::Left, &line.style);
            if !text.is_empty() {
                let text = apply_block_style(&text, &line.style);
                let gap = if i + 1 == lines.len() { "[6pt]" } else { "" };
                writeln!(self.buf, "{{\\small {text}}}\\\\{gap}").unwrap();
            }
        }
    }

    // ─── Table Environment ───

    fn render_begin(&mut self) {
        let col_spec = self.build_column_spec();
        let env = if self.use_longtable() {
            "longtable"
        } else {
            "tabular"
        };
        writeln!(self.buf, "\\begin{{{env}}}{{{col_spec}}}").unwrap();
    }

    fn render_end(&mut self) {
        let env = if self.use_longtable() {
            "longtable"
        } else {
            "tabular"
        };
        writeln!(self.buf, "\\end{{{env}}}").unwrap();
    }

    /// `p{…}` for columns with a usable width, else the alignment letter.
    fn build_column_spec(&self) -> String {
        self.rt
            .columns
            .iter()
            .map(|col| match col.width.as_ref().and_then(latex_width) {
                Some(w) => format!("p{{{w}}}"),
                None => align_letter(&col.align).to_string(),
            })
            .collect()
    }

    // ─── Rules ───

    fn rule(&mut self, booktabs: &str) {
        let rule = if self.config.booktabs {
            booktabs
        } else {
            "\\hline"
        };
        writeln!(self.buf, "{rule}").unwrap();
    }

    fn render_toprule(&mut self) {
        self.rule("\\toprule");
    }

    fn render_midrule(&mut self) {
        self.rule("\\midrule");
    }

    fn render_bottomrule(&mut self) {
        self.rule("\\bottomrule");
    }

    // ─── Rows ───

    fn render_thead(&mut self) {
        let rt = self.rt;
        if rt.head.is_empty() {
            return;
        }
        for (r, _) in rt.head.rows.iter().enumerate() {
            self.render_row(&rt.head.rows, r);
        }
        self.render_midrule();
        if self.use_longtable() {
            writeln!(self.buf, "\\endhead").unwrap();
        }
    }

    fn render_tbody(&mut self) {
        let rt = self.rt;
        let cols = self.width();
        for (g, group) in rt.groups.iter().enumerate() {
            if let Some(label) = &group.label {
                let text = apply_style(
                    &self.content(label.content, &HAlign::Left, &label.style),
                    &label.style,
                );
                if self.config.booktabs {
                    writeln!(self.buf, "\\midrule").unwrap();
                }
                writeln!(
                    self.buf,
                    "\\multicolumn{{{cols}}}{{l}}{{\\bfseries {text}}} \\\\"
                )
                .unwrap();
                if self.config.booktabs {
                    writeln!(self.buf, "\\midrule").unwrap();
                }
            }

            for r in 0..group.rows.rows.len() {
                self.render_row(&group.rows.rows, r);
            }

            if !group.summary_rows.is_empty() {
                if self.config.booktabs {
                    writeln!(self.buf, "\\cmidrule(lr){{1-{cols}}}").unwrap();
                }
                for r in 0..group.summary_rows.rows.len() {
                    self.render_row(&group.summary_rows.rows, r);
                }
            }

            if g + 1 < rt.groups.len() && group.label.is_none() {
                writeln!(self.buf, "\\addlinespace").unwrap();
            }
        }
    }

    /// One `\\`-terminated row. Every visible column gets an entry: a position
    /// covered from above is an empty cell (as wide as the spanning cell, via
    /// `\multicolumn`, for a 2D span); positions covered from the left are absorbed
    /// by the origin's `\multicolumn`.
    fn render_row(&mut self, rows: &[ResolvedRow], r: usize) {
        let row = &rows[r];
        let mut cells: Vec<String> = Vec::new();
        for (col, slot) in row.slots.iter().enumerate() {
            match slot {
                Slot::Origin(cell) => cells.push(self.render_cell(cell)),
                Slot::CoveredV {
                    origin_row,
                    origin_col,
                    ..
                } if *origin_col == col => {
                    let span = rows[*origin_row].slots[col]
                        .origin()
                        .map_or(1, |c| c.colspan);
                    cells.push(if span > 1 {
                        format!("\\multicolumn{{{span}}}{{l}}{{}}")
                    } else {
                        String::new()
                    });
                }
                Slot::CoveredH { .. } | Slot::CoveredV { .. } => {}
                Slot::Empty => cells.push(String::new()),
            }
        }
        writeln!(self.buf, "{} \\\\", cells.join(" & ")).unwrap();
    }

    fn render_cell(&self, cell: &ResolvedCell) -> String {
        let column = &self.rt.columns[cell.col];
        let fixed_width = column.width.as_ref().and_then(latex_width).is_some();
        let mut out = self.content(cell.content, &cell.align, &cell.style);
        out = apply_style(&out, &cell.style);

        if cell.rowspan > 1 {
            out = format!("\\multirow{{{}}}{{*}}{{{out}}}", cell.rowspan);
        }
        // A cell's own alignment differs from its column's: a one-column
        // `\multicolumn` overrides it (not for fixed-width `p{}` columns, whose
        // width it would drop).
        let own_align = cell.colspan == 1
            && !fixed_width
            && align_letter(&cell.align) != align_letter(&column.align);
        if cell.colspan > 1 || own_align {
            out = format!(
                "\\multicolumn{{{}}}{{{}}}{{{out}}}",
                cell.colspan,
                align_letter(&cell.align)
            );
        }
        out
    }

    /// Content as LaTeX. Line breaks become a nested one-column tabular (the only
    /// form that works in `l`/`c`/`r` columns as well as `p{}`). `block`'s
    /// underline and strike-through are applied to each line.
    fn content(&self, nodes: &[ContentNode], align: &HAlign, block: &ResolvedStyle) -> String {
        let mut lines = vec![String::new()];
        for node in nodes {
            let out = lines.last_mut().unwrap();
            match node {
                ContentNode::Text { value } => out.push_str(&escape_latex(value)),
                ContentNode::StyledText { value, style_id } => {
                    let style = style_id
                        .as_deref()
                        .map(|id| self.rt.style(id))
                        .unwrap_or_default();
                    out.push_str(&apply_inline_style(&escape_latex(value), &style));
                }
                ContentNode::LineBreak {} => lines.push(String::new()),
                ContentNode::FootnoteMark { mark_text, .. } => {
                    write!(out, "\\textsuperscript{{{}}}", escape_latex(mark_text)).unwrap();
                }
                ContentNode::Image { alt, .. } => match alt {
                    Some(alt) => out.push_str(&escape_latex(alt)),
                    None => out.push_str("[image]"),
                },
                ContentNode::Raw { format, value } => {
                    if format == "latex" {
                        out.push_str(value);
                    }
                }
                ContentNode::Unknown => {}
            }
        }
        let lines: Vec<String> = lines
            .iter()
            .map(|l| {
                if l.is_empty() {
                    String::new()
                } else {
                    decorate(l, block)
                }
            })
            .collect();
        if lines.len() == 1 {
            return lines.into_iter().next().unwrap();
        }
        format!(
            "\\begin{{tabular}}[t]{{@{{}}{}@{{}}}}{}\\end{{tabular}}",
            align_letter(align),
            lines.join("\\\\")
        )
    }

    // ─── Footnotes ───

    fn render_footnotes(&mut self) {
        let footer = &self.rt.footer;
        if !footer.footnotes.is_empty() {
            writeln!(self.buf).unwrap();
            for note in &footer.footnotes {
                let text = apply_block_style(
                    &self.content(note.content, &HAlign::Left, &note.style),
                    &note.style,
                );
                writeln!(
                    self.buf,
                    "\\textsuperscript{{{mark}}} {text}\\\\",
                    mark = escape_latex(note.mark)
                )
                .unwrap();
            }
        }
        if !footer.source_notes.is_empty() {
            writeln!(self.buf).unwrap();
            for note in &footer.source_notes {
                let text = apply_block_style(
                    &self.content(note.content, &HAlign::Left, &note.style),
                    &note.style,
                );
                writeln!(self.buf, "{{\\footnotesize {text}}}\\\\").unwrap();
            }
        }
    }
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &LatexWriterConfig) -> Result<String, RenderError> {
    let rt = resolve(table);
    LatexRenderer::new(&rt, config).render()
}

fn align_letter(align: &HAlign) -> &'static str {
    match align {
        HAlign::Right => "r",
        HAlign::Center => "c",
        _ => "l",
    }
}

/// A column width usable in `p{…}`: absolute units and `em` keep their unit
/// (px → pt at 0.75pt/px), `%` is a fraction of `\linewidth`. `fr`, `auto` and
/// non-positive widths have no `p{}` form.
fn latex_width(w: &Length) -> Option<String> {
    fn num(v: f64) -> Option<String> {
        (v.is_finite() && v > 0.0).then(|| format!("{}", (v * 1e4).round() / 1e4))
    }
    Some(match *w {
        Length::Px(v) => format!("{}pt", num(v * 0.75)?),
        Length::Pt(v) => format!("{}pt", num(v)?),
        Length::Em(v) | Length::Rem(v) => format!("{}em", num(v)?),
        Length::In(v) => format!("{}in", num(v)?),
        Length::Cm(v) => format!("{}cm", num(v)?),
        Length::Mm(v) => format!("{}mm", num(v)?),
        Length::Percent(v) => format!("{}\\linewidth", num(v / 100.0)?),
        Length::Fr(_) | Length::Auto => return None,
    })
}

/// Bold, italic, colour, underline, strike-through and size for an inline run
/// (single-line text).
fn apply_inline_style(content: &str, style: &ResolvedStyle) -> String {
    styled(content, style, true)
}

/// A whole cell's, title's or note's inline formatting, without underline and
/// strike-through: those are applied per line by `content`, since `ulem` can't
/// wrap the nested tabular that holds several lines.
fn apply_block_style(content: &str, style: &ResolvedStyle) -> String {
    styled(content, style, false)
}

/// Underline and strike-through (`ulem`, `\usepackage[normalem]{ulem}`: unlike
/// `\underline`, its lines break inside `p{}` columns).
fn decorate(content: &str, style: &ResolvedStyle) -> String {
    let mut result = content.to_string();
    if style.is_underline() {
        result = format!("\\uline{{{result}}}");
    }
    if style.is_strike() {
        result = format!("\\sout{{{result}}}");
    }
    result
}

fn styled(content: &str, style: &ResolvedStyle, decorated: bool) -> String {
    let mut result = content.to_string();
    if style.is_bold() {
        result = format!("\\textbf{{{result}}}");
    }
    if style.is_italic() {
        result = format!("\\textit{{{result}}}");
    }
    if let Some(c) = style.paint() {
        result = format!("\\textcolor{}{{{result}}}", latex_color(c));
    }
    if decorated {
        result = decorate(&result, style);
    }
    // Relative sizes resolve against the document's 10pt; leading is 1.2×.
    if let Some(pt) = style.size_pt(10.0) {
        let num = |v: f64| format!("{}", (v * 100.0).round() / 100.0);
        result = format!(
            "{{\\fontsize{{{}}}{{{}}}\\selectfont {result}}}",
            num(pt),
            num(pt * 1.2)
        );
    }
    result
}

/// A cell's style: inline formatting, monospace families, and the cell
/// background (`\cellcolor` must come first in the cell).
fn apply_style(content: &str, style: &ResolvedStyle) -> String {
    let mut result = apply_block_style(content, style);
    if style
        .font_family
        .as_deref()
        .is_some_and(|f| f.contains("monospace") || f.contains("Courier"))
    {
        result = format!("\\texttt{{{result}}}");
    }
    if let Some(bg) = style.fill() {
        result = format!("\\cellcolor{}{result}", latex_color(bg));
    }
    result
}

/// An xcolor `[HTML]{RRGGBB}` spec; LaTeX colours have no alpha, so translucent
/// colours are flattened onto white.
fn latex_color(c: Color) -> String {
    format!("[HTML]{{{}}}", c.flatten().to_rrggbb())
}

// ─── LaTeX Escaping ───

/// Escape text for LaTeX: the ten special characters, `<`, `>` and `|` (which
/// print as other glyphs in the default OT1 encoding), newlines as spaces (a blank
/// line would end the paragraph mid-table), other control characters dropped.
fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '\\' => out.push_str("\\textbackslash{}"),
            '<' => out.push_str("\\textless{}"),
            '>' => out.push_str("\\textgreater{}"),
            '|' => out.push_str("\\textbar{}"),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_convert_to_latex_units() {
        let w = |s: &str| latex_width(&s.parse().unwrap());
        assert_eq!(w("120px").as_deref(), Some("90pt"));
        assert_eq!(w("25%").as_deref(), Some("0.25\\linewidth"));
        assert_eq!(w("3cm").as_deref(), Some("3cm"));
        assert_eq!(w("1.5em").as_deref(), Some("1.5em"));
        assert_eq!(w("1fr"), None);
        assert_eq!(w("auto"), None);
        assert_eq!(w("0"), None);
    }

    #[test]
    fn escaping_covers_specials_and_controls() {
        assert_eq!(
            escape_latex("a&b%c$d#e_f{g}h"),
            "a\\&b\\%c\\$d\\#e\\_f\\{g\\}h"
        );
        assert_eq!(
            escape_latex("<|>"),
            "\\textless{}\\textbar{}\\textgreater{}"
        );
        assert_eq!(escape_latex("x\n\ny\u{7}z"), "x  yz");
    }
}
