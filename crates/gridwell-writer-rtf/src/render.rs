use gridwell_core::Color;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{vmerge_layout, FontStyle, FontWeight, MergeCell, Row, Table, VMerge};
use std::collections::HashMap;
use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Default column width in twips (1 inch = 1440 twips).
const DEFAULT_COL_WIDTH: u32 = 2160; // 1.5 inches

struct RtfRenderer<'a> {
    table: &'a Table,
    buf: String,
    color_table: Vec<(u8, u8, u8)>,
    color_map: HashMap<String, usize>,
    font_table: Vec<String>,
    font_map: HashMap<String, usize>,
}

impl<'a> RtfRenderer<'a> {
    fn new(table: &'a Table) -> Self {
        let mut r = Self {
            table,
            buf: String::with_capacity(4096),
            color_table: vec![(0, 0, 0), (255, 255, 255)], // black, white
            color_map: HashMap::new(),
            font_table: vec!["Arial".to_string()],
            font_map: HashMap::new(),
        };
        r.color_map.insert("#000000".to_string(), 0);
        r.color_map.insert("#FFFFFF".to_string(), 1);
        r.font_map.insert("Arial".to_string(), 0);
        r
    }

    fn register_color(&mut self, hex: &str) -> usize {
        if let Some(&idx) = self.color_map.get(hex) {
            return idx;
        }
        if let Some(c) = paint(hex) {
            let idx = self.color_table.len();
            self.color_table.push((c.r, c.g, c.b));
            self.color_map.insert(hex.to_string(), idx);
            idx
        } else {
            0 // default to black
        }
    }

    fn col_widths_twips(&self) -> Vec<u32> {
        self.table
            .column_spec
            .iter()
            .map(|col| {
                if col.width == "auto" || col.hidden {
                    DEFAULT_COL_WIDTH
                } else if let Some(px) = col.width.strip_suffix("px") {
                    px.parse::<f64>()
                        .map(|v| (v * 15.0) as u32) // 1px ≈ 15 twips
                        .unwrap_or(DEFAULT_COL_WIDTH)
                } else {
                    DEFAULT_COL_WIDTH
                }
            })
            .collect()
    }

    fn render(mut self) -> Result<String, RenderError> {
        // Pre-scan colors from styles
        self.prescan_colors();

        self.write_header();
        self.write_title();
        self.write_table();
        self.write_footnotes();
        self.buf.push('}'); // close RTF group
        Ok(self.buf)
    }

    fn prescan_colors(&mut self) {
        let mut keys: Vec<&String> = self.table.styles.defs.keys().collect();
        keys.sort();
        for key in keys {
            let def = &self.table.styles.defs[key];
            if let Some(ref c) = def.color {
                self.register_color(c);
            }
            if let Some(ref c) = def.background_color {
                self.register_color(c);
            }
        }
    }

    fn write_header(&mut self) {
        self.buf.push_str("{\\rtf1\\ansi\\deff0\n");

        // Font table
        self.buf.push_str("{\\fonttbl");
        for (i, font) in self.font_table.iter().enumerate() {
            write!(self.buf, "{{\\f{i} {font};}}").unwrap();
        }
        self.buf.push_str("}\n");

        // Color table
        self.buf.push_str("{\\colortbl ;");
        for (r, g, b) in &self.color_table {
            write!(self.buf, "\\red{r}\\green{g}\\blue{b};").unwrap();
        }
        self.buf.push_str("}\n");
    }

    fn write_title(&mut self) {
        if let Some(ref header) = self.table.header {
            if let Some(ref title) = header.title {
                let text = content_to_rtf(&title.content);
                writeln!(self.buf, "\\pard\\b\\fs36 {text}\\b0\\par").unwrap();
            }
            if let Some(ref subtitle) = header.subtitle {
                let text = content_to_rtf(&subtitle.content);
                writeln!(self.buf, "\\pard\\fs24 {text}\\par").unwrap();
            }
            self.buf.push_str("\\par\n");
        }
    }

    fn write_table(&mut self) {
        let col_widths = self.col_widths_twips();

        // Thead
        if !self.table.config.column_labels_hidden {
            let rows = &self.table.table.thead.rows;
            self.write_section(rows, &col_widths, true);
        }

        // Tbody
        for group in &self.table.table.tbody {
            if let Some(ref label) = group.label {
                let text = content_to_rtf(&label.content);
                let total_width: u32 = col_widths.iter().sum();
                writeln!(
                    self.buf,
                    "\\trowd\\trqc\\cellx{total_width}\n\\pard\\intbl\\b {text}\\b0\\cell\n\\row"
                )
                .unwrap();
            }

            self.write_section(&group.rows, &col_widths, false);
            self.write_section(&group.summary_rows, &col_widths, false);
        }
    }

    /// Write one section (thead, a group's rows, or its summary rows). Spans are
    /// resolved per section: a rowspan never crosses a section boundary.
    fn write_section(&mut self, rows: &[Row], col_widths: &[u32], is_header: bool) {
        for cells in vmerge_layout(rows) {
            self.write_row(&cells, col_widths, is_header);
        }
    }

    fn write_row(&mut self, cells: &[MergeCell<'_>], col_widths: &[u32], is_header: bool) {
        // Row definition
        self.buf.push_str("\\trowd");
        if is_header {
            self.buf.push_str("\\trhdr");
        }

        // Cell definitions. `\cellx` is the cell's right edge (cumulative twips), so
        // it is derived from the grid column, never from a running count of emitted
        // cells.
        for rc in cells {
            let end = (rc.col + rc.span).min(col_widths.len());
            let right_edge: u32 = col_widths[..end].iter().sum();

            match rc.vmerge {
                VMerge::Start => self.buf.push_str("\\clvmgf"),
                VMerge::Continue => self.buf.push_str("\\clvmrg"),
                VMerge::None => {}
            }

            // Background color
            if let Some(cell) = rc.cell {
                if let Some(ref style_id) = cell.style_id {
                    if let Some(def) = self.table.styles.defs.get(style_id.as_str()) {
                        if let Some(ref bg) = def.background_color {
                            // Unregistered = not a paintable colour: no fill (not black).
                            if let Some(&ci) = self.color_map.get(bg) {
                                write!(self.buf, "\\clcbpat{}", ci + 1).unwrap();
                            }
                        }
                    }
                }
            }

            write!(self.buf, "\\cellx{right_edge}").unwrap();
        }
        self.buf.push('\n');

        // Cell contents (one `\cell` per definition above, in the same order)
        for rc in cells {
            let Some(cell) = rc.cell else {
                self.buf.push_str("\\pard\\intbl\\cell\n");
                continue;
            };
            let text = content_to_rtf(&cell.content);
            let mut fmt = String::new();
            if is_header {
                fmt.push_str("\\b");
            }
            if let Some(ref style_id) = cell.style_id {
                if let Some(def) = self.table.styles.defs.get(style_id.as_str()) {
                    if def.font_weight.as_ref().is_some_and(FontWeight::is_bold) {
                        fmt.push_str("\\b");
                    }
                    if def.font_style.as_ref().is_some_and(FontStyle::is_italic) {
                        fmt.push_str("\\i");
                    }
                    if let Some(ref color) = def.color {
                        if let Some(&ci) = self.color_map.get(color.as_str()) {
                            write!(fmt, "\\cf{}", ci + 1).unwrap();
                        }
                    }
                }
            }
            writeln!(self.buf, "\\pard\\intbl{fmt} {text}\\cell").unwrap();
        }
        self.buf.push_str("\\row\n");
    }

    fn write_footnotes(&mut self) {
        if let Some(ref footer) = self.table.footer {
            if !footer.footnotes.is_empty() {
                self.buf.push_str("\\par\n");
                for note in &footer.footnotes {
                    let text = content_to_rtf(&note.content);
                    let mark = escape_rtf(&note.mark);
                    writeln!(
                        self.buf,
                        "\\pard\\fs18 \\super {mark}\\nosupersub  {text}\\par"
                    )
                    .unwrap();
                }
            }
            if !footer.source_notes.is_empty() {
                self.buf.push_str("\\par\n");
                for note in &footer.source_notes {
                    let text = content_to_rtf(&note.content);
                    writeln!(self.buf, "\\pard\\fs18 {text}\\par").unwrap();
                }
            }
        }
    }
}

pub fn render(table: &Table) -> Result<String, RenderError> {
    let renderer = RtfRenderer::new(table);
    renderer.render()
}

fn content_to_rtf(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } => out.push_str(&escape_rtf(value)),
            ContentNode::StyledText { value, .. } => out.push_str(&escape_rtf(value)),
            ContentNode::LineBreak {} => out.push_str("\\line "),
            ContentNode::FootnoteMark { mark_text, .. } => {
                write!(out, "\\super {}\\nosupersub ", escape_rtf(mark_text)).unwrap();
            }
            ContentNode::Image { alt, .. } => {
                if let Some(alt_text) = alt {
                    out.push_str(&escape_rtf(alt_text));
                }
            }
            ContentNode::Raw { format, value } => {
                if format == "rtf" {
                    out.push_str(value);
                }
            }
            ContentNode::Unknown => {}
        }
    }
    out
}

fn escape_rtf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c if c as u32 > 127 => write!(out, "\\u{}?", c as i16).unwrap(),
            c => out.push(c),
        }
    }
    out
}

/// Any CSS colour except a fully transparent one (RTF has no alpha; transparent
/// means "don't paint").
fn paint(color: &str) -> Option<Color> {
    color.parse::<Color>().ok().filter(|c| !c.is_transparent())
}
