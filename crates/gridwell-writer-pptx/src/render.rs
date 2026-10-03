use gridwell_core::xml::escape as escape_xml;
use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{HAlign, Table};
use gridwell_layout::{resolve, ResolvedStyle, ResolvedTable, Section, Slot};
use std::fmt::Write;
use std::io::Cursor;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::xml;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Render the full .pptx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let slide_xml = render_slide_xml(table)?;

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    std::io::Write::write_all(&mut zip, xml::CONTENT_TYPES.as_bytes())?;

    zip.start_file("_rels/.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::RELS.as_bytes())?;

    zip.start_file("ppt/presentation.xml", options)?;
    std::io::Write::write_all(&mut zip, xml::PRESENTATION.as_bytes())?;

    zip.start_file("ppt/_rels/presentation.xml.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::PRESENTATION_RELS.as_bytes())?;

    zip.start_file("ppt/slides/slide1.xml", options)?;
    std::io::Write::write_all(&mut zip, slide_xml.as_bytes())?;

    zip.start_file("ppt/slides/_rels/slide1.xml.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::SLIDE_RELS.as_bytes())?;

    zip.start_file("ppt/slideLayouts/slideLayout1.xml", options)?;
    std::io::Write::write_all(&mut zip, xml::SLIDE_LAYOUT.as_bytes())?;

    zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::SLIDE_LAYOUT_RELS.as_bytes())?;

    zip.start_file("ppt/slideMasters/slideMaster1.xml", options)?;
    std::io::Write::write_all(&mut zip, xml::SLIDE_MASTER.as_bytes())?;

    zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::SLIDE_MASTER_RELS.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
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

    // A table with no grid columns is invalid DrawingML: with every column hidden
    // the slide is empty.
    if !rt.is_empty() {
        write_table_frame(buf, rt)?;
    }

    buf.push_str("</p:spTree></p:cSld>\n");
    buf.push_str("<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>\n");
    buf.push_str("</p:sld>\n");
    Ok(())
}

fn write_table_frame(buf: &mut String, rt: &ResolvedTable) -> Result<(), RenderError> {
    let col_widths = compute_col_widths(rt);
    let total_width: u32 = col_widths.iter().sum();
    let total_rows = rt.head.rows.len()
        + rt.groups
            .iter()
            .map(|g| usize::from(g.label.is_some()) + g.rows.rows.len() + g.summary_rows.rows.len())
            .sum::<usize>();
    let total_height = (total_rows as u32).saturating_mul(xml::DEFAULT_ROW_HEIGHT_EMU);

    // Centred on the 4:3 slide (9144000 × 6858000 EMU).
    let offset_x = 9144000u32.saturating_sub(total_width) / 2;
    let offset_y = 6858000u32.saturating_sub(total_height) / 2;

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
    color: Option<Color>,
    superscript: bool,
}

impl Fmt {
    fn with(self, style: &ResolvedStyle) -> Self {
        Self {
            bold: self.bold || style.is_bold(),
            italic: self.italic || style.is_italic(),
            color: style.paint().or(self.color),
            superscript: self.superscript,
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
