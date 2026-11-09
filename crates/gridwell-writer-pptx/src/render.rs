use gridwell_core::xml::escape as escape_xml;
use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{BorderStyle, HAlign, Table};
use gridwell_layout::look::{BaseRules, LOOK};
use gridwell_layout::{
    resolve, CellVAlign, ResolvedCell, ResolvedStyle, ResolvedTable, Section, Sides, Stroke,
};
use gridwell_ooxml::{package, Part};
use std::fmt::Write;
use thiserror::Error;

use crate::{xml, PptxConfig};
use rules::{Line, Strokes};

mod heights;
mod rules;
mod widths;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("packaging error: {0}")]
    Package(#[from] gridwell_ooxml::PackageError),
}

/// Render the full .pptx ZIP file as bytes.
pub fn render(table: &Table, config: &PptxConfig) -> Result<Vec<u8>, RenderError> {
    let slide_xml = render_slide_xml(table, config)?;

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
        Part::new("ppt/theme/theme1.xml", xml::THEME),
    ])?)
}

/// Render only the slide XML content (for snapshot testing).
pub fn render_slide_xml(table: &Table, config: &PptxConfig) -> Result<String, RenderError> {
    let rt = resolve(table);
    let mut buf = String::with_capacity(8192);
    write_slide_xml(&mut buf, &rt, config)?;
    Ok(buf)
}

/// The slide: a `<p:sld>` whose shape tree holds the table's graphic frame, the
/// title and notes boxes, and the look's lines.
fn write_slide_xml(
    buf: &mut String,
    rt: &ResolvedTable,
    config: &PptxConfig,
) -> Result<(), RenderError> {
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
    // centered vertically on the 4:3 slide (9144000 × 6858000 EMU).
    let h = &rt.header;
    let title = Fmt {
        bold: true,
        size: Some(2400),
        ..Fmt::default()
    };
    let line = Fmt {
        size: Some(1400),
        ..Fmt::default()
    };
    let heading: Vec<Para> = h
        .title
        .iter()
        .map(|l| Para::line(l, title))
        .chain(
            h.subtitle
                .iter()
                .chain(&h.extra_lines)
                .map(|l| Para::line(l, line)),
        )
        .collect();
    let note = Fmt {
        size: Some(1000),
        ..Fmt::default()
    };
    let notes: Vec<Para> = rt
        .footer
        .footnotes
        .iter()
        .map(|n| Para {
            mark: Some(n.mark),
            content: &n.content,
            fmt: note.with(&n.style),
        })
        .chain(rt.footer.source_notes.iter().map(|l| Para::line(l, note)))
        .collect();
    // A table with no grid columns is invalid DrawingML: with every column hidden
    // only the text boxes remain.
    let (col_widths, row_heights) = if rt.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let w = widths::column_widths(rt);
        let h = heights::row_heights(rt, &w);
        (w, h)
    };
    let table_w: u32 = col_widths.iter().sum();
    let table_h = row_heights.iter().fold(0u32, |a, h| a.saturating_add(*h));
    let table_x = SLIDE_W.saturating_sub(table_w) / 2;
    let (box_x, box_w, need) = text_span(rt, heading.iter().chain(&notes), table_x, table_w);
    let header_h = heights::box_height(rt, &heading, box_w);
    let notes_h = heights::box_height(rt, &notes, box_w);
    let gap = |a: u32, b: u32| if a > 0 && b > 0 { GAP_EMU } else { 0 };
    // The look's outer rules: above the heading and below the notes, across the
    // table and the boxes' text.
    let rules = config.base_styles.then(|| BaseRules::new(rt));
    let outer = LOOK.outer_rule;
    let outer_h = |on: bool| {
        if on && rules.is_some() {
            rules::rule_emu(outer)
        } else {
            0
        }
    };
    let (top_rule, bottom_rule) = (outer_h(!heading.is_empty()), outer_h(!notes.is_empty()));
    let (rule_x, rule_w) = rule_span(rt, (box_x, box_w, need), table_x, table_w);
    let total = [
        top_rule,
        header_h,
        gap(header_h, table_h),
        table_h,
        gap(table_h.saturating_add(header_h), notes_h),
        notes_h,
        bottom_rule,
    ]
    .into_iter()
    .fold(0u32, u32::saturating_add);
    let mut y = (SLIDE_H.saturating_sub(total) / 2).max(MARGIN_EMU);
    let mut lines = Vec::new();
    let mut outer_line = |y: u32, h: u32| {
        if h > 0 {
            lines.push(Line {
                x: rule_x,
                y: y.saturating_add(h / 2),
                width: rule_w,
                rule: outer,
            });
        }
    };

    outer_line(y, top_rule);
    y = y.saturating_add(top_rule);
    if !heading.is_empty() {
        write_text_box(buf, rt, 3, "Title", (box_x, box_w), y, header_h, &heading)?;
        y = y
            .saturating_add(header_h)
            .saturating_add(gap(header_h, table_h.saturating_add(notes_h)));
    }
    let mut spanners = Vec::new();
    if !rt.is_empty() {
        let strokes = rules::cell_strokes(rt, rules.as_ref());
        write_table_frame(buf, rt, &col_widths, &row_heights, &strokes, table_x, y)?;
        if let Some(rules) = &rules {
            spanners = rules::spanner_lines(rt, rules, &col_widths, &row_heights, (table_x, y));
        }
        y = y
            .saturating_add(table_h)
            .saturating_add(gap(table_h, notes_h));
    }
    if !notes.is_empty() {
        write_text_box(buf, rt, 4, "Notes", (box_x, box_w), y, notes_h, &notes)?;
        y = y.saturating_add(notes_h);
    }
    outer_line(y, bottom_rule);
    // Drawn over the table: after it.
    for (id, line) in (5..).zip(lines.iter().chain(&spanners)) {
        rules::write_line(buf, id, line)?;
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
/// The slide less half-inch margins: what the text boxes may span.
const TEXT_X: u32 = 457_200;
const TEXT_W: u32 = 8_229_600;

/// A paragraph of a text box: a footnote's mark and a space, then content.
struct Para<'a> {
    mark: Option<&'a str>,
    content: &'a [ContentNode],
    fmt: Fmt<'a>,
}

impl<'a> Para<'a> {
    /// A heading line or source note, its style over `fmt`.
    fn line(line: &'a gridwell_layout::Line, fmt: Fmt<'a>) -> Self {
        Self {
            mark: None,
            content: &line.content,
            fmt: fmt.with(&line.style),
        }
    }

    /// The paragraph as it is written: each run, in order.
    fn for_each_run<E>(
        &self,
        rt: &ResolvedTable,
        f: &mut dyn FnMut(Run) -> Result<(), E>,
    ) -> Result<(), E> {
        if let Some(mark) = self.mark {
            let sup = Fmt {
                superscript: true,
                ..self.fmt
            };
            f(Some((mark, sup)))?;
            f(Some((" ", self.fmt)))?;
        }
        for_each_run(rt, self.content, self.fmt, f)
    }
}

/// Where the title and notes boxes go, `(x, width)` in EMU: from the table's
/// start edge (its right when right to left) to the far margin, so their text
/// lines up with the table, moved toward the near margin as far as their widest
/// line needs; and that line's room, the third value.
fn text_span<'a>(
    rt: &ResolvedTable,
    paras: impl IntoIterator<Item = &'a Para<'a>>,
    table_x: u32,
    table_w: u32,
) -> (u32, u32, u32) {
    let (lo, hi) = (TEXT_X, TEXT_X + TEXT_W);
    let need = (widths::text_room(rt, paras) * widths::EMU_PER_PT)
        .ceil()
        .min(f64::from(TEXT_W)) as u32;
    if rt.is_empty() {
        return (lo, TEXT_W, need);
    }
    if rt.is_rtl() {
        let end = table_x.saturating_add(table_w).clamp(lo, hi);
        let end = end.max(lo + need);
        (lo, end - lo, need)
    } else {
        let start = table_x.clamp(lo, hi).min(hi - need);
        (start, hi - start, need)
    }
}

/// Where the outer rules go, `(x, width)` in EMU: across the table and the text of
/// the boxes `text_span` places, which starts at the box's start edge.
fn rule_span(
    rt: &ResolvedTable,
    (box_x, box_w, need): (u32, u32, u32),
    table_x: u32,
    table_w: u32,
) -> (u32, u32) {
    let (lo, hi) = if rt.is_rtl() {
        let end = box_x.saturating_add(box_w);
        (end.saturating_sub(need), end)
    } else {
        (box_x, box_x.saturating_add(need))
    };
    let (lo, hi) = if rt.is_empty() {
        (lo, hi)
    } else {
        (lo.min(table_x), hi.max(table_x.saturating_add(table_w)))
    };
    (lo, hi - lo)
}

/// A text box shape: `paras` at `(x, width)`.
#[allow(clippy::too_many_arguments)]
fn write_text_box(
    buf: &mut String,
    rt: &ResolvedTable,
    id: u32,
    name: &str,
    (x, width): (u32, u32),
    y: u32,
    height: u32,
    paras: &[Para],
) -> Result<(), RenderError> {
    write!(
        buf,
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr>\
         <p:txBody><a:bodyPr wrap=\"square\"/><a:lstStyle/>"
    )?;
    for para in paras {
        open_paragraph(buf, rt, None);
        if !write_runs(buf, rt, |f| para.for_each_run(rt, f))? {
            buf.push_str("<a:endParaRPr/>");
        }
        buf.push_str("</a:p>");
    }
    buf.push_str("</p:txBody></p:sp>\n");
    Ok(())
}

/// Open an `<a:p>` with its properties: an alignment (`algn`), and right to left
/// when the table is. `algn` is physical, so in a right-to-left table a paragraph
/// without one is put on the right, where it starts.
fn open_paragraph(buf: &mut String, rt: &ResolvedTable, algn: Option<&str>) {
    buf.push_str("<a:p>");
    let (algn, rtl) = match (algn, rt.is_rtl()) {
        (algn, false) => (algn, ""),
        (algn, true) => (algn.or(Some("r")), " rtl=\"1\""),
    };
    if let Some(algn) = algn {
        write!(buf, "<a:pPr algn=\"{algn}\"{rtl}/>").unwrap();
    }
}

/// The table's graphic frame, as wide as its columns and as tall as its rows, each
/// position with its borders from `strokes` ([`rules::cell_strokes`]).
fn write_table_frame(
    buf: &mut String,
    rt: &ResolvedTable,
    col_widths: &[u32],
    row_heights: &[u32],
    strokes: &[Vec<Strokes>],
    offset_x: u32,
    offset_y: u32,
) -> Result<(), RenderError> {
    let total_width: u32 = col_widths.iter().sum();
    let total_height = row_heights.iter().fold(0u32, |a, h| a.saturating_add(*h));

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
    // Right to left, the columns are written as they are seen, the first on the
    // right ([`visual`]), not as `rtl="1"` asks: LibreOffice and Apple's
    // importer draw such a table left to right.
    writeln!(buf, "<a:tblPr firstRow=\"1\" bandRow=\"1\"/>")?;

    buf.push_str("<a:tblGrid>");
    for v in 0..col_widths.len() {
        let w = col_widths[visual(rt, col_widths.len(), v)];
        write!(buf, "<a:gridCol w=\"{w}\"/>")?;
    }
    buf.push_str("</a:tblGrid>\n");

    // In the order `heights::row_heights` gives them, one for each row, as
    // `strokes` does.
    let mut heights = row_heights.iter().copied().zip(strokes);
    let mut next = || {
        heights
            .next()
            .map_or((1, &[][..]), |(h, s)| (h, s.as_slice()))
    };
    write_section(buf, rt, &rt.head, true, &mut next)?;
    for group in &rt.groups {
        if let Some(label) = &group.label {
            let (h, strokes) = next();
            write_group_label_row(buf, rt, label, col_widths.len(), h, strokes)?;
        }
        write_section(buf, rt, &group.rows, false, &mut next)?;
        write_section(buf, rt, &group.summary_rows, true, &mut next)?;
    }

    buf.push_str("</a:tbl>\n");
    buf.push_str("</a:graphicData></a:graphic>\n");
    buf.push_str("</p:graphicFrame>\n");
    Ok(())
}

/// The column written `v`th of `ncols`: the columns in the order they are seen,
/// left to right, so reversed in a right-to-left table.
fn visual(rt: &ResolvedTable, ncols: usize, v: usize) -> usize {
    if rt.is_rtl() {
        ncols - 1 - v
    } else {
        v
    }
}

/// The borders of the position in column `col` of a row's `strokes`, by the
/// sides they are drawn on. [`rules::cell_strokes`] gives a position's start
/// edge as its left, so right to left they swap back.
fn drawn_strokes(rt: &ResolvedTable, strokes: &[Strokes], col: usize) -> Strokes {
    strokes
        .get(col)
        .cloned()
        .unwrap_or_default()
        .mirrored(rt.is_rtl())
}

/// Write one section. Spans are resolved per section, so a merge never crosses a
/// section boundary. `next` gives each row's height and its positions' borders.
fn write_section<'s>(
    buf: &mut String,
    rt: &ResolvedTable,
    section: &Section,
    strong: bool,
    next: &mut dyn FnMut() -> (u32, &'s [Strokes]),
) -> Result<(), RenderError> {
    for (r, row) in section.rows.iter().enumerate() {
        let (height, strokes) = next();
        write!(buf, "<a:tr h=\"{height}\">")?;
        let ncols = row.slots.len();
        for v in 0..ncols {
            let col = visual(rt, ncols, v);
            let strokes = drawn_strokes(rt, strokes, col);
            write_slot(buf, rt, section, (r, col), &strokes, strong)?;
        }
        buf.push_str("</a:tr>\n");
    }
    Ok(())
}

/// One `<a:tc>`, for the position at `(row, col)`. DrawingML needs a cell for
/// every grid position: a cell's first position as written (its last column right
/// to left) carries `gridSpan`/`rowSpan`; the others are empty cells marked
/// `hMerge` (covered from the left) and/or `vMerge` (covered from above).
fn write_slot(
    buf: &mut String,
    rt: &ResolvedTable,
    section: &Section,
    (row, col): (usize, usize),
    strokes: &Sides<Option<Stroke>>,
    strong: bool,
) -> Result<(), RenderError> {
    // Invalid IR only: an empty, unmerged cell keeps the grid intact.
    let Some((cell, outline)) = section.covering(row, col) else {
        return write_merged_cell(buf, false, false, strokes);
    };
    let first = if rt.is_rtl() {
        outline.right
    } else {
        outline.left
    };
    if !first || !outline.top {
        return write_merged_cell(buf, !first, !outline.top, strokes);
    }

    buf.push_str("<a:tc");
    if cell.colspan > 1 {
        write!(buf, " gridSpan=\"{}\"", cell.colspan)?;
    }
    if cell.rowspan > 1 {
        write!(buf, " rowSpan=\"{}\"", cell.rowspan)?;
    }
    buf.push('>');

    buf.push_str("<a:txBody><a:bodyPr/><a:lstStyle/>");
    let algn = match (&cell.align, rt.is_rtl()) {
        (HAlign::Right, _) => Some("r"),
        (HAlign::Center, _) => Some("ctr"),
        (HAlign::Justify, _) => Some("just"),
        (_, false) => None,
        // Left, said outright: unset is the start.
        (_, true) => Some("l"),
    };
    open_paragraph(buf, rt, algn);
    let fmt = cell_fmt(cell, strong);
    let mut wrote = write_content(buf, rt, &cell.content, fmt)?;
    if let Some(pad) = cell.align_pad {
        wrote |= write_run(buf, rt, &pad.text(), pad_fmt(fmt))?;
    }
    if !wrote {
        buf.push_str("<a:endParaRPr/>");
    }
    buf.push_str("</a:p></a:txBody>");

    write_tc_pr(buf, strokes, cell.style.fill(), cell.style.cell_valign())?;
    buf.push_str("</a:tc>");
    Ok(())
}

/// A cell's run format: bold in a `strong` section (the head, summary rows).
fn cell_fmt<'s>(cell: &'s ResolvedCell, strong: bool) -> Fmt<'s> {
    Fmt {
        bold: strong,
        ..Fmt::default()
    }
    .with(&cell.style)
}

/// Aligned on a character: the padding as text without a fill, keeping its
/// room exactly (LibreOffice hangs spaces ending a line in the margin, where
/// they move nothing), undecorated.
fn pad_fmt(fmt: Fmt) -> Fmt {
    Fmt {
        underline: false,
        strike: false,
        hidden: true,
        ..fmt
    }
}

/// The size relative sizes resolve against: PowerPoint's 18pt table text.
const BASE_PT: f64 = 18.0;

/// `<a:tcPr>`: the cell's vertical alignment (`anchor`; unset is top), its
/// border lines (in schema order: left, right, top, bottom), then its fill.
fn write_tc_pr(
    buf: &mut String,
    strokes: &Sides<Option<Stroke>>,
    fill: Option<Color>,
    valign: Option<CellVAlign>,
) -> Result<(), RenderError> {
    buf.push_str("<a:tcPr");
    match valign {
        Some(CellVAlign::Top) => buf.push_str(" anchor=\"t\""),
        Some(CellVAlign::Middle) => buf.push_str(" anchor=\"ctr\""),
        Some(CellVAlign::Bottom) => buf.push_str(" anchor=\"b\""),
        None => {}
    }
    buf.push('>');
    for (tag, stroke) in [
        ("lnL", &strokes.left),
        ("lnR", &strokes.right),
        ("lnT", &strokes.top),
        ("lnB", &strokes.bottom),
    ] {
        let Some(k) = stroke else { continue };
        // EMU: 12700 per point.
        let w = (k.width_pt * 12700.0).round().clamp(1.0, 20_116_800.0) as u32;
        write!(buf, "<a:{tag} w=\"{w}\"")?;
        if k.style == BorderStyle::Double {
            buf.push_str(" cmpd=\"dbl\"");
        }
        let color = k.color.map_or("000000".into(), |c| c.flatten().to_rrggbb());
        write!(
            buf,
            "><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>"
        )?;
        match k.style {
            BorderStyle::Dashed => buf.push_str("<a:prstDash val=\"dash\"/>"),
            BorderStyle::Dotted => buf.push_str("<a:prstDash val=\"sysDot\"/>"),
            _ => {}
        }
        write!(buf, "</a:{tag}>")?;
    }
    if let Some(fill) = fill {
        write!(
            buf,
            "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
            fill.flatten().to_rrggbb()
        )?;
    }
    buf.push_str("</a:tcPr>");
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
    superscript: bool,
    /// Font size in hundredths of a point (`None`: inherited).
    size: Option<u32>,
    /// The font (`None`: the theme's).
    font: Option<&'s str>,
    /// Drawn without a fill: invisible, keeping its room.
    hidden: bool,
}

impl<'s> Fmt<'s> {
    /// This format with a style laid over it. A relative size resolves against
    /// the size so far, or PowerPoint's 18pt table text.
    fn with(self, style: &'s ResolvedStyle) -> Self {
        let parent = self.size.map_or(BASE_PT, |s| f64::from(s) / 100.0);
        Self {
            bold: style.bold_over(self.bold),
            italic: style.italic_over(self.italic),
            underline: self.underline || style.is_underline(),
            strike: self.strike || style.is_strike(),
            color: style.paint().or(self.color),
            size: style
                .size_pt(parent)
                .map(|pt| (pt * 100.0).round().clamp(100.0, 400_000.0) as u32)
                .or(self.size),
            font: style.font_name().or(self.font),
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
    write_runs(buf, rt, |f| for_each_run(rt, nodes, fmt, f))
}

/// The runs `runs` gives, as `<a:r>`s and `<a:br/>`s. Returns whether anything
/// was written.
fn write_runs(
    buf: &mut String,
    rt: &ResolvedTable,
    runs: impl FnOnce(&mut dyn FnMut(Run) -> Result<(), RenderError>) -> Result<(), RenderError>,
) -> Result<bool, RenderError> {
    let mut wrote = false;
    runs(&mut |run| {
        wrote |= match run {
            Some((text, fmt)) => write_run(buf, rt, text, fmt)?,
            None => {
                buf.push_str("<a:br/>");
                true
            }
        };
        Ok(())
    })?;
    Ok(wrote)
}

/// A run's text and format, or a line break (`None`).
type Run<'t, 's> = Option<(&'t str, Fmt<'s>)>;

/// Content as it is written: each run, in order. Raw and unknown nodes are
/// nothing; an image is its alt text.
fn for_each_run<E>(
    rt: &ResolvedTable,
    nodes: &[ContentNode],
    fmt: Fmt,
    f: &mut dyn FnMut(Run) -> Result<(), E>,
) -> Result<(), E> {
    for node in nodes {
        match node {
            ContentNode::Text { value } => f(Some((value, fmt)))?,
            ContentNode::StyledText { value, style_id } => {
                let style = style_id
                    .as_deref()
                    .map(|id| rt.style(id))
                    .unwrap_or_default();
                f(Some((value, fmt.with(&style))))?
            }
            ContentNode::LineBreak {} => f(None)?,
            ContentNode::FootnoteMark { mark_text, .. } => f(Some((
                mark_text,
                Fmt {
                    superscript: true,
                    ..fmt
                },
            )))?,
            ContentNode::Image { alt, .. } => {
                if let Some(alt) = alt {
                    f(Some((alt, fmt)))?
                }
            }
            ContentNode::Raw { .. } | ContentNode::Unknown(_) => {}
        }
    }
    Ok(())
}

/// One `<a:r>`, in the table's language (`en-US` unless it has one).
fn write_run(
    buf: &mut String,
    rt: &ResolvedTable,
    text: &str,
    fmt: Fmt,
) -> Result<bool, RenderError> {
    if text.is_empty() {
        return Ok(false);
    }
    let lang = rt.table.config.lang.as_deref().unwrap_or("en-US");
    write!(buf, "<a:r><a:rPr lang=\"{}\"", escape_xml(lang))?;
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
    if fmt.color.is_none() && fmt.font.is_none() && !fmt.hidden {
        buf.push_str("/>");
    } else {
        // CT_TextCharacterProperties is a sequence: the fill, then the fonts.
        buf.push('>');
        if fmt.hidden {
            buf.push_str("<a:noFill/>");
        } else if let Some(c) = fmt.color {
            write!(
                buf,
                "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                c.flatten().to_rrggbb()
            )?;
        }
        if let Some(f) = fmt.font {
            let f = escape_xml(f);
            write!(
                buf,
                "<a:latin typeface=\"{f}\"/><a:ea typeface=\"{f}\"/><a:cs typeface=\"{f}\"/>"
            )?;
        }
        buf.push_str("</a:rPr>");
    }
    write!(buf, "<a:t>{}</a:t></a:r>", escape_xml(text))?;
    Ok(true)
}

/// An empty `<a:tc>` for a position covered by another cell's span, carrying
/// that cell's borders on the sides it shares with the cell's outline.
fn write_merged_cell(
    buf: &mut String,
    h_merge: bool,
    v_merge: bool,
    strokes: &Sides<Option<Stroke>>,
) -> Result<(), RenderError> {
    buf.push_str("<a:tc");
    if h_merge {
        buf.push_str(" hMerge=\"1\"");
    }
    if v_merge {
        buf.push_str(" vMerge=\"1\"");
    }
    buf.push_str("><a:txBody><a:bodyPr/><a:lstStyle/>");
    buf.push_str("<a:p><a:endParaRPr/></a:p></a:txBody>");
    if *strokes == Sides::default() {
        buf.push_str("<a:tcPr/>");
    } else {
        write_tc_pr(buf, strokes, None, None)?;
    }
    buf.push_str("</a:tc>");
    Ok(())
}

fn write_group_label_row(
    buf: &mut String,
    rt: &ResolvedTable,
    label: &gridwell_layout::Line,
    num_cols: usize,
    height: u32,
    strokes: &[Strokes],
) -> Result<(), RenderError> {
    write!(buf, "<a:tr h=\"{height}\">")?;

    // One cell spanning every column, then `hMerge` placeholders.
    buf.push_str("<a:tc");
    if num_cols > 1 {
        write!(buf, " gridSpan=\"{num_cols}\"")?;
    }
    buf.push('>');
    buf.push_str("<a:txBody><a:bodyPr/><a:lstStyle/>");
    open_paragraph(buf, rt, None);
    if !write_content(buf, rt, &label.content, label_fmt(label))? {
        buf.push_str("<a:endParaRPr/>");
    }
    buf.push_str("</a:p></a:txBody>");
    let piece = |v: usize| drawn_strokes(rt, strokes, visual(rt, num_cols, v));
    let fill = label.style.fill().unwrap_or(Color::rgb(0xF0, 0xF0, 0xF0));
    write_tc_pr(buf, &piece(0), Some(fill), None)?;
    buf.push_str("</a:tc>");
    for v in 1..num_cols {
        write_merged_cell(buf, true, false, &piece(v))?;
    }

    buf.push_str("</a:tr>\n");
    Ok(())
}

/// A group label's run format: bold.
fn label_fmt<'s>(label: &'s gridwell_layout::Line) -> Fmt<'s> {
    Fmt {
        bold: true,
        ..Fmt::default()
    }
    .with(&label.style)
}
