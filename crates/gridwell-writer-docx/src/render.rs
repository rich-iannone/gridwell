use gridwell_core::xml::escape as escape_xml;
use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table, VMerge};
use gridwell_layout::{resolve, MergeCell, ResolvedStyle, ResolvedTable, Section};
use gridwell_ooxml::{package, Part};
use std::fmt::Write;
use thiserror::Error;

use crate::xml;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("packaging error: {0}")]
    Package(#[from] gridwell_ooxml::PackageError),
}

/// Render the full .docx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let document_xml = render_document_xml(table)?;

    Ok(package(&[
        Part::new("[Content_Types].xml", xml::CONTENT_TYPES),
        Part::new("_rels/.rels", xml::RELS),
        Part::new("word/_rels/document.xml.rels", xml::DOCUMENT_RELS),
        Part::new("word/document.xml", document_xml.as_str()),
    ])?)
}

/// Render only the document.xml content (for snapshot testing).
pub fn render_document_xml(table: &Table) -> Result<String, RenderError> {
    let rt = resolve(table);
    let mut buf = String::with_capacity(8192);
    write_document_xml(&mut buf, &rt)?;
    Ok(buf)
}

fn write_document_xml(buf: &mut String, rt: &ResolvedTable) -> Result<(), RenderError> {
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    buf.push_str(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    );
    buf.push_str(
        r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
    );
    buf.push('\n');
    buf.push_str("<w:body>\n");

    write_title(buf, rt)?;
    // A table with no grid columns is invalid OOXML: with every column hidden,
    // only the title and notes remain.
    if !rt.is_empty() {
        write_table(buf, rt)?;
    }
    write_footnotes(buf, rt)?;

    buf.push_str("</w:body>\n");
    buf.push_str("</w:document>\n");
    Ok(())
}

/// Run formatting.
#[derive(Debug, Clone, Copy, Default)]
struct Fmt {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<Color>,
    /// Font size in half-points (`w:sz`).
    half_points: Option<u32>,
    superscript: bool,
}

/// The size relative font sizes resolve against: Word's default 11pt.
const BASE_PT: f64 = 11.0;

impl Fmt {
    /// This format with a style's weight, slant, decoration, colour and size
    /// laid over it (a relative size resolves against the size so far).
    fn with(self, style: &ResolvedStyle) -> Self {
        let parent = self.half_points.map_or(BASE_PT, |h| f64::from(h) / 2.0);
        Self {
            bold: self.bold || style.is_bold(),
            italic: self.italic || style.is_italic(),
            underline: self.underline || style.is_underline(),
            strike: self.strike || style.is_strike(),
            color: style.paint().or(self.color),
            half_points: style
                .size_pt(parent)
                .map(|pt| (pt * 2.0).round().clamp(2.0, 3276.0) as u32)
                .or(self.half_points),
            superscript: self.superscript,
        }
    }

    fn is_plain(&self) -> bool {
        !(self.bold
            || self.italic
            || self.underline
            || self.strike
            || self.color.is_some()
            || self.half_points.is_some()
            || self.superscript)
    }
}

fn write_title(buf: &mut String, rt: &ResolvedTable) -> Result<(), RenderError> {
    let header = &rt.header;
    if let Some(title) = &header.title {
        buf.push_str("<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr>");
        let fmt = Fmt {
            bold: true,
            ..Fmt::default()
        };
        write_content(buf, rt, title.content, fmt.with(&title.style))?;
        buf.push_str("</w:p>\n");
    }
    for line in header.subtitle.iter().chain(&header.extra_lines) {
        buf.push_str("<w:p><w:pPr><w:pStyle w:val=\"Subtitle\"/></w:pPr>");
        write_content(buf, rt, line.content, Fmt::default().with(&line.style))?;
        buf.push_str("</w:p>\n");
    }
    Ok(())
}

fn write_table(buf: &mut String, rt: &ResolvedTable) -> Result<(), RenderError> {
    let col_widths = compute_col_widths(rt);

    buf.push_str("<w:tbl>\n");

    buf.push_str("<w:tblPr>");
    buf.push_str("<w:tblStyle w:val=\"TableGrid\"/>");
    let total_width: u32 = col_widths.iter().sum();
    write!(buf, "<w:tblW w:w=\"{total_width}\" w:type=\"dxa\"/>")?;
    buf.push_str("<w:tblBorders>");
    buf.push_str("<w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>");
    buf.push_str("<w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>");
    buf.push_str("<w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>");
    buf.push_str("<w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>");
    buf.push_str("<w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>");
    buf.push_str("<w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>");
    buf.push_str("</w:tblBorders>");
    buf.push_str("<w:tblLook w:val=\"04A0\"/>");
    buf.push_str("</w:tblPr>\n");

    buf.push_str("<w:tblGrid>");
    for w in &col_widths {
        write!(buf, "<w:gridCol w:w=\"{w}\"/>")?;
    }
    buf.push_str("</w:tblGrid>\n");

    write_section(buf, rt, &rt.head, &col_widths, true)?;
    for group in &rt.groups {
        if let Some(label) = &group.label {
            write_group_label_row(buf, rt, label, &col_widths)?;
        }
        write_section(buf, rt, &group.rows, &col_widths, false)?;
        write_section(buf, rt, &group.summary_rows, &col_widths, false)?;
    }

    buf.push_str("</w:tbl>\n");
    Ok(())
}

/// Write one section. Spans are resolved per section, so a `vMerge` range never
/// crosses a section boundary.
fn write_section(
    buf: &mut String,
    rt: &ResolvedTable,
    section: &Section,
    col_widths: &[u32],
    is_header: bool,
) -> Result<(), RenderError> {
    for cells in section.continuation_rows() {
        write_row(buf, rt, &cells, col_widths, is_header)?;
    }
    Ok(())
}

/// Write one `<w:tr>`. Each emitted cell carries its visible column, so `gridSpan`
/// and widths come from the grid, and the `gridSpan`s of a row sum to the number
/// of visible columns.
fn write_row(
    buf: &mut String,
    rt: &ResolvedTable,
    cells: &[MergeCell<'_, '_>],
    col_widths: &[u32],
    is_header: bool,
) -> Result<(), RenderError> {
    buf.push_str("<w:tr>");
    if is_header {
        buf.push_str("<w:trPr><w:tblHeader/></w:trPr>");
    }

    for mc in cells {
        let cell_width: u32 = col_widths[mc.col..mc.col + mc.span].iter().sum();

        buf.push_str("<w:tc>");
        buf.push_str("<w:tcPr>");
        write!(buf, "<w:tcW w:w=\"{cell_width}\" w:type=\"dxa\"/>")?;
        if mc.span > 1 {
            write!(buf, "<w:gridSpan w:val=\"{}\"/>", mc.span)?;
        }
        match mc.vmerge {
            VMerge::Start => buf.push_str("<w:vMerge w:val=\"restart\"/>"),
            VMerge::Continue => buf.push_str("<w:vMerge/>"),
            VMerge::None => {}
        }

        let Some(cell) = mc.cell else {
            // Continuation (or empty position): properties only, empty paragraph.
            buf.push_str("</w:tcPr><w:p/></w:tc>");
            continue;
        };

        if let Some(fill) = cell.style.fill() {
            write!(
                buf,
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>",
                fill.flatten().to_rrggbb()
            )?;
        }
        buf.push_str("</w:tcPr>");

        buf.push_str("<w:p>");
        let jc = match cell.align {
            HAlign::Right => Some("right"),
            HAlign::Center => Some("center"),
            HAlign::Justify => Some("both"),
            _ => None,
        };
        if let Some(jc) = jc {
            write!(buf, "<w:pPr><w:jc w:val=\"{jc}\"/></w:pPr>")?;
        }
        let fmt = Fmt {
            bold: is_header,
            ..Fmt::default()
        };
        write_content(buf, rt, cell.content, fmt.with(&cell.style))?;
        buf.push_str("</w:p>");
        buf.push_str("</w:tc>");
    }

    buf.push_str("</w:tr>\n");
    Ok(())
}

fn write_group_label_row(
    buf: &mut String,
    rt: &ResolvedTable,
    label: &gridwell_layout::Line,
    col_widths: &[u32],
) -> Result<(), RenderError> {
    let total_width: u32 = col_widths.iter().sum();
    buf.push_str("<w:tr>");
    buf.push_str("<w:tc>");
    buf.push_str("<w:tcPr>");
    write!(buf, "<w:tcW w:w=\"{total_width}\" w:type=\"dxa\"/>")?;
    if col_widths.len() > 1 {
        write!(buf, "<w:gridSpan w:val=\"{}\"/>", col_widths.len())?;
    }
    let fill = label
        .style
        .fill()
        .map_or("F0F0F0".into(), |c| c.flatten().to_rrggbb());
    write!(
        buf,
        "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/>"
    )?;
    buf.push_str("</w:tcPr>");
    buf.push_str("<w:p>");
    let fmt = Fmt {
        bold: true,
        ..Fmt::default()
    };
    write_content(buf, rt, label.content, fmt.with(&label.style))?;
    buf.push_str("</w:p>");
    buf.push_str("</w:tc>");
    buf.push_str("</w:tr>\n");
    Ok(())
}

/// Content as runs: styled text gets its own formatting, line breaks become
/// `<w:br/>`, footnote marks are superscript, images show their alt text.
fn write_content(
    buf: &mut String,
    rt: &ResolvedTable,
    nodes: &[ContentNode],
    fmt: Fmt,
) -> Result<(), RenderError> {
    for node in nodes {
        match node {
            ContentNode::Text { value } => write_run(buf, value, fmt)?,
            ContentNode::StyledText { value, style_id } => {
                let style = style_id
                    .as_deref()
                    .map(|id| rt.style(id))
                    .unwrap_or_default();
                write_run(buf, value, fmt.with(&style))?;
            }
            ContentNode::LineBreak {} => buf.push_str("<w:r><w:br/></w:r>"),
            ContentNode::FootnoteMark { mark_text, .. } => {
                let sup = Fmt {
                    superscript: true,
                    ..fmt
                };
                write_run(buf, mark_text, sup)?;
            }
            ContentNode::Image { alt, .. } => {
                if let Some(alt) = alt {
                    write_run(buf, alt, fmt)?;
                }
            }
            ContentNode::Raw { .. } | ContentNode::Unknown => {}
        }
    }
    Ok(())
}

fn write_run(buf: &mut String, text: &str, fmt: Fmt) -> Result<(), RenderError> {
    if text.is_empty() {
        return Ok(());
    }
    buf.push_str("<w:r>");
    if !fmt.is_plain() {
        // CT_RPr is a sequence: b, i, strike, color, sz, szCs, u, vertAlign.
        buf.push_str("<w:rPr>");
        if fmt.bold {
            buf.push_str("<w:b/>");
        }
        if fmt.italic {
            buf.push_str("<w:i/>");
        }
        if fmt.strike {
            buf.push_str("<w:strike/>");
        }
        if let Some(c) = fmt.color {
            write!(buf, "<w:color w:val=\"{}\"/>", c.flatten().to_rrggbb())?;
        }
        if let Some(h) = fmt.half_points {
            write!(buf, "<w:sz w:val=\"{h}\"/><w:szCs w:val=\"{h}\"/>")?;
        }
        if fmt.underline {
            buf.push_str("<w:u w:val=\"single\"/>");
        }
        if fmt.superscript {
            buf.push_str("<w:vertAlign w:val=\"superscript\"/>");
        }
        buf.push_str("</w:rPr>");
    }
    write!(
        buf,
        "<w:t xml:space=\"preserve\">{}</w:t>",
        escape_xml(text)
    )?;
    buf.push_str("</w:r>");
    Ok(())
}

fn write_footnotes(buf: &mut String, rt: &ResolvedTable) -> Result<(), RenderError> {
    let footer = &rt.footer;
    if !footer.footnotes.is_empty() {
        buf.push_str("<w:p/>\n"); // spacer
        for note in &footer.footnotes {
            buf.push_str("<w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr>");
            let fmt = Fmt::default().with(&note.style);
            write_run(
                buf,
                note.mark,
                Fmt {
                    superscript: true,
                    ..fmt
                },
            )?;
            write_run(buf, " ", fmt)?;
            write_content(buf, rt, note.content, fmt)?;
            buf.push_str("</w:p>\n");
        }
    }
    if !footer.source_notes.is_empty() {
        buf.push_str("<w:p/>\n"); // spacer
        for note in &footer.source_notes {
            buf.push_str("<w:p>");
            let fmt = Fmt {
                italic: true,
                ..Fmt::default()
            };
            write_content(buf, rt, note.content, fmt.with(&note.style))?;
            buf.push_str("</w:p>\n");
        }
    }
    Ok(())
}

/// Column widths in dxa: absolute lengths convert exactly; `%`, `fr` and `auto`
/// get the default.
fn compute_col_widths(rt: &ResolvedTable) -> Vec<u32> {
    rt.columns
        .iter()
        .map(|col| {
            col.width
                .as_ref()
                .and_then(|w| w.to_twips(12.0, 12.0))
                .filter(|t| t.is_finite() && *t >= 1.0 && *t <= u32::MAX as f64)
                .map_or(xml::DEFAULT_COL_WIDTH_DXA, |t| t as u32)
        })
        .collect()
}
