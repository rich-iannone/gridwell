use gridwell_core::link::link_url;
use gridwell_core::{Color, Image, ImageFormat};
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

/// Bold and italic, for left-to-right and right-to-left text alike.
const BOLD: &str = "\\b\\ab";
const ITALIC: &str = "\\i\\ai";

/// Default column width in twips (1 inch = 1440 twips).
const DEFAULT_COL_WIDTH: u32 = 2160; // 1.5 inches

/// The width of the line, in points, that `%` and `fr` widths divide: RTF's
/// default page (`\paperw12240`, 8.5in) less its default margins (`\margl1800`,
/// `\margr1800`), 6in. The document sets no page, so it fits Letter and A4.
const LINE_PT: f64 = 432.0;

struct RtfRenderer<'r, 'a> {
    rt: &'r ResolvedTable<'a>,
    buf: String,
    /// The color table: index `i` here is `\cf{i + 1}` (RTF's index 0 is "auto").
    colors: Vec<Color>,
    /// The font table: index `i` here is `\f{i}`; `\f0` is the default, Arial.
    fonts: Vec<(String, &'static str)>,
    col_widths: Vec<u32>,
    /// The table runs right to left.
    rtl: bool,
}

impl<'r, 'a> RtfRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>) -> Self {
        // In twips; a `\cellx` edge is a signed 32-bit number.
        let col_widths = rt.column_widths_in(
            LINE_PT,
            f64::from(DEFAULT_COL_WIDTH) / 20.0,
            BASE_PT,
            20.0,
            i32::MAX as u32,
        );
        let mut r = Self {
            rt,
            buf: String::with_capacity(4096),
            colors: vec![Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)],
            fonts: vec![("Arial".to_string(), "")],
            col_widths,
            rtl: rt.is_rtl(),
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

    /// `\rtlrow` for a right-to-left table: the row's first cell is on the
    /// right, and `\cellx` edges count from there.
    fn row_dir(&self) -> &'static str {
        if self.rtl {
            "\\rtlrow"
        } else {
            ""
        }
    }

    /// `\rtlpar` for a paragraph of a right-to-left table.
    fn par_dir(&self) -> &'static str {
        if self.rtl {
            "\\rtlpar"
        } else {
            ""
        }
    }

    /// A paragraph that starts on the table's start side (a line outside the
    /// table, a group label): right to left and on the right, if the table is
    /// (`\ql`, the default, is physical).
    fn line_dir(&self) -> &'static str {
        if self.rtl {
            "\\rtlpar\\qr"
        } else {
            ""
        }
    }

    /// Cell border definitions (`\clbrdr*`), which come before the cell's
    /// shading and `\cellx`. Every color was registered up front. Left and right
    /// are the cell's start and end, so a right-to-left row's strokes come
    /// mirrored.
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
    /// `\b0`, which un-bolds what holds it (a bold cell, title or group label);
    /// an upright style is `\i0`, likewise. Bold and italic are also set for
    /// right-to-left text (`\ab`, `\ai`, its "associated" formatting).
    fn char_format(&self, style: &ResolvedStyle) -> String {
        let mut f = String::new();
        match style.font_weight {
            Some(_) if style.is_bold() => f.push_str(BOLD),
            Some(_) => f.push_str("\\b0\\ab0"),
            None => {}
        }
        match style.font_style {
            Some(_) if style.is_italic() => f.push_str(ITALIC),
            Some(_) => f.push_str("\\i0\\ai0"),
            None => {}
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
                ContentNode::Text { value, href } => {
                    out.push_str(&linked(&escape_rtf(value), href))
                }
                ContentNode::StyledText {
                    value,
                    style_id,
                    href,
                } => {
                    let style = style_id
                        .as_deref()
                        .map(|id| self.rt.style(id))
                        .unwrap_or_default();
                    let fmt = self.char_format(&style);
                    let text = if fmt.is_empty() {
                        escape_rtf(value)
                    } else {
                        format!("{{{fmt} {}}}", escape_rtf(value))
                    };
                    out.push_str(&linked(&text, href));
                }
                ContentNode::LineBreak {} => out.push_str("\\line "),
                ContentNode::FootnoteMark { mark_text, .. } => {
                    write!(out, "{{\\super {}}}", escape_rtf(mark_text)).unwrap();
                }
                ContentNode::Image {
                    src,
                    alt,
                    width,
                    height,
                } => match pict(src, alt.as_deref(), width.as_deref(), height.as_deref()) {
                    Some(pict) => out.push_str(&pict),
                    None => {
                        if let Some(alt_text) = alt {
                            out.push_str(&escape_rtf(alt_text));
                        }
                    }
                },
                ContentNode::Raw { format, value } => {
                    if format == "rtf" {
                        out.push_str(value);
                    }
                }
                ContentNode::Unknown(_) => {}
            }
        }
        out
    }

    /// A paragraph outside the table: `base` control words plus the line's style.
    fn line(&self, base: &str, line: &gridwell_layout::Line) -> String {
        let text = self.content(&line.content);
        let fmt = self.char_format(&line.style);
        let dir = self.line_dir();
        format!("\\pard{dir}{{{base}{fmt} {text}}}\\par")
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
            lines.push(self.line(&format!("{BOLD}\\fs36"), title));
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
                let borders = self.borders(&label.style.strokes(BASE_PT).mirrored(self.rtl));
                let fill = label
                    .style
                    .fill()
                    .and_then(|c| self.color_ref(c))
                    .map(|i| format!("\\clcbpat{i}"))
                    .unwrap_or_default();
                let total_width: u32 = self.col_widths.iter().sum();
                writeln!(
                    self.buf,
                    "\\trowd{}\\trqc{borders}{fill}\\cellx{total_width}\n\\pard\\intbl{}{{{BOLD}{fmt} {text}}}\\cell\n\\row",
                    self.row_dir(),
                    self.line_dir(),
                )
                .unwrap();
            }
            for s in group.sections() {
                self.write_section(s, false);
            }
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
        self.buf.push_str(self.row_dir());
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
                // Mirrored before the mask: a span's start edge is its
                // outline's left.
                let strokes = owner.style.strokes(BASE_PT).mirrored(self.rtl);
                let borders = self.borders(&strokes.masked(mc.outline));
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
                writeln!(self.buf, "\\pard\\intbl{}\\plain\\cell", self.par_dir()).unwrap();
                continue;
            };
            let mut text = self.content(&cell.content);
            // Aligned on a character: the padding as figure and punctuation
            // spaces (RTF can't hide text and keep its room), undecorated.
            if let Some(pad) = cell.align_pad {
                write!(text, "{{\\ulnone\\strike0 {}}}", escape_rtf(&pad.spaces())).unwrap();
            }
            let mut fmt = String::new();
            // `\ql` and `\qr` are physical; a right-to-left paragraph says
            // which it is rather than rely on the default.
            match cell.align {
                HAlign::Right => fmt.push_str("\\qr"),
                HAlign::Center => fmt.push_str("\\qc"),
                HAlign::Justify => fmt.push_str("\\qj"),
                _ if self.rtl => fmt.push_str("\\ql"),
                _ => {}
            }
            if cell.style.bold_over(is_header) {
                fmt.push_str(BOLD);
            }
            if cell.style.is_italic() {
                fmt.push_str(ITALIC);
            }
            if let Some(c) = cell.style.paint() {
                let i = self.color_index(c);
                write!(fmt, "\\cf{i}").unwrap();
            }
            fmt.push_str(&self.font_format(&cell.style));
            fmt.push_str(&text_format(&cell.style));
            // `\pard` resets paragraph formatting only: without `\plain`, the
            // previous cell's bold, italic or color would carry over.
            let dir = self.par_dir();
            writeln!(self.buf, "\\pard\\intbl{dir}\\plain{fmt} {text}\\cell").unwrap();
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
                    "\\pard{}{{\\fs18{fmt} {{\\super {mark}}} {text}}}\\par",
                    self.line_dir()
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

/// A PNG or JPEG embedded as a `data:` URI, as a `\\pict` group drawn at its
/// size (`\\picwgoal`/`\\pichgoal`, twips; `\\picw`/`\\pich` are its pixels), its
/// alt text the picture's `wzDescription` property (as Word writes it), its
/// bytes in hex; or `None` for any other image (RTF has no GIF or SVG picture,
/// and `limits` warns of those).
fn pict(src: &str, alt: Option<&str>, width: Option<&str>, height: Option<&str>) -> Option<String> {
    let image = Image::from_src(src)?;
    let blip = match image.format {
        ImageFormat::Png => "\\pngblip",
        ImageFormat::Jpeg => "\\jpegblip",
        ImageFormat::Gif | ImageFormat::Svg => return None,
    };
    let (w, h) = image.size_pt(width, height, BASE_PT);
    let (pw, ph) = image.natural_px?;
    let mut out = String::with_capacity(image.data.len() * 2 + image.data.len() / 32 + 80);
    out.push_str("{\\pict");
    if let Some(alt) = alt.filter(|a| !a.is_empty()) {
        write!(
            out,
            "{{\\*\\picprop{{\\sp{{\\sn wzDescription}}{{\\sv {}}}}}}}",
            escape_rtf(alt)
        )
        .unwrap();
    }
    write!(
        out,
        "{blip}\\picw{pw}\\pich{ph}\\picwgoal{}\\pichgoal{}",
        (w * 20.0).round(),
        (h * 20.0).round()
    )
    .unwrap();
    for (i, b) in image.data.iter().enumerate() {
        if i % 64 == 0 {
            out.push('\n');
        }
        write!(out, "{b:02x}").unwrap();
    }
    out.push('}');
    Some(out)
}

/// `text` as a link to `href` (a `HYPERLINK` field showing `text`), or as it is if
/// it has none or it isn't a link (see [`gridwell_core::link`]). [`link_url`]
/// leaves no `"`, `\` or brace to escape in the field instruction.
fn linked(text: &str, href: &Option<String>) -> String {
    match href.as_deref().and_then(link_url) {
        Some(url) => {
            format!("{{\\field{{\\*\\fldinst{{HYPERLINK \"{url}\"}}}}{{\\fldrslt{{{text}}}}}}}")
        }
        None => text.to_string(),
    }
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
