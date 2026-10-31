use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table, VMerge};
use gridwell_layout::{
    resolve, CellVAlign, MergeCell, ResolvedStyle, ResolvedTable, Section, Sides, Stroke,
};
use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// The size relative sizes resolve against: RTF's default 12pt (`\\fs24`).
const BASE_PT: f64 = 12.0;

/// Default column width in twips (1 inch = 1440 twips).
const DEFAULT_COL_WIDTH: u32 = 2160; // 1.5 inches

struct RtfRenderer<'r, 'a> {
    rt: &'r ResolvedTable<'a>,
    buf: String,
    /// The color table: index `i` here is `\cf{i + 1}` (RTF's index 0 is "auto").
    colors: Vec<Color>,
    /// The font table: index `i` here is `\f{i}`; `\f0` is the default, Arial.
    fonts: Vec<(String, &'static str)>,
    col_widths: Vec<u32>,
}

impl<'r, 'a> RtfRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>) -> Self {
        let col_widths = rt
            .columns
            .iter()
            .map(|col| {
                // Absolute widths convert exactly; `%`, `fr` and `auto` have nothing
                // to resolve against in RTF and get the default.
                col.width
                    .as_ref()
                    .and_then(|w| w.to_twips(BASE_PT, BASE_PT))
                    .filter(|t| t.is_finite() && *t >= 1.0 && *t <= u32::MAX as f64)
                    .map_or(DEFAULT_COL_WIDTH, |t| t as u32)
            })
            .collect();
        let mut r = Self {
            rt,
            buf: String::with_capacity(4096),
            colors: vec![Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)],
            fonts: vec![("Arial".to_string(), "")],
            col_widths,
        };
        // Register every color and font the document uses, in document order, so
        // the color and font tables are complete before they are written: cell
        // text and fills, line styles, and inline styled runs.
        let mut styles: Vec<ResolvedStyle> = Vec::new();
        let h = &rt.header;
        styles.extend(
            h.title
                .iter()
                .chain(&h.subtitle)
                .chain(&h.extra_lines)
                .map(|l| l.style.clone()),
        );
        for section in rt.sections() {
            for row in &section.rows {
                styles.extend(row.cells().map(|c| c.style.clone()));
            }
        }
        styles.extend(
            rt.groups
                .iter()
                .filter_map(|g| g.label.as_ref())
                .map(|l| l.style.clone()),
        );
        styles.extend(rt.footer.footnotes.iter().map(|n| n.style.clone()));
        styles.extend(rt.footer.source_notes.iter().map(|n| n.style.clone()));
        for nodes in rt.contents() {
            for node in nodes {
                if let ContentNode::StyledText {
                    style_id: Some(id), ..
                } = node
                {
                    styles.push(rt.style(id));
                }
            }
        }
        for style in &styles {
            for c in [style.paint(), style.fill()].into_iter().flatten() {
                r.color_index(c);
            }
            let strokes = style.strokes(BASE_PT);
            for (_, k) in strokes.iter() {
                if let Some(c) = k.as_ref().and_then(|k| k.color) {
                    r.color_index(c);
                }
            }
            if let Some(name) = style.font_name() {
                if r.font_ref(name).is_none() {
                    r.fonts.push((name.to_string(), font_class(style)));
                }
            }
        }
        r
    }

    /// Cell border definitions (`\clbrdr*`), which come before the cell's
    /// shading and `\cellx`. Every color was registered up front.
    fn borders(&self, strokes: &Sides<Option<Stroke>>) -> String {
        let mut out = String::new();
        for (word, stroke) in [
            ("t", &strokes.top),
            ("l", &strokes.left),
            ("b", &strokes.bottom),
            ("r", &strokes.right),
        ] {
            let Some(k) = stroke else { continue };
            // `\brdrw` is in twips and stops at 75; a solid line can go to twice
            // that as `\brdrth`, which doubles it.
            let twips = (k.width_pt * 20.0).round().max(1.0);
            let (kind, twips) = match k.style {
                BorderStyle::Dashed => ("\\brdrdash", twips),
                BorderStyle::Dotted => ("\\brdrdot", twips),
                BorderStyle::Double => ("\\brdrdb", twips),
                _ if twips > 75.0 => ("\\brdrth", (twips / 2.0).round()),
                _ => ("\\brdrs", twips),
            };
            write!(out, "\\clbrdr{word}{kind}\\brdrw{}", twips.min(75.0) as u32).unwrap();
            if let Some(i) = k.color.and_then(|c| self.color_ref(c)) {
                write!(out, "\\brdrcf{i}").unwrap();
            }
        }
        out
    }

    /// Character formatting for a style: bold, italic, color and font (every
    /// color and font was registered up front). A weight that isn't bold is
    /// `\b0`, which un-bolds what holds it (a bold cell, title or group label).
    fn char_format(&self, style: &ResolvedStyle) -> String {
        let mut f = String::new();
        match style.font_weight {
            Some(_) if style.is_bold() => f.push_str("\\b"),
            Some(_) => f.push_str("\\b0"),
            None => {}
        }
        if style.is_italic() {
            f.push_str("\\i");
        }
        if let Some(i) = style.paint().and_then(|c| self.color_ref(c)) {
            write!(f, "\\cf{i}").unwrap();
        }
        f.push_str(&self.font_format(style));
        f.push_str(&text_format(style));
        f
    }

    /// `\f` for the style's font, if it names one.
    fn font_format(&self, style: &ResolvedStyle) -> String {
        style
            .font_name()
            .and_then(|name| self.font_ref(name))
            .map(|i| format!("\\f{i}"))
            .unwrap_or_default()
    }

    /// The index of an already registered font. Font names match regardless of
    /// case, as fonts do.
    fn font_ref(&self, name: &str) -> Option<usize> {
        self.fonts
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// The index of an already registered color.
    fn color_ref(&self, c: Color) -> Option<usize> {
        let c = c.flatten();
        self.colors.iter().position(|&k| k == c).map(|i| i + 1)
    }

    /// Content as RTF; styled runs are groups with their own formatting.
    fn content(&self, nodes: &[ContentNode]) -> String {
        let mut out = String::new();
        for node in nodes {
            match node {
                ContentNode::Text { value } => out.push_str(&escape_rtf(value)),
                ContentNode::StyledText { value, style_id } => {
                    let style = style_id
                        .as_deref()
                        .map(|id| self.rt.style(id))
                        .unwrap_or_default();
                    let fmt = self.char_format(&style);
                    if fmt.is_empty() {
                        out.push_str(&escape_rtf(value));
                    } else {
                        write!(out, "{{{fmt} {}}}", escape_rtf(value)).unwrap();
                    }
                }
                ContentNode::LineBreak {} => out.push_str("\\line "),
                ContentNode::FootnoteMark { mark_text, .. } => {
                    write!(out, "{{\\super {}}}", escape_rtf(mark_text)).unwrap();
                }
                ContentNode::Image { alt, .. } => {
                    if let Some(alt_text) = alt {
                        out.push_str(&escape_rtf(alt_text));
                    }
                }
                ContentNode::Raw { format, value } => {
                    if format == "rtf" {
                        out.push_str(value);
                    }
                }
                ContentNode::Unknown => {}
            }
        }
        out
    }

    /// A paragraph outside the table: `base` control words plus the line's style.
    fn line(&self, base: &str, line: &gridwell_layout::Line) -> String {
        let text = self.content(&line.content);
        let fmt = self.char_format(&line.style);
        format!("\\pard{{{base}{fmt} {text}}}\\par")
    }

    /// The `\cf` / `\clcbpat` index of a color, registering it if new. RTF has no
    /// alpha: translucent colors are flattened onto white.
    fn color_index(&mut self, c: Color) -> usize {
        let c = c.flatten();
        let i = match self.colors.iter().position(|&k| k == c) {
            Some(i) => i,
            None => {
                self.colors.push(c);
                self.colors.len() - 1
            }
        };
        i + 1
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.write_header();
        self.write_title();
        if !self.rt.is_empty() {
            self.write_table();
        }
        self.write_footnotes();
        self.buf.push('}'); // close RTF group
        Ok(self.buf)
    }

    fn write_header(&mut self) {
        self.buf.push_str("{\\rtf1\\ansi\\deff0\n");
        self.buf.push_str("{\\fonttbl{\\f0 Arial;}");
        for (i, (name, class)) in self.fonts.iter().enumerate().skip(1) {
            write!(self.buf, "{{\\f{i}{class} {};}}", escape_rtf(name)).unwrap();
        }
        self.buf.push_str("}\n");
        self.buf.push_str("{\\colortbl ;");
        for c in &self.colors {
            write!(self.buf, "\\red{}\\green{}\\blue{};", c.r, c.g, c.b).unwrap();
        }
        self.buf.push_str("}\n");
    }

    fn write_title(&mut self) {
        let header = &self.rt.header;
        if header.title.is_none() && header.subtitle.is_none() && header.extra_lines.is_empty() {
            return;
        }
        let mut lines = Vec::new();
        if let Some(title) = &header.title {
            lines.push(self.line("\\b\\fs36", title));
        }
        for l in header.subtitle.iter().chain(&header.extra_lines) {
            lines.push(self.line("\\fs24", l));
        }
        for l in lines {
            writeln!(self.buf, "{l}").unwrap();
        }
        self.buf.push_str("\\par\n");
    }

    fn write_table(&mut self) {
        let rt = self.rt;
        self.write_section(&rt.head, true);
        for group in &rt.groups {
            if let Some(label) = &group.label {
                let text = self.content(&label.content);
                let fmt = self.char_format(&label.style);
                let borders = self.borders(&label.style.strokes(BASE_PT));
                let fill = label
                    .style
                    .fill()
                    .and_then(|c| self.color_ref(c))
                    .map(|i| format!("\\clcbpat{i}"))
                    .unwrap_or_default();
                let total_width: u32 = self.col_widths.iter().sum();
                writeln!(
                    self.buf,
                    "\\trowd\\trqc{borders}{fill}\\cellx{total_width}\n\\pard\\intbl{{\\b{fmt} {text}}}\\cell\n\\row"
                )
                .unwrap();
            }
            self.write_section(&group.rows, false);
            self.write_section(&group.summary_rows, false);
        }
    }

    /// Write one section. Spans are resolved per section: a rowspan never crosses
    /// a section boundary.
    fn write_section(&mut self, section: &Section, is_header: bool) {
        for cells in section.continuation_rows() {
            self.write_row(&cells, is_header);
        }
    }

    fn write_row(&mut self, cells: &[MergeCell<'_, '_>], is_header: bool) {
        self.buf.push_str("\\trowd");
        if is_header {
            self.buf.push_str("\\trhdr");
        }

        // Cell definitions. `\cellx` is the cell's right edge (cumulative twips),
        // derived from the visible column, never from a count of emitted cells.
        for mc in cells {
            let right_edge: u32 = self.col_widths[..mc.col + mc.span].iter().sum();
            match mc.vmerge {
                VMerge::Start => self.buf.push_str("\\clvmgf"),
                VMerge::Continue => self.buf.push_str("\\clvmrg"),
                VMerge::None => {}
            }
            // Before the borders, as the spec orders cell definitions.
            match mc.owner.and_then(|o| o.style.cell_valign()) {
                Some(CellVAlign::Top) => self.buf.push_str("\\clvertalt"),
                Some(CellVAlign::Middle) => self.buf.push_str("\\clvertalc"),
                Some(CellVAlign::Bottom) => self.buf.push_str("\\clvertalb"),
                None => {}
            }
            if let Some(owner) = mc.owner {
                let borders = self.borders(&owner.style.strokes(BASE_PT).masked(mc.outline));
                self.buf.push_str(&borders);
            }
            if let Some(fill) = mc.cell.and_then(|c| c.style.fill()) {
                let i = self.color_index(fill);
                write!(self.buf, "\\clcbpat{i}").unwrap();
            }
            write!(self.buf, "\\cellx{right_edge}").unwrap();
        }
        self.buf.push('\n');

        // Cell contents (one `\cell` per definition above, in the same order).
        for mc in cells {
            let Some(cell) = mc.cell else {
                self.buf.push_str("\\pard\\intbl\\plain\\cell\n");
                continue;
            };
            let text = self.content(&cell.content);
            let mut fmt = String::new();
            match cell.align {
                HAlign::Right => fmt.push_str("\\qr"),
                HAlign::Center => fmt.push_str("\\qc"),
                HAlign::Justify => fmt.push_str("\\qj"),
                _ => {}
            }
            if cell.style.bold_over(is_header) {
                fmt.push_str("\\b");
            }
            if cell.style.is_italic() {
                fmt.push_str("\\i");
            }
            if let Some(c) = cell.style.paint() {
                let i = self.color_index(c);
                write!(fmt, "\\cf{i}").unwrap();
            }
            fmt.push_str(&self.font_format(&cell.style));
            fmt.push_str(&text_format(&cell.style));
            // `\pard` resets paragraph formatting only: without `\plain`, the
            // previous cell's bold, italic or color would carry over.
            writeln!(self.buf, "\\pard\\intbl\\plain{fmt} {text}\\cell").unwrap();
        }
        self.buf.push_str("\\row\n");
    }

    fn write_footnotes(&mut self) {
        let footer = &self.rt.footer;
        if !footer.footnotes.is_empty() {
            self.buf.push_str("\\par\n");
            for note in &footer.footnotes {
                let text = self.content(&note.content);
                let mark = escape_rtf(note.mark);
                let fmt = self.char_format(&note.style);
                writeln!(
                    self.buf,
                    "\\pard{{\\fs18{fmt} {{\\super {mark}}} {text}}}\\par"
                )
                .unwrap();
            }
        }
        if !footer.source_notes.is_empty() {
            self.buf.push_str("\\par\n");
            for note in &footer.source_notes {
                let line = self.line("\\fs18", note);
                writeln!(self.buf, "{line}").unwrap();
            }
        }
    }
}

/// The font table's family class for a style's font, from the generic family
/// in its `font_family` list: what a reader substitutes from when the font
/// itself is missing.
fn font_class(style: &ResolvedStyle) -> &'static str {
    let list = style.font_family.as_deref().unwrap_or("");
    for entry in list.split(',') {
        match entry.trim().to_ascii_lowercase().as_str() {
            "serif" | "ui-serif" => return "\\froman",
            "sans-serif" | "ui-sans-serif" => return "\\fswiss",
            "monospace" | "ui-monospace" => return "\\fmodern",
            _ => {}
        }
    }
    "\\fnil"
}

/// Underline, strike-through and size (`\\fs` in half-points; relative sizes
/// resolve against RTF's default 12pt).
fn text_format(style: &ResolvedStyle) -> String {
    let mut f = String::new();
    if style.is_underline() {
        f.push_str("\\ul");
    }
    if style.is_strike() {
        f.push_str("\\strike");
    }
    if let Some(pt) = style.size_pt(BASE_PT) {
        write!(f, "\\fs{}", (pt * 2.0).round().clamp(1.0, 32767.0) as u32).unwrap();
    }
    f
}

pub fn render(table: &Table) -> Result<String, RenderError> {
    let rt = resolve(table);
    RtfRenderer::new(&rt).render()
}

/// Escape text for RTF: control words for `\`, `{`, `}`; non-ASCII as `\uN?` with
/// N the signed 16-bit UTF-16 code unit (surrogate pairs for characters beyond the
/// BMP); tabs and newlines as `\tab` / `\line`; other control characters dropped.
fn escape_rtf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\t' => out.push_str("\\tab "),
            '\n' => out.push_str("\\line "),
            c if c.is_ascii_control() => {}
            c if c.is_ascii() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    write!(out, "\\u{}?", *unit as i16).unwrap();
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_rtf;

    #[test]
    fn escapes_specials_and_encodes_utf16() {
        assert_eq!(escape_rtf(r"a\b{c}"), r"a\\b\{c\}");
        assert_eq!(escape_rtf("é"), "\\u233?");
        // U+FFFF and up: signed code units.
        assert_eq!(escape_rtf("\u{FFFD}"), "\\u-3?");
        // Beyond the BMP: a surrogate pair (U+1F600 = D83D DE00).
        assert_eq!(escape_rtf("😀"), "\\u-10179?\\u-8704?");
        assert_eq!(escape_rtf("a\tb\nc\u{7}\u{1b}d"), "a\\tab b\\line cd");
    }
}
