use gridwell_ir::content::ContentNode;
use gridwell_ir::{vmerge_layout, FontStyle, FontWeight, MergeCell, Row, Table, VMerge};
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

/// Render the full .docx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let document_xml = render_document_xml(table)?;

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    std::io::Write::write_all(&mut zip, xml::CONTENT_TYPES.as_bytes())?;

    zip.start_file("_rels/.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::RELS.as_bytes())?;

    zip.start_file("word/_rels/document.xml.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::DOCUMENT_RELS.as_bytes())?;

    zip.start_file("word/document.xml", options)?;
    std::io::Write::write_all(&mut zip, document_xml.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

/// Render only the document.xml content (for snapshot testing).
pub fn render_document_xml(table: &Table) -> Result<String, RenderError> {
    let mut buf = String::with_capacity(8192);
    write_document_xml(&mut buf, table)?;
    Ok(buf)
}

fn write_document_xml(buf: &mut String, table: &Table) -> Result<(), RenderError> {
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

    // Title
    write_title(buf, table)?;

    // Table
    write_table(buf, table)?;

    // Footnotes (as paragraphs after table)
    write_footnotes(buf, table)?;

    buf.push_str("</w:body>\n");
    buf.push_str("</w:document>\n");
    Ok(())
}

fn write_title(buf: &mut String, table: &Table) -> Result<(), RenderError> {
    if let Some(ref header) = table.header {
        if let Some(ref title) = header.title {
            let text = content_to_text(&title.content);
            buf.push_str("<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr>");
            write_run(buf, &text, true, false, None)?;
            buf.push_str("</w:p>\n");
        }
        if let Some(ref subtitle) = header.subtitle {
            let text = content_to_text(&subtitle.content);
            buf.push_str("<w:p><w:pPr><w:pStyle w:val=\"Subtitle\"/></w:pPr>");
            write_run(buf, &text, false, false, None)?;
            buf.push_str("</w:p>\n");
        }
    }
    Ok(())
}

fn write_table(buf: &mut String, table: &Table) -> Result<(), RenderError> {
    let col_widths = compute_col_widths(table);
    let table_cols = table.config.table_cols as usize;

    buf.push_str("<w:tbl>\n");

    // Table properties
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

    // Grid definition
    buf.push_str("<w:tblGrid>");
    for w in &col_widths {
        write!(buf, "<w:gridCol w:w=\"{w}\"/>")?;
    }
    buf.push_str("</w:tblGrid>\n");

    // Header rows
    if !table.config.column_labels_hidden {
        write_section(buf, &table.table.thead.rows, &col_widths, table, true)?;
    }

    // Body rows
    for group in &table.table.tbody {
        if let Some(ref label) = group.label {
            write_group_label_row(buf, label, &col_widths, table_cols)?;
        }
        write_section(buf, &group.rows, &col_widths, table, false)?;
        write_section(buf, &group.summary_rows, &col_widths, table, false)?;
    }

    buf.push_str("</w:tbl>\n");
    Ok(())
}

/// Write one section (thead, a group's rows, or its summary rows). Spans are resolved
/// per section, so a `vMerge` range never crosses a section boundary.
fn write_section(
    buf: &mut String,
    rows: &[Row],
    col_widths: &[u32],
    table: &Table,
    is_header: bool,
) -> Result<(), RenderError> {
    for cells in vmerge_layout(rows) {
        write_row(buf, &cells, col_widths, table, is_header)?;
    }
    Ok(())
}

/// Write one `<w:tr>`. Each emitted cell carries its grid column, so `gridSpan` and
/// widths come from the grid, and the `gridSpan`s of a valid row sum to `table_cols`.
fn write_row(
    buf: &mut String,
    cells: &[MergeCell<'_>],
    col_widths: &[u32],
    table: &Table,
    is_header: bool,
) -> Result<(), RenderError> {
    buf.push_str("<w:tr>");
    if is_header {
        buf.push_str("<w:trPr><w:tblHeader/></w:trPr>");
    }

    for mc in cells {
        let start = mc.col.min(col_widths.len());
        let end = (mc.col + mc.span).min(col_widths.len());
        let cell_width: u32 = col_widths[start..end].iter().sum();

        buf.push_str("<w:tc>");
        buf.push_str("<w:tcPr>");
        write!(buf, "<w:tcW w:w=\"{cell_width}\" w:type=\"dxa\"/>")?;

        // Horizontal merge (gridSpan)
        if mc.span > 1 {
            write!(buf, "<w:gridSpan w:val=\"{}\"/>", mc.span)?;
        }

        // Vertical merge
        match mc.vmerge {
            VMerge::Start => buf.push_str("<w:vMerge w:val=\"restart\"/>"),
            VMerge::Continue => buf.push_str("<w:vMerge/>"),
            VMerge::None => {}
        }

        let Some(cell) = mc.cell else {
            // Continuation (or orphan): properties only, empty paragraph.
            buf.push_str("</w:tcPr><w:p/></w:tc>");
            continue;
        };

        // Background color from style
        if let Some(ref style_id) = cell.style_id {
            if let Some(def) = table.styles.defs.get(style_id.as_str()) {
                if let Some(ref bg) = def.background_color {
                    if let Some(color) = xml::hex_to_ooxml_color(bg) {
                        write!(
                            buf,
                            "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{color}\"/>"
                        )?;
                    }
                }
            }
        }

        buf.push_str("</w:tcPr>");

        // Cell content
        let text = content_to_text(&cell.content);
        let is_bold = is_header
            || cell
                .style_id
                .as_ref()
                .and_then(|sid| table.styles.defs.get(sid.as_str()))
                .is_some_and(|def| def.font_weight.as_ref().is_some_and(FontWeight::is_bold));
        let is_italic = cell
            .style_id
            .as_ref()
            .and_then(|sid| table.styles.defs.get(sid.as_str()))
            .is_some_and(|def| def.font_style.as_ref().is_some_and(FontStyle::is_italic));
        let color = cell
            .style_id
            .as_ref()
            .and_then(|sid| table.styles.defs.get(sid.as_str()))
            .and_then(|def| def.color.as_ref())
            .and_then(|c| xml::hex_to_ooxml_color(c));

        buf.push_str("<w:p>");
        if !text.is_empty() {
            write_run(buf, &text, is_bold, is_italic, color.as_deref())?;
        }
        buf.push_str("</w:p>");
        buf.push_str("</w:tc>");
    }

    buf.push_str("</w:tr>\n");
    Ok(())
}

fn write_group_label_row(
    buf: &mut String,
    label: &gridwell_ir::cell::GroupLabel,
    col_widths: &[u32],
    table_cols: usize,
) -> Result<(), RenderError> {
    let text = content_to_text(&label.content);
    let total_width: u32 = col_widths.iter().sum();

    buf.push_str("<w:tr>");
    buf.push_str("<w:tc>");
    buf.push_str("<w:tcPr>");
    write!(buf, "<w:tcW w:w=\"{total_width}\" w:type=\"dxa\"/>")?;
    write!(buf, "<w:gridSpan w:val=\"{table_cols}\"/>")?;
    buf.push_str("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F0F0F0\"/>");
    buf.push_str("</w:tcPr>");
    buf.push_str("<w:p>");
    write_run(buf, &text, true, false, None)?;
    buf.push_str("</w:p>");
    buf.push_str("</w:tc>");
    buf.push_str("</w:tr>\n");
    Ok(())
}

fn write_run(
    buf: &mut String,
    text: &str,
    bold: bool,
    italic: bool,
    color: Option<&str>,
) -> Result<(), RenderError> {
    buf.push_str("<w:r>");
    if bold || italic || color.is_some() {
        buf.push_str("<w:rPr>");
        if bold {
            buf.push_str("<w:b/>");
        }
        if italic {
            buf.push_str("<w:i/>");
        }
        if let Some(c) = color {
            write!(buf, "<w:color w:val=\"{c}\"/>")?;
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

fn write_footnotes(buf: &mut String, table: &Table) -> Result<(), RenderError> {
    if let Some(ref footer) = table.footer {
        if !footer.footnotes.is_empty() {
            buf.push_str("<w:p/>\n"); // spacer
            for note in &footer.footnotes {
                let text = content_to_text(&note.content);
                buf.push_str("<w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr>");
                // Superscript mark
                buf.push_str("<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr>");
                write!(buf, "<w:t>{}</w:t>", escape_xml(&note.mark))?;
                buf.push_str("</w:r>");
                // Space + content
                write_run(buf, &format!(" {text}"), false, false, None)?;
                buf.push_str("</w:p>\n");
            }
        }
        if !footer.source_notes.is_empty() {
            buf.push_str("<w:p/>\n"); // spacer
            for note in &footer.source_notes {
                let text = content_to_text(&note.content);
                buf.push_str("<w:p>");
                write_run(buf, &text, false, true, None)?;
                buf.push_str("</w:p>\n");
            }
        }
    }
    Ok(())
}

fn compute_col_widths(table: &Table) -> Vec<u32> {
    table
        .column_spec
        .iter()
        .map(|col| {
            if col.width == "auto" || col.hidden {
                xml::DEFAULT_COL_WIDTH_DXA
            } else if let Some(px) = col.width.strip_suffix("px") {
                px.parse::<f64>()
                    .map(xml::px_to_dxa)
                    .unwrap_or(xml::DEFAULT_COL_WIDTH_DXA)
            } else {
                xml::DEFAULT_COL_WIDTH_DXA
            }
        })
        .collect()
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
