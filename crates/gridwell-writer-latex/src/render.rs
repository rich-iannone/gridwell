use gridwell_core::{Color, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table};
use gridwell_layout::{
    resolve, ColumnWidth, ResolvedCell, ResolvedStyle, ResolvedTable, Section, Sides, Slot, Stroke,
};
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
    /// How each column's width is written.
    widths: Vec<ColWidth>,
    buf: String,
    /// A `\midrule` is due before the next row.
    midrule_due: bool,
    /// The last line drawn was a full-width rule (nothing since but `\endhead`).
    ruled: bool,
    /// The bottom edges of the line just written, one per column, not drawn yet:
    /// they collapse with the next line's top edges if nothing comes between.
    pending: Option<Vec<Option<Stroke>>>,
}

impl<'r, 'a> LatexRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>, config: &'r LatexWriterConfig) -> Self {
        let mut r = Self {
            rt,
            config,
            widths: Vec::new(),
            buf: String::with_capacity(4096),
            midrule_due: false,
            ruled: false,
            pending: None,
        };
        r.widths = r.column_widths();
        r
    }

    /// The width `%` and `fr` columns divide: the table's `table_width`, else
    /// the line.
    fn table_width(&self) -> String {
        let width = self.rt.table.config.table_width.as_deref();
        match width.and_then(|w| w.parse::<Length>().ok()) {
            Some(Length::Percent(p)) if p > 0.0 && p != 100.0 => {
                format!("{}\\linewidth", num(p / 100.0))
            }
            Some(w) => absolute_width(&w).unwrap_or_else(|| "\\linewidth".into()),
            None => "\\linewidth".into(),
        }
    }

    /// Each column's width. A `%` column is that share of the table's width less
    /// every column's padding (so shares adding up to 100% fill the table); `fr`
    /// columns share what the other columns leave, in proportion. When the table
    /// has a width and no column is `fr`, the columns without a width are `1fr`
    /// (see [`ResolvedTable::column_widths`]). A tabular leaves that to
    /// `tabularx`, which measures the natural-width columns; a longtable can't use
    /// it, so there each `fr` column's width is worked out from the fixed and `%`
    /// columns alone, and natural-width columns add to it.
    fn column_widths(&self) -> Vec<ColWidth> {
        let n = self.width();
        let content = format!(
            "\\dimexpr {}-{}\\tabcolsep\\relax",
            self.table_width(),
            2 * n
        );
        let mut widths: Vec<ColWidth> = self
            .rt
            .column_widths()
            .into_iter()
            .map(|w| match w {
                ColumnWidth::Percent(p) => ColWidth::Fixed(format!("{}{content}", num(p / 100.0))),
                ColumnWidth::Fr(f) => ColWidth::Fr(f),
                ColumnWidth::Fixed(w) => {
                    absolute_width(&w).map_or(ColWidth::Natural, ColWidth::Fixed)
                }
                ColumnWidth::Auto => ColWidth::Natural,
            })
            .collect();
        if !self.use_longtable() {
            return widths;
        }
        let total_fr: f64 = widths
            .iter()
            .map(|w| if let ColWidth::Fr(f) = w { *f } else { 0.0 })
            .sum();
        if total_fr == 0.0 {
            return widths;
        }
        let percent: f64 = self
            .rt
            .columns
            .iter()
            .filter_map(|c| match c.width {
                Some(Length::Percent(p)) if p.is_finite() && p > 0.0 => Some(p / 100.0),
                _ => None,
            })
            .sum();
        let mut rest = format!("{}{content}", num((1.0 - percent).max(0.0)));
        for (c, w) in self.rt.columns.iter().zip(&widths) {
            if let (ColWidth::Fixed(_), Some(abs)) = (w, c.width.as_ref().and_then(absolute_width))
            {
                write!(rest, "-{abs}").unwrap();
            }
        }
        for w in &mut widths {
            if let ColWidth::Fr(f) = *w {
                *w = ColWidth::Fixed(format!("{}\\dimexpr {rest}\\relax", num(f / total_fr)));
            }
        }
        widths
    }

    /// `tabularx` when a column is `fr` (a longtable's `fr` columns are fixed).
    fn env(&self) -> &'static str {
        if self.use_longtable() {
            "longtable"
        } else if self.widths.iter().any(|w| matches!(w, ColWidth::Fr(_))) {
            "tabularx"
        } else {
            "tabular"
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
        // A tabular needs at least one column: with every column hidden only the
        // title and notes remain.
        if self.rt.is_empty() {
            self.render_title();
            self.render_footnotes();
        } else if self.use_longtable() {
            self.render_longtable();
        } else {
            self.render_title();
            self.render_begin();
            self.render_toprule();
            self.render_thead();
            self.render_tbody();
            self.render_bottomrule();
            self.render_end();
            self.render_footnotes();
        }
        Ok(self.buf)
    }

    /// A longtable carries its title and notes as rows of its own, so a page
    /// break can't part them from the table: the title (with the head) is the
    /// first page's head, the notes (with the bottom rule) the last page's foot.
    /// Every other page repeats the head and ends with the bottom rule.
    fn render_longtable(&mut self) {
        self.render_begin();
        let heading = self.heading_lines();
        for (text, gap) in &heading {
            writeln!(self.buf, "{} \\\\{gap}", self.wide_row(text, 'b')).unwrap();
        }
        let start = self.buf.len();
        self.render_toprule();
        self.render_thead();
        self.flush_midrule();
        self.settle();
        let head = self.buf[start..].to_string();
        if !heading.is_empty() {
            writeln!(self.buf, "\\endfirsthead").unwrap();
            self.buf.push_str(&head);
        }
        writeln!(self.buf, "\\endhead").unwrap();
        let rule = if self.config.booktabs {
            "\\bottomrule"
        } else {
            "\\hline"
        };
        writeln!(self.buf, "{rule}\n\\endfoot").unwrap();
        let notes: Vec<String> = self.note_lines().into_iter().flatten().collect();
        if !notes.is_empty() {
            writeln!(self.buf, "{rule}").unwrap();
            for text in &notes {
                writeln!(self.buf, "{} \\\\", self.wide_row(text, 't')).unwrap();
            }
            writeln!(self.buf, "\\endlastfoot").unwrap();
        }
        self.ruled = true;
        self.render_tbody();
        self.midrule_due = false;
        self.settle();
        self.render_end();
    }

    /// A row of one line of text across the table, which neither widens it nor
    /// is boxed in its columns: a paragraph as wide as the line (as outside a
    /// table), from the table's left edge, in a box of no width. Its baseline is
    /// the paragraph's last line (`b`) or first (`t`).
    fn wide_row(&self, text: &str, pos: char) -> String {
        format!(
            "\\multicolumn{{{}}}{{@{{}}l@{{}}}}{{\\makebox[0pt][l]{{\\parbox[{pos}]{{\\linewidth}}{{\\raggedright {text}}}}}}}",
            self.width()
        )
    }

    // ─── Title / Subtitle ───

    fn render_title(&mut self) {
        for (text, gap) in self.heading_lines() {
            writeln!(self.buf, "{text}\\\\{gap}").unwrap();
        }
    }

    /// The title, subtitle and header lines, each with the space below it (the
    /// last line before the table is set apart). They take inline styling only
    /// (`\cellcolor` is table-only).
    fn heading_lines(&self) -> Vec<(String, &'static str)> {
        let header = &self.rt.header;
        let mut out = Vec::new();
        if let Some(title) = &header.title {
            let text = self.content(&title.content, &HAlign::Left, &title.style);
            if !text.is_empty() {
                let text = apply_block_style(&text, &title.style);
                out.push((format!("{{\\large\\bfseries {text}}}"), ""));
            }
        }
        let lines: Vec<_> = header.subtitle.iter().chain(&header.extra_lines).collect();
        for (i, line) in lines.iter().enumerate() {
            let text = self.content(&line.content, &HAlign::Left, &line.style);
            if !text.is_empty() {
                let text = apply_block_style(&text, &line.style);
                let gap = if i + 1 == lines.len() { "[6pt]" } else { "" };
                out.push((format!("{{\\small {text}}}"), gap));
            }
        }
        out
    }

    // ─── Table Environment ───

    fn render_begin(&mut self) {
        let col_spec = self.build_column_spec();
        let env = self.env();
        // A longtable starts at the left margin, as a tabular does.
        let width = match env {
            "tabularx" => format!("{{{}}}", self.table_width()),
            "longtable" => "[l]".to_string(),
            _ => String::new(),
        };
        writeln!(self.buf, "\\begin{{{env}}}{width}{{{col_spec}}}").unwrap();
    }

    fn render_end(&mut self) {
        writeln!(self.buf, "\\end{{{}}}", self.env()).unwrap();
    }

    fn build_column_spec(&self) -> String {
        (0..self.width()).map(|c| self.column_spec(c)).collect()
    }

    /// The column's spec with its own alignment.
    fn column_spec(&self, col: usize) -> String {
        self.aligned_spec(col, &self.rt.columns[col].align)
    }

    /// Column `col`'s spec with alignment `align`: a letter for a natural-width
    /// column; a paragraph column (`p{…}`, `fr`'s `X`) is aligned by its
    /// paragraphs (`\raggedright`…; `array`'s `>{…}`, loaded by `colortbl`).
    /// `X` columns divide their space by their `\hsize`, which must add up to
    /// their count.
    fn aligned_spec(&self, col: usize, align: &HAlign) -> String {
        let (setup, column) = match &self.widths[col] {
            ColWidth::Natural => return align_letter(align).to_string(),
            ColWidth::Fixed(w) => (String::new(), format!("p{{{w}}}")),
            ColWidth::Fr(f) => {
                let frs = self.widths.iter().filter_map(|w| match w {
                    ColWidth::Fr(f) => Some(*f),
                    _ => None,
                });
                let (count, total) = frs.fold((0.0, 0.0), |(n, t), f| (n + 1.0, t + f));
                let share = f * count / total;
                let setup = if (share - 1.0).abs() < 1e-9 {
                    String::new()
                } else {
                    format!("\\hsize={}\\hsize", num(share))
                };
                (setup, "X".to_string())
            }
        };
        let ragged = match align {
            HAlign::Left => "\\raggedright\\arraybackslash",
            HAlign::Center => "\\centering\\arraybackslash",
            HAlign::Right => "\\raggedleft\\arraybackslash",
            _ => "",
        };
        if ragged.is_empty() && setup.is_empty() {
            column
        } else {
            format!(">{{{ragged}{setup}}}{column}")
        }
    }

    // ─── Rules ───

    fn rule(&mut self, booktabs: &str) {
        self.settle();
        let rule = if self.config.booktabs {
            booktabs
        } else {
            "\\hline"
        };
        writeln!(self.buf, "{rule}").unwrap();
        self.ruled = true;
    }

    fn render_toprule(&mut self) {
        self.rule("\\toprule");
    }

    /// Ask for a `\midrule` before the next row. Requests in a row, or right
    /// after another full rule, draw one rule; none is drawn above the bottom rule.
    fn render_midrule(&mut self) {
        self.midrule_due = true;
    }

    /// Draw a requested `\midrule` now, unless a rule was just drawn.
    fn flush_midrule(&mut self) {
        if std::mem::take(&mut self.midrule_due) && !self.ruled {
            self.rule("\\midrule");
        }
    }

    /// Start a line of the tabular that isn't a rule.
    fn begin_row(&mut self) {
        self.settle();
        self.flush_midrule();
        self.ruled = false;
    }

    /// Start a line of cells whose top edges are `tops`. They are drawn together
    /// with the line above's bottom edges, unless a rule comes between.
    fn begin_line(&mut self, tops: Vec<Option<Stroke>>) {
        let above = if self.midrule_due && !self.ruled {
            None
        } else {
            self.pending.take()
        };
        self.begin_row();
        let edges = match above {
            Some(above) => above
                .into_iter()
                .zip(tops)
                .map(|(a, b)| stronger(a, b))
                .collect(),
            None => tops,
        };
        self.draw_edges(&edges);
    }

    /// Draw the pending bottom edges on their own.
    fn settle(&mut self) {
        if let Some(edges) = self.pending.take() {
            self.draw_edges(&edges);
        }
    }

    /// Horizontal cell borders, one per column: a line of `\omit`ted cells, each
    /// filled with its edge (rule leaders, or dashes and dots as box leaders).
    /// The line is as tall as its widest edge, with narrower edges centered in
    /// it, so the next row's `\cellcolor` can't paint over it (it would over a
    /// `\cline`). A double edge's second line is a line of its own, a gap below.
    fn draw_edges(&mut self, edges: &[Option<Stroke>]) {
        if edges.iter().all(Option::is_none) {
            return;
        }
        // Columns with the same edge share one `\multispan`; trailing columns
        // without one are left out.
        let line = |edges: &[Option<&Stroke>]| {
            let height = edges
                .iter()
                .flatten()
                .map(|k| thickness(k))
                .fold(0.0, f64::max);
            let end = edges.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
            let mut cells = Vec::new();
            let mut c = 0;
            while c < end {
                let n = edges[c..end].iter().take_while(|e| **e == edges[c]).count();
                let span = if n == 1 {
                    "\\omit".to_string()
                } else {
                    format!("\\multispan{{{n}}}")
                };
                let fill = edges[c].map(|k| leaders(k, height)).unwrap_or_default();
                cells.push(format!("{span}{fill}"));
                c += n;
            }
            format!("{}\\cr", cells.join("&"))
        };
        let first: Vec<Option<&Stroke>> = edges.iter().map(Option::as_ref).collect();
        writeln!(self.buf, "{}", line(&first)).unwrap();
        let second: Vec<Option<&Stroke>> = first
            .iter()
            .map(|e| e.filter(|k| k.style == BorderStyle::Double))
            .collect();
        let gap = second
            .iter()
            .flatten()
            .map(|k| thickness(k))
            .fold(0.0, f64::max);
        if gap > 0.0 {
            writeln!(self.buf, "\\noalign{{\\vskip {}}}", pt(gap)).unwrap();
            writeln!(self.buf, "{}", line(&second)).unwrap();
        }
    }

    /// Each column's top and bottom border in section row `r`: the covering
    /// cell's, on the sides that lie on its outline (a cell spanning rows has no
    /// edge between them).
    fn horizontal_edges(&self, section: &Section, r: usize) -> Sides<Vec<Option<Stroke>>> {
        let mut edges = Sides::<Vec<Option<Stroke>>>::default();
        for c in 0..self.width() {
            let s = section
                .covering(r, c)
                .map(|(cell, outline)| cell.style.strokes(BASE_PT).masked(outline))
                .unwrap_or_default();
            edges.top.push(s.top);
            edges.bottom.push(s.bottom);
        }
        edges
    }

    /// The border at column boundary `b` (0: the table's left edge) in section
    /// row `r`: the stronger of the left cell's right side and the right cell's
    /// left side.
    fn vertical_edge(&self, section: &Section, r: usize, b: usize) -> Option<Stroke> {
        let side = |c: usize| {
            let (cell, outline) = section.covering(r, c)?;
            Some(cell.style.strokes(BASE_PT).masked(outline))
        };
        let left = b.checked_sub(1).and_then(side).and_then(|s| s.right);
        let right = (b < self.width())
            .then(|| side(b))
            .flatten()
            .and_then(|s| s.left);
        stronger(left, right)
    }

    fn render_bottomrule(&mut self) {
        self.midrule_due = false;
        self.rule("\\bottomrule");
    }

    /// Rules under the head cells that end in head row `r` (spanners): each
    /// non-empty cell gets a `\cmidrule` (`\cline` without booktabs) over exactly
    /// its columns, so it reads as grouping the labels below it. Nothing is drawn
    /// under the last head row, which the head's `\midrule` closes.
    fn render_spanner_rules(&mut self, r: usize) {
        let rows = &self.rt.head.rows;
        if r + 1 >= rows.len() {
            return;
        }
        for (origin, row) in rows[..=r].iter().enumerate() {
            for cell in row.cells() {
                let blank = gridwell_layout::plain_text(&cell.content, " ")
                    .trim()
                    .is_empty();
                if origin + cell.rowspan != r + 1 || blank {
                    continue;
                }
                let (first, last) = (cell.col + 1, cell.col + cell.colspan);
                self.settle();
                if self.config.booktabs {
                    writeln!(self.buf, "\\cmidrule(lr){{{first}-{last}}}").unwrap();
                } else {
                    writeln!(self.buf, "\\cline{{{first}-{last}}}").unwrap();
                }
            }
        }
    }

    // ─── Rows ───

    fn render_thead(&mut self) {
        let rt = self.rt;
        if rt.head.is_empty() {
            return;
        }
        for r in 0..rt.head.rows.len() {
            self.render_row(&rt.head, r);
            self.render_spanner_rules(r);
        }
        self.render_midrule();
    }

    fn render_tbody(&mut self) {
        let rt = self.rt;
        let cols = self.width();
        for (g, group) in rt.groups.iter().enumerate() {
            if let Some(label) = &group.label {
                let text = apply_style(
                    &self.content(&label.content, &HAlign::Left, &label.style),
                    &label.style,
                );
                if self.config.booktabs {
                    self.render_midrule();
                }
                let s = label.style.strokes(BASE_PT);
                self.begin_line(vec![s.top.clone(); cols]);
                writeln!(
                    self.buf,
                    "\\multicolumn{{{cols}}}{{{}l{}}}{{\\bfseries {text}}} \\\\",
                    vrule(s.left.as_ref()),
                    vrule(s.right.as_ref())
                )
                .unwrap();
                self.pending = Some(vec![s.bottom; cols]);
                if self.config.booktabs {
                    self.render_midrule();
                }
            }

            for r in 0..group.rows.rows.len() {
                self.render_row(&group.rows, r);
            }

            if !group.summary_rows.is_empty() {
                if self.config.booktabs {
                    self.begin_row();
                    writeln!(self.buf, "\\cmidrule(lr){{1-{cols}}}").unwrap();
                }
                for r in 0..group.summary_rows.rows.len() {
                    self.render_row(&group.summary_rows, r);
                }
            }

            if g + 1 < rt.groups.len() && group.label.is_none() {
                self.begin_row();
                writeln!(self.buf, "\\addlinespace").unwrap();
            }
        }
    }

    /// One `\\`-terminated row. Every visible column gets an entry: a position
    /// covered from above is an empty cell (as wide as the spanning cell, via
    /// `\multicolumn`, for a 2D span); positions covered from the left are absorbed
    /// by the origin's `\multicolumn`.
    fn render_row(&mut self, section: &Section, r: usize) {
        let edges = self.horizontal_edges(section, r);
        self.begin_line(edges.top);
        let rows = &section.rows;
        let mut cells: Vec<String> = Vec::new();
        for (col, slot) in rows[r].slots.iter().enumerate() {
            let (span, content, align) = match slot {
                Slot::Origin(cell) => {
                    let (content, align) = self.render_cell(cell);
                    (cell.colspan, content, align)
                }
                Slot::CoveredV {
                    origin_row,
                    origin_col,
                    ..
                } if *origin_col == col => {
                    let span = rows[*origin_row].slots[col]
                        .origin()
                        .map_or(1, |c| c.colspan);
                    (span, String::new(), (span > 1).then(|| "l".to_string()))
                }
                Slot::CoveredH { .. } | Slot::CoveredV { .. } => continue,
                Slot::Empty => (1, String::new(), None),
            };
            cells.push(self.entry(section, r, col, span, content, align.as_deref()));
        }
        writeln!(self.buf, "{} \\\\", cells.join(" & ")).unwrap();
        self.pending = Some(edges.bottom);
    }

    /// A row entry over `span` columns from `col`, as a `\multicolumn` when it
    /// spans columns, has its own alignment (`align`), or has a vertical border:
    /// a boundary's border is drawn by the entry to its left (the table's left
    /// edge by the first entry), as `!{…}` in its column spec.
    fn entry(
        &self,
        section: &Section,
        r: usize,
        col: usize,
        span: usize,
        content: String,
        align: Option<&str>,
    ) -> String {
        let left = (col == 0)
            .then(|| self.vertical_edge(section, r, 0))
            .flatten();
        let right = self.vertical_edge(section, r, col + span);
        if align.is_none() && left.is_none() && right.is_none() {
            return content;
        }
        let spec = align.map_or_else(|| self.column_spec(col), String::from);
        format!(
            "\\multicolumn{{{span}}}{{{}{spec}{}}}{{{content}}}",
            vrule(left.as_ref()),
            vrule(right.as_ref())
        )
    }

    /// A cell's content, and the alignment letter its `\multicolumn` needs, if any.
    fn render_cell(&self, cell: &ResolvedCell) -> (String, Option<String>) {
        let column = &self.rt.columns[cell.col];
        // Aligned on a character: the padding, invisible, after the last line.
        let pad = cell.align_pad.map_or_else(String::new, |pad| {
            format!("\\phantom{{{}}}", escape_latex(&pad.text()))
        });
        let mut out = self.content_ending(&cell.content, &cell.align, &cell.style, &pad);
        out = apply_style(&out, &cell.style);

        if cell.rowspan > 1 {
            out = format!("\\multirow{{{}}}{{*}}{{{out}}}", cell.rowspan);
        }
        // A cell's own alignment differs from its column's: a one-column
        // `\multicolumn` overrides it, keeping the column's width.
        let spec = if cell.colspan > 1 {
            Some(align_letter(&cell.align).to_string())
        } else {
            let own = self.aligned_spec(cell.col, &cell.align);
            (own != self.aligned_spec(cell.col, &column.align)).then_some(own)
        };
        (out, spec)
    }

    /// Content as LaTeX. Line breaks become a nested one-column tabular (the only
    /// form that works in `l`/`c`/`r` columns as well as `p{}`). `block`'s
    /// underline and strike-through are applied to each line.
    fn content(&self, nodes: &[ContentNode], align: &HAlign, block: &ResolvedStyle) -> String {
        self.content_ending(nodes, align, block, "")
    }

    /// [`Self::content`], its last line ending with `tail` (undecorated).
    fn content_ending(
        &self,
        nodes: &[ContentNode],
        align: &HAlign,
        block: &ResolvedStyle,
        tail: &str,
    ) -> String {
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
                ContentNode::Unknown(_) => {}
            }
        }
        let mut lines: Vec<String> = lines
            .iter()
            .map(|l| {
                if l.is_empty() {
                    String::new()
                } else {
                    decorate(l, block)
                }
            })
            .collect();
        lines.last_mut().unwrap().push_str(tail);
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
        for block in self.note_lines() {
            writeln!(self.buf).unwrap();
            for text in block {
                writeln!(self.buf, "{text}\\\\").unwrap();
            }
        }
    }

    /// The footnotes and the source notes, as blocks of lines (an empty block
    /// is left out).
    fn note_lines(&self) -> Vec<Vec<String>> {
        let footer = &self.rt.footer;
        let footnotes = footer.footnotes.iter().map(|note| {
            let text = apply_block_style(
                &self.content(&note.content, &HAlign::Left, &note.style),
                &note.style,
            );
            format!("\\textsuperscript{{{}}} {text}", escape_latex(note.mark))
        });
        let sources = footer.source_notes.iter().map(|note| {
            let text = apply_block_style(
                &self.content(&note.content, &HAlign::Left, &note.style),
                &note.style,
            );
            format!("{{\\footnotesize {text}}}")
        });
        [footnotes.collect::<Vec<_>>(), sources.collect()]
            .into_iter()
            .filter(|b| !b.is_empty())
            .collect()
    }
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &LatexWriterConfig) -> Result<String, RenderError> {
    let rt = resolve(table);
    LatexRenderer::new(&rt, config).render()
}

/// The size relative sizes and border widths resolve against: the document's
/// 10pt.
pub(crate) const BASE_PT: f64 = 10.0;

fn pt(v: f64) -> String {
    format!("{}pt", (v * 1e4).round() / 1e4)
}

/// The edge drawn where two cells' borders meet, as CSS collapses them: the
/// wider, then the more prominent style, then the first (above or left).
fn stronger(a: Option<Stroke>, b: Option<Stroke>) -> Option<Stroke> {
    let rank = |k: &Stroke| match k.style {
        BorderStyle::Double => 4,
        BorderStyle::Solid => 3,
        BorderStyle::Dashed => 2,
        BorderStyle::Dotted => 1,
        _ => 0,
    };
    match (a, b) {
        (Some(a), Some(b)) if (b.width_pt, rank(&b)) > (a.width_pt, rank(&a)) => Some(b),
        (a, b) => a.or(b),
    }
}

/// How thick an edge's line is: a double edge's two lines and the gap between
/// them are a third of its width each.
fn thickness(k: &Stroke) -> f64 {
    if k.style == BorderStyle::Double {
        k.width_pt / 3.0
    } else {
        k.width_pt
    }
}

/// One cell of a horizontal border line `height` tall: its edge, centered, in
/// its color (grouped, so the color ends with the cell).
fn leaders(k: &Stroke, height: f64) -> String {
    let t = thickness(k);
    let raise = (height - t) / 2.0;
    let extent = if raise > 0.0 {
        format!("height {} depth -{}", pt(t + raise), pt(raise))
    } else {
        format!("height {}", pt(t))
    };
    let dashes = |period: f64, len: f64| {
        format!(
            "\\xleaders\\hbox to {}{{\\hss\\vrule {extent} width {}\\hss}}\\hfill",
            pt(period),
            pt(len)
        )
    };
    let line = match k.style {
        BorderStyle::Dashed => dashes(6.0 * t, 3.0 * t),
        BorderStyle::Dotted => dashes(2.0 * t, t),
        _ => format!("\\leaders\\hrule {extent}\\hfill"),
    };
    match k.color {
        Some(c) => format!("{{\\color{}{line}}}", latex_color(c)),
        None => line,
    }
}

/// A vertical border as `!{…}` in a column spec (from `array`, which `colortbl`
/// loads). A rule can't be dashed or dotted, so those are drawn solid.
fn vrule(k: Option<&Stroke>) -> String {
    let Some(k) = k else {
        return String::new();
    };
    let rule = match k.style {
        BorderStyle::Double => {
            let l = pt(k.width_pt / 3.0);
            format!("\\vrule width {l}\\hskip {l}\\vrule width {l}")
        }
        _ => format!("\\vrule width {}", pt(k.width_pt)),
    };
    match k.color {
        Some(c) => format!("!{{{{\\color{}{rule}}}}}", latex_color(c)),
        None => format!("!{{{rule}}}"),
    }
}

fn align_letter(align: &HAlign) -> &'static str {
    match align {
        HAlign::Right => "r",
        HAlign::Center => "c",
        _ => "l",
    }
}

/// How a column's width is written.
enum ColWidth {
    /// As wide as its content: an alignment letter.
    Natural,
    /// A paragraph column, `p{…}`, this wide.
    Fixed(String),
    /// A `tabularx` `X` column taking this share of the space left.
    Fr(f64),
}

/// A number for TeX, to four decimals.
fn num(v: f64) -> String {
    format!("{}", (v * 1e4).round() / 1e4)
}

/// A width with an absolute meaning, usable in `p{…}`: absolute units and `em`
/// keep their unit (px → pt at 0.75pt/px). `%`, `fr`, `auto` and non-positive
/// widths have none.
fn absolute_width(w: &Length) -> Option<String> {
    let pos = |v: f64| (v.is_finite() && v > 0.0).then(|| num(v));
    Some(match *w {
        Length::Px(v) => format!("{}pt", pos(v * 0.75)?),
        Length::Pt(v) => format!("{}pt", pos(v)?),
        Length::Em(v) | Length::Rem(v) => format!("{}em", pos(v)?),
        Length::In(v) => format!("{}in", pos(v)?),
        Length::Cm(v) => format!("{}cm", pos(v)?),
        Length::Mm(v) => format!("{}mm", pos(v)?),
        Length::Percent(_) | Length::Fr(_) | Length::Auto => return None,
    })
}

/// Bold, italic, color, underline, strike-through and size for an inline run
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
    // A weight that isn't bold un-bolds what holds it (a bold cell, title or
    // group label).
    match style.font_weight {
        Some(_) if style.is_bold() => result = format!("\\textbf{{{result}}}"),
        Some(_) => result = format!("\\textmd{{{result}}}"),
        None => {}
    }
    // And an upright style un-italics an italic one.
    match style.font_style {
        Some(_) if style.is_italic() => result = format!("\\textit{{{result}}}"),
        Some(_) => result = format!("\\textup{{{result}}}"),
        None => {}
    }
    if let Some(c) = style.paint() {
        result = format!("\\textcolor{}{{{result}}}", latex_color(c));
    }
    if decorated {
        result = decorate(&result, style);
    }
    // Relative sizes resolve against the document's 10pt; leading is 1.2×.
    if let Some(pt) = style.size_pt(BASE_PT) {
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
    if monospace(style) {
        result = format!("\\texttt{{{result}}}");
    }
    if let Some(bg) = style.fill() {
        result = format!("\\cellcolor{}{result}", latex_color(bg));
    }
    result
}

/// Whether a style's family is monospace, which a cell or group label draws in
/// `\texttt` (the one family pdfLaTeX has without naming a font).
pub(crate) fn monospace(style: &ResolvedStyle) -> bool {
    style
        .font_family
        .as_deref()
        .is_some_and(|f| f.contains("monospace") || f.contains("Courier"))
}

/// An xcolor `[HTML]{RRGGBB}` spec; LaTeX colors have no alpha, so translucent
/// colors are flattened onto white.
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
        let w = |s: &str| absolute_width(&s.parse().unwrap());
        assert_eq!(w("120px").as_deref(), Some("90pt"));
        assert_eq!(w("25%"), None);
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
