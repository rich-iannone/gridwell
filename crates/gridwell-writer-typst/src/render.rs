use gridwell_core::{Color, FontSize, Image, ImageFormat, LanguageTag, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table};
use gridwell_layout::{
    resolve, CellVAlign, ColumnWidth, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable,
    Sides, Stroke,
};
use std::fmt::Write;
use thiserror::Error;

use crate::TypstWriterConfig;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

struct TypstRenderer<'r, 'a> {
    rt: &'r ResolvedTable<'a>,
    config: &'r TypstWriterConfig,
    buf: String,
    indent_level: usize,
}

impl<'r, 'a> TypstRenderer<'r, 'a> {
    fn new(rt: &'r ResolvedTable<'a>, config: &'r TypstWriterConfig) -> Self {
        Self {
            rt,
            config,
            buf: String::with_capacity(4096),
            indent_level: 0,
        }
    }

    fn indent(&self) -> String {
        "  ".repeat(self.indent_level)
    }

    fn line(&mut self, content: &str) {
        let indent = self.indent();
        writeln!(self.buf, "{indent}{content}").unwrap();
    }

    fn push(&mut self) {
        self.indent_level += 1;
    }

    fn pop(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn render(mut self) -> Result<String, RenderError> {
        self.render_title();
        self.render_table();
        self.render_footnotes();
        // The language and direction are set for the table only: in a content
        // block, which scopes the set rule.
        match text_settings(self.rt) {
            Some(set) => Ok(format!("#[\n#set text({set})\n{}]\n", self.buf)),
            None => Ok(self.buf),
        }
    }

    /// Where the heading lines go: the start side when a direction is set (the
    /// right, in a right-to-left table), else the left, as always.
    fn lead(&self) -> &'static str {
        if self.rt.table.config.dir.is_some() {
            "start"
        } else {
            "left"
        }
    }

    // ─── Title / Subtitle / header lines ───

    /// The title (bold, 16pt), then the subtitle and extra header lines (12pt,
    /// gray). A line's style is laid over those defaults.
    fn render_title(&mut self) {
        let header = &self.rt.header;
        if let Some(title) = &header.title {
            let text = self.content(&title.content);
            if !text.is_empty() {
                let styled = line_style(&text, &title.style, true, &[("size", "16pt")]);
                self.line(&format!("#align({})[{styled}]", self.lead()));
            }
        }
        let mut lines = false;
        for line in header.subtitle.iter().chain(&header.extra_lines) {
            let text = self.content(&line.content);
            if !text.is_empty() {
                let styled = line_style(&text, &line.style, false, SUBTITLE);
                self.line(&format!("#align({})[{styled}]", self.lead()));
                lines = true;
            }
        }
        if lines {
            self.line("#v(6pt)");
        }
    }

    // ─── Table ───

    fn render_table(&mut self) {
        let rt = self.rt;
        // With every column hidden there is no table to draw (and `columns: ()` would
        // not compile).
        if rt.is_empty() {
            return;
        }
        // Typst sizes `%` and `fr` columns against the container, so a table with a
        // width is a block that wide.
        let columns: Vec<String> = rt
            .column_widths()
            .iter()
            .map(|w| {
                match w {
                    ColumnWidth::Auto => None,
                    ColumnWidth::Fixed(l) => typst_len(l),
                    ColumnWidth::Percent(p) => typst_len(&Length::Percent(*p)),
                    ColumnWidth::Fr(f) => typst_len(&Length::Fr(*f)),
                }
                .unwrap_or_else(|| "auto".into())
            })
            .collect();
        let aligns: Vec<&str> = rt.columns.iter().map(|c| typst_align(&c.align)).collect();
        let block = rt.table_width().as_ref().and_then(typst_len);
        match &block {
            Some(width) => self.line(&format!("#block(width: {width}, table(")),
            None => self.line("#table("),
        }
        self.push();
        // A trailing comma keeps a one-element array an array.
        self.line(&format!("columns: ({},),", columns.join(", ")));
        self.line(&format!("align: ({},),", aligns.join(", ")));
        self.line("stroke: none,");

        self.render_thead();
        self.render_tbody();

        self.pop();
        self.line(if block.is_some() { "))" } else { ")" });
    }

    fn render_thead(&mut self) {
        let rt = self.rt;
        if rt.head.is_empty() {
            return;
        }
        if self.config.repeat_header {
            self.line("table.header(");
        }
        self.push();
        for (r, row) in rt.head.rows.iter().enumerate() {
            for cell in row.cells() {
                let rule = self.spanner_rule(cell, r);
                self.render_cell(cell, true, rule);
            }
        }
        self.line("table.hline(stroke: 0.5pt),");
        self.pop();
        if self.config.repeat_header {
            self.line("),");
        }
    }

    fn render_tbody(&mut self) {
        let rt = self.rt;
        for (g, group) in rt.groups.iter().enumerate() {
            if let Some(label) = &group.label {
                let text = self.content(&label.content);
                let cols = rt.columns.len();
                self.line("table.hline(stroke: 0.5pt),");
                // Bold on light gray, unless the label's style says otherwise.
                let fill = label.style.fill().map_or("luma(240)".into(), typst_color);
                let stroke = typst_stroke(&label.style.strokes(BASE_PT))
                    .map(|s| format!(", {s}"))
                    .unwrap_or_default();
                let styled = text_style(&text, &label.style, true);
                self.line(&format!(
                    "table.cell(colspan: {cols}, fill: {fill}{stroke})[{styled}],"
                ));
                self.line("table.hline(stroke: 0.5pt),");
            }

            for row in &group.rows.rows {
                self.render_row(row, false);
            }

            if !group.summary_rows.is_empty() {
                self.line("table.hline(stroke: 0.3pt),");
                for row in &group.summary_rows.rows {
                    self.render_row(row, false);
                }
            }

            if g + 1 < rt.groups.len() {
                self.line("table.hline(stroke: 0.3pt),");
            }
        }
    }

    fn render_row(&mut self, row: &ResolvedRow, is_header: bool) {
        // Covered positions are implied by the origins' spans.
        for cell in row.cells() {
            self.render_cell(cell, is_header, false);
        }
    }

    /// Whether a head cell is underlined: a spanner, so one with head rows below it
    /// (as LaTeX's `\cmidrule`), unless it's blank or its style sets (or removes) a
    /// bottom border, which then stands instead. `r` is the cell's head row.
    fn spanner_rule(&self, cell: &ResolvedCell, r: usize) -> bool {
        let image = |n: &ContentNode| matches!(n, ContentNode::Image { .. });
        let text = gridwell_layout::plain_text(&cell.content, " ");
        let blank = text.trim().is_empty() && !cell.content.iter().any(image);
        r + cell.rowspan < self.rt.head.rows.len()
            && !blank
            && cell.style.border.bottom.is_none()
            && !cell.style.border_cleared.bottom
    }

    /// `rule`: underline the cell's content box (so a rule stops short of the cell's
    /// ends by its inset, and neighboring spanners' rules stay apart), on the cell's
    /// bottom edge: the default inset below the content.
    fn render_cell(&mut self, cell: &ResolvedCell, is_header: bool, rule: bool) {
        let mut content = self.content(&cell.content);
        // Aligned on a character: the padding, hidden, in the cell's font.
        if let Some(pad) = cell.align_pad {
            write!(content, "#hide[{}]", escape_typst(&pad.text())).unwrap();
        }
        let mut attrs = Vec::new();
        if cell.colspan > 1 {
            attrs.push(format!("colspan: {}", cell.colspan));
        }
        if cell.rowspan > 1 {
            attrs.push(format!("rowspan: {}", cell.rowspan));
        }
        if let Some(fill) = cell.style.fill() {
            attrs.push(format!("fill: {}", typst_color(fill)));
        }
        attrs.extend(typst_stroke(&cell.style.strokes(BASE_PT)));
        // The column's alignment applies unless the cell's differs (its style, or a
        // centered spanner label). A vertical alignment is written with the
        // horizontal one, so the cell's alignment reads whole.
        let own = typst_align(&cell.align);
        let vertical = cell.style.cell_valign().map(|v| match v {
            CellVAlign::Top => "top",
            CellVAlign::Middle => "horizon",
            CellVAlign::Bottom => "bottom",
        });
        if let Some(v) = vertical {
            attrs.push(format!("align: {own} + {v}"));
        } else if own != typst_align(&self.rt.columns[cell.col].align) {
            attrs.push(format!("align: {own}"));
        }

        let mut styled = text_style(&content, &cell.style, is_header);
        if rule {
            styled.push_str(SPANNER_RULE);
        }
        if attrs.is_empty() {
            self.line(&format!("[{styled}],"));
        } else {
            self.line(&format!("table.cell({})[{styled}],", attrs.join(", ")));
        }
    }

    // ─── Footnotes ───

    /// Footnotes (9pt) and source notes (9pt, gray), each note's style laid over
    /// those defaults.
    fn render_footnotes(&mut self) {
        let footer = &self.rt.footer;
        if !footer.footnotes.is_empty() {
            self.line("");
            // A blank line between notes: consecutive source lines would be
            // joined into one paragraph.
            for (i, note) in footer.footnotes.iter().enumerate() {
                if i > 0 {
                    self.line("");
                }
                let text = self.content(&note.content);
                let mark = escape_typst(note.mark);
                let text = format!("#super[{mark}] {text}");
                let styled = line_style(&text, &note.style, false, &[("size", "9pt")]);
                self.line(&styled);
            }
        }
        if !footer.source_notes.is_empty() {
            self.line("");
            for (i, note) in footer.source_notes.iter().enumerate() {
                if i > 0 {
                    self.line("");
                }
                let text = self.content(&note.content);
                let styled = line_style(&text, &note.style, false, SOURCE_NOTE);
                self.line(&styled);
            }
        }
    }

    // ─── Content ───

    fn content(&self, nodes: &[ContentNode]) -> String {
        let mut out = String::new();
        for node in nodes {
            match node {
                ContentNode::Text { value } => out.push_str(&escape_typst(value)),
                ContentNode::StyledText { value, style_id } => {
                    let style = style_id
                        .as_deref()
                        .map(|id| self.rt.style(id))
                        .unwrap_or_default();
                    out.push_str(&text_style(&escape_typst(value), &style, false));
                }
                ContentNode::LineBreak {} => out.push_str("\\ "),
                ContentNode::FootnoteMark { mark_text, .. } => {
                    write!(out, "#super[{}]", escape_typst(mark_text)).unwrap();
                }
                ContentNode::Image {
                    src,
                    alt,
                    width,
                    height,
                } => match (typst_image(src, alt, width, height), alt) {
                    (Some(image), _) => out.push_str(&image),
                    (None, Some(alt)) => out.push_str(&escape_typst(alt)),
                    (None, None) => out.push_str("[image]"),
                },
                ContentNode::Raw { format, value } => {
                    if format == "typst" {
                        out.push_str(value);
                    }
                }
                ContentNode::Unknown => {}
            }
        }
        out
    }
}

/// Main entry point for rendering.
pub fn render(table: &Table, config: &TypstWriterConfig) -> Result<String, RenderError> {
    let rt = resolve(table);
    TypstRenderer::new(&rt, config).render()
}

/// The `#text` defaults of subtitle and header lines, and of source notes.
const SUBTITLE: &[(&str, &str)] = &[("size", "12pt"), ("fill", "luma(100)")];
const SOURCE_NOTE: &[(&str, &str)] = &[("size", "9pt"), ("fill", "luma(100)")];

/// `content` wrapped in `#text(…)` for the style's weight, slant, size, color and
/// font family, and in `#underline[…]` / `#strike[…]` for its decoration.
/// Header cells are bold unless their style sets another weight.
fn text_style(content: &str, style: &ResolvedStyle, is_header: bool) -> String {
    line_style(content, style, is_header, &[])
}

/// As `text_style`, over `#text` arguments the style replaces where it sets the
/// same one (a title's size, a note's gray).
fn line_style(
    content: &str,
    style: &ResolvedStyle,
    is_header: bool,
    defaults: &[(&str, &str)],
) -> String {
    let inner = text_attrs(content, style, is_header, defaults);
    if content.is_empty() {
        inner
    } else if style.is_underline() {
        format!("#underline[{inner}]")
    } else if style.is_strike() {
        format!("#strike[{inner}]")
    } else {
        inner
    }
}

fn text_attrs(
    content: &str,
    style: &ResolvedStyle,
    is_header: bool,
    defaults: &[(&str, &str)],
) -> String {
    let mut attrs = Vec::new();
    // A weight the style sets is written either way: a run's un-bolds a bold
    // cell's.
    match &style.font_weight {
        Some(_) if !style.is_bold() => attrs.push("weight: \"regular\"".to_string()),
        _ if style.bold_over(is_header) => attrs.push("weight: \"bold\"".to_string()),
        _ => {}
    }
    // Likewise a slant: a run's upright un-italics an italic cell's.
    match &style.font_style {
        Some(_) if style.is_italic() => attrs.push("style: \"italic\"".to_string()),
        Some(_) => attrs.push("style: \"normal\"".to_string()),
        None => {}
    }
    if let Some(size) = style.font_size.as_ref().and_then(typst_font_size) {
        attrs.push(format!("size: {size}"));
    }
    if let Some(c) = style.paint() {
        attrs.push(format!("fill: {}", typst_color(c)));
    }
    // Typst has no generic families: they map to fonts, and Typst takes the
    // first of the list it has.
    match style.font_names().as_slice() {
        [] => {}
        [name] => attrs.push(format!("font: {}", typst_string(name))),
        names => {
            let names: Vec<String> = names.iter().map(|n| typst_string(n)).collect();
            attrs.push(format!("font: ({})", names.join(", ")));
        }
    }
    // Typst rejects a repeated argument: one the style sets replaces the default.
    let set = |k: &str| {
        attrs
            .iter()
            .any(|a| a.split_once(':').is_some_and(|(n, _)| n == k))
    };
    let mut all: Vec<String> = defaults
        .iter()
        .filter(|(k, _)| !set(k))
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    all.extend(attrs);
    let attrs = all;
    if attrs.is_empty() || content.is_empty() {
        content.to_string()
    } else {
        format!("#text({})[{content}]", attrs.join(", "))
    }
}

/// The `text` settings for the table's language and direction, if it has either.
/// Typst takes a 2–3 letter language and a 2-letter region, so other tags give no
/// `lang`. The direction is always explicit: Typst would otherwise take it from
/// the language, which gridwell never does (`auto` is resolved from the text, as
/// HTML does).
fn text_settings(rt: &ResolvedTable) -> Option<String> {
    let config = &rt.table.config;
    if config.lang.is_none() && config.dir.is_none() {
        return None;
    }
    let tag = config
        .lang
        .as_deref()
        .and_then(|l| LanguageTag::parse(l).ok());
    let mut set = Vec::new();
    if let Some(tag) = &tag {
        if let Some(lang) = tag.language.as_ref().filter(|l| l.len() <= 3) {
            set.push(format!("lang: {}", typst_string(lang)));
            if let Some(region) = tag.region.as_ref().filter(|r| r.len() == 2) {
                set.push(format!("region: {}", typst_string(region)));
            }
        }
    }
    set.push(format!("dir: {}", if rt.is_rtl() { "rtl" } else { "ltr" }));
    Some(set.join(", "))
}

/// A Typst string literal.
fn typst_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn typst_align(a: &HAlign) -> &'static str {
    match a {
        HAlign::Right => "right",
        HAlign::Center => "center",
        _ => "left",
    }
}

/// A Typst text size: lengths convert as for widths, but a percentage is relative
/// to the surrounding size (`em`, as in CSS), keywords use their CSS px size, and
/// `smaller`/`larger` scale by 1/1.2 and 1.2.
fn typst_font_size(f: &FontSize) -> Option<String> {
    match f {
        FontSize::Length(Length::Percent(p)) => typst_len(&Length::Em(p / 100.0)),
        FontSize::Length(Length::Auto | Length::Fr(_)) => None,
        FontSize::Length(l) => typst_len(l),
        FontSize::Keyword(_, px) => typst_len(&Length::Px(*px)),
        FontSize::Smaller => typst_len(&Length::Em(1.0 / 1.2)),
        FontSize::Larger => typst_len(&Length::Em(1.2)),
    }
}

/// The size relative sizes resolve against: Typst's default 11pt.
pub(crate) const BASE_PT: f64 = 11.0;

/// A spanner's rule, appended to its content: as wide as the content box, on the
/// cell's bottom edge (Typst's default inset, 5pt, below the content). The table's
/// other rules are 0.5pt too.
const SPANNER_RULE: &str = "#place(bottom + center, dy: 5pt, line(length: 100%, stroke: 0.5pt))";

/// A cell's `stroke:` argument, one entry per side with a border. Typst has no
/// double line: a double border is drawn solid at its full width.
fn typst_stroke(strokes: &Sides<Option<Stroke>>) -> Option<String> {
    fn one(k: &Stroke) -> Option<String> {
        let mut parts = vec![format!(
            "thickness: {}",
            typst_len(&Length::Pt(k.width_pt))?
        )];
        if let Some(c) = k.color {
            parts.push(format!("paint: {}", typst_color(c)));
        }
        match k.style {
            BorderStyle::Dashed => parts.push("dash: \"dashed\"".into()),
            BorderStyle::Dotted => parts.push("dash: \"dotted\"".into()),
            _ => {}
        }
        Some(format!("({})", parts.join(", ")))
    }
    // The same edge all round is one stroke; otherwise one per side.
    let Sides { top, .. } = strokes;
    if top.is_some() && strokes.iter().all(|(_, k)| k == top) {
        return Some(format!("stroke: {}", one(top.as_ref()?)?));
    }
    let sides: Vec<String> = strokes
        .iter()
        .filter_map(|(side, k)| Some(format!("{side}: {}", one(k.as_ref()?)?)))
        .collect();
    (!sides.is_empty()).then(|| format!("stroke: ({})", sides.join(", ")))
}

/// A Typst `rgb("#RRGGBB[AA]")`; Typst colors keep alpha.
fn typst_color(c: Color) -> String {
    format!("rgb(\"{}\")", c.to_hex())
}

/// An image embedded as a `data:` URI, drawn inline (on the baseline, as in
/// HTML) at its size, stretched to it if both sides are set; or `None` if it
/// isn't one (`limits` warns of those). Typst 0.13 made `image` take the bytes
/// that 0.12's `image.decode` does.
fn typst_image(
    src: &str,
    alt: &Option<String>,
    width: &Option<String>,
    height: &Option<String>,
) -> Option<String> {
    let image = Image::from_src(src)?;
    let (w, h) = image.size_pt(width.as_deref(), height.as_deref(), BASE_PT);
    let data = match image.format {
        // An SVG is text (it was sniffed as UTF-8).
        ImageFormat::Svg => typst_string(std::str::from_utf8(&image.data).ok()?),
        _ => {
            let mut bytes = String::with_capacity(image.data.len() * 4 + 2);
            bytes.push('(');
            for b in &image.data {
                write!(bytes, "{b},").unwrap();
            }
            bytes.push(')');
            bytes
        }
    };
    let format = match image.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
    };
    let alt = alt
        .as_deref()
        .map(|a| format!(", alt: {}", typst_string(a)))
        .unwrap_or_default();
    Some(format!(
        "#box((if sys.version >= version(0, 13) {{ image }} else {{ image.decode }})(\
         bytes({data}), format: \"{format}\", width: {}, height: {}, fit: \"stretch\"{alt}));",
        typst_len(&Length::Pt(w))?,
        typst_len(&Length::Pt(h))?
    ))
}

// ─── Escaping ───

/// Escape text for Typst markup so it renders verbatim.
///
/// Every ASCII punctuation character gets a backslash. Typst accepts `\<char>` for
/// any of them, and escaping all of them (not just the obvious `# [ ] $ \\`) also
/// neutralizes emphasis (`*`, `_`), raw (`` ` ``), comments (`//`, `/*`), labels and
/// references (`<`, `>`, `@`), line-start markers (`=`, `-`, `+`, `/ term:`), and
/// shorthands (`--`, `...`, `~`, `-5` → minus sign, smart quotes). Raw newlines
/// become spaces; explicit breaks are `line_break` content nodes.
fn escape_typst(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    for ch in s.chars() {
        match ch {
            '\r' | '\n' => out.push(' '),
            c if c.is_ascii_punctuation() => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// Convert a length to Typst, or `None` if it can't be expressed. Typst has no `px`
/// or `rem`: px → pt at 0.75pt/px (CSS reference pixel), rem → em (Typst has no
/// root font size).
fn typst_len(length: &Length) -> Option<String> {
    fn num(v: f64) -> Option<String> {
        // f64 Display never uses exponent notation for these magnitudes and drops
        // a trailing ".0", which is what Typst wants.
        v.is_finite()
            .then(|| format!("{}", (v * 1e4).round() / 1e4))
    }
    Some(match *length {
        Length::Auto => "auto".to_string(),
        Length::Px(v) => format!("{}pt", num(v * 0.75)?),
        Length::Pt(v) => format!("{}pt", num(v)?),
        Length::Em(v) | Length::Rem(v) => format!("{}em", num(v)?),
        Length::In(v) => format!("{}in", num(v)?),
        Length::Cm(v) => format!("{}cm", num(v)?),
        Length::Mm(v) => format!("{}mm", num(v)?),
        Length::Percent(v) => format!("{}%", num(v)?),
        Length::Fr(v) => format!("{}fr", num(v)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_convert_to_valid_typst() {
        for (input, want) in [
            ("auto", Some("auto")),
            ("120px", Some("90pt")),
            ("15px", Some("11.25pt")),
            ("12pt", Some("12pt")),
            ("1.5em", Some("1.5em")),
            ("2rem", Some("2em")),
            ("1in", Some("1in")),
            ("2.5cm", Some("2.5cm")),
            ("10mm", Some("10mm")),
            ("25%", Some("25%")),
            ("1fr", Some("1fr")),
            ("2fr", Some("2fr")),
            ("", None),
            ("wide", None),
            ("12", None),
            ("NaNpx", None),
            ("infpt", None),
        ] {
            assert_eq!(
                input.parse().ok().as_ref().and_then(typst_len).as_deref(),
                want,
                "input {input:?}"
            );
        }
    }

    #[test]
    fn escape_backslashes_all_ascii_punctuation_only() {
        assert_eq!(escape_typst("a*b"), "a\\*b");
        assert_eq!(escape_typst("-5 // x"), "\\-5 \\/\\/ x");
        assert_eq!(escape_typst("é 東京 😀 abc 123"), "é 東京 😀 abc 123");
        assert_eq!(escape_typst("a\nb\r\nc"), "a b  c");
        for c in (0u8..128)
            .map(char::from)
            .filter(|c| c.is_ascii_punctuation())
        {
            assert_eq!(escape_typst(&c.to_string()), format!("\\{c}"));
        }
    }
}
