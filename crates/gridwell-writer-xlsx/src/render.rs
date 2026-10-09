use gridwell_core::xml::escape as escape_xml;
use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table, ValueType};
use gridwell_layout::{resolve, ResolvedBorder, ResolvedStyle, ResolvedTable, Section};
use gridwell_ooxml::{package, Part};
use std::collections::HashMap;
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

/// Render the full .xlsx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let rt = resolve(table);
    let mut styles = StyleSheet::default();
    let sheet_xml = sheet_xml(&rt, &mut styles)?;
    let styles_xml = styles.to_xml()?;

    Ok(package(&[
        Part::new("[Content_Types].xml", xml::CONTENT_TYPES),
        Part::new("_rels/.rels", xml::RELS),
        Part::new("xl/_rels/workbook.xml.rels", xml::WORKBOOK_RELS),
        Part::new("xl/workbook.xml", xml::WORKBOOK),
        Part::new("xl/styles.xml", styles_xml.as_str()),
        Part::new("xl/sharedStrings.xml", xml::SHARED_STRINGS),
        Part::new("xl/worksheets/sheet1.xml", sheet_xml.as_str()),
    ])?)
}

/// Render only the sheet XML content (for snapshot testing).
pub fn render_sheet_xml(table: &Table) -> Result<String, RenderError> {
    sheet_xml(&resolve(table), &mut StyleSheet::default())
}

/// Render only the styles part (for testing): the cell formats the sheet uses.
pub fn render_styles_xml(table: &Table) -> Result<String, RenderError> {
    let mut styles = StyleSheet::default();
    sheet_xml(&resolve(table), &mut styles)?;
    styles.to_xml()
}

/// A cell format: what a `<c s="…">` index points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct Format {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    /// Font size in hundredths of a point (`None`: the default 11pt).
    size: Option<u32>,
    /// Font colour, already flattened (no alpha in SpreadsheetML fonts here).
    color: Option<Color>,
    /// Solid fill, already flattened.
    fill: Option<Color>,
    /// `horizontal` alignment; `None` is Excel's "general" (text left, numbers
    /// right), which is also what a left-aligned column gets.
    align: Option<&'static str>,
    /// Border edges in SpreadsheetML order: left, right, top, bottom.
    border: [Option<Edge>; 4],
}

/// One drawn border edge: a SpreadsheetML line style and an optional colour
/// (`None`: automatic, i.e. black).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Edge {
    style: &'static str,
    color: Option<Color>,
}

impl Edge {
    /// SpreadsheetML has named line weights, not widths: up to 1.5px is `thin`,
    /// up to 2.5px `medium`, wider `thick`; dashed lines have thin and medium
    /// forms; dotted and double have one each.
    fn from(b: &ResolvedBorder) -> Self {
        let px = b
            .width
            .as_ref()
            .and_then(|w| w.to_pt(11.0, 11.0))
            .map_or(1.0, |pt| pt / 0.75);
        let weight = if px <= 1.5 {
            0
        } else if px <= 2.5 {
            1
        } else {
            2
        };
        let style = match (&b.style, weight) {
            (BorderStyle::Dashed, 0) => "dashed",
            (BorderStyle::Dashed, _) => "mediumDashed",
            (BorderStyle::Dotted, _) => "dotted",
            (BorderStyle::Double, _) => "double",
            (_, 0) => "thin",
            (_, 1) => "medium",
            _ => "thick",
        };
        Self {
            style,
            color: b.color.filter(|c| !c.is_transparent()).map(|c| c.flatten()),
        }
    }
}

impl Format {
    fn new(style: &ResolvedStyle, bold: bool, align: &HAlign) -> Self {
        Self {
            bold: bold || style.is_bold(),
            italic: style.is_italic(),
            underline: style.is_underline(),
            strike: style.is_strike(),
            // Relative sizes resolve against Excel's default 11pt.
            size: style
                .size_pt(11.0)
                .map(|pt| (pt * 100.0).round().clamp(100.0, 40_900.0) as u32),
            color: style.paint().map(|c| c.flatten()),
            fill: style.fill().map(|c| c.flatten()),
            align: match align {
                HAlign::Right => Some("right"),
                HAlign::Center => Some("center"),
                HAlign::Justify => Some("justify"),
                _ => None,
            },
            border: [None; 4],
        }
    }

    /// A cell's format: `new` plus its border edges.
    fn cell(style: &ResolvedStyle, bold: bool, align: &HAlign) -> Self {
        let b = &style.border;
        let edge = |e: &Option<ResolvedBorder>| e.as_ref().map(Edge::from);
        Self {
            border: [edge(&b.left), edge(&b.right), edge(&b.top), edge(&b.bottom)],
            ..Self::new(style, bold, align)
        }
    }
}

/// A font entry: what `<fonts>` deduplicates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct FontKey {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    size: Option<u32>,
    color: Option<Color>,
}

/// The workbook's cell formats, registered as cells use them. Format 0 is the
/// default; fonts, fills and `cellXfs` are emitted in registration order.
#[derive(Debug, Default)]
struct StyleSheet {
    formats: Vec<Format>,
    index: HashMap<Format, usize>,
}

impl StyleSheet {
    /// The `s` index of a format (0 for the default).
    fn id(&mut self, f: Format) -> usize {
        if f == Format::default() {
            return 0;
        }
        *self.index.entry(f).or_insert_with(|| {
            self.formats.push(f);
            self.formats.len()
        })
    }

    fn to_xml(&self) -> Result<String, RenderError> {
        // Fonts and fills are deduplicated separately from formats.
        let mut fonts: Vec<FontKey> = vec![FontKey::default()];
        let mut fills: Vec<Color> = Vec::new();
        // Border 0 is "no border".
        let mut borders: Vec<[Option<Edge>; 4]> = vec![[None; 4]];
        let mut xfs = Vec::new();
        for f in &self.formats {
            let font_key = FontKey {
                bold: f.bold,
                italic: f.italic,
                underline: f.underline,
                strike: f.strike,
                size: f.size,
                color: f.color,
            };
            let font = fonts
                .iter()
                .position(|k| *k == font_key)
                .unwrap_or_else(|| {
                    fonts.push(font_key);
                    fonts.len() - 1
                });
            // Fills 0 and 1 are reserved (none, gray125).
            let fill = f.fill.map_or(0, |c| {
                2 + fills.iter().position(|k| *k == c).unwrap_or_else(|| {
                    fills.push(c);
                    fills.len() - 1
                })
            });
            let border = borders
                .iter()
                .position(|k| *k == f.border)
                .unwrap_or_else(|| {
                    borders.push(f.border);
                    borders.len() - 1
                });
            xfs.push((font, fill, border, f.align));
        }

        let mut buf = String::new();
        buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        buf.push_str(
            "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\n",
        );
        writeln!(buf, "  <fonts count=\"{}\">", fonts.len())?;
        for font in &fonts {
            buf.push_str("    <font>");
            if font.bold {
                buf.push_str("<b/>");
            }
            if font.italic {
                buf.push_str("<i/>");
            }
            if font.strike {
                buf.push_str("<strike/>");
            }
            if font.underline {
                buf.push_str("<u/>");
            }
            match font.size {
                Some(h) => write!(buf, "<sz val=\"{}\"/>", f64::from(h) / 100.0)?,
                None => buf.push_str("<sz val=\"11\"/>"),
            }
            if let Some(c) = font.color {
                write!(buf, "<color rgb=\"FF{}\"/>", c.to_rrggbb())?;
            }
            buf.push_str("<name val=\"Calibri\"/></font>\n");
        }
        buf.push_str("  </fonts>\n");
        writeln!(buf, "  <fills count=\"{}\">", fills.len() + 2)?;
        buf.push_str("    <fill><patternFill patternType=\"none\"/></fill>\n");
        buf.push_str("    <fill><patternFill patternType=\"gray125\"/></fill>\n");
        for c in &fills {
            writeln!(
                buf,
                "    <fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{}\"/><bgColor indexed=\"64\"/></patternFill></fill>",
                c.to_rrggbb()
            )?;
        }
        buf.push_str("  </fills>\n");
        writeln!(buf, "  <borders count=\"{}\">", borders.len())?;
        for b in &borders {
            buf.push_str("    <border>");
            for (side, edge) in ["left", "right", "top", "bottom"].iter().zip(b) {
                match edge {
                    None => write!(buf, "<{side}/>")?,
                    Some(Edge { style, color: None }) => {
                        write!(buf, "<{side} style=\"{style}\"/>")?
                    }
                    Some(Edge {
                        style,
                        color: Some(c),
                    }) => write!(
                        buf,
                        "<{side} style=\"{style}\"><color rgb=\"FF{}\"/></{side}>",
                        c.to_rrggbb()
                    )?,
                }
            }
            buf.push_str("<diagonal/></border>\n");
        }
        buf.push_str("  </borders>\n");
        buf.push_str("  <cellStyleXfs count=\"1\">\n    <xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/>\n  </cellStyleXfs>\n");
        writeln!(buf, "  <cellXfs count=\"{}\">", xfs.len() + 1)?;
        buf.push_str(
            "    <xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\n",
        );
        for (font, fill, border, align) in xfs {
            write!(
                buf,
                "    <xf numFmtId=\"0\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\""
            )?;
            if font != 0 {
                buf.push_str(" applyFont=\"1\"");
            }
            if fill != 0 {
                buf.push_str(" applyFill=\"1\"");
            }
            if border != 0 {
                buf.push_str(" applyBorder=\"1\"");
            }
            match align {
                Some(h) => writeln!(
                    buf,
                    " applyAlignment=\"1\"><alignment horizontal=\"{h}\"/></xf>"
                )?,
                None => buf.push_str("/>\n"),
            }
        }
        buf.push_str("  </cellXfs>\n");
        buf.push_str("</styleSheet>\n");
        Ok(buf)
    }
}

/// The sheet, registering the formats it uses in `styles`.
fn sheet_xml(rt: &ResolvedTable, styles: &mut StyleSheet) -> Result<String, RenderError> {
    let mut buf = String::with_capacity(8192);
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    buf.push_str(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    buf.push('\n');

    // Column widths, in characters of the default font (≈ 7px each). `<cols>`
    // may not be empty.
    if !rt.columns.is_empty() {
        buf.push_str("<cols>");
        for (i, col) in rt.columns.iter().enumerate() {
            let width = col
                .width
                .as_ref()
                .and_then(|w| w.to_pt(12.0, 12.0))
                .map(|pt| pt / 0.75 / 7.0)
                .filter(|w| w.is_finite() && *w > 0.0 && *w <= 255.0)
                .unwrap_or(12.0);
            let n = i + 1;
            write!(
                buf,
                "<col min=\"{n}\" max=\"{n}\" width=\"{width:.2}\" customWidth=\"1\"/>"
            )?;
        }
        buf.push_str("</cols>\n");
    }

    buf.push_str("<sheetData>\n");
    let mut sheet = Sheet {
        buf: &mut buf,
        styles,
        row_num: 1,
        merges: Vec::new(),
    };

    // Title and subtitle lines.
    let header = &rt.header;
    if let Some(title) = &header.title {
        let f = Format::new(&title.style, true, &HAlign::Left);
        sheet.text_row(&content_to_text(title.content), f)?;
    }
    for line in header.subtitle.iter().chain(&header.extra_lines) {
        let f = Format::new(&line.style, false, &HAlign::Left);
        sheet.text_row(&content_to_text(line.content), f)?;
    }

    if !rt.is_empty() {
        sheet.section(&rt.head, true)?;
        for group in &rt.groups {
            if let Some(label) = &group.label {
                // One cell merged across every column.
                if rt.columns.len() > 1 {
                    let r = sheet.row_num;
                    sheet.merges.push((0, r, rt.columns.len() - 1, r));
                }
                let f = Format::new(&label.style, true, &HAlign::Left);
                sheet.text_row(&content_to_text(label.content), f)?;
            }
            sheet.section(&group.rows, false)?;
            sheet.section(&group.summary_rows, true)?;
        }
    }

    // Notes, each block after a blank row.
    let footer = &rt.footer;
    if !footer.footnotes.is_empty() {
        sheet.row_num += 1;
        for note in &footer.footnotes {
            let text = format!("{} {}", note.mark, content_to_text(note.content));
            let f = Format::new(&note.style, false, &HAlign::Left);
            sheet.text_row(&text, f)?;
        }
    }
    if !footer.source_notes.is_empty() {
        sheet.row_num += 1;
        for note in &footer.source_notes {
            let f = Format::new(&note.style, false, &HAlign::Left);
            sheet.text_row(&content_to_text(note.content), f)?;
        }
    }

    let merges = std::mem::take(&mut sheet.merges);
    buf.push_str("</sheetData>\n");
    // <mergeCells> must follow <sheetData> (CT_Worksheet element order).
    if !merges.is_empty() {
        write!(buf, "<mergeCells count=\"{}\">", merges.len())?;
        for (c0, r0, c1, r1) in merges {
            write!(
                buf,
                "<mergeCell ref=\"{}:{}\"/>",
                xml::cell_ref(c0, r0),
                xml::cell_ref(c1, r1)
            )?;
        }
        buf.push_str("</mergeCells>\n");
    }
    buf.push_str("</worksheet>\n");
    Ok(buf)
}

/// Writes rows, tracking the current row number and the merged ranges.
struct Sheet<'b> {
    buf: &'b mut String,
    styles: &'b mut StyleSheet,
    /// The next row's 1-based number.
    row_num: usize,
    /// (first col, first row, last col, last row), 0-based columns, 1-based rows.
    merges: Vec<(usize, usize, usize, usize)>,
}

impl Sheet<'_> {
    /// A row with one text cell in column A.
    fn text_row(&mut self, text: &str, f: Format) -> Result<(), RenderError> {
        let r = self.row_num;
        write!(self.buf, "<row r=\"{r}\">")?;
        let s = self.styles.id(f);
        write_text_cell(self.buf, &xml::cell_ref(0, r), s, text)?;
        self.buf.push_str("</row>\n");
        self.row_num += 1;
        Ok(())
    }

    /// A section's rows. Each cell sits at its visible column; covered positions
    /// write nothing and are covered by a `<mergeCell>`.
    fn section(&mut self, section: &Section, strong: bool) -> Result<(), RenderError> {
        for row in &section.rows {
            let r = self.row_num;
            write!(self.buf, "<row r=\"{r}\">")?;
            for cell in row.cells() {
                let cell_ref = xml::cell_ref(cell.col, r);
                let s = self
                    .styles
                    .id(Format::cell(&cell.style, strong, &cell.align));
                if cell.colspan > 1 || cell.rowspan > 1 {
                    self.merges.push((
                        cell.col,
                        r,
                        cell.col + cell.colspan - 1,
                        r + cell.rowspan - 1,
                    ));
                }

                let number = cell
                    .typed_value
                    .filter(|t| matches!(t.value_type, ValueType::Number | ValueType::Integer))
                    .and_then(|t| t.value.as_f64())
                    .filter(|v| v.is_finite());
                if let Some(num) = number {
                    write!(
                        self.buf,
                        "<c r=\"{cell_ref}\"{}><v>{num}</v></c>",
                        s_attr(s)
                    )?;
                    continue;
                }
                write_text_cell(self.buf, &cell_ref, s, &content_to_text(cell.content))?;
            }
            self.buf.push_str("</row>\n");
            self.row_num += 1;
        }
        Ok(())
    }
}

fn s_attr(s: usize) -> String {
    if s == 0 {
        String::new()
    } else {
        format!(" s=\"{s}\"")
    }
}

/// An inline-string cell; an empty string with a format still writes the cell so
/// its fill shows.
fn write_text_cell(
    buf: &mut String,
    cell_ref: &str,
    s: usize,
    text: &str,
) -> Result<(), RenderError> {
    if text.is_empty() {
        if s != 0 {
            write!(buf, "<c r=\"{cell_ref}\"{}/>", s_attr(s))?;
        }
        return Ok(());
    }
    write!(
        buf,
        "<c r=\"{cell_ref}\" t=\"inlineStr\"{}><is><t xml:space=\"preserve\">{}</t></is></c>",
        s_attr(s),
        escape_xml(text)
    )?;
    Ok(())
}

/// A line break is a newline inside the cell (shown when wrapping is on).
fn content_to_text(nodes: &[ContentNode]) -> String {
    gridwell_layout::plain_text(nodes, "\n")
}
