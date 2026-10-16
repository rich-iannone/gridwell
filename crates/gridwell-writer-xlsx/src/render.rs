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
    let mut strings = SharedStrings::default();
    let sheet_xml = sheet_xml(&rt, &mut styles, &mut strings)?;
    let styles_xml = styles.to_xml()?;
    let strings_xml = strings.to_xml()?;

    Ok(package(&[
        Part::new("[Content_Types].xml", xml::CONTENT_TYPES),
        Part::new("_rels/.rels", xml::RELS),
        Part::new("xl/_rels/workbook.xml.rels", xml::WORKBOOK_RELS),
        Part::new("xl/workbook.xml", xml::WORKBOOK),
        Part::new("xl/styles.xml", styles_xml.as_str()),
        Part::new("xl/sharedStrings.xml", strings_xml.as_str()),
        Part::new("xl/worksheets/sheet1.xml", sheet_xml.as_str()),
    ])?)
}

/// Render only the sheet XML content (for snapshot testing).
pub fn render_sheet_xml(table: &Table) -> Result<String, RenderError> {
    sheet_xml(
        &resolve(table),
        &mut StyleSheet::default(),
        &mut SharedStrings::default(),
    )
}

/// Render only the styles part (for testing): the cell formats the sheet uses.
pub fn render_styles_xml(table: &Table) -> Result<String, RenderError> {
    let mut styles = StyleSheet::default();
    sheet_xml(&resolve(table), &mut styles, &mut SharedStrings::default())?;
    styles.to_xml()
}

/// Render only the shared-string table (for testing): the sheet's rich text.
pub fn render_shared_strings_xml(table: &Table) -> Result<String, RenderError> {
    let mut strings = SharedStrings::default();
    sheet_xml(&resolve(table), &mut StyleSheet::default(), &mut strings)?;
    strings.to_xml()
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
    /// Font color, already flattened (no alpha in SpreadsheetML fonts here).
    color: Option<Color>,
    /// Solid fill, already flattened.
    fill: Option<Color>,
    /// `horizontal` alignment; `None` is Excel's "general" (text left, numbers
    /// right), which is also what a left-aligned column gets.
    align: Option<&'static str>,
    /// Border edges in SpreadsheetML order: left, right, top, bottom.
    border: [Option<Edge>; 4],
    /// Wrap text: set for text with line breaks, which Excel shows only when
    /// wrapping is on.
    wrap: bool,
    /// How a numeric cell shows its value (`None`: General).
    num_fmt: Option<NumFmt>,
}

/// A number format that shows a value as the table displays it: `0.0` for
/// `76.0`, `#,##0` for `1,250`. The IR carries the display text and the value,
/// not a format pattern, so the format is read off the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NumFmt {
    decimals: u8,
    /// Thousands separators.
    grouping: bool,
    /// Negatives show U+2212 MINUS SIGN (as gt writes them), not `-`.
    unicode_minus: bool,
}

impl NumFmt {
    /// The format that shows `value` as `shown`, if `shown` is a plain number
    /// (optional `-` or `−`, digits with optional `,` grouping, optional
    /// decimals) that rounds from `value`. Anything else has none: such a cell
    /// keeps its text.
    fn infer(shown: &str, value: f64) -> Option<Self> {
        let (negative, unicode_minus, body) = if let Some(b) = shown.strip_prefix('-') {
            (true, false, b)
        } else if let Some(b) = shown.strip_prefix('\u{2212}') {
            (true, true, b)
        } else {
            (false, false, shown)
        };
        if negative != (value < 0.0) {
            return None;
        }
        let (int, frac) = match body.split_once('.') {
            Some((i, f)) => (i, Some(f)),
            None => (body, None),
        };
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let mut groups = int.split(',');
        let first = groups.next().unwrap_or_default();
        let grouping = int.contains(',');
        let int_ok = if grouping {
            digits(first) && first.len() <= 3 && groups.all(|g| g.len() == 3 && digits(g))
        } else {
            digits(int)
        };
        // A format shows no leading zeros (`007`), nor a leading 0 group (`0,123`).
        if !int_ok || (first.len() > 1 && first.starts_with('0')) || (grouping && first == "0") {
            return None;
        }
        let decimals = match frac {
            Some(f) if digits(f) && f.len() <= 30 => f.len(),
            Some(_) => return None,
            None => 0,
        };
        let shown_value: f64 = format!("{}.{}", int.replace(',', ""), frac.unwrap_or("0"))
            .parse()
            .ok()?;
        // The display is the value rounded to its decimals (with slack for
        // binary floating point).
        let half = 0.5 * 10f64.powi(-(decimals as i32));
        if (value.abs() - shown_value).abs() > half * (1.0 + 1e-9) + value.abs() * 1e-15 {
            return None;
        }
        Some(Self {
            decimals: decimals as u8,
            grouping,
            unicode_minus,
        })
    }

    /// The built-in `numFmtId` with this pattern, if there is one.
    fn builtin(&self) -> Option<u32> {
        match (self.decimals, self.grouping, self.unicode_minus) {
            (0, false, false) => Some(1),
            (2, false, false) => Some(2),
            (0, true, false) => Some(3),
            (2, true, false) => Some(4),
            _ => None,
        }
    }

    /// The format code: a second section writes negatives with `−`.
    fn code(&self) -> String {
        let mut pattern = String::from(if self.grouping { "#,##0" } else { "0" });
        if self.decimals > 0 {
            pattern.push('.');
            pattern.push_str(&"0".repeat(self.decimals.into()));
        }
        if self.unicode_minus {
            format!("{pattern};\"\u{2212}\"{pattern}")
        } else {
            pattern
        }
    }
}

/// One drawn border edge: a SpreadsheetML line style and an optional color
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
            wrap: false,
            num_fmt: None,
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

impl FontKey {
    /// A cell format's font.
    fn of(f: &Format) -> Self {
        Self {
            bold: f.bold,
            italic: f.italic,
            underline: f.underline,
            strike: f.strike,
            size: f.size,
            color: f.color,
        }
    }

    /// This font with a run's style laid over it (a relative size resolves
    /// against the font's size).
    fn with(self, style: &ResolvedStyle) -> Self {
        let parent = self.size.map_or(11.0, |h| f64::from(h) / 100.0);
        Self {
            bold: self.bold || style.is_bold(),
            italic: self.italic || style.is_italic(),
            underline: self.underline || style.is_underline(),
            strike: self.strike || style.is_strike(),
            size: style
                .size_pt(parent)
                .map(|pt| (pt * 100.0).round().clamp(100.0, 40_900.0) as u32)
                .or(self.size),
            color: style.paint().map(|c| c.flatten()).or(self.color),
        }
    }
}

/// A font's elements, in `<font>` (`name`) or a rich run's `<rPr>` (`rFont`).
fn write_font(buf: &mut String, font: &FontKey, name: &str) -> Result<(), RenderError> {
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
    write!(buf, "<{name} val=\"Calibri\"/>")?;
    Ok(())
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
        // Number formats without a built-in id get custom ids from 164.
        let mut custom: Vec<NumFmt> = Vec::new();
        let mut xfs = Vec::new();
        for f in &self.formats {
            let num_fmt = f.num_fmt.map_or(0, |n| {
                n.builtin().unwrap_or_else(|| {
                    164 + custom.iter().position(|k| *k == n).unwrap_or_else(|| {
                        custom.push(n);
                        custom.len() - 1
                    }) as u32
                })
            });
            let font_key = FontKey::of(f);
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
            xfs.push((num_fmt, font, fill, border, f.align, f.wrap));
        }

        let mut buf = String::new();
        buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        buf.push_str(
            "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\n",
        );
        // <numFmts> comes first in CT_Stylesheet.
        if !custom.is_empty() {
            writeln!(buf, "  <numFmts count=\"{}\">", custom.len())?;
            for (i, n) in custom.iter().enumerate() {
                writeln!(
                    buf,
                    "    <numFmt numFmtId=\"{}\" formatCode=\"{}\"/>",
                    164 + i,
                    escape_xml(&n.code())
                )?;
            }
            buf.push_str("  </numFmts>\n");
        }
        writeln!(buf, "  <fonts count=\"{}\">", fonts.len())?;
        for font in &fonts {
            buf.push_str("    <font>");
            write_font(&mut buf, font, "name")?;
            buf.push_str("</font>\n");
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
        for (num_fmt, font, fill, border, align, wrap) in xfs {
            write!(
                buf,
                "    <xf numFmtId=\"{num_fmt}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\""
            )?;
            if num_fmt != 0 {
                buf.push_str(" applyNumberFormat=\"1\"");
            }
            if font != 0 {
                buf.push_str(" applyFont=\"1\"");
            }
            if fill != 0 {
                buf.push_str(" applyFill=\"1\"");
            }
            if border != 0 {
                buf.push_str(" applyBorder=\"1\"");
            }
            if align.is_none() && !wrap {
                buf.push_str("/>\n");
                continue;
            }
            buf.push_str(" applyAlignment=\"1\"><alignment");
            if let Some(h) = align {
                write!(buf, " horizontal=\"{h}\"")?;
            }
            if wrap {
                buf.push_str(" wrapText=\"1\"");
            }
            buf.push_str("/></xf>\n");
        }
        buf.push_str("  </cellXfs>\n");
        buf.push_str("</styleSheet>\n");
        Ok(buf)
    }
}

/// The sheet, registering the formats it uses in `styles`.
fn sheet_xml(
    rt: &ResolvedTable,
    styles: &mut StyleSheet,
    strings: &mut SharedStrings,
) -> Result<String, RenderError> {
    let mut buf = String::with_capacity(8192);
    buf.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    buf.push('\n');
    buf.push_str(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    buf.push('\n');

    // Column widths, in characters of the default font (≈ 7px each). `<cols>`
    // may not be empty.
    let col_chars: Vec<f64> = rt
        .columns
        .iter()
        .map(|col| {
            col.width
                .as_ref()
                .and_then(|w| w.to_pt(12.0, 12.0))
                .map(|pt| pt / 0.75 / 7.0)
                .filter(|w| w.is_finite() && *w > 0.0 && *w <= 255.0)
                .unwrap_or(12.0)
        })
        .collect();
    if !rt.columns.is_empty() {
        buf.push_str("<cols>");
        for (i, width) in col_chars.iter().enumerate() {
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
        strings,
        row_num: 1,
        merges: Vec::new(),
        col_chars: col_chars.clone(),
    };

    // Title and subtitle lines.
    let header = &rt.header;
    if let Some(title) = &header.title {
        let f = Format::new(&title.style, true, &HAlign::Left);
        sheet.text_row(&rich(rt, title.content, FontKey::of(&f)), f, false)?;
    }
    for line in header.subtitle.iter().chain(&header.extra_lines) {
        let f = Format::new(&line.style, false, &HAlign::Left);
        sheet.text_row(&rich(rt, line.content, FontKey::of(&f)), f, false)?;
    }

    if !rt.is_empty() {
        sheet.section(rt, &rt.head, true)?;
        for group in &rt.groups {
            if let Some(label) = &group.label {
                // One cell merged across every column.
                if rt.columns.len() > 1 {
                    let r = sheet.row_num;
                    sheet.merges.push((0, r, rt.columns.len() - 1, r));
                }
                let f = Format::new(&label.style, true, &HAlign::Left);
                sheet.text_row(&rich(rt, label.content, FontKey::of(&f)), f, true)?;
            }
            sheet.section(rt, &group.rows, false)?;
            sheet.section(rt, &group.summary_rows, true)?;
        }
    }

    // Notes, each block after a blank row.
    let footer = &rt.footer;
    if !footer.footnotes.is_empty() {
        sheet.row_num += 1;
        for note in &footer.footnotes {
            let f = Format::new(&note.style, false, &HAlign::Left);
            let font = FontKey::of(&f);
            let mut runs = vec![
                RichRun {
                    text: note.mark.to_string(),
                    font,
                    superscript: true,
                },
                RichRun {
                    text: " ".into(),
                    font,
                    superscript: false,
                },
            ];
            runs.extend(rich(rt, note.content, font));
            sheet.text_row(&runs, f, false)?;
        }
    }
    if !footer.source_notes.is_empty() {
        sheet.row_num += 1;
        for note in &footer.source_notes {
            let f = Format::new(&note.style, false, &HAlign::Left);
            sheet.text_row(&rich(rt, note.content, FontKey::of(&f)), f, false)?;
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
    strings: &'b mut SharedStrings,
    /// The next row's 1-based number.
    row_num: usize,
    /// (first col, first row, last col, last row), 0-based columns, 1-based rows.
    merges: Vec<(usize, usize, usize, usize)>,
    /// Column widths in characters, for estimating where wrapped text breaks.
    col_chars: Vec<f64>,
}

impl Sheet<'_> {
    /// A row with one text cell in column A (`merged`: across every column).
    fn text_row(
        &mut self,
        runs: &[RichRun],
        mut f: Format,
        merged: bool,
    ) -> Result<(), RenderError> {
        let r = self.row_num;
        f.wrap = has_breaks(runs);
        let width = if merged {
            self.col_chars.iter().sum()
        } else {
            self.col_chars.first().copied().unwrap_or(12.0)
        };
        let height = text_height(runs, f.wrap.then_some(width));
        write!(self.buf, "<row r=\"{r}\"{}>", row_height(height))?;
        let s = self.styles.id(f);
        write_text_cell(
            self.buf,
            self.strings,
            &xml::cell_ref(0, r),
            s,
            runs,
            FontKey::of(&f),
        )?;
        self.buf.push_str("</row>\n");
        self.row_num += 1;
        Ok(())
    }

    /// A section's rows. Each cell sits at its visible column; covered positions
    /// write nothing and are covered by a `<mergeCell>`.
    fn section(
        &mut self,
        rt: &ResolvedTable,
        section: &Section,
        strong: bool,
    ) -> Result<(), RenderError> {
        for row in &section.rows {
            let r = self.row_num;
            // Cells first: the row's height depends on them.
            let mut cells = String::new();
            let mut height = 0.0f64;
            for cell in row.cells() {
                let cell_ref = xml::cell_ref(cell.col, r);
                let mut format = Format::cell(&cell.style, strong, &cell.align);
                let runs = rich(rt, cell.content, FontKey::of(&format));
                format.wrap = has_breaks(&runs);
                // A spanned cell's text spreads over its rows: leave them be.
                if cell.rowspan == 1 {
                    let width = self.col_chars[cell.col..cell.col + cell.colspan]
                        .iter()
                        .sum();
                    height = height.max(text_height(&runs, format.wrap.then_some(width)));
                }
                if cell.colspan > 1 || cell.rowspan > 1 {
                    self.merges.push((
                        cell.col,
                        r,
                        cell.col + cell.colspan - 1,
                        r + cell.rowspan - 1,
                    ));
                }

                // A number shows as the table displays it, through a number
                // format; a display no format reproduces stays text.
                let number = cell
                    .typed_value
                    .filter(|t| matches!(t.value_type, ValueType::Number | ValueType::Integer))
                    .and_then(|t| t.value.as_f64())
                    .filter(|v| v.is_finite())
                    .and_then(|v| {
                        let shown = plain_display(cell.content)?;
                        Some((v, NumFmt::infer(&shown, v)?))
                    });
                if let Some((num, num_fmt)) = number {
                    let s = self.styles.id(Format {
                        num_fmt: Some(num_fmt),
                        ..format
                    });
                    write!(cells, "<c r=\"{cell_ref}\"{}><v>{num}</v></c>", s_attr(s))?;
                    continue;
                }
                let s = self.styles.id(format);
                write_text_cell(
                    &mut cells,
                    self.strings,
                    &cell_ref,
                    s,
                    &runs,
                    FontKey::of(&format),
                )?;
            }
            write!(self.buf, "<row r=\"{r}\"{}>", row_height(height))?;
            self.buf.push_str(&cells);
            self.buf.push_str("</row>\n");
            self.row_num += 1;
        }
        Ok(())
    }
}

/// Excel's row height for 11pt Calibri, and the ratio of row height to font size.
const DEFAULT_ROW_PT: f64 = 15.0;
const ROW_PER_FONT_PT: f64 = DEFAULT_ROW_PT / 11.0;

fn has_breaks(runs: &[RichRun]) -> bool {
    runs.iter().any(|r| r.text.contains('\n'))
}

/// The height text needs: its lines at the size of its largest run. Wrapped text
/// (`width`: the cell's width in characters) also breaks where a line is wider
/// than the cell; each character counts as one digit-width at its size — a
/// generous estimate, as a row a little too tall is better than a clipped line.
fn text_height(runs: &[RichRun], width: Option<f64>) -> f64 {
    let mut lines = 0usize;
    let mut chars = 0.0f64;
    let mut end_line = |chars: &mut f64| {
        lines += match width {
            Some(w) => ((*chars / (w - 1.0).max(1.0)).ceil() as usize).max(1),
            None => 1,
        };
        *chars = 0.0;
    };
    for r in runs {
        let scale = r.font.size.map_or(1.0, |h| f64::from(h) / 1100.0);
        for c in r.text.chars() {
            if c == '\n' {
                end_line(&mut chars);
            } else {
                chars += scale;
            }
        }
    }
    end_line(&mut chars);
    let size = runs
        .iter()
        .filter(|r| !r.text.is_empty())
        .map(|r| r.font.size.map_or(11.0, |h| f64::from(h) / 100.0))
        .fold(11.0, f64::max);
    lines as f64 * size * ROW_PER_FONT_PT
}

/// A row's height attributes, when its text needs more than the default.
fn row_height(pt: f64) -> String {
    if pt > DEFAULT_ROW_PT + 0.01 {
        // Excel's maximum row height is 409pt.
        let pt = pt.min(409.0);
        format!(
            " ht=\"{}\" customHeight=\"1\"",
            (pt * 100.0).round() / 100.0
        )
    } else {
        String::new()
    }
}

fn s_attr(s: usize) -> String {
    if s == 0 {
        String::new()
    } else {
        format!(" s=\"{s}\"")
    }
}

/// A text cell; an empty string with a format still writes the cell so its fill
/// shows. Text whose runs all have the cell's font is an inline string; otherwise
/// it is rich text, each run with its full font (Excel doesn't inherit run
/// formatting from the cell's font), in the shared-string table: apps such as
/// Numbers ignore run formatting in inline strings.
fn write_text_cell(
    buf: &mut String,
    strings: &mut SharedStrings,
    cell_ref: &str,
    s: usize,
    runs: &[RichRun],
    font: FontKey,
) -> Result<(), RenderError> {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    if text.is_empty() {
        if s != 0 {
            write!(buf, "<c r=\"{cell_ref}\"{}/>", s_attr(s))?;
        }
        return Ok(());
    }
    if runs.iter().all(|r| r.font == font && !r.superscript) {
        write!(
            buf,
            "<c r=\"{cell_ref}\" t=\"inlineStr\"{}><is><t xml:space=\"preserve\">{}</t></is></c>",
            s_attr(s),
            escape_xml(&text)
        )?;
        return Ok(());
    }
    let mut item = String::new();
    for r in runs.iter().filter(|r| !r.text.is_empty()) {
        item.push_str("<r><rPr>");
        write_font(&mut item, &r.font, "rFont")?;
        if r.superscript {
            item.push_str("<vertAlign val=\"superscript\"/>");
        }
        write!(
            item,
            "</rPr><t xml:space=\"preserve\">{}</t></r>",
            escape_xml(&r.text)
        )?;
    }
    let i = strings.id(item);
    write!(
        buf,
        "<c r=\"{cell_ref}\" t=\"s\"{}><v>{i}</v></c>",
        s_attr(s)
    )?;
    Ok(())
}

/// The workbook's shared strings (`<si>` contents), each stored once.
#[derive(Debug, Default)]
struct SharedStrings {
    items: Vec<String>,
    index: HashMap<String, usize>,
    /// References from cells, counting repeats.
    count: usize,
}

impl SharedStrings {
    /// The index of a string item, adding it if new.
    fn id(&mut self, item: String) -> usize {
        self.count += 1;
        let next = self.items.len();
        *self.index.entry(item).or_insert_with_key(|item| {
            self.items.push(item.clone());
            next
        })
    }

    fn to_xml(&self) -> Result<String, RenderError> {
        let mut buf = String::new();
        buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        write!(
            buf,
            "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{}\" uniqueCount=\"{}\">",
            self.count,
            self.items.len()
        )?;
        for item in &self.items {
            write!(buf, "\n<si>{item}</si>")?;
        }
        buf.push_str("</sst>\n");
        Ok(buf)
    }
}

/// The text of content that is only plain text (what a number's display can
/// be); `None` if it has marks, styled runs, breaks or other nodes.
fn plain_display(nodes: &[ContentNode]) -> Option<String> {
    nodes
        .iter()
        .map(|n| match n {
            ContentNode::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

/// A run of cell text and its font.
#[derive(Debug, Clone, PartialEq)]
struct RichRun {
    text: String,
    font: FontKey,
    superscript: bool,
}

/// Content as runs: styled text gets the cell's font with its style laid over,
/// footnote marks are superscript, line breaks are newlines (shown when wrapping
/// is on), images show their alt text.
fn rich(rt: &ResolvedTable, nodes: &[ContentNode], font: FontKey) -> Vec<RichRun> {
    let plain = |text: String| RichRun {
        text,
        font,
        superscript: false,
    };
    let mut runs = Vec::new();
    for node in nodes {
        match node {
            ContentNode::StyledText { value, style_id } => {
                let style = style_id
                    .as_deref()
                    .map(|id| rt.style(id))
                    .unwrap_or_default();
                runs.push(RichRun {
                    text: value.clone(),
                    font: font.with(&style),
                    superscript: false,
                });
            }
            ContentNode::FootnoteMark { mark_text, .. } => runs.push(RichRun {
                text: mark_text.clone(),
                font,
                superscript: true,
            }),
            other => runs.push(plain(content_to_text(std::slice::from_ref(other)))),
        }
    }
    runs
}

/// A line break is a newline inside the cell (shown when wrapping is on).
fn content_to_text(nodes: &[ContentNode]) -> String {
    gridwell_layout::plain_text(nodes, "\n")
}
