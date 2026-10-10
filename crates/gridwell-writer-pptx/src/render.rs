use gridwell_core::xml::escape as escape_xml;
use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table};
use gridwell_layout::{resolve, ResolvedStyle, ResolvedTable, Section, Slot};
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

/// Render the full .pptx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let slide_xml = render_slide_xml(table)?;

    Ok(package(&[
        Part::new("[Content_Types].xml", xml::CONTENT_TYPES),
        Part::new("_rels/.rels", xml::RELS),
        Part::new("ppt/presentation.xml", xml::PRESENTATION),
        Part::new("ppt/_rels/presentation.xml.rels", xml::PRESENTATION_RELS),
        Part::new("ppt/slides/slide1.xml", slide_xml.as_str()),
        Part::new("ppt/slides/_rels/slide1.xml.rels", xml::SLIDE_RELS),
        Part::new("ppt/slideLayouts/slideLayout1.xml", xml::SLIDE_LAYOUT),
        Part::new(
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            xml::SLIDE_LAYOUT_RELS,
        ),
        Part::new("ppt/slideMasters/slideMaster1.xml", xml::SLIDE_MASTER),
        Part::new(
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            xml::SLIDE_MASTER_RELS,
        ),
    ])?)
}

/// Render only the slide XML content (for snapshot testing).
pub fn render_slide_xml(table: &Table) -> Result<String, RenderError> {
    let rt = resolve(table);
    let mut buf = String::with_capacity(8192);
    write_slide_xml(&mut buf, &rt)?;
    Ok(buf)
}

/// The slide: a `<p:sld>` whose shape tree holds the table's graphic frame.
fn write_slide_xml(buf: &mut String, rt: &ResolvedTable) -> Result<(), RenderError> {
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    buf.push_str(r#"<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#);
    buf.push_str(r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" "#);
    buf.push_str(
        r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
    );
    buf.push('\n');
    buf.push_str("<p:cSld><p:spTree>\n");
    buf.push_str(
        "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>",
    );
    buf.push_str("<p:grpSpPr/>\n");

    // Title lines above the table and notes below it, stacked as one block
    // centred vertically on the 4:3 slide (9144000 × 6858000 EMU).
    let header: Vec<(&gridwell_layout::Line, Fmt, u32)> = {
        let h = &rt.header;
        let title = h.title.iter().map(|l| {
            let f = Fmt {
                bold: true,
                size: Some(2400),
                ..Fmt::default()
            };
            (l, f, TITLE_LINE_EMU)
        });
        let rest = h.subtitle.iter().chain(&h.extra_lines).map(|l| {
            let f = Fmt {
                size: Some(1400),
                ..Fmt::default()
            };
            (l, f, LINE_EMU)
        });
        title.chain(rest).collect()
    };
    let note_count = rt.footer.footnotes.len() + rt.footer.source_notes.len();
    let header_h: u32 = header.iter().map(|(_, _, h)| h).sum();
    let notes_h = note_count as u32 * NOTE_LINE_EMU;
    // A table with no grid columns is invalid DrawingML: with every column hidden
    // only the text boxes remain.
    let (table_w, table_h) = if rt.is_empty() {
        (0, 0)
    } else {
        table_size(rt)
    };
    let gap = |a: u32, b: u32| if a > 0 && b > 0 { GAP_EMU } else { 0 };
    let total =
        header_h + gap(header_h, table_h) + table_h + gap(table_h + header_h, notes_h) + notes_h;
    let mut y = (SLIDE_H.saturating_sub(total) / 2).max(MARGIN_EMU);

    if !header.is_empty() {
        write_text_box(buf, 3, "Title", y, header_h, |buf| {
            for (line, fmt, _) in &header {
                write_paragraph(buf, rt, line.content, fmt.with(&line.style))?;
            }
            Ok(())
        })?;
        y += header_h + gap(header_h, table_h + notes_h);
    }
    if !rt.is_empty() {
        let x = SLIDE_W.saturating_sub(table_w) / 2;
        write_table_frame(buf, rt, x, y)?;
        y += table_h + gap(table_h, notes_h);
    }
    if note_count > 0 {
        write_text_box(buf, 4, "Notes", y, notes_h, |buf| {
            let base = Fmt {
                size: Some(1000),
                ..Fmt::default()
            };
            for n in &rt.footer.footnotes {
                let fmt = base.with(&n.style);
                buf.push_str("<a:p>");
                write_run(
                    buf,
                    n.mark,
                    Fmt {
                        superscript: true,
                        ..fmt
                    },
                )?;
                write_run(buf, " ", fmt)?;
                write_content(buf, rt, n.content, fmt)?;
                buf.push_str("</a:p>");
            }
            for n in &rt.footer.source_notes {
                write_paragraph(buf, rt, n.content, base.with(&n.style))?;
            }
            Ok(())
        })?;
    }

    buf.push_str("</p:spTree></p:cSld>\n");
    buf.push_str("<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>\n");
    buf.push_str("</p:sld>\n");
    Ok(())
}

const SLIDE_W: u32 = 9_144_000;
const SLIDE_H: u32 = 6_858_000;
/// Space above the first shape when the content is taller than the slide.
const MARGIN_EMU: u32 = 228_600;
/// Space between the title box, the table and the notes box.
const GAP_EMU: u32 = 91_440;
const TITLE_LINE_EMU: u32 = 457_200;
const LINE_EMU: u32 = 274_320;
const NOTE_LINE_EMU: u32 = 228_600;
/// Text boxes span the slide less half-inch margins.
const TEXT_X: u32 = 457_200;
const TEXT_W: u32 = 8_229_600;

/// The table's width and height in EMU.
fn table_size(rt: &ResolvedTable) -> (u32, u32) {
    let width: u32 = compute_col_widths(rt).iter().sum();
    let rows = rt.head.rows.len()
        + rt.groups
            .iter()
            .map(|g| usize::from(g.label.is_some()) + g.rows.rows.len() + g.summary_rows.rows.len())
            .sum::<usize>();
    (
        width,
        (rows as u32).saturating_mul(xml::DEFAULT_ROW_HEIGHT_EMU),
    )
}

/// A text box shape spanning the slide's text width.
fn write_text_box(
    buf: &mut String,
    id: u32,
    name: &str,
    y: u32,
    height: u32,
    body: impl FnOnce(&mut String) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    write!(
        buf,
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"{TEXT_X}\" y=\"{y}\"/><a:ext cx=\"{TEXT_W}\" cy=\"{height}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr>\
         <p:txBody><a:bodyPr wrap=\"square\"/><a:lstStyle/>"
    )?;
    body(buf)?;
    buf.push_str("</p:txBody></p:sp>\n");
    Ok(())
}

/// One `<a:p>` of content (an empty paragraph if the content is empty).
fn write_paragraph(
    buf: &mut String,
    rt: &ResolvedTable,
    nodes: &[ContentNode],
    fmt: Fmt,
) -> Result<(), RenderError> {
    buf.push_str("<a:p>");
    if !write_content(buf, rt, nodes, fmt)? {
        buf.push_str("<a:endParaRPr/>");
    }
    buf.push_str("</a:p>");
    Ok(())
}

fn write_table_frame(
    buf: &mut String,
    rt: &ResolvedTable,
    offset_x: u32,
    offset_y: u32,
) -> Result<(), RenderError> {
    let col_widths = compute_col_widths(rt);
    let (total_width, total_height) = table_size(rt);

    buf.push_str("<p:graphicFrame>\n");
    buf.push_str("<p:nvGraphicFramePr><p:cNvPr id=\"2\" name=\"Table\"/>");
    buf.push_str("<p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr>");
    buf.push_str("<p:nvPr/></p:nvGraphicFramePr>\n");
    writeln!(
        buf,
        "<p:xfrm><a:off x=\"{offset_x}\" y=\"{offset_y}\"/>\
         <a:ext cx=\"{total_width}\" cy=\"{total_height}\"/></p:xfrm>"
    )?;
    buf.push_str("<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">\n");
    buf.push_str("<a:tbl>\n");
    buf.push_str("<a:tblPr firstRow=\"1\" bandRow=\"1\"/>\n");

    buf.push_str("<a:tblGrid>");
    for w in &col_widths {
        write!(buf, "<a:gridCol w=\"{w}\"/>")?;
    }
    buf.push_str("</a:tblGrid>\n");

    write_section(buf, rt, &rt.head, true)?;
    for group in &rt.groups {
        if let Some(label) = &group.label {
            write_group_label_row(buf, rt, label, col_widths.len())?;
        }
        write_section(buf, rt, &group.rows, false)?;
        write_section(buf, rt, &group.summary_rows, true)?;
    }

    buf.push_str("</a:tbl>\n");
    buf.push_str("</a:graphicData></a:graphic>\n");
    buf.push_str("</p:graphicFrame>\n");
    Ok(())
}

/// Write one section. Spans are resolved per section, so a merge never crosses a
/// section boundary.
fn write_section(
    buf: &mut String,
    rt: &ResolvedTable,
    section: &Section,
    strong: bool,
) -> Result<(), RenderError> {
    for row in &section.rows {
        write!(buf, "<a:tr h=\"{}\">", xml::DEFAULT_ROW_HEIGHT_EMU)?;
        for (col, slot) in row.slots.iter().enumerate() {
            write_slot(buf, rt, col, slot, strong)?;
        }
        buf.push_str("</a:tr>\n");
    }
    Ok(())
}

/// One `<a:tc>`. DrawingML needs a cell for every grid position: origins carry
/// `gridSpan`/`rowSpan`; covered positions are empty cells marked `hMerge`
/// (covered from the left) and/or `vMerge` (covered from above).
fn write_slot(
    buf: &mut String,
    rt: &ResolvedTable,
    col: usize,
    slot: &Slot,
    strong: bool,
) -> Result<(), RenderError> {
    let cell = match slot {
        Slot::Origin(cell) => cell,
        Slot::CoveredH { .. } => {
            write_merged_cell(buf, true, false);
            return Ok(());
        }
        Slot::CoveredV { origin_col, .. } => {
            write_merged_cell(buf, col != *origin_col, true);
            return Ok(());
        }
        // Invalid IR only: an empty, unmerged cell keeps the grid intact.
        Slot::Empty => {
            write_merged_cell(buf, false, false);
            return Ok(());
        }
    };

    buf.push_str("<a:tc");
    if cell.colspan > 1 {
        write!(buf, " gridSpan=\"{}\"", cell.colspan)?;
    }
    if cell.rowspan > 1 {
        write!(buf, " rowSpan=\"{}\"", cell.rowspan)?;
    }
    buf.push('>');

    buf.push_str("<a:txBody><a:bodyPr/><a:lstStyle/><a:p>");
    let algn = match cell.align {
        HAlign::Right => Some("r"),
        HAlign::Center => Some("ctr"),
        HAlign::Justify => Some("just"),
        _ => None,
    };
    if let Some(algn) = algn {
        write!(buf, "<a:pPr algn=\"{algn}\"/>")?;
    }
    let fmt = Fmt {
        bold: strong,
        ..Fmt::default()
    }
    .with(&cell.style);
    let wrote = write_content(buf, rt, cell.content, fmt)?;
    if !wrote {
        buf.push_str("<a:endParaRPr/>");
    }
    buf.push_str("</a:p></a:txBody>");

    buf.push_str("<a:tcPr>");
    if let Some(fill) = cell.style.fill() {
        write!(
            buf,
            "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
            fill.flatten().to_rrggbb()
        )?;
    }
    buf.push_str("</a:tcPr>");
    buf.push_str("</a:tc>");
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
    superscript: bool,
    /// Font size in hundredths of a point (`None`: inherited).
    size: Option<u32>,
}

impl Fmt {
    /// This format with a style laid over it. A relative size resolves against
    /// the size so far, or PowerPoint's 18pt table text.
    fn with(self, style: &ResolvedStyle) -> Self {
        let parent = self.size.map_or(18.0, |s| f64::from(s) / 100.0);
        Self {
            bold: self.bold || style.is_bold(),
            italic: self.italic || style.is_italic(),
            underline: self.underline || style.is_underline(),
            strike: self.strike || style.is_strike(),
            color: style.paint().or(self.color),
            size: style
                .size_pt(parent)
                .map(|pt| (pt * 100.0).round().clamp(100.0, 400_000.0) as u32)
                .or(self.size),
            ..self
        }
    }
}

/// Content as runs. Returns whether anything was written.
fn write_content(
    buf: &mut String,
    rt: &ResolvedTable,
    nodes: &[ContentNode],
    fmt: Fmt,
) -> Result<bool, RenderError> {
    let mut wrote = false;
    for node in nodes {
        wrote |= match node {
            ContentNode::Text { value } => write_run(buf, value, fmt)?,
            ContentNode::StyledText { value, style_id } => {
                let style = style_id
                    .as_deref()
                    .map(|id| rt.style(id))
                    .unwrap_or_default();
                write_run(buf, value, fmt.with(&style))?
            }
            ContentNode::LineBreak {} => {
                buf.push_str("<a:br/>");
                true
            }
            ContentNode::FootnoteMark { mark_text, .. } => write_run(
                buf,
                mark_text,
                Fmt {
                    superscript: true,
                    ..fmt
                },
            )?,
            ContentNode::Image { alt, .. } => match alt {
                Some(alt) => write_run(buf, alt, fmt)?,
                None => false,
            },
            ContentNode::Raw { .. } | ContentNode::Unknown => false,
        };
    }
    Ok(wrote)
}

fn write_run(buf: &mut String, text: &str, fmt: Fmt) -> Result<bool, RenderError> {
    if text.is_empty() {
        return Ok(false);
    }
    buf.push_str("<a:r><a:rPr lang=\"en-US\"");
    if fmt.bold {
        buf.push_str(" b=\"1\"");
    }
    if fmt.italic {
        buf.push_str(" i=\"1\"");
    }
    if fmt.underline {
        buf.push_str(" u=\"sng\"");
    }
    if fmt.strike {
        buf.push_str(" strike=\"sngStrike\"");
    }
    if let Some(sz) = fmt.size {
        write!(buf, " sz=\"{sz}\"")?;
    }
    if fmt.superscript {
        buf.push_str(" baseline=\"30000\"");
    }
    match fmt.color {
        Some(c) => write!(
            buf,
            "><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:rPr>",
            c.flatten().to_rrggbb()
        )?,
        None => buf.push_str("/>"),
    }
    write!(buf, "<a:t>{}</a:t></a:r>", escape_xml(text))?;
    Ok(true)
}

/// An empty `<a:tc>` for a position covered by another cell's span.
fn write_merged_cell(buf: &mut String, h_merge: bool, v_merge: bool) {
    buf.push_str("<a:tc");
    if h_merge {
        buf.push_str(" hMerge=\"1\"");
    }
    if v_merge {
        buf.push_str(" vMerge=\"1\"");
    }
    buf.push_str("><a:txBody><a:bodyPr/><a:lstStyle/>");
    buf.push_str("<a:p><a:endParaRPr/></a:p></a:txBody><a:tcPr/></a:tc>");
}

fn write_group_label_row(
    buf: &mut String,
    rt: &ResolvedTable,
    label: &gridwell_layout::Line,
    num_cols: usize,
) -> Result<(), RenderError> {
    write!(buf, "<a:tr h=\"{}\">", xml::DEFAULT_ROW_HEIGHT_EMU)?;

    // One cell spanning every column, then `hMerge` placeholders.
    buf.push_str("<a:tc");
    if num_cols > 1 {
        write!(buf, " gridSpan=\"{num_cols}\"")?;
    }
    buf.push('>');
    buf.push_str("<a:txBody><a:bodyPr/><a:lstStyle/><a:p>");
    let fmt = Fmt {
        bold: true,
        ..Fmt::default()
    }
    .with(&label.style);
    if !write_content(buf, rt, label.content, fmt)? {
        buf.push_str("<a:endParaRPr/>");
    }
    buf.push_str("</a:p></a:txBody>");
    let fill = label
        .style
        .fill()
        .map_or("F0F0F0".into(), |c| c.flatten().to_rrggbb());
    write!(
        buf,
        "<a:tcPr><a:solidFill><a:srgbClr val=\"{fill}\"/></a:solidFill></a:tcPr>"
    )?;
    buf.push_str("</a:tc>");
    for _ in 1..num_cols {
        write_merged_cell(buf, true, false);
    }

    buf.push_str("</a:tr>\n");
    Ok(())
}

/// Column widths in EMU: absolute lengths convert exactly; `%`, `fr` and `auto`
/// get the default.
fn compute_col_widths(rt: &ResolvedTable) -> Vec<u32> {
    rt.columns
        .iter()
        .map(|col| {
            col.width
                .as_ref()
                .and_then(|w| w.to_emu(12.0, 12.0))
                .filter(|e| e.is_finite() && *e >= 1.0 && *e <= u32::MAX as f64)
                .map_or(xml::DEFAULT_COL_WIDTH_EMU, |e| e as u32)
        })
        .collect()
}
