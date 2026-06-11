use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table};
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
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

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
    let mut buf = String::with_capacity(8192);
    write_slide_xml(&mut buf, table)?;
    Ok(buf)
}

fn write_slide_xml(buf: &mut String, table: &Table) -> Result<(), RenderError> {
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    buf.push_str(r#"<p:sp xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#);
    buf.push_str(r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" "#);
    buf.push_str(
        r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
    );
    buf.push('\n');

    // Shape properties (position and size on slide)
    let col_widths = compute_col_widths(table);
    let total_width: u32 = col_widths.iter().sum();
    let total_rows = count_total_rows(table);
    let total_height = total_rows as u32 * xml::DEFAULT_ROW_HEIGHT_EMU;

    // Center on slide (9144000 x 6858000 EMU)
    let offset_x = (9144000u32.saturating_sub(total_width)) / 2;
    let offset_y = (6858000u32.saturating_sub(total_height)) / 2;

    buf.push_str("<p:nvSpPr><p:cNvPr id=\"2\" name=\"Table\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>\n");
    writeln!(
        buf,
        "<p:spPr><a:xfrm><a:off x=\"{offset_x}\" y=\"{offset_y}\"/>\
         <a:ext cx=\"{total_width}\" cy=\"{total_height}\"/></a:xfrm></p:spPr>"
    )?;

    // Table (DrawingML)
    buf.push_str("<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr/></a:p></p:txBody>\n");

    // The actual table goes in a graphicFrame; for simplicity we emit it as a separate element
    buf.push_str("</p:sp>\n");

    // Now emit the graphic frame with the table
    buf.push_str("<p:graphicFrame>\n");
    buf.push_str("<p:nvGraphicFramePr><p:cNvPr id=\"3\" name=\"Table\"/>");
    buf.push_str("<p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr>");
    buf.push_str("<p:nvPr/></p:nvGraphicFramePr>\n");
    writeln!(
        buf,
        "<p:xfrm><a:off x=\"{offset_x}\" y=\"{offset_y}\"/>\
         <a:ext cx=\"{total_width}\" cy=\"{total_height}\"/></p:xfrm>"
    )?;

    buf.push_str("<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\">\n");

    // Table element
    buf.push_str("<a:tbl>\n");
    buf.push_str("<a:tblPr firstRow=\"1\" bandRow=\"1\"/>\n");

    // Grid columns
    buf.push_str("<a:tblGrid>");
    for w in &col_widths {
        write!(buf, "<a:gridCol w=\"{w}\"/>")?;
    }
    buf.push_str("</a:tblGrid>\n");

    // Header rows
    if !table.config.column_labels_hidden {
        for row in &table.table.thead.rows {
            write_table_row(buf, row, &col_widths, table, true)?;
        }
    }

    // Body rows
    for group in &table.table.tbody {
        if let Some(ref label) = group.label {
            write_group_label_row(buf, label, &col_widths)?;
        }
        for row in &group.rows {
            write_table_row(buf, row, &col_widths, table, false)?;
        }
        for row in &group.summary_rows {
            write_table_row(buf, row, &col_widths, table, true)?;
        }
    }

    buf.push_str("</a:tbl>\n");
    buf.push_str("</a:graphicData></a:graphic>\n");
    buf.push_str("</p:graphicFrame>\n");

    Ok(())
}

fn write_table_row(
    buf: &mut String,
    row: &Row,
    col_widths: &[u32],
    table: &Table,
    is_header: bool,
) -> Result<(), RenderError> {
    write!(buf, "<a:tr h=\"{}\">", xml::DEFAULT_ROW_HEIGHT_EMU)?;

    let mut col_idx = 0;
    for cell in &row.cells {
        if col_idx >= col_widths.len() {
            break;
        }
        if cell.is_placeholder {
            // Merged continuation
            buf.push_str("<a:tc hMerge=\"1\"><a:txBody><a:bodyPr/><a:lstStyle/>");
            buf.push_str("<a:p><a:endParaRPr/></a:p></a:txBody><a:tcPr/></a:tc>");
            col_idx += 1;
            continue;
        }

        let colspan = cell.colspan as usize;
        let text = content_to_text(&cell.content);

        buf.push_str("<a:tc");
        if colspan > 1 {
            write!(buf, " gridSpan=\"{colspan}\"")?;
        }
        if cell.rowspan > 1 {
            write!(buf, " rowSpan=\"{}\"", cell.rowspan)?;
        }
        buf.push('>');

        // Text body
        buf.push_str("<a:txBody><a:bodyPr/><a:lstStyle/>");
        buf.push_str("<a:p>");
        if !text.is_empty() {
            buf.push_str("<a:r>");
            buf.push_str("<a:rPr lang=\"en-US\"");
            if is_header {
                buf.push_str(" b=\"1\"");
            } else if let Some(ref style_id) = cell.style_id {
                if let Some(def) = table.styles.defs.get(style_id.as_str()) {
                    if def.font_weight.as_deref() == Some("bold") {
                        buf.push_str(" b=\"1\"");
                    }
                    if def.font_style.as_deref() == Some("italic") {
                        buf.push_str(" i=\"1\"");
                    }
                }
            }
            buf.push_str("/>");
            write!(buf, "<a:t>{}</a:t>", escape_xml(&text))?;
            buf.push_str("</a:r>");
        } else {
            buf.push_str("<a:endParaRPr/>");
        }
        buf.push_str("</a:p>");
        buf.push_str("</a:txBody>");

        // Cell properties
        buf.push_str("<a:tcPr>");
        if let Some(ref style_id) = cell.style_id {
            if let Some(def) = table.styles.defs.get(style_id.as_str()) {
                if let Some(ref bg) = def.background_color {
                    if let Some(color) = hex_to_drawingml(bg) {
                        write!(buf, "<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>")?;
                    }
                }
            }
        }
        buf.push_str("</a:tcPr>");
        buf.push_str("</a:tc>");

        col_idx += colspan;
    }

    buf.push_str("</a:tr>\n");
    Ok(())
}

fn write_group_label_row(
    buf: &mut String,
    label: &gridwell_ir::cell::GroupLabel,
    col_widths: &[u32],
) -> Result<(), RenderError> {
    let text = content_to_text(&label.content);
    let num_cols = col_widths.len();

    write!(buf, "<a:tr h=\"{}\">", xml::DEFAULT_ROW_HEIGHT_EMU)?;

    // Single merged cell spanning all columns
    buf.push_str("<a:tc");
    if num_cols > 1 {
        write!(buf, " gridSpan=\"{num_cols}\"")?;
    }
    buf.push('>');
    buf.push_str("<a:txBody><a:bodyPr/><a:lstStyle/><a:p>");
    buf.push_str("<a:r><a:rPr lang=\"en-US\" b=\"1\"/>");
    write!(buf, "<a:t>{}</a:t>", escape_xml(&text))?;
    buf.push_str("</a:r></a:p></a:txBody>");
    buf.push_str("<a:tcPr><a:solidFill><a:srgbClr val=\"F0F0F0\"/></a:solidFill></a:tcPr>");
    buf.push_str("</a:tc>");

    // Placeholder merged cells
    for _ in 1..num_cols {
        buf.push_str("<a:tc hMerge=\"1\"><a:txBody><a:bodyPr/><a:lstStyle/>");
        buf.push_str("<a:p><a:endParaRPr/></a:p></a:txBody><a:tcPr/></a:tc>");
    }

    buf.push_str("</a:tr>\n");
    Ok(())
}

fn compute_col_widths(table: &Table) -> Vec<u32> {
    table
        .column_spec
        .iter()
        .map(|col| {
            if col.width == "auto" || col.hidden {
                xml::DEFAULT_COL_WIDTH_EMU
            } else if let Some(px) = col.width.strip_suffix("px") {
                px.parse::<f64>()
                    .map(xml::px_to_emu)
                    .unwrap_or(xml::DEFAULT_COL_WIDTH_EMU)
            } else {
                xml::DEFAULT_COL_WIDTH_EMU
            }
        })
        .collect()
}

fn count_total_rows(table: &Table) -> usize {
    let mut count = 0;
    if !table.config.column_labels_hidden {
        count += table.table.thead.rows.len();
    }
    for group in &table.table.tbody {
        if group.label.is_some() {
            count += 1;
        }
        count += group.rows.len();
        count += group.summary_rows.len();
    }
    count
}

fn content_to_text(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => out.push_str(value),
            ContentNode::StyledText { value, .. } => out.push_str(value),
            ContentNode::LineBreak {} => out.push(' '),
            ContentNode::FootnoteMark { mark_text, .. } => out.push_str(mark_text),
            ContentNode::Image { alt, .. } => {
                if let Some(alt_text) = alt {
                    out.push_str(alt_text);
                }
            }
            ContentNode::Raw { .. } | ContentNode::Unknown => {}
        }
    }
    out
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn hex_to_drawingml(hex: &str) -> Option<String> {
    let h = hex.strip_prefix('#')?;
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(h.to_uppercase())
    } else {
        None
    }
}
