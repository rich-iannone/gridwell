use gridwell_core::xml::escape as escape_xml;
use gridwell_core::{Color, Image, ImageFormat, LanguageTag};
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table, VMerge};
use gridwell_layout::{
    resolve, CellVAlign, MergeCell, ResolvedStyle, ResolvedTable, Section, Sides, Stroke,
};
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
    let rt = resolve(table);
    let mut buf = String::with_capacity(8192);
    let mut media = Media::default();
    write_document_xml(&mut buf, &rt, &mut media)?;
    let names: Vec<String> = media.images.iter().map(|m| m.target()).collect();
    let formats: Vec<ImageFormat> = media.images.iter().map(|m| m.format).collect();
    let rels: Vec<(String, &str)> = media
        .images
        .iter()
        .zip(&names)
        .map(|(m, name)| (m.rel_id(), name.as_str()))
        .collect();
    let content_types = xml::content_types(&formats);
    let document_rels = xml::document_rels(&rels);
    let styles_xml = render_styles_xml(table);
    let paths: Vec<String> = names.iter().map(|n| format!("word/{n}")).collect();
    let mut parts = vec![
        Part::new("[Content_Types].xml", content_types.as_str()),
        Part::new("_rels/.rels", xml::RELS),
        Part::new("word/_rels/document.xml.rels", document_rels.as_str()),
        Part::new("word/document.xml", buf.as_str()),
        Part::new("word/styles.xml", styles_xml.as_str()),
    ];
    for (m, path) in media.images.iter().zip(&paths) {
        parts.push(Part::new(path, &m.data));
    }
    Ok(package(&parts)?)
}

/// The images the document embeds, each once (`word/media/imageN.ext`), and how
/// many drawings show them.
#[derive(Default)]
struct Media {
    images: Vec<MediaImage>,
    drawings: u32,
}

struct MediaImage {
    /// 1-based.
    n: usize,
    format: ImageFormat,
    data: Vec<u8>,
}

impl MediaImage {
    fn rel_id(&self) -> String {
        format!("rIdImage{}", self.n)
    }

    /// The part's name, relative to `word/`.
    fn target(&self) -> String {
        format!("media/image{}.{}", self.n, self.format.extension())
    }
}

impl Media {
    /// The relationship id of `image`, adding it if new.
    fn add(&mut self, image: Image) -> String {
        if let Some(m) = self.images.iter().find(|m| m.data == image.data) {
            return m.rel_id();
        }
        let m = MediaImage {
            n: self.images.len() + 1,
            format: image.format,
            data: image.data,
        };
        let id = m.rel_id();
        self.images.push(m);
        id
    }
}

/// The `word/styles.xml` content: the paragraph styles, and the table's
/// language as the document default.
pub fn render_styles_xml(table: &Table) -> String {
    let lang = table
        .config
        .lang
        .as_deref()
        .and_then(|l| LanguageTag::parse(l).ok());
    xml::styles(lang.as_ref())
}

/// Render only the document.xml content (for snapshot testing).
pub fn render_document_xml(table: &Table) -> Result<String, RenderError> {
    let rt = resolve(table);
    let mut buf = String::with_capacity(8192);
    write_document_xml(&mut buf, &rt, &mut Media::default())?;
    Ok(buf)
}

fn write_document_xml(
    buf: &mut String,
    rt: &ResolvedTable,
    media: &mut Media,
) -> Result<(), RenderError> {
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

    write_title(buf, rt, media)?;
    // A table with no grid columns is invalid OOXML: with every column hidden,
    // only the title and notes remain.
    if !rt.is_empty() {
        write_table(buf, rt, media)?;
    }
    write_footnotes(buf, rt, media)?;

    buf.push_str("</w:body>\n");
    buf.push_str("</w:document>\n");
    Ok(())
}

/// Run formatting.
#[derive(Debug, Clone, Copy, Default)]
struct Fmt<'s> {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<Color>,
    /// Font size in half-points (`w:sz`).
    half_points: Option<u32>,
    superscript: bool,
    /// The font (`w:rFonts`); `None` leaves Word's default.
    font: Option<&'s str>,
    /// The paragraph style's size in half-points, which relative sizes resolve
    /// against when the runs set none (`None`: the document's 11pt).
    inherited: Option<u32>,
}

/// The size relative font sizes resolve against: the document default, 11pt.
const BASE_PT: f64 = xml::BODY_HALF_POINTS as f64 / 2.0;

impl<'s> Fmt<'s> {
    /// This format with a style's weight, slant, decoration, color, size and
    /// font laid over it (a relative size resolves against the size so far).
    fn with(self, style: &'s ResolvedStyle) -> Self {
        let parent = self
            .half_points
            .or(self.inherited)
            .map_or(BASE_PT, |h| f64::from(h) / 2.0);
        Self {
            bold: style.bold_over(self.bold),
            italic: style.italic_over(self.italic),
            underline: self.underline || style.is_underline(),
            strike: self.strike || style.is_strike(),
            color: style.paint().or(self.color),
            half_points: style
                .size_pt(parent)
                .map(|pt| (pt * 2.0).round().clamp(2.0, 3276.0) as u32)
                .or(self.half_points),
            superscript: self.superscript,
            font: style.font_name().or(self.font),
            inherited: self.inherited,
        }
    }

    fn is_plain(&self) -> bool {
        !(self.bold
            || self.italic
            || self.underline
            || self.strike
            || self.color.is_some()
            || self.half_points.is_some()
            || self.superscript
            || self.font.is_some())
    }
}

fn write_title(buf: &mut String, rt: &ResolvedTable, media: &mut Media) -> Result<(), RenderError> {
    let header = &rt.header;
    if let Some(title) = &header.title {
        open_paragraph(buf, rt, Some("Title"), None);
        let fmt = Fmt {
            bold: true,
            inherited: Some(xml::TITLE_HALF_POINTS),
            ..Fmt::default()
        };
        write_content(buf, rt, media, &title.content, fmt.with(&title.style))?;
        buf.push_str("</w:p>\n");
    }
    for line in header.subtitle.iter().chain(&header.extra_lines) {
        open_paragraph(buf, rt, Some("Subtitle"), None);
        let fmt = Fmt {
            inherited: Some(xml::SUBTITLE_HALF_POINTS),
            ..Fmt::default()
        };
        write_content(buf, rt, media, &line.content, fmt.with(&line.style))?;
        buf.push_str("</w:p>\n");
    }
    Ok(())
}

fn write_table(buf: &mut String, rt: &ResolvedTable, media: &mut Media) -> Result<(), RenderError> {
    let col_widths = compute_col_widths(rt);

    buf.push_str("<w:tbl>\n");

    buf.push_str("<w:tblPr>");
    buf.push_str("<w:tblStyle w:val=\"TableGrid\"/>");
    // Right to left: the first cell of a row is the rightmost.
    if rt.is_rtl() {
        buf.push_str("<w:bidiVisual/>");
    }
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

    write_section(buf, rt, media, &rt.head, &col_widths, true)?;
    for group in &rt.groups {
        if let Some(label) = &group.label {
            write_group_label_row(buf, rt, media, label, &col_widths)?;
        }
        write_section(buf, rt, media, &group.rows, &col_widths, false)?;
        write_section(buf, rt, media, &group.summary_rows, &col_widths, false)?;
    }

    buf.push_str("</w:tbl>\n");
    Ok(())
}

/// Write one section. Spans are resolved per section, so a `vMerge` range never
/// crosses a section boundary.
fn write_section(
    buf: &mut String,
    rt: &ResolvedTable,
    media: &mut Media,
    section: &Section,
    col_widths: &[u32],
    is_header: bool,
) -> Result<(), RenderError> {
    for cells in section.continuation_rows() {
        write_row(buf, rt, media, &cells, col_widths, is_header)?;
    }
    Ok(())
}

/// Write one `<w:tr>`. Each emitted cell carries its visible column, so `gridSpan`
/// and widths come from the grid, and the `gridSpan`s of a row sum to the number
/// of visible columns.
fn write_row(
    buf: &mut String,
    rt: &ResolvedTable,
    media: &mut Media,
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
        if let Some(owner) = mc.owner {
            // Mirrored before the mask: a span's start edge is its outline's left.
            let strokes = owner.style.strokes(BASE_PT).mirrored(rt.is_rtl());
            write_borders(buf, &strokes.masked(mc.outline))?;
        }
        // Word aligns a vertically merged cell by its first piece; every piece
        // carries it, as every piece carries the owner's borders.
        let valign = mc.owner.and_then(|o| o.style.cell_valign());

        let Some(cell) = mc.cell else {
            // Continuation (or empty position): properties only, empty paragraph.
            write_valign(buf, valign);
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
        write_valign(buf, valign);
        buf.push_str("</w:tcPr>");

        // In a right-to-left paragraph Word and LibreOffice read `left` and
        // `right` as its start and end, so alignment, which is physical, swaps;
        // and a left-aligned cell must say so (unset is the start).
        let jc = match (&cell.align, rt.is_rtl()) {
            (HAlign::Center, _) => Some("center"),
            (HAlign::Justify, _) => Some("both"),
            (HAlign::Right, false) => Some("right"),
            (HAlign::Right, true) => Some("left"),
            (_, false) => None,
            (_, true) => Some("right"),
        };
        open_paragraph(buf, rt, None, jc);
        let fmt = Fmt {
            bold: is_header,
            ..Fmt::default()
        };
        write_content(buf, rt, media, &cell.content, fmt.with(&cell.style))?;
        buf.push_str("</w:p>");
        buf.push_str("</w:tc>");
    }

    buf.push_str("</w:tr>\n");
    Ok(())
}

fn write_group_label_row(
    buf: &mut String,
    rt: &ResolvedTable,
    media: &mut Media,
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
    write_borders(buf, &label.style.strokes(BASE_PT).mirrored(rt.is_rtl()))?;
    let fill = label
        .style
        .fill()
        .map_or("F0F0F0".into(), |c| c.flatten().to_rrggbb());
    write!(
        buf,
        "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/>"
    )?;
    buf.push_str("</w:tcPr>");
    open_paragraph(buf, rt, None, None);
    let fmt = Fmt {
        bold: true,
        ..Fmt::default()
    };
    write_content(buf, rt, media, &label.content, fmt.with(&label.style))?;
    buf.push_str("</w:p>");
    buf.push_str("</w:tc>");
    buf.push_str("</w:tr>\n");
    Ok(())
}

/// Open a `<w:p>` with its properties: a paragraph style, right to left (`w:bidi`)
/// when the table is, and a justification. Unset, a paragraph starts on the left,
/// or on the right when right to left.
fn open_paragraph(buf: &mut String, rt: &ResolvedTable, style: Option<&str>, jc: Option<&str>) {
    let bidi = rt.is_rtl();
    if style.is_none() && jc.is_none() && !bidi {
        buf.push_str("<w:p>");
        return;
    }
    // CT_PPr order: pStyle, …, bidi, …, jc.
    buf.push_str("<w:p><w:pPr>");
    if let Some(style) = style {
        buf.push_str("<w:pStyle w:val=\"");
        buf.push_str(style);
        buf.push_str("\"/>");
    }
    if bidi {
        buf.push_str("<w:bidi/>");
    }
    if let Some(jc) = jc {
        buf.push_str("<w:jc w:val=\"");
        buf.push_str(jc);
        buf.push_str("\"/>");
    }
    buf.push_str("</w:pPr>");
}

/// `w:vAlign`, last in `w:tcPr` among what the writer sets. Unset is Word's top.
fn write_valign(buf: &mut String, valign: Option<CellVAlign>) {
    let val = match valign {
        Some(CellVAlign::Top) => "top",
        Some(CellVAlign::Middle) => "center",
        Some(CellVAlign::Bottom) => "bottom",
        None => return,
    };
    buf.push_str("<w:vAlign w:val=\"");
    buf.push_str(val);
    buf.push_str("\"/>");
}

/// A cell's own borders (`w:tcBorders`), which replace the table's grid lines on
/// the sides they set; sides without one keep the grid line. `left` and `right`
/// are the cell's start and end, so a right-to-left (`bidiVisual`) table's
/// strokes come mirrored.
fn write_borders(buf: &mut String, strokes: &Sides<Option<Stroke>>) -> Result<(), RenderError> {
    if *strokes == Sides::default() {
        return Ok(());
    }
    buf.push_str("<w:tcBorders>");
    // CT_TcBorders order: top, left (start), bottom, right (end).
    for (side, stroke) in [
        ("top", &strokes.top),
        ("left", &strokes.left),
        ("bottom", &strokes.bottom),
        ("right", &strokes.right),
    ] {
        let Some(k) = stroke else { continue };
        let val = match k.style {
            BorderStyle::Dashed => "dashed",
            BorderStyle::Dotted => "dotted",
            BorderStyle::Double => "double",
            _ => "single",
        };
        // `w:sz` is in eighths of a point; Word allows 2 to 96.
        let sz = (k.width_pt * 8.0).round().clamp(2.0, 96.0) as u32;
        let color = k.color.map_or("auto".into(), |c| c.flatten().to_rrggbb());
        write!(
            buf,
            "<w:{side} w:val=\"{val}\" w:sz=\"{sz}\" w:space=\"0\" w:color=\"{color}\"/>"
        )?;
    }
    buf.push_str("</w:tcBorders>");
    Ok(())
}

/// Content as runs: styled text gets its own formatting, line breaks become
/// `<w:br/>`, footnote marks are superscript, images are inline drawings (or
/// their alt text, if they can't be embedded).
fn write_content(
    buf: &mut String,
    rt: &ResolvedTable,
    media: &mut Media,
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
            ContentNode::Image {
                src,
                alt,
                width,
                height,
            } => match embeddable(src) {
                Some(image) => {
                    let size = image.size_pt(width.as_deref(), height.as_deref(), BASE_PT);
                    let rel = media.add(image);
                    media.drawings += 1;
                    write_drawing(buf, &rel, media.drawings, size, alt.as_deref())?;
                }
                None => {
                    if let Some(alt) = alt {
                        write_run(buf, alt, fmt)?;
                    }
                }
            },
            ContentNode::Raw { .. } | ContentNode::Unknown => {}
        }
    }
    Ok(())
}

/// An image Word can embed: a PNG, JPEG or GIF `data:` URI (an SVG needs a
/// bitmap fallback the writer can't make).
pub(crate) fn embeddable(src: &str) -> Option<Image> {
    Image::from_src(src).filter(|i| i.format != ImageFormat::Svg)
}

/// A run holding an inline picture (`wp:inline`, on the baseline as in HTML)
/// of `size` points, stretched to it; drawing `id` is unique in the document.
/// The drawing namespaces are declared where they're used.
fn write_drawing(
    buf: &mut String,
    rel: &str,
    id: u32,
    (w, h): (f64, f64),
    alt: Option<&str>,
) -> Result<(), RenderError> {
    let (cx, cy) = ((w * 12700.0).round() as i64, (h * 12700.0).round() as i64);
    let descr = alt
        .map(|a| format!(" descr=\"{}\"", escape_xml(a)))
        .unwrap_or_default();
    write!(
        buf,
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
         <wp:docPr id=\"{id}\" name=\"Picture {id}\"{descr}/>\
         <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
         <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"Picture {id}\"{descr}/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{rel}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )?;
    Ok(())
}

fn write_run(buf: &mut String, text: &str, fmt: Fmt) -> Result<(), RenderError> {
    if text.is_empty() {
        return Ok(());
    }
    buf.push_str("<w:r>");
    if !fmt.is_plain() {
        // CT_RPr is a sequence: rFonts, b, bCs, i, iCs, strike, color, sz, szCs, u,
        // vertAlign. The `Cs` forms are for complex scripts (Arabic, Hebrew, …).
        buf.push_str("<w:rPr>");
        if let Some(f) = fmt.font {
            let f = escape_xml(f);
            write!(
                buf,
                "<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:eastAsia=\"{f}\" w:cs=\"{f}\"/>"
            )?;
        }
        if fmt.bold {
            buf.push_str("<w:b/><w:bCs/>");
        }
        if fmt.italic {
            buf.push_str("<w:i/><w:iCs/>");
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

fn write_footnotes(
    buf: &mut String,
    rt: &ResolvedTable,
    media: &mut Media,
) -> Result<(), RenderError> {
    let footer = &rt.footer;
    if !footer.footnotes.is_empty() {
        buf.push_str("<w:p/>\n"); // spacer
        for note in &footer.footnotes {
            open_paragraph(buf, rt, Some("FootnoteText"), None);
            let fmt = Fmt {
                inherited: Some(xml::NOTE_HALF_POINTS),
                ..Fmt::default()
            }
            .with(&note.style);
            write_run(
                buf,
                note.mark,
                Fmt {
                    superscript: true,
                    ..fmt
                },
            )?;
            write_run(buf, " ", fmt)?;
            write_content(buf, rt, media, &note.content, fmt)?;
            buf.push_str("</w:p>\n");
        }
    }
    if !footer.source_notes.is_empty() {
        buf.push_str("<w:p/>\n"); // spacer
        for note in &footer.source_notes {
            open_paragraph(buf, rt, Some("SourceNote"), None);
            let fmt = Fmt {
                italic: true,
                inherited: Some(xml::NOTE_HALF_POINTS),
                ..Fmt::default()
            };
            write_content(buf, rt, media, &note.content, fmt.with(&note.style))?;
            buf.push_str("</w:p>\n");
        }
    }
    Ok(())
}

/// The width of the line, in points, that `%` and `fr` widths divide. The
/// document sets no page, so the application chooses it: 6in fits Letter and A4 at
/// their usual margins.
const LINE_PT: f64 = 432.0;

/// Column widths in dxa: absolute lengths convert exactly, `%` and `fr` resolve
/// against the table's width (else the line), and `auto` gets the default (see
/// [`ResolvedTable::column_widths_pt`]). The table's width is a signed 32-bit
/// number, which they add up to at most.
fn compute_col_widths(rt: &ResolvedTable) -> Vec<u32> {
    let auto_pt = f64::from(xml::DEFAULT_COL_WIDTH_DXA) / 20.0;
    rt.column_widths_in(LINE_PT, auto_pt, BASE_PT, 20.0, i32::MAX as u32)
}
