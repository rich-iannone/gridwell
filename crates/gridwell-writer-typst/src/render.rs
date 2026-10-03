use gridwell_core::{Color, FontSize, Length};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table};
use gridwell_layout::{resolve, ResolvedCell, ResolvedRow, ResolvedStyle, ResolvedTable};
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
        Ok(self.buf)
    }

    // ─── Title / Subtitle ───

    fn render_title(&mut self) {
        let rt = self.rt;
        if let Some(title) = &rt.header.title {
            let text = self.content(title.content);
            if !text.is_empty() {
                self.line(&format!(
                    "#align(left)[#text(size: 16pt, weight: \"bold\")[{text}]]"
                ));
            }
        }
        if let Some(subtitle) = &rt.header.subtitle {
            let text = self.content(subtitle.content);
            if !text.is_empty() {
                self.line(&format!(
                    "#align(left)[#text(size: 12pt, fill: luma(100))[{text}]]"
                ));
                self.line("#v(6pt)");
            }
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
        let columns: Vec<String> = rt
            .columns
            .iter()
            .map(|c| {
                c.width
                    .as_ref()
                    .and_then(typst_len)
                    .unwrap_or_else(|| "auto".into())
            })
            .collect();
        let aligns: Vec<&str> = rt.columns.iter().map(|c| typst_align(&c.align)).collect();
        self.line("#table(");
        self.push();
        // A trailing comma keeps a one-element array an array.
        self.line(&format!("columns: ({},),", columns.join(", ")));
        self.line(&format!("align: ({},),", aligns.join(", ")));
        self.line("stroke: none,");

        self.render_thead();
        self.render_tbody();

        self.pop();
        self.line(")");
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
        for row in &rt.head.rows {
            self.render_row(row, true);
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
                let text = self.content(label.content);
                let cols = rt.columns.len();
                self.line("table.hline(stroke: 0.5pt),");
                self.line(&format!(
                    "table.cell(colspan: {cols}, fill: luma(240))[#text(weight: \"bold\")[{text}]],"
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
            self.render_cell(cell, is_header);
        }
    }

    fn render_cell(&mut self, cell: &ResolvedCell, is_header: bool) {
        let content = self.content(cell.content);
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
        // Only a cell's own alignment; otherwise the column's applies.
        if let Some(a) = &cell.style.text_align {
            attrs.push(format!("align: {}", typst_align(a)));
        }

        let styled = text_style(&content, &cell.style, is_header);
        if attrs.is_empty() {
            self.line(&format!("[{styled}],"));
        } else {
            self.line(&format!("table.cell({})[{styled}],", attrs.join(", ")));
        }
    }

    // ─── Footnotes ───

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
                let text = self.content(note.content);
                let mark = escape_typst(note.mark);
                self.line(&format!("#text(size: 9pt)[#super[{mark}] {text}]"));
            }
        }
        if !footer.source_notes.is_empty() {
            self.line("");
            for (i, note) in footer.source_notes.iter().enumerate() {
                if i > 0 {
                    self.line("");
                }
                let text = self.content(note.content);
                self.line(&format!("#text(size: 9pt, fill: luma(100))[{text}]"));
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
                ContentNode::Image { alt, .. } => match alt {
                    Some(alt) => out.push_str(&escape_typst(alt)),
                    None => out.push_str("[image]"),
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

/// `content` wrapped in `#text(…)` for the style's weight, slant, size, colour and
/// monospace family. Header cells are bold unless their style sets a weight.
fn text_style(content: &str, style: &ResolvedStyle, is_header: bool) -> String {
    let mut attrs = Vec::new();
    let bold = match &style.font_weight {
        Some(w) => w.is_bold(),
        None => is_header,
    };
    if bold {
        attrs.push("weight: \"bold\"".to_string());
    }
    if style.is_italic() {
        attrs.push("style: \"italic\"".to_string());
    }
    if let Some(size) = style.font_size.as_ref().and_then(typst_font_size) {
        attrs.push(format!("size: {size}"));
    }
    if let Some(c) = style.paint() {
        attrs.push(format!("fill: {}", typst_color(c)));
    }
    if style
        .font_family
        .as_deref()
        .is_some_and(|f| f.contains("monospace") || f.contains("Courier"))
    {
        attrs.push("font: \"Courier New\"".to_string());
    }
    if attrs.is_empty() || content.is_empty() {
        content.to_string()
    } else {
        format!("#text({})[{content}]", attrs.join(", "))
    }
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

/// A Typst `rgb("#RRGGBB[AA]")`; Typst colours keep alpha.
fn typst_color(c: Color) -> String {
    format!("rgb(\"{}\")", c.to_hex())
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
