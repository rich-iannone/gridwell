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

/// Render the full .xlsx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let sheet_xml = render_sheet_xml(table)?;
    let shared_strings_xml = render_shared_strings(table)?;
    let merge_cells_xml = render_merge_cells(table);

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    std::io::Write::write_all(&mut zip, xml::CONTENT_TYPES.as_bytes())?;

    zip.start_file("_rels/.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::RELS.as_bytes())?;

    zip.start_file("xl/_rels/workbook.xml.rels", options)?;
    std::io::Write::write_all(&mut zip, xml::WORKBOOK_RELS.as_bytes())?;

    zip.start_file("xl/workbook.xml", options)?;
    std::io::Write::write_all(&mut zip, xml::WORKBOOK.as_bytes())?;

    zip.start_file("xl/styles.xml", options)?;
    std::io::Write::write_all(&mut zip, xml::STYLES.as_bytes())?;

    zip.start_file("xl/sharedStrings.xml", options)?;
    std::io::Write::write_all(&mut zip, shared_strings_xml.as_bytes())?;

    // Combine sheet XML with merge cells
    let full_sheet = if merge_cells_xml.is_empty() {
        sheet_xml
    } else {
        sheet_xml.replace("</worksheet>", &format!("{merge_cells_xml}</worksheet>"))
    };

    zip.start_file("xl/worksheets/sheet1.xml", options)?;
    std::io::Write::write_all(&mut zip, full_sheet.as_bytes())?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

/// Render only the sheet XML content (for snapshot testing).
pub fn render_sheet_xml(table: &Table) -> Result<String, RenderError> {
    let mut buf = String::with_capacity(8192);
    write_sheet_xml(&mut buf, table)?;
    Ok(buf)
}

fn write_sheet_xml(buf: &mut String, table: &Table) -> Result<(), RenderError> {
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    buf.push_str(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#);
    buf.push('\n');

    // Column widths
    let col_count = table.config.table_cols as usize;
    buf.push_str("<cols>");
    for (i, col) in table.column_spec.iter().enumerate() {
        let width = if col.width == "auto" || col.hidden {
            12.0
        } else if let Some(px) = col.width.strip_suffix("px") {
            px.parse::<f64>().unwrap_or(80.0) / 7.0 // Excel char width approximation
        } else {
            12.0
        };
        let col_num = i + 1;
        write!(
            buf,
            "<col min=\"{col_num}\" max=\"{col_num}\" width=\"{width:.2}\" customWidth=\"1\"/>"
        )?;
    }
    buf.push_str("</cols>\n");

    // Sheet data
    buf.push_str("<sheetData>\n");

    let mut row_num: usize = 1;

    // Title row
    if let Some(ref header) = table.header {
        if let Some(ref title) = header.title {
            let text = content_to_text(&title.content);
            write!(buf, "<row r=\"{row_num}\">")?;
            write!(
                buf,
                "<c r=\"{}\" t=\"inlineStr\" s=\"1\"><is><t>{}</t></is></c>",
                xml::cell_ref(0, row_num),
                escape_xml(&text)
            )?;
            buf.push_str("</row>\n");
            row_num += 1;
        }
        if let Some(ref subtitle) = header.subtitle {
            let text = content_to_text(&subtitle.content);
            write!(buf, "<row r=\"{row_num}\">")?;
            write!(
                buf,
                "<c r=\"{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                xml::cell_ref(0, row_num),
                escape_xml(&text)
            )?;
            buf.push_str("</row>\n");
            row_num += 1;
        }
    }

    // Header rows
    if !table.config.column_labels_hidden {
        for row in &table.table.thead.rows {
            write_sheet_row(buf, row, row_num, col_count, true)?;
            row_num += 1;
        }
    }

    // Body rows
    for group in &table.table.tbody {
        if let Some(ref label) = group.label {
            let text = content_to_text(&label.content);
            write!(buf, "<row r=\"{row_num}\">")?;
            write!(
                buf,
                "<c r=\"{}\" t=\"inlineStr\" s=\"1\"><is><t>{}</t></is></c>",
                xml::cell_ref(0, row_num),
                escape_xml(&text)
            )?;
            buf.push_str("</row>\n");
            row_num += 1;
        }
        for row in &group.rows {
            write_sheet_row(buf, row, row_num, col_count, false)?;
            row_num += 1;
        }
        for row in &group.summary_rows {
            write_sheet_row(buf, row, row_num, col_count, true)?;
            row_num += 1;
        }
    }

    // Footnotes
    if let Some(ref footer) = table.footer {
        if !footer.footnotes.is_empty() {
            row_num += 1; // blank row
            for note in &footer.footnotes {
                let text = format!("{} {}", note.mark, content_to_text(&note.content));
                write!(buf, "<row r=\"{row_num}\">")?;
                write!(
                    buf,
                    "<c r=\"{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                    xml::cell_ref(0, row_num),
                    escape_xml(&text)
                )?;
                buf.push_str("</row>\n");
                row_num += 1;
            }
        }
        if !footer.source_notes.is_empty() {
            row_num += 1; // blank row
            for note in &footer.source_notes {
                let text = content_to_text(&note.content);
                write!(buf, "<row r=\"{row_num}\">")?;
                write!(
                    buf,
                    "<c r=\"{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                    xml::cell_ref(0, row_num),
                    escape_xml(&text)
                )?;
                buf.push_str("</row>\n");
                row_num += 1;
            }
        }
    }

    buf.push_str("</sheetData>\n");
    buf.push_str("</worksheet>\n");
    Ok(())
}

fn write_sheet_row(
    buf: &mut String,
    row: &Row,
    row_num: usize,
    _col_count: usize,
    is_bold: bool,
) -> Result<(), RenderError> {
    write!(buf, "<row r=\"{row_num}\">")?;

    let mut col_idx = 0;
    for cell in &row.cells {
        if cell.is_placeholder {
            col_idx += 1;
            continue;
        }

        let text = content_to_text(&cell.content);
        let cell_ref = xml::cell_ref(col_idx, row_num);
        let style_attr = if is_bold { " s=\"1\"" } else { "" };

        // Determine if cell holds a numeric value
        if let Some(ref typed) = cell.typed_value {
            if typed.value_type == "number" {
                if let Some(num) = typed.value.as_f64() {
                    write!(buf, "<c r=\"{cell_ref}\"{style_attr}><v>{num}</v></c>")?;
                    col_idx += cell.colspan as usize;
                    continue;
                }
            }
        }

        if !text.is_empty() {
            write!(
                buf,
                "<c r=\"{cell_ref}\" t=\"inlineStr\"{style_attr}><is><t>{}</t></is></c>",
                escape_xml(&text)
            )?;
        }

        col_idx += cell.colspan as usize;
    }

    buf.push_str("</row>\n");
    Ok(())
}

/// Generate <mergeCells> XML for spans.
fn render_merge_cells(table: &Table) -> String {
    let mut merges = Vec::new();
    let mut row_num: usize = 1;

    // Skip title/subtitle rows
    if let Some(ref header) = table.header {
        if header.title.is_some() {
            row_num += 1;
        }
        if header.subtitle.is_some() {
            row_num += 1;
        }
    }

    // Header rows
    if !table.config.column_labels_hidden {
        for row in &table.table.thead.rows {
            collect_merges(&mut merges, row, row_num);
            row_num += 1;
        }
    }

    // Body rows
    for group in &table.table.tbody {
        if group.label.is_some() {
            // Group label spans all columns
            let table_cols = table.config.table_cols as usize;
            if table_cols > 1 {
                let start = xml::cell_ref(0, row_num);
                let end = xml::cell_ref(table_cols - 1, row_num);
                merges.push(format!("{start}:{end}"));
            }
            row_num += 1;
        }
        for row in &group.rows {
            collect_merges(&mut merges, row, row_num);
            row_num += 1;
        }
        for row in &group.summary_rows {
            collect_merges(&mut merges, row, row_num);
            row_num += 1;
        }
    }

    if merges.is_empty() {
        return String::new();
    }

    let mut buf = String::new();
    write!(buf, "<mergeCells count=\"{}\">", merges.len()).unwrap();
    for m in &merges {
        write!(buf, "<mergeCell ref=\"{m}\"/>").unwrap();
    }
    buf.push_str("</mergeCells>\n");
    buf
}

fn collect_merges(merges: &mut Vec<String>, row: &Row, row_num: usize) {
    let mut col_idx = 0;
    for cell in &row.cells {
        if cell.is_placeholder {
            col_idx += 1;
            continue;
        }
        if cell.colspan > 1 || cell.rowspan > 1 {
            let end_col = col_idx + cell.colspan as usize - 1;
            let end_row = row_num + cell.rowspan as usize - 1;
            let start = xml::cell_ref(col_idx, row_num);
            let end = xml::cell_ref(end_col, end_row);
            if start != end {
                merges.push(format!("{start}:{end}"));
            }
        }
        col_idx += cell.colspan as usize;
    }
}

fn render_shared_strings(table: &Table) -> Result<String, RenderError> {
    // Minimal shared strings (we use inline strings instead)
    let mut buf = String::new();
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    write!(
        buf,
        r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0">"#
    )?;
    buf.push_str("</sst>\n");
    let _ = table; // suppress unused warning
    Ok(buf)
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
