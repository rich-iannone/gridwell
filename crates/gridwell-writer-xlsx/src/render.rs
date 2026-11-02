use gridwell_core::{Color, ImageFormat};
use gridwell_ir::cell::TypedValue;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table, ValueType};
use gridwell_layout::unsupported::Place;
use gridwell_layout::{resolve, CellVAlign, ResolvedBorder, ResolvedStyle, ResolvedTable, Section};
use rust_xlsxwriter::{
    ColNum, DocProperties, ExcelDateTime, FormatAlign, FormatBorder, FormatPattern, FormatScript,
    FormatUnderline, RowNum, Workbook, Worksheet, XlsxError,
};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use thiserror::Error;

use crate::date::Moment;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("xlsx error: {0}")]
    Xlsx(#[from] XlsxError),
    #[error("reading the package: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Render the full .xlsx ZIP file as bytes.
pub fn render(table: &Table) -> Result<Vec<u8>, RenderError> {
    let (mut workbook, _) = workbook(&resolve(table))?;
    Ok(workbook.save_to_buffer()?)
}

/// Render only the sheet XML content (for snapshot testing).
pub fn render_sheet_xml(table: &Table) -> Result<String, RenderError> {
    package_part(table, "xl/worksheets/sheet1.xml")
}

/// Render only the styles part (for testing): the cell formats the sheet uses.
pub fn render_styles_xml(table: &Table) -> Result<String, RenderError> {
    package_part(table, "xl/styles.xml")
}

/// Render only the drawing part (for testing): the images drawn over cells;
/// empty when there are none (the package then has no such part).
pub fn render_drawing_xml(table: &Table) -> Result<String, RenderError> {
    package_part(table, "xl/drawings/drawing1.xml")
}

/// Render only the shared-string table (for testing): the sheet's strings; empty
/// when the sheet has none (the package then has no such part).
pub fn render_shared_strings_xml(table: &Table) -> Result<String, RenderError> {
    package_part(table, "xl/sharedStrings.xml")
}

/// One part of the rendered package, as text ("" if the package has no such
/// part).
fn package_part(table: &Table, name: &str) -> Result<String, RenderError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(render(table)?))?;
    let mut text = String::new();
    match zip.by_name(name) {
        Ok(mut part) => part.read_to_string(&mut text)?,
        Err(zip::result::ZipError::FileNotFound) => 0,
        Err(e) => return Err(e.into()),
    };
    Ok(text)
}

/// Excel's limit on the characters in a cell.
pub(crate) const MAX_CELL_CHARS: usize = 32_767;

/// Where the workbook loses text: cells cut at Excel's limit, and rich text
/// written plain because its markup is over the limit (rust_xlsxwriter counts a
/// rich string's run formatting toward it).
#[derive(Debug, Default)]
pub(crate) struct Losses {
    pub cut: Vec<Place>,
    pub plain: Vec<Place>,
}

/// What rendering `rt` loses (nothing if it can't be rendered at all: that is
/// an error, not a warning).
pub(crate) fn losses(rt: &ResolvedTable) -> Losses {
    workbook(rt).map(|(_, losses)| losses).unwrap_or_default()
}

/// A cell format, converted to a `rust_xlsxwriter::Format` when written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
struct Format {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    /// Font size in hundredths of a point (`None`: the default 11pt).
    size: Option<u32>,
    /// Font color, already flattened (no alpha in SpreadsheetML fonts here).
    color: Option<Color>,
    /// Font name (`None`: the default Calibri).
    font: Option<String>,
    /// Solid fill, already flattened.
    fill: Option<Color>,
    /// Horizontal alignment; `None` is Excel's "general" (text left, numbers
    /// right), which is also what a left-aligned column gets.
    align: Option<FormatAlign>,
    /// Vertical alignment; `None` is Excel's bottom.
    valign: Option<FormatAlign>,
    /// Border edges: left, right, top, bottom.
    border: [Option<Edge>; 4],
    /// Wrap text: set for text with line breaks, which Excel shows only when
    /// wrapping is on.
    wrap: bool,
    /// How a typed cell shows its value (`None`: General).
    num_fmt: Option<ValueFmt>,
}

/// How a typed cell shows its value: a number format, or a date and time
/// format's code.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ValueFmt {
    Number(NumFmt),
    Date(String),
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
    fn builtin(&self) -> Option<u8> {
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
    style: FormatBorder,
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
            (BorderStyle::Dashed, 0) => FormatBorder::Dashed,
            (BorderStyle::Dashed, _) => FormatBorder::MediumDashed,
            (BorderStyle::Dotted, _) => FormatBorder::Dotted,
            (BorderStyle::Double, _) => FormatBorder::Double,
            (_, 0) => FormatBorder::Thin,
            (_, 1) => FormatBorder::Medium,
            _ => FormatBorder::Thick,
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
            bold: style.bold_over(bold),
            italic: style.is_italic(),
            underline: style.is_underline(),
            strike: style.is_strike(),
            // Relative sizes resolve against Excel's default 11pt.
            size: style
                .size_pt(11.0)
                .map(|pt| (pt * 100.0).round().clamp(100.0, 40_900.0) as u32),
            color: style.paint().map(|c| c.flatten()),
            font: style.font_name().map(str::to_string),
            fill: style.fill().map(|c| c.flatten()),
            align: match align {
                HAlign::Right => Some(FormatAlign::Right),
                HAlign::Center => Some(FormatAlign::Center),
                HAlign::Justify => Some(FormatAlign::Justify),
                _ => None,
            },
            valign: style.cell_valign().map(|v| match v {
                CellVAlign::Top => FormatAlign::Top,
                CellVAlign::Middle => FormatAlign::VerticalCenter,
                CellVAlign::Bottom => FormatAlign::Bottom,
            }),
            border: [None; 4],
            wrap: false,
            num_fmt: None,
        }
    }

    /// A cell's format: `new` plus its border edges. In a right-to-left sheet,
    /// which Excel shows mirrored, a border's `left` and `right` are the cell's
    /// start and end, so borders (which are physical) swap. Horizontal `left` and
    /// `right` stay physical even then (ECMA-376, `ST_HorizontalAlignment`), but
    /// general alignment starts text on the right, so a left-aligned cell says
    /// so.
    fn cell(style: &ResolvedStyle, bold: bool, align: &HAlign, rtl: bool) -> Self {
        let b = &style.border;
        let edge = |e: &Option<ResolvedBorder>| e.as_ref().map(Edge::from);
        let (left, right) = if rtl {
            (&b.right, &b.left)
        } else {
            (&b.left, &b.right)
        };
        let base = Self::new(style, bold, align);
        Self {
            border: [edge(left), edge(right), edge(&b.top), edge(&b.bottom)],
            align: match (align, rtl) {
                (HAlign::Right | HAlign::Center | HAlign::Justify, _) | (_, false) => base.align,
                (_, true) => Some(FormatAlign::Left),
            },
            ..base
        }
    }

    fn to_xlsx(&self) -> rust_xlsxwriter::Format {
        let mut f = FontKey::of(self).apply(rust_xlsxwriter::Format::new());
        if let Some(c) = self.fill {
            f = f
                .set_pattern(FormatPattern::Solid)
                .set_background_color(xlsx_color(c));
        }
        for a in [self.align, self.valign].into_iter().flatten() {
            f = f.set_align(a);
        }
        let [left, right, top, bottom] = self.border;
        if let Some(e) = left {
            f = f.set_border_left(e.style);
            if let Some(c) = e.color {
                f = f.set_border_left_color(xlsx_color(c));
            }
        }
        if let Some(e) = right {
            f = f.set_border_right(e.style);
            if let Some(c) = e.color {
                f = f.set_border_right_color(xlsx_color(c));
            }
        }
        if let Some(e) = top {
            f = f.set_border_top(e.style);
            if let Some(c) = e.color {
                f = f.set_border_top_color(xlsx_color(c));
            }
        }
        if let Some(e) = bottom {
            f = f.set_border_bottom(e.style);
            if let Some(c) = e.color {
                f = f.set_border_bottom_color(xlsx_color(c));
            }
        }
        if self.wrap {
            f = f.set_text_wrap();
        }
        match &self.num_fmt {
            // Excel's built-in date formats follow the reader's locale, so
            // dates always get their own code.
            Some(ValueFmt::Number(n)) => match n.builtin() {
                Some(id) => f.set_num_format_index(id),
                None => f.set_num_format(n.code()),
            },
            Some(ValueFmt::Date(code)) => f.set_num_format(code),
            None => f,
        }
    }
}

fn xlsx_color(c: Color) -> rust_xlsxwriter::Color {
    rust_xlsxwriter::Color::RGB(u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b))
}

/// A font: a cell format's, or a rich-text run's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
struct FontKey {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    size: Option<u32>,
    color: Option<Color>,
    name: Option<String>,
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
            name: f.font.clone(),
        }
    }

    /// This font with a run's style laid over it (a relative size resolves
    /// against the font's size).
    fn with(self, style: &ResolvedStyle) -> Self {
        let parent = self.size.map_or(11.0, |h| f64::from(h) / 100.0);
        Self {
            bold: style.bold_over(self.bold),
            italic: style.italic_over(self.italic),
            underline: self.underline || style.is_underline(),
            strike: self.strike || style.is_strike(),
            size: style
                .size_pt(parent)
                .map(|pt| (pt * 100.0).round().clamp(100.0, 40_900.0) as u32)
                .or(self.size),
            color: style.paint().map(|c| c.flatten()).or(self.color),
            name: style.font_name().map(str::to_string).or(self.name),
        }
    }

    /// `f` with this font's properties (the default font's are left unset).
    fn apply(&self, mut f: rust_xlsxwriter::Format) -> rust_xlsxwriter::Format {
        if self.bold {
            f = f.set_bold();
        }
        if self.italic {
            f = f.set_italic();
        }
        if self.underline {
            f = f.set_underline(FormatUnderline::Single);
        }
        if self.strike {
            f = f.set_font_strikethrough();
        }
        if let Some(h) = self.size {
            f = f.set_font_size(f64::from(h) / 100.0);
        }
        if let Some(c) = self.color {
            f = f.set_font_color(xlsx_color(c));
        }
        if let Some(name) = &self.name {
            f = f.set_font_name(name);
        }
        f
    }

    /// A rich-text run's format. Each run names its font, even the default
    /// Calibri: a run without properties would take the cell's font, and
    /// Excel doesn't inherit run formatting from it.
    fn run_format(&self, superscript: bool) -> rust_xlsxwriter::Format {
        let mut f = self
            .apply(rust_xlsxwriter::Format::new())
            .set_font_name(self.name.as_deref().unwrap_or("Calibri"));
        if superscript {
            f = f.set_font_script(FormatScript::Superscript);
        }
        f
    }
}

/// The workbook and what it loses of the table's text.
fn workbook(rt: &ResolvedTable) -> Result<(Workbook, Losses), RenderError> {
    let mut workbook = Workbook::new();
    // The same table always makes the same file: the creation date is fixed
    // (the zip epoch, as the other OOXML writers' timestamps), not "now".
    workbook.set_properties(
        &DocProperties::new().set_creation_datetime(&ExcelDateTime::from_ymd(1980, 1, 1)?),
    );
    let ws = workbook.add_worksheet();
    ws.set_name("Table")?;
    // A right-to-left sheet: column A is on the right.
    if rt.is_rtl() {
        ws.set_right_to_left(true);
    }

    // Column widths, in characters of the default font (7px each): a column's
    // own, or 12 (`None`) and room for the widest image drawn alone in one of
    // its cells, as HTML's auto widths grow to fit an image.
    let fixed: Vec<Option<f64>> = rt
        .columns
        .iter()
        .map(|col| {
            col.width
                .as_ref()
                .and_then(|w| w.to_pt(12.0, 12.0))
                .map(|pt| pt / 0.75 / 7.0)
                .filter(|w| w.is_finite() && *w > 0.0 && *w <= 255.0)
        })
        .collect();
    let mut col_chars: Vec<f64> = fixed.iter().map(|w| w.unwrap_or(12.0)).collect();
    let sections =
        std::iter::once(&rt.head).chain(rt.groups.iter().flat_map(|g| [&g.rows, &g.summary_rows]));
    for row in sections.flat_map(|s| &s.rows) {
        for cell in row.cells() {
            if cell.colspan > 1 || fixed[cell.col].is_some() {
                continue;
            }
            if let Some(pic) = picture(&cell.content, font_pt(&cell.style)) {
                let chars = f64::from(pic.px.0 + 2 * PAD_PX) / 7.0;
                col_chars[cell.col] = col_chars[cell.col].max(chars.min(255.0));
            }
        }
    }
    let mut col_px = Vec::with_capacity(col_chars.len());
    for (i, chars) in col_chars.iter().enumerate() {
        // A width of 0 hides a column: the narrowest drawn is 1px.
        let px = ((chars * 7.0).round() as u32).max(1);
        ws.set_column_width_pixels(col_num(i)?, px)?;
        col_px.push(px);
    }

    let mut sheet = Sheet {
        ws,
        formats: HashMap::new(),
        run_formats: HashMap::new(),
        row: 0,
        col_chars,
        col_px,
        rtl: rt.is_rtl(),
        losses: Losses::default(),
    };

    // Title and subtitle lines.
    let header = &rt.header;
    if let Some(title) = &header.title {
        let f = Format::new(&title.style, true, &HAlign::Left);
        sheet.text_row(
            rich(rt, &title.content, FontKey::of(&f)),
            picture(&title.content, font_pt(&title.style)),
            f,
            false,
            Place::Title,
        )?;
    }
    if let Some(line) = &header.subtitle {
        let f = Format::new(&line.style, false, &HAlign::Left);
        sheet.text_row(
            rich(rt, &line.content, FontKey::of(&f)),
            picture(&line.content, font_pt(&line.style)),
            f,
            false,
            Place::Subtitle,
        )?;
    }
    for (i, line) in header.extra_lines.iter().enumerate() {
        let f = Format::new(&line.style, false, &HAlign::Left);
        sheet.text_row(
            rich(rt, &line.content, FontKey::of(&f)),
            picture(&line.content, font_pt(&line.style)),
            f,
            false,
            Place::HeaderLine(i),
        )?;
    }

    if !rt.is_empty() {
        sheet.section(rt, &rt.head, true, None)?;
        for (g, group) in rt.groups.iter().enumerate() {
            if let Some(label) = &group.label {
                // One cell merged across every column.
                let f = Format::new(&label.style, true, &HAlign::Left);
                sheet.text_row(
                    rich(rt, &label.content, FontKey::of(&f)),
                    picture(&label.content, font_pt(&label.style)),
                    f,
                    rt.columns.len() > 1,
                    Place::GroupLabel(g),
                )?;
            }
            sheet.section(rt, &group.rows, false, Some(g))?;
            sheet.section(rt, &group.summary_rows, true, Some(g))?;
        }
    }

    // Notes, each block after a blank row.
    let footer = &rt.footer;
    if !footer.footnotes.is_empty() {
        sheet.row += 1;
        for (i, note) in footer.footnotes.iter().enumerate() {
            let f = Format::new(&note.style, false, &HAlign::Left);
            let font = FontKey::of(&f);
            let mut runs = vec![
                RichRun {
                    text: note.mark.to_string(),
                    font: font.clone(),
                    superscript: true,
                },
                RichRun {
                    text: " ".into(),
                    font: font.clone(),
                    superscript: false,
                },
            ];
            runs.extend(rich(rt, &note.content, font));
            // The mark goes with the note's text: an image here shows its alt.
            sheet.text_row(runs, None, f, false, Place::Footnote(i))?;
        }
    }
    if !footer.source_notes.is_empty() {
        sheet.row += 1;
        for (i, note) in footer.source_notes.iter().enumerate() {
            let f = Format::new(&note.style, false, &HAlign::Left);
            sheet.text_row(
                rich(rt, &note.content, FontKey::of(&f)),
                picture(&note.content, font_pt(&note.style)),
                f,
                false,
                Place::SourceNote(i),
            )?;
        }
    }

    let losses = sheet.losses;
    Ok((workbook, losses))
}

fn col_num(col: usize) -> Result<ColNum, XlsxError> {
    ColNum::try_from(col).map_err(|_| XlsxError::RowColumnLimitError)
}

fn row_num(row: usize) -> Result<RowNum, XlsxError> {
    RowNum::try_from(row).map_err(|_| XlsxError::RowColumnLimitError)
}

/// Writes rows, tracking the current row and what is lost.
struct Sheet<'w> {
    ws: &'w mut Worksheet,
    /// Converted formats, by cell format and by run font.
    formats: HashMap<Format, rust_xlsxwriter::Format>,
    run_formats: HashMap<(FontKey, bool), rust_xlsxwriter::Format>,
    /// The next row, from 0.
    row: usize,
    /// Column widths in characters, for estimating where wrapped text breaks.
    col_chars: Vec<f64>,
    /// Column widths as set, in pixels, for placing images.
    col_px: Vec<u32>,
    rtl: bool,
    losses: Losses,
}

impl Sheet<'_> {
    fn format(&mut self, f: &Format) -> rust_xlsxwriter::Format {
        self.formats
            .entry(f.clone())
            .or_insert_with(|| f.to_xlsx())
            .clone()
    }

    /// A row with one text cell in column A (`merged`: across every column), or
    /// with `pic` drawn over it.
    fn text_row(
        &mut self,
        mut runs: Vec<RichRun>,
        pic: Option<Picture>,
        mut f: Format,
        merged: bool,
        place: Place,
    ) -> Result<(), RenderError> {
        let row = self.row;
        if pic.is_some() {
            runs.clear();
        }
        f.wrap = has_breaks(&runs);
        let width = if merged {
            self.col_chars.iter().sum()
        } else {
            self.col_chars.first().copied().unwrap_or(12.0)
        };
        let mut height = text_height(&runs, f.wrap.then_some(width));
        if let Some(pic) = &pic {
            height = height.max(pic.row_pt());
        }
        self.set_height(row, height)?;
        let last = if merged { self.col_chars.len() - 1 } else { 0 };
        self.write_text(row, 0, row, last, &mut runs, &f, place)?;
        if let Some(pic) = &pic {
            self.draw(row, 0, last, Some(height), pic, &f)?;
        }
        self.row += 1;
        Ok(())
    }

    /// A section's rows. Each cell sits at its visible column; a spanning cell
    /// is merged over the positions it covers.
    fn section(
        &mut self,
        rt: &ResolvedTable,
        section: &Section,
        strong: bool,
        group: Option<usize>,
    ) -> Result<(), RenderError> {
        for (i, r) in section.rows.iter().enumerate() {
            let row = self.row;
            let mut height = 0.0f64;
            let mut pics = Vec::new();
            for cell in r.cells() {
                let mut format = Format::cell(&cell.style, strong, &cell.align, rt.is_rtl());
                let mut runs = rich(rt, &cell.content, FontKey::of(&format));
                let pic = picture(&cell.content, font_pt(&cell.style));
                if let Some(pic) = &pic {
                    runs.clear();
                    if cell.rowspan == 1 {
                        height = height.max(pic.row_pt());
                    }
                }
                format.wrap = has_breaks(&runs);
                // A spanned cell's text spreads over its rows: leave them be.
                if cell.rowspan == 1 {
                    let width = self.col_chars[cell.col..cell.col + cell.colspan]
                        .iter()
                        .sum();
                    height = height.max(text_height(&runs, format.wrap.then_some(width)));
                }
                let (last_row, last_col) = (row + cell.rowspan - 1, cell.col + cell.colspan - 1);

                // A typed value shows as the table displays it; a display no
                // format reproduces stays text.
                let typed = cell
                    .typed_value
                    .zip(plain_display(&cell.content))
                    .and_then(|(t, shown)| typed_cell(t, &shown));
                if let Some((value, num_fmt)) = typed {
                    let f = self.format(&Format { num_fmt, ..format });
                    self.merge(row, cell.col, last_row, last_col, &f)?;
                    let (r, c) = (row_num(row)?, col_num(cell.col)?);
                    match value {
                        Value::Number(v) => self.ws.write_number_with_format(r, c, v, &f)?,
                        Value::Boolean(b) => self.ws.write_boolean_with_format(r, c, b, &f)?,
                    };
                    continue;
                }
                let place = Place::Cell {
                    section: section.kind,
                    group,
                    row: i,
                    col: cell.grid_col,
                };
                self.write_text(row, cell.col, last_row, last_col, &mut runs, &format, place)?;
                if let Some(pic) = pic {
                    // A spanned cell's height isn't known yet: its image sits
                    // at the top.
                    pics.push((cell.col, last_col, cell.rowspan == 1, pic, format));
                }
            }
            self.set_height(row, height)?;
            for (col, last_col, one_row, pic, format) in pics {
                let height = one_row.then_some(height);
                self.draw(row, col, last_col, height, &pic, &format)?;
            }
            self.row += 1;
        }
        Ok(())
    }

    /// Merge a spanning cell's range, which gives every position in it the
    /// cell's format (so its borders and fill reach the range's edges).
    fn merge(
        &mut self,
        row: usize,
        col: usize,
        last_row: usize,
        last_col: usize,
        f: &rust_xlsxwriter::Format,
    ) -> Result<(), RenderError> {
        if (row, col) != (last_row, last_col) {
            self.ws.merge_range(
                row_num(row)?,
                col_num(col)?,
                row_num(last_row)?,
                col_num(last_col)?,
                "",
                f,
            )?;
        }
        Ok(())
    }

    /// A text cell, merged up to `(last_row, last_col)`. An empty string with a
    /// format still writes the cell so its fill shows. Text whose runs all have
    /// the cell's font is a plain string; otherwise it is rich text, each run
    /// with its full font. Text over Excel's limit is cut; rich text whose
    /// markup is over it is written plain.
    #[allow(clippy::too_many_arguments)]
    fn write_text(
        &mut self,
        row: usize,
        col: usize,
        last_row: usize,
        last_col: usize,
        runs: &mut Vec<RichRun>,
        format: &Format,
        place: Place,
    ) -> Result<(), RenderError> {
        let f = self.format(format);
        self.merge(row, col, last_row, last_col, &f)?;
        if cut(runs, MAX_CELL_CHARS) {
            self.losses.cut.push(place);
        }
        let (r, c) = (row_num(row)?, col_num(col)?);
        let text: String = runs.iter().map(|r| r.text.as_str()).collect();
        if text.is_empty() {
            if *format != Format::default() {
                self.ws.write_blank(r, c, &f)?;
            }
            return Ok(());
        }
        let font = FontKey::of(format);
        if runs.iter().all(|r| r.font == font && !r.superscript) {
            self.ws.write_string_with_format(r, c, &text, &f)?;
            return Ok(());
        }
        let run_formats: Vec<rust_xlsxwriter::Format> = runs
            .iter()
            .map(|run| {
                self.run_formats
                    .entry((run.font.clone(), run.superscript))
                    .or_insert_with(|| run.font.run_format(run.superscript))
                    .clone()
            })
            .collect();
        let segments: Vec<(&rust_xlsxwriter::Format, &str)> = run_formats
            .iter()
            .zip(runs.iter())
            .map(|(f, run)| (f, run.text.as_str()))
            .collect();
        match self.ws.write_rich_string_with_format(r, c, &segments, &f) {
            Err(XlsxError::MaxStringLengthExceeded) => {
                self.losses.plain.push(place);
                self.ws.write_string_with_format(r, c, &text, &f)?;
            }
            other => {
                other?;
            }
        }
        Ok(())
    }

    /// Set a row's height when its text needs more than the default.
    fn set_height(&mut self, row: usize, pt: f64) -> Result<(), RenderError> {
        if pt > DEFAULT_ROW_PT + 0.01 {
            self.ws.set_row_height(row_num(row)?, pt.min(MAX_ROW_PT))?;
        }
        Ok(())
    }

    /// Draw `pic` over the cell at `row` spanning columns `col..=last_col`,
    /// placed as the cell's text would be: by its alignment across the cell,
    /// and down a row `height` points tall (as asked of [`Self::set_height`];
    /// `None`: a spanned cell, whose image sits at the top). A zero-sized image
    /// draws nothing.
    fn draw(
        &mut self,
        row: usize,
        col: usize,
        last_col: usize,
        height: Option<f64>,
        pic: &Picture,
        f: &Format,
    ) -> Result<(), RenderError> {
        let (w, h) = pic.px;
        if w == 0 || h == 0 {
            return Ok(());
        }
        let free = self.col_px[col..=last_col]
            .iter()
            .sum::<u32>()
            .saturating_sub(w);
        let pad = PAD_PX.min(free);
        let from_left = match f.align {
            Some(FormatAlign::Right) => free - pad,
            Some(FormatAlign::Center) => free / 2,
            _ => pad,
        };
        // A right-to-left sheet is drawn mirrored, offsets too: they're
        // measured from the cell's right edge.
        let x = if self.rtl {
            free - from_left
        } else {
            from_left
        };
        let y = match height {
            Some(pt) => {
                let row_px = (pt.clamp(DEFAULT_ROW_PT, MAX_ROW_PT) / 0.75).round() as u32;
                let free = row_px.saturating_sub(h);
                let pad = PAD_PX.min(free);
                match f.valign {
                    Some(FormatAlign::Top) => pad,
                    Some(FormatAlign::VerticalCenter) => free / 2,
                    _ => free - pad,
                }
            }
            None => PAD_PX,
        };
        self.ws
            .insert_image_with_offset(row_num(row)?, col_num(col)?, &pic.image, x, y)?;
        Ok(())
    }
}

/// An image drawn over a cell, `px` (width, height) in pixels.
pub(crate) struct Picture {
    image: rust_xlsxwriter::Image,
    px: (u32, u32),
}

impl Picture {
    /// The row height that holds the image, with room around it.
    fn row_pt(&self) -> f64 {
        f64::from(self.px.1 + 2 * PAD_PX) * 0.75
    }
}

/// The room left between an image and its cell's edges, in pixels.
const PAD_PX: u32 = 2;

/// Excel's maximum row height.
const MAX_ROW_PT: f64 = 409.0;

/// The image content draws, if it is one image (blank text aside) embedded as
/// a PNG, JPEG or GIF `data:` URI, at its size in the content's font
/// (`font_pt`, for `em` sizes). A cell can't hold an image beside text, so
/// any other content shows its images' alt text.
pub(crate) fn picture(nodes: &[ContentNode], font_pt: f64) -> Option<Picture> {
    let mut found = None;
    for node in nodes {
        match node {
            ContentNode::Text { value } | ContentNode::StyledText { value, .. }
                if value.trim().is_empty() => {}
            ContentNode::Image {
                src,
                alt,
                width,
                height,
            } if found.is_none() => found = Some((src, alt, width, height)),
            _ => return None,
        }
    }
    let (src, alt, width, height) = found?;
    let img = gridwell_core::Image::from_src(src).filter(|i| i.format != ImageFormat::Svg)?;
    let (w, h) = img.size_pt(width.as_deref(), height.as_deref(), font_pt);
    let px = ((w / 0.75).round() as u32, (h / 0.75).round() as u32);
    if !reads_safely(&img.data) {
        return None;
    }
    let mut image = rust_xlsxwriter::Image::new_from_buffer(&img.data).ok()?;
    if image.width() <= 0.0 || image.height() <= 0.0 {
        return None;
    }
    // Excel sizes an image by its pixels at its DPI (96 unless the file says).
    let (sx, sy) = (
        f64::from(px.0) * image.width_dpi() / 96.0 / image.width(),
        f64::from(px.1) * image.height_dpi() / 96.0 / image.height(),
    );
    image = image.set_scale_width(sx).set_scale_height(sy);
    if let Some(alt) = alt.as_deref().filter(|a| !a.is_empty()) {
        image = image.set_alt_text(alt);
    }
    Some(Picture { image, px })
}

/// Whether `rust_xlsxwriter::Image::new_from_buffer` (0.96) can read `data`
/// without indexing past its end. It walks a file's headers without checking
/// their lengths, so a damaged or truncated image (from an untrusted table)
/// would panic. This follows the same walk, checking each read; review it on
/// upgrading rust_xlsxwriter.
fn reads_safely(data: &[u8]) -> bool {
    // It sniffs bytes 0..4 whatever the format.
    if data.len() < 4 {
        return false;
    }
    let u16_at = |i: usize| u16::from_be_bytes([data[i], data[i + 1]]);
    let u32_at = |i: usize| u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    if &data[1..4] == b"PNG" {
        // Chunks from byte 8 to IEND: the length and type, IHDR's size, and
        // pHYs's density and unit.
        let mut at = 8usize;
        while at < data.len() {
            if at + 8 > data.len() {
                return false;
            }
            let kind = &data[at + 4..at + 8];
            let needs = match kind {
                b"IHDR" => 16,
                b"pHYs" => 17,
                _ => 8,
            };
            if at + needs > data.len() {
                return false;
            }
            if kind == b"IEND" {
                return true;
            }
            at = match (at + 12).checked_add(u32_at(at) as usize) {
                Some(next) => next,
                None => return false,
            };
        }
        true
    } else if u16_at(0) == 0xFFD8 {
        // Segments from byte 2 to the scan: the marker and length, a frame
        // header's size, and JFIF's density and unit.
        let mut at = 2usize;
        while at < data.len() {
            if at + 2 > data.len() {
                return false;
            }
            let marker = u16_at(at);
            let frame = marker & 0xFFF0 == 0xFFC0 && ![0xFFC4, 0xFFC8, 0xFFCC].contains(&marker);
            let needs = match marker {
                _ if frame => 9,
                0xFFE0 => 16,
                _ => 4,
            };
            if at + needs > data.len() {
                return false;
            }
            if marker == 0xFFDA {
                return true;
            }
            at += 2 + usize::from(u16_at(at + 2));
        }
        true
    } else if &data[..2] == b"BM" {
        data.len() >= 26
    } else if &data[..4] == b"GIF8" {
        data.len() >= 10
    } else {
        // Unknown: an error, not a panic.
        true
    }
}

/// The font size content is set in: its style's, or Excel's default 11pt.
pub(crate) fn font_pt(style: &ResolvedStyle) -> f64 {
    style.size_pt(11.0).unwrap_or(11.0)
}

/// Cut `runs` to at most `max` characters, dropping empty runs (a rich string
/// has none). True if any text was cut.
fn cut(runs: &mut Vec<RichRun>, max: usize) -> bool {
    let mut left = max;
    let mut was_cut = false;
    for run in runs.iter_mut() {
        match run.text.char_indices().nth(left) {
            Some((end, _)) => {
                run.text.truncate(end);
                was_cut = true;
                left = 0;
            }
            None => left -= run.text.chars().count(),
        }
    }
    runs.retain(|r| !r.text.is_empty());
    was_cut
}

/// A typed cell's value.
enum Value {
    Number(f64),
    Boolean(bool),
}

/// A typed value as a cell holds it, and the format that shows it as `shown`.
/// A number (`NumFmt::infer`) or a date or time (`Moment::format`, a serial
/// number) needs a format that reproduces its display; a boolean shows as
/// Excel's `TRUE` or `FALSE`, so its display must be that word, in any case.
/// Anything else is `None`: the cell keeps its text.
fn typed_cell(t: &TypedValue, shown: &str) -> Option<(Value, Option<ValueFmt>)> {
    match t.value_type {
        ValueType::Number | ValueType::Integer => {
            let v = t.value.as_f64().filter(|v| v.is_finite())?;
            let num_fmt = NumFmt::infer(shown, v)?;
            Some((Value::Number(v), Some(ValueFmt::Number(num_fmt))))
        }
        ValueType::Boolean => {
            let b = t.value.as_bool()?;
            let word = if b { "true" } else { "false" };
            shown
                .eq_ignore_ascii_case(word)
                .then_some((Value::Boolean(b), None))
        }
        ValueType::Date | ValueType::Datetime | ValueType::Time => {
            let moment = Moment::parse(&t.value_type, t.value.as_str()?)?;
            let code = moment.format(shown)?;
            Some((Value::Number(moment.serial()), Some(ValueFmt::Date(code))))
        }
        _ => None,
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
        font: font.clone(),
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
                    font: font.clone().with(&style),
                    superscript: false,
                });
            }
            ContentNode::FootnoteMark { mark_text, .. } => runs.push(RichRun {
                text: mark_text.clone(),
                font: font.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use gridwell_testkit::images::{LOGO_GIF, LOGO_PNG, WIDE_JPEG};

    /// Whether rust_xlsxwriter panics reading `data`.
    fn panics(data: &[u8]) -> bool {
        let data = data.to_vec();
        std::panic::catch_unwind(|| rust_xlsxwriter::Image::new_from_buffer(&data)).is_err()
    }

    #[test]
    fn reads_safely_is_whether_rust_xlsxwriter_can_read_an_image() {
        // A BMP header, cut at every byte.
        let bmp = [&b"BM"[..], &[1; 24]].concat();
        let mut inputs: Vec<Vec<u8>> = (0..=bmp.len()).map(|n| bmp[..n].to_vec()).collect();
        for src in [LOGO_PNG, WIDE_JPEG, LOGO_GIF] {
            let data = gridwell_core::Image::from_src(src).unwrap().data;
            assert!(reads_safely(&data));
            inputs.extend((0..=data.len()).map(|n| data[..n].to_vec()));
            // A byte changed, in the headers.
            for i in 0..data.len().min(64) {
                for b in [0x00, 0xFF, 0x10] {
                    let mut d = data.clone();
                    d[i] = b;
                    inputs.push(d);
                }
            }
        }
        // A pHYs chunk cut short, and a second IHDR.
        let png = gridwell_core::Image::from_src(LOGO_PNG).unwrap().data;
        inputs.push([&png[..33], &[0, 0, 0, 9], b"pHYs", &[0; 8]].concat());
        inputs.push([&png[..33], &[0, 0, 0, 13], b"IHDR", &[0, 0, 0, 1]].concat());
        let mut unsafe_inputs = 0;
        for d in &inputs {
            let safe = reads_safely(d);
            assert_eq!(safe, !panics(d), "{d:?}");
            unsafe_inputs += usize::from(!safe);
        }
        assert!(unsafe_inputs > 100, "{unsafe_inputs}");
    }
}
