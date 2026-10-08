//! Style read-back: how each cell of a rendered table *looks* — weight, slant,
//! decoration, colour, size, font, fill, alignment, borders — recovered from the
//! format's own structure, as the grid readers recover text.
//!
//! Each reader returns every table cell (span origins only, in document order)
//! with its text and look, plus the document's text outside the table (titles,
//! notes). The feature matrix (`tests/feature_matrix.rs`) finds cells by their
//! text and compares looks.
//!
//! Readers apply the format's own inheritance and defaults where they change
//! what is shown (RTF character formatting persisting across cells until
//! `\plain`; DOCX table borders when a cell sets none), so a writer that leaks
//! formatting from one cell into the next reads as leaking.

#![allow(dead_code)]

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::ooxml::attr;

/// An sRGB colour.
pub type Rgb = (u8, u8, u8);

/// Parse `RRGGBB`, `#RRGGBB` or `AARRGGBB` (alpha ignored).
pub fn hex(s: &str) -> Option<Rgb> {
    let s = s.trim_start_matches('#');
    let s = if s.len() == 8 { &s[2..] } else { s };
    if s.len() != 6 {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some((b(0)?, b(2)?, b(4)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Solid,
    Dashed,
    Dotted,
    Double,
}

/// One drawn cell edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub line: Line,
    /// Width in points, where the format states one.
    pub width_pt: Option<f64>,
    pub color: Option<Rgb>,
}

/// A run of text with uniform character formatting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub superscript: bool,
    pub color: Option<Rgb>,
    /// Font size in points, where the format states one.
    pub size_pt: Option<f64>,
    pub family: Option<String>,
}

/// How one cell looks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    pub text: String,
    pub runs: Vec<Run>,
    pub fill: Option<Rgb>,
    pub align: Option<Align>,
    pub valign: Option<VAlign>,
    /// Top, right, bottom, left.
    pub borders: [Option<Edge>; 4],
}

impl Look {
    /// Runs that show something (whitespace-only runs don't).
    fn visible(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter().filter(|r| !r.text.trim().is_empty())
    }
    /// Every visible run has the property (and there is one).
    pub fn all(&self, f: impl Fn(&Run) -> bool) -> bool {
        self.visible().count() > 0 && self.visible().all(f)
    }
    /// Some visible run has the property.
    pub fn any(&self, f: impl Fn(&Run) -> bool) -> bool {
        self.visible().any(f)
    }
    /// The single value every visible run has, if they agree.
    pub fn uniform<T: PartialEq>(&self, f: impl Fn(&Run) -> T) -> Option<T> {
        let mut it = self.visible().map(f);
        let first = it.next()?;
        it.all(|v| v == first).then_some(first)
    }
    /// The visible run containing `needle`.
    pub fn run_with(&self, needle: &str) -> Option<&Run> {
        self.visible().find(|r| r.text.contains(needle))
    }
}

/// A rendered document as the readers see it.
#[derive(Debug, Clone, Default)]
pub struct Doc {
    /// Table cells (span origins), in document order.
    pub cells: Vec<Look>,
    /// Text outside the table: titles, notes.
    pub outside: String,
}

impl Doc {
    /// The cell whose text (whitespace removed) is `text`.
    pub fn cell(&self, text: &str) -> Option<&Look> {
        let squash = |s: &str| s.split_whitespace().collect::<String>();
        self.cells.iter().find(|c| squash(&c.text) == squash(text))
    }
}

/// Append a run, merging it into the last one when the formatting is the same.
pub fn push_run(runs: &mut Vec<Run>, run: Run) {
    if run.text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        let same = Run {
            text: String::new(),
            ..last.clone()
        } == Run {
            text: String::new(),
            ..run.clone()
        };
        if same {
            last.text.push_str(&run.text);
            return;
        }
    }
    runs.push(run);
}

pub fn finish(mut look: Look) -> Look {
    while look.runs.last().is_some_and(|r| r.text.trim().is_empty()) {
        look.runs.pop();
    }
    look.text = look.runs.iter().map(|r| r.text.as_str()).collect();
    look
}

fn local(e: &BytesStart) -> String {
    String::from_utf8(e.local_name().as_ref().to_vec()).unwrap()
}

fn local_end(e: &quick_xml::events::BytesEnd) -> String {
    String::from_utf8(e.local_name().as_ref().to_vec()).unwrap()
}

/// An OOXML on/off property: on when present without `val`, or with a true value.
fn on(e: &BytesStart) -> bool {
    !matches!(
        attr(e, "val").as_deref(),
        Some("0" | "false" | "off" | "none")
    )
}

fn side(name: &str) -> Option<usize> {
    Some(match name {
        "top" => 0,
        "right" | "end" => 1,
        "bottom" => 2,
        "left" | "start" => 3,
        _ => return None,
    })
}

// ─── DOCX ───

fn docx_edge(e: &BytesStart) -> Option<Edge> {
    let line = match attr(e, "val")?.as_str() {
        "nil" | "none" => return None,
        "dashed" | "dashSmallGap" | "dashDotStroked" => Line::Dashed,
        "dotted" | "dotDash" | "dotDotDash" => Line::Dotted,
        "double" => Line::Double,
        _ => Line::Solid,
    };
    Some(Edge {
        line,
        // Eighths of a point.
        width_pt: attr(e, "sz")
            .and_then(|s| s.parse::<f64>().ok())
            .map(|s| s / 8.0),
        color: attr(e, "color").as_deref().and_then(hex),
    })
}

/// State while reading a DOCX table cell.
#[derive(Default)]
struct DocxState {
    look: Option<Look>,
    run: Run,
    para_align: Option<Align>,
    cell_borders: [Option<Option<Edge>>; 4],
    tbl_borders: HashMap<String, Option<Edge>>,
    span: usize,
    cols: usize,
}

impl DocxState {
    /// A start or empty element, inside `parent`.
    fn element(&mut self, e: &BytesStart, parent: &str) {
        let name = local(e);
        match (parent, name.as_str()) {
            (_, "gridCol") => self.cols += 1,
            ("tblBorders", s) => {
                self.tbl_borders.insert(s.to_string(), docx_edge(e));
            }
            ("tcBorders", s) => {
                if let Some(i) = side(s) {
                    self.cell_borders[i] = Some(docx_edge(e));
                }
            }
            ("tcPr", "shd") => {
                if let Some(l) = self.look.as_mut() {
                    l.fill = attr(e, "fill").as_deref().and_then(hex);
                }
            }
            ("tcPr", "gridSpan") => {
                self.span = attr(e, "val").and_then(|v| v.parse().ok()).unwrap_or(1)
            }
            ("tcPr", "vAlign") => {
                if let Some(l) = self.look.as_mut() {
                    l.valign = Some(match attr(e, "val").as_deref() {
                        Some("center") => VAlign::Middle,
                        Some("bottom") => VAlign::Bottom,
                        _ => VAlign::Top,
                    });
                }
            }
            ("pPr", "jc") => {
                self.para_align = Some(match attr(e, "val").as_deref() {
                    Some("center") => Align::Center,
                    Some("right" | "end") => Align::Right,
                    Some("both" | "distribute") => Align::Justify,
                    _ => Align::Left,
                })
            }
            ("rPr", "b") => self.run.bold = on(e),
            ("rPr", "i") => self.run.italic = on(e),
            ("rPr", "u") => self.run.underline = on(e),
            ("rPr", "strike" | "dstrike") => self.run.strike = on(e),
            ("rPr", "vertAlign") => {
                self.run.superscript = attr(e, "val").as_deref() == Some("superscript")
            }
            ("rPr", "color") => self.run.color = attr(e, "val").as_deref().and_then(hex),
            ("rPr", "sz") => {
                self.run.size_pt = attr(e, "val")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v / 2.0)
            }
            ("rPr", "rFonts") => self.run.family = attr(e, "ascii"),
            ("r", "br" | "tab") => {
                if let Some(l) = self.look.as_mut() {
                    let r = Run {
                        text: " ".into(),
                        ..self.run.clone()
                    };
                    push_run(&mut l.runs, r);
                }
            }
            _ => {}
        }
    }
}

/// Read `word/document.xml`. A cell's borders are its own `tcBorders`, else the
/// table's `tblBorders` (outer edges, or `insideH` / `insideV`).
pub fn docx(xml: &str) -> Doc {
    let mut reader = Reader::from_str(xml);
    let mut doc = Doc::default();
    let mut st = DocxState::default();
    let mut path: Vec<String> = Vec::new();
    // Each cell's place and its own borders (`None`: not set, use the table's).
    type Place = (usize, usize, usize, [Option<Option<Edge>>; 4]);
    let mut places: Vec<Place> = Vec::new();
    let (mut row, mut col, mut rows) = (0usize, 0usize, 0usize);
    let mut in_text = false;
    loop {
        match reader.read_event().expect("well-formed XML") {
            Event::Start(e) => {
                let name = local(&e);
                match name.as_str() {
                    "tr" => col = 0,
                    "tc" => {
                        st.look = Some(Look::default());
                        st.cell_borders = [None; 4];
                        st.span = 1;
                    }
                    "p" => st.para_align = None,
                    "r" => st.run = Run::default(),
                    "t" => in_text = true,
                    _ => {}
                }
                st.element(&e, path.last().map(String::as_str).unwrap_or(""));
                path.push(name);
            }
            Event::Empty(e) => st.element(&e, path.last().map(String::as_str).unwrap_or("")),
            Event::Text(t) if in_text => {
                let text = t.unescape().unwrap().into_owned();
                match st.look.as_mut() {
                    Some(l) => push_run(
                        &mut l.runs,
                        Run {
                            text,
                            ..st.run.clone()
                        },
                    ),
                    None => doc.outside.push_str(&text),
                }
            }
            Event::End(e) => {
                path.pop();
                match local_end(&e).as_str() {
                    "t" => in_text = false,
                    "p" => match st.look.as_mut() {
                        Some(l) => {
                            if l.align.is_none() {
                                l.align = st.para_align;
                            }
                            push_run(
                                &mut l.runs,
                                Run {
                                    text: " ".into(),
                                    ..Run::default()
                                },
                            );
                        }
                        None => doc.outside.push('\n'),
                    },
                    "tc" => {
                        places.push((row, col, st.span, st.cell_borders));
                        col += st.span;
                        doc.cells.push(finish(st.look.take().unwrap()));
                    }
                    "tr" => {
                        row += 1;
                        rows = row;
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let names = [
        ["top", "insideH"],
        ["right", "insideV"],
        ["bottom", "insideH"],
        ["left", "insideV"],
    ];
    for (look, (r, c, span, own)) in doc.cells.iter_mut().zip(places) {
        let outer = [r == 0, c + span >= st.cols, r + 1 >= rows, c == 0];
        for i in 0..4 {
            look.borders[i] = match own[i] {
                Some(edge) => edge,
                None => st
                    .tbl_borders
                    .get(names[i][usize::from(!outer[i])])
                    .copied()
                    .flatten(),
            };
        }
    }
    doc
}

// ─── XLSX ───

#[derive(Debug, Default, Clone)]
struct Xf {
    font: usize,
    fill: usize,
    border: usize,
    align: Option<Align>,
    valign: Option<VAlign>,
}

#[derive(Default)]
struct XlsxStyles {
    fonts: Vec<Run>,
    fills: Vec<Option<Rgb>>,
    borders: Vec<[Option<Edge>; 4]>,
    xfs: Vec<Xf>,
}

impl XlsxStyles {
    fn element(&mut self, e: &BytesStart, path: &[String]) {
        let name = local(e);
        let parent = path.last().map(String::as_str).unwrap_or("");
        let within = |s: &str| path.iter().any(|p| p == s);
        match (parent, name.as_str()) {
            ("fonts", "font") => self.fonts.push(Run::default()),
            ("font", n) if within("fonts") => {
                let f = self.fonts.last_mut().unwrap();
                match n {
                    "b" => f.bold = on(e),
                    "i" => f.italic = on(e),
                    "u" => f.underline = on(e),
                    "strike" => f.strike = on(e),
                    "vertAlign" => f.superscript = attr(e, "val").as_deref() == Some("superscript"),
                    "sz" => f.size_pt = attr(e, "val").and_then(|v| v.parse().ok()),
                    "color" => f.color = attr(e, "rgb").as_deref().and_then(hex),
                    "name" => f.family = attr(e, "val"),
                    _ => {}
                }
            }
            ("fills", "fill") => self.fills.push(None),
            ("patternFill", "fgColor") if within("fills") => {
                *self.fills.last_mut().unwrap() = attr(e, "rgb").as_deref().and_then(hex)
            }
            ("borders", "border") => self.borders.push([None; 4]),
            ("border", s) if within("borders") => {
                if let (Some(i), Some(style)) = (side(s), attr(e, "style")) {
                    let (line, width) = match style.as_str() {
                        "dashed" => (Line::Dashed, 0.75),
                        "mediumDashed" | "dashDot" | "mediumDashDot" => (Line::Dashed, 1.5),
                        "dotted" => (Line::Dotted, 0.75),
                        "hair" => (Line::Dotted, 0.25),
                        "double" => (Line::Double, 0.75),
                        "medium" => (Line::Solid, 1.5),
                        "thick" => (Line::Solid, 2.25),
                        _ => (Line::Solid, 0.75),
                    };
                    self.borders.last_mut().unwrap()[i] = Some(Edge {
                        line,
                        width_pt: Some(width),
                        color: None,
                    });
                }
            }
            (s, "color") if within("borders") => {
                if let Some(edge) =
                    side(s).and_then(|i| self.borders.last_mut().unwrap()[i].as_mut())
                {
                    edge.color = attr(e, "rgb").as_deref().and_then(hex);
                }
            }
            ("cellXfs", "xf") => {
                let n = |k: &str| attr(e, k).and_then(|v| v.parse().ok()).unwrap_or(0);
                self.xfs.push(Xf {
                    font: n("fontId"),
                    fill: n("fillId"),
                    border: n("borderId"),
                    ..Xf::default()
                })
            }
            ("xf", "alignment") if within("cellXfs") => {
                let xf = self.xfs.last_mut().unwrap();
                xf.align = match attr(e, "horizontal").as_deref() {
                    Some("center" | "centerContinuous") => Some(Align::Center),
                    Some("right") => Some(Align::Right),
                    Some("justify" | "distributed") => Some(Align::Justify),
                    Some("left") => Some(Align::Left),
                    _ => None,
                };
                xf.valign = match attr(e, "vertical").as_deref() {
                    Some("top") => Some(VAlign::Top),
                    Some("center") => Some(VAlign::Middle),
                    Some("bottom") => Some(VAlign::Bottom),
                    _ => None,
                };
            }
            _ => {}
        }
    }

    fn look(&self, s: usize, text: String) -> Look {
        let xf = self.xfs.get(s).cloned().unwrap_or_default();
        let font = self.fonts.get(xf.font).cloned().unwrap_or_default();
        finish(Look {
            runs: vec![Run { text, ..font }],
            fill: self.fills.get(xf.fill).copied().flatten(),
            align: xf.align,
            valign: xf.valign,
            borders: self.borders.get(xf.border).copied().unwrap_or([None; 4]),
            ..Look::default()
        })
    }
}

/// Read the worksheet and styles parts (cells without `s` use style 0). Text
/// outside the table (title rows, notes) is not separated: every cell is listed.
pub fn xlsx(sheet: &str, styles: &str) -> Doc {
    let mut st = XlsxStyles::default();
    let mut reader = Reader::from_str(styles);
    let mut path: Vec<String> = Vec::new();
    loop {
        match reader.read_event().expect("well-formed styles.xml") {
            Event::Start(e) => {
                st.element(&e, &path);
                path.push(local(&e));
            }
            Event::Empty(e) => st.element(&e, &path),
            Event::End(_) => {
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut doc = Doc::default();
    let mut reader = Reader::from_str(sheet);
    let mut cell: Option<(usize, String)> = None;
    let mut in_text = false;
    let style = |e: &BytesStart| attr(e, "s").and_then(|v| v.parse().ok()).unwrap_or(0);
    loop {
        match reader.read_event().expect("well-formed sheet.xml") {
            Event::Start(e) if local(&e) == "c" => cell = Some((style(&e), String::new())),
            Event::Empty(e) if local(&e) == "c" => {
                doc.cells.push(st.look(style(&e), String::new()))
            }
            Event::Start(e) if matches!(local(&e).as_str(), "t" | "v") => in_text = true,
            Event::Text(t) if in_text => {
                if let Some(c) = cell.as_mut() {
                    c.1.push_str(&t.unescape().unwrap());
                }
            }
            Event::End(e) => match local_end(&e).as_str() {
                "t" | "v" => in_text = false,
                "c" => {
                    let (s, text) = cell.take().unwrap();
                    doc.cells.push(st.look(s, text));
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    doc
}

// ─── RTF ───

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String, Option<i32>),
    Text(char),
    Open,
    Close,
}

fn rtf_tokens(rtf: &str) -> Vec<Tok> {
    let chars: Vec<char> = rtf.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => out.push(Tok::Open),
            '}' => out.push(Tok::Close),
            '\\' => {
                i += 1;
                let Some(&c) = chars.get(i) else { break };
                if c.is_ascii_alphabetic() {
                    let mut name = String::new();
                    while i < chars.len() && chars[i].is_ascii_alphabetic() {
                        name.push(chars[i]);
                        i += 1;
                    }
                    let mut num = String::new();
                    if i < chars.len() && chars[i] == '-' {
                        num.push('-');
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        num.push(chars[i]);
                        i += 1;
                    }
                    // One space after a control word is its delimiter.
                    if i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                    out.push(Tok::Word(name, num.parse().ok()));
                    continue;
                }
                out.push(Tok::Text(c));
            }
            '\r' | '\n' => {}
            c => out.push(Tok::Text(c)),
        }
        i += 1;
    }
    out
}

/// Character formatting: RTF keeps it per group, until `\plain` — not `\pard`.
#[derive(Debug, Clone, Default)]
struct Chars {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    superscript: bool,
    color: usize,
    half_points: Option<i32>,
    font: Option<i32>,
}

/// One cell definition (`\cl…` words before its `\cellx`).
#[derive(Debug, Clone, Default)]
struct CellDef {
    fill: Option<Rgb>,
    borders: [Option<Edge>; 4],
    valign: Option<VAlign>,
    merged: bool,
}

/// The colour and font tables.
fn rtf_tables(toks: &[Tok]) -> (Vec<Option<Rgb>>, HashMap<i32, String>) {
    let mut colors = Vec::new();
    let mut fonts = HashMap::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Word(w, _) if w == "colortbl" => {
                let mut cur: Option<Rgb> = None;
                i += 1;
                while i < toks.len() && toks[i] != Tok::Close {
                    let c = cur.get_or_insert((0, 0, 0));
                    match &toks[i] {
                        Tok::Word(w, Some(n)) if w == "red" => c.0 = *n as u8,
                        Tok::Word(w, Some(n)) if w == "green" => c.1 = *n as u8,
                        Tok::Word(w, Some(n)) if w == "blue" => c.2 = *n as u8,
                        Tok::Text(';') => {
                            // The first entry is empty: "auto".
                            colors.push(if colors.is_empty() { None } else { cur });
                            cur = None;
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            Tok::Word(w, _) if w == "fonttbl" => {
                let mut depth = 1;
                let mut num = None;
                let mut name = String::new();
                i += 1;
                while i < toks.len() && depth > 0 {
                    match &toks[i] {
                        Tok::Open => depth += 1,
                        Tok::Close => depth -= 1,
                        Tok::Word(w, n) if w == "f" => num = *n,
                        Tok::Text(';') => {
                            if let Some(n) = num.take() {
                                fonts.insert(n, name.trim().to_string());
                            }
                            name.clear();
                        }
                        Tok::Text(c) => name.push(*c),
                        _ => {}
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (colors, fonts)
}

/// Read an RTF document. Paragraph alignment resets at `\pard`; character
/// formatting persists through `\pard` and `\cell` until `\plain` or the end of
/// its group, as Word and LibreOffice apply it — so formatting a writer forgets
/// to reset shows up in later cells. Font size defaults to 12pt (`\fs24`).
pub fn rtf(rtf: &str) -> Doc {
    let toks = rtf_tokens(rtf);
    let (colors, fonts) = rtf_tables(&toks);
    let color = |n: usize| colors.get(n).copied().flatten();

    let mut doc = Doc::default();
    let mut stack: Vec<Chars> = vec![Chars::default()];
    let mut skip_from: Option<usize> = None;
    let mut align: Option<Align> = None;
    let mut defs: Vec<CellDef> = Vec::new();
    let mut def = CellDef::default();
    let mut border_side: Option<usize> = None;
    let mut in_row = false;
    let mut cell_index = 0;
    let mut look = Look::default();
    let mut units: Vec<u16> = Vec::new();
    let mut skip_q = false;

    let run_of = |text: String, st: &Chars| Run {
        text,
        bold: st.bold,
        italic: st.italic,
        underline: st.underline,
        strike: st.strike,
        superscript: st.superscript,
        color: color(st.color),
        size_pt: Some(f64::from(st.half_points.unwrap_or(24)) / 2.0),
        family: st.font.and_then(|f| fonts.get(&f).cloned()),
    };
    // Text goes to the current cell inside a row, else outside the table.
    let emit = |text: String, st: &Chars, look: &mut Look, doc: &mut Doc, in_row: bool| {
        if in_row {
            push_run(&mut look.runs, run_of(text, st));
        } else {
            doc.outside.push_str(&text);
        }
    };

    for t in toks {
        if let Some(d) = skip_from {
            match t {
                Tok::Open => stack.push(stack.last().unwrap().clone()),
                Tok::Close => {
                    stack.pop();
                    if stack.len() < d {
                        skip_from = None;
                    }
                }
                _ => {}
            }
            continue;
        }
        // Pending `\uN` units end at anything but another `\u`.
        if !matches!(&t, Tok::Word(w, _) if w == "u") && !units.is_empty() {
            let text = String::from_utf16_lossy(&units);
            units.clear();
            emit(text, stack.last().unwrap(), &mut look, &mut doc, in_row);
        }
        match t {
            Tok::Open => {
                let top = stack.last().unwrap().clone();
                stack.push(top);
            }
            Tok::Close => {
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            Tok::Text(c) => {
                if skip_q && c == '?' {
                    skip_q = false;
                    continue;
                }
                emit(
                    c.to_string(),
                    stack.last().unwrap(),
                    &mut look,
                    &mut doc,
                    in_row,
                );
            }
            Tok::Word(w, n) => {
                let st = stack.last_mut().unwrap();
                let flag = n != Some(0);
                match w.as_str() {
                    "fonttbl" | "colortbl" | "stylesheet" | "info" => skip_from = Some(stack.len()),
                    "plain" => *st = Chars::default(),
                    "b" => st.bold = flag,
                    "i" => st.italic = flag,
                    "ul" => st.underline = flag,
                    "ulnone" => st.underline = false,
                    "strike" => st.strike = flag,
                    "super" => st.superscript = true,
                    "nosupersub" => st.superscript = false,
                    "cf" => st.color = n.unwrap_or(0) as usize,
                    "fs" => st.half_points = n,
                    "f" => st.font = n,
                    "pard" => align = None,
                    "ql" => align = Some(Align::Left),
                    "qc" => align = Some(Align::Center),
                    "qr" => align = Some(Align::Right),
                    "qj" => align = Some(Align::Justify),
                    "trowd" => {
                        defs.clear();
                        def = CellDef::default();
                        border_side = None;
                        in_row = true;
                        cell_index = 0;
                        look = Look::default();
                    }
                    "clcbpat" => def.fill = color(n.unwrap_or(0) as usize),
                    "clvmrg" => def.merged = true,
                    "clvertalt" => def.valign = Some(VAlign::Top),
                    "clvertalc" => def.valign = Some(VAlign::Middle),
                    "clvertalb" => def.valign = Some(VAlign::Bottom),
                    "clbrdrt" => border_side = Some(0),
                    "clbrdrr" => border_side = Some(1),
                    "clbrdrb" => border_side = Some(2),
                    "clbrdrl" => border_side = Some(3),
                    "brdrs" | "brdrth" | "brdrdash" | "brdrdashsm" | "brdrdot" | "brdrdb"
                    | "brdrnone" => {
                        if let Some(i) = border_side {
                            def.borders[i] = (w != "brdrnone").then_some(Edge {
                                line: match w.as_str() {
                                    "brdrdash" | "brdrdashsm" => Line::Dashed,
                                    "brdrdot" => Line::Dotted,
                                    "brdrdb" => Line::Double,
                                    _ => Line::Solid,
                                },
                                width_pt: None,
                                color: None,
                            });
                        }
                    }
                    "brdrw" => {
                        if let Some(e) = border_side.and_then(|i| def.borders[i].as_mut()) {
                            // Twips.
                            e.width_pt = n.map(|v| f64::from(v) / 20.0);
                        }
                    }
                    "brdrcf" => {
                        if let Some(e) = border_side.and_then(|i| def.borders[i].as_mut()) {
                            e.color = color(n.unwrap_or(0) as usize);
                        }
                    }
                    "cellx" => {
                        defs.push(std::mem::take(&mut def));
                        border_side = None;
                    }
                    "cell" if in_row => {
                        let d = defs.get(cell_index).cloned().unwrap_or_default();
                        let mut l = std::mem::take(&mut look);
                        l.fill = d.fill;
                        l.borders = d.borders;
                        l.valign = d.valign;
                        l.align = Some(align.unwrap_or(Align::Left));
                        if !d.merged {
                            doc.cells.push(finish(l));
                        }
                        cell_index += 1;
                    }
                    "row" => in_row = false,
                    "par" | "line" | "tab" => {
                        let text = if w == "par" && !in_row { "\n" } else { " " };
                        emit(
                            text.to_string(),
                            stack.last().unwrap(),
                            &mut look,
                            &mut doc,
                            in_row,
                        );
                    }
                    "u" => {
                        if let Some(n) = n {
                            units.push(n as i16 as u16);
                            skip_q = true;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
        }
        skip_q = false;
    }
    doc
}

// ─── HTML ───

/// One CSS rule: its selector (compounds, outermost first), specificity,
/// position and declarations.
struct Rule {
    compounds: Vec<Compound>,
    specificity: (usize, usize, usize),
    order: usize,
    decls: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct Compound {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
}

fn parse_compound(s: &str) -> Option<Compound> {
    if s.contains([':', '[', '>', '+', '~']) {
        return None;
    }
    let mut c = Compound::default();
    let mut cur = String::new();
    let mut kind = 't';
    let flush = |kind: char, cur: &mut String, c: &mut Compound| {
        if !cur.is_empty() {
            match kind {
                't' => c.tag = Some(cur.to_ascii_lowercase()),
                '.' => c.classes.push(cur.clone()),
                _ => c.id = Some(cur.clone()),
            }
        }
        cur.clear();
    };
    for ch in s.chars() {
        if ch == '.' || ch == '#' {
            flush(kind, &mut cur, &mut c);
            kind = ch;
        } else {
            cur.push(ch);
        }
    }
    flush(kind, &mut cur, &mut c);
    (c.tag.as_deref() != Some("*")).then_some(c)
}

fn parse_css(css: &str, rules: &mut Vec<Rule>) {
    let mut rest = css;
    // Comments.
    let css: String = {
        let mut out = String::new();
        while let Some(i) = rest.find("/*") {
            out.push_str(&rest[..i]);
            rest = rest[i..].find("*/").map_or("", |j| &rest[i + j + 2..]);
        }
        out.push_str(rest);
        out
    };
    for block in css.split('}') {
        let Some((sel, body)) = block.split_once('{') else {
            continue;
        };
        let decls: Vec<(String, String)> = body
            .split(';')
            .filter_map(|d| d.split_once(':'))
            .map(|(k, v)| {
                (
                    k.trim().to_ascii_lowercase(),
                    v.trim().trim_end_matches("!important").trim().to_string(),
                )
            })
            .collect();
        for one in sel.split(',') {
            let compounds: Option<Vec<Compound>> =
                one.split_whitespace().map(parse_compound).collect();
            let Some(compounds) = compounds.filter(|c| !c.is_empty()) else {
                continue;
            };
            let specificity = compounds.iter().fold((0, 0, 0), |(a, b, c), x| {
                (
                    a + usize::from(x.id.is_some()),
                    b + x.classes.len(),
                    c + usize::from(x.tag.is_some()),
                )
            });
            rules.push(Rule {
                compounds,
                specificity,
                order: rules.len(),
                decls: decls.clone(),
            });
        }
    }
}

fn matches(c: &Compound, e: &scraper::node::Element) -> bool {
    c.tag.as_deref().is_none_or(|t| t == e.name())
        && c.id.as_deref().is_none_or(|id| e.id() == Some(id))
        && c.classes
            .iter()
            .all(|k| e.has_class(k, scraper::CaseSensitivity::CaseSensitive))
}

/// Whether `rule` selects `el` (descendant combinators only).
fn selects(rule: &Rule, el: scraper::ElementRef) -> bool {
    let (last, ancestors) = rule.compounds.split_last().unwrap();
    if !matches(last, el.value()) {
        return false;
    }
    let mut need = ancestors.iter().rev().peekable();
    for anc in el.ancestors().filter_map(scraper::ElementRef::wrap) {
        if let Some(c) = need.peek() {
            if matches(c, anc.value()) {
                need.next();
            }
        }
    }
    need.peek().is_none()
}

/// An element's declared properties: matching rules by specificity and order,
/// then its inline `style`.
fn declared(el: scraper::ElementRef, rules: &[Rule]) -> HashMap<String, String> {
    let mut hits: Vec<&Rule> = rules.iter().filter(|r| selects(r, el)).collect();
    hits.sort_by_key(|r| (r.specificity, r.order));
    let mut out = HashMap::new();
    for r in hits {
        for (k, v) in &r.decls {
            out.insert(k.clone(), v.clone());
        }
    }
    if let Some(style) = el.value().attr("style") {
        for d in style.split(';').filter_map(|d| d.split_once(':')) {
            out.insert(d.0.trim().to_ascii_lowercase(), d.1.trim().to_string());
        }
    }
    out
}

fn css_color(v: &str) -> Option<Rgb> {
    let c: gridwell_core::Color = v.trim().parse().ok()?;
    if c.is_transparent() {
        return None;
    }
    hex(&c.flatten().to_rrggbb())
}

fn css_pt(v: &str, parent_pt: f64) -> Option<f64> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse::<f64>().ok().map(|n| n * 0.75);
    }
    if let Some(n) = v.strip_suffix("pt") {
        return n.trim().parse().ok();
    }
    if let Some(n) = v.strip_suffix("rem") {
        return n.trim().parse::<f64>().ok().map(|n| n * 12.0);
    }
    if let Some(n) = v.strip_suffix("em") {
        return n.trim().parse::<f64>().ok().map(|n| n * parent_pt);
    }
    if let Some(n) = v.strip_suffix('%') {
        return n.trim().parse::<f64>().ok().map(|n| n / 100.0 * parent_pt);
    }
    None
}

fn css_edge(v: &str) -> Option<Edge> {
    let mut line = None;
    let mut width = None;
    let mut color = None;
    for part in v.split_whitespace() {
        match part {
            "none" | "hidden" => return None,
            "solid" => line = Some(Line::Solid),
            "dashed" => line = Some(Line::Dashed),
            "dotted" => line = Some(Line::Dotted),
            "double" => line = Some(Line::Double),
            "thin" => width = Some(0.75),
            "medium" => width = Some(2.25),
            "thick" => width = Some(3.75),
            p => {
                if let Some(w) = css_pt(p, 12.0) {
                    width = Some(w);
                } else if let Some(c) = css_color(p) {
                    color = Some(c);
                }
            }
        }
    }
    Some(Edge {
        line: line?,
        width_pt: width,
        color,
    })
}

/// Inherited text formatting while walking down from the table.
#[derive(Debug, Clone)]
struct Inherited {
    run: Run,
    align: Option<Align>,
    transform: Option<String>,
}

fn apply_text(props: &HashMap<String, String>, tag: &str, inh: &mut Inherited) {
    let run = &mut inh.run;
    match tag {
        "b" | "strong" | "th" => run.bold = true,
        "i" | "em" => run.italic = true,
        "u" | "ins" => run.underline = true,
        "s" | "strike" | "del" => run.strike = true,
        "sup" => run.superscript = true,
        _ => {}
    }
    if tag == "th" {
        inh.align = Some(Align::Center);
    }
    if let Some(w) = props.get("font-weight") {
        run.bold =
            matches!(w.as_str(), "bold" | "bolder") || w.parse::<u32>().is_ok_and(|n| n >= 600);
    }
    if let Some(s) = props.get("font-style") {
        run.italic = s == "italic" || s == "oblique";
    }
    for k in ["text-decoration", "text-decoration-line"] {
        if let Some(d) = props.get(k) {
            // Decorations propagate to descendants; they add up.
            run.underline |= d.contains("underline");
            run.strike |= d.contains("line-through");
        }
    }
    if let Some(c) = props.get("color").and_then(|c| css_color(c)) {
        run.color = Some(c);
    }
    if let Some(s) = props.get("font-size") {
        let parent = run.size_pt.unwrap_or(12.0);
        run.size_pt = css_pt(s, parent).or(run.size_pt);
    }
    if let Some(f) = props.get("font-family") {
        run.family = f
            .split(',')
            .next()
            .map(|f| f.trim().trim_matches(['"', '\'']).to_string());
    }
    if let Some(a) = props.get("text-align") {
        inh.align = match a.as_str() {
            "right" | "end" => Some(Align::Right),
            "center" => Some(Align::Center),
            "justify" => Some(Align::Justify),
            _ => Some(Align::Left),
        };
    }
    if let Some(t) = props.get("text-transform") {
        inh.transform = Some(t.clone());
    }
}

fn transformed(text: &str, transform: Option<&str>) -> String {
    match transform {
        Some("uppercase") => text.to_uppercase(),
        Some("lowercase") => text.to_lowercase(),
        _ => text.to_string(),
    }
}

/// Read an HTML fragment: the (first) table's cells with their computed look,
/// from the writer's `<style>` blocks, inline styles, tag defaults (`th` is bold
/// and centred, `strong`/`em`/`u`/`s`/`sup` as named) and inheritance. A cell's
/// fill is its own background, else its row's, else its section's.
pub fn html(html: &str) -> Doc {
    let doc_html = scraper::Html::parse_fragment(html);
    let mut rules = Vec::new();
    for style in doc_html.select(&scraper::Selector::parse("style").unwrap()) {
        parse_css(&style.text().collect::<String>(), &mut rules);
    }
    let mut doc = Doc::default();
    let cells = scraper::Selector::parse("table td, table th").unwrap();
    let base = Inherited {
        run: Run {
            size_pt: Some(12.0),
            ..Run::default()
        },
        align: None,
        transform: None,
    };
    for td in doc_html.select(&cells) {
        // Inherit from the table down to the cell.
        let mut chain: Vec<scraper::ElementRef> = td
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .take_while(|a| a.value().name() != "div" && a.value().name() != "html")
            .collect();
        chain.reverse();
        let mut inh = base.clone();
        let mut fill = None;
        for a in &chain {
            let props = declared(*a, &rules);
            apply_text(&props, a.value().name(), &mut inh);
            if let Some(c) = props
                .get("background-color")
                .or(props.get("background"))
                .and_then(|c| css_color(c))
            {
                fill = Some(c);
            }
        }
        let props = declared(td, &rules);
        apply_text(&props, td.value().name(), &mut inh);
        if let Some(c) = props
            .get("background-color")
            .or(props.get("background"))
            .and_then(|c| css_color(c))
        {
            fill = Some(c);
        }
        let mut look = Look {
            fill,
            align: inh.align,
            valign: props.get("vertical-align").and_then(|v| match v.as_str() {
                "top" => Some(VAlign::Top),
                "middle" => Some(VAlign::Middle),
                "bottom" => Some(VAlign::Bottom),
                _ => None,
            }),
            ..Look::default()
        };
        let sides = ["top", "right", "bottom", "left"];
        if let Some(b) = props.get("border") {
            look.borders = [css_edge(b); 4];
        }
        for (i, s) in sides.iter().enumerate() {
            if let Some(b) = props.get(&format!("border-{s}")) {
                look.borders[i] = css_edge(b);
            }
            let style = props.get(&format!("border-{s}-style"));
            let width = props.get(&format!("border-{s}-width"));
            let color = props.get(&format!("border-{s}-color"));
            if style.is_some() || width.is_some() || color.is_some() {
                let mut e = look.borders[i].unwrap_or(Edge {
                    line: Line::Solid,
                    width_pt: None,
                    color: None,
                });
                if let Some(s) = style {
                    match css_edge(s) {
                        Some(x) => e.line = x.line,
                        None => {
                            look.borders[i] = None;
                            continue;
                        }
                    }
                }
                if let Some(w) = width {
                    e.width_pt = css_pt(w, 12.0);
                }
                if let Some(c) = color {
                    e.color = css_color(c);
                }
                look.borders[i] = Some(e);
            }
        }
        html_runs(td, &rules, &inh, &mut look.runs);
        doc.cells.push(finish(look));
    }
    for el in doc_html
        .root_element()
        .descendants()
        .filter_map(scraper::ElementRef::wrap)
        .filter(|e| {
            matches!(
                e.value().name(),
                "caption" | "p" | "div" | "span" | "h1" | "h2" | "h3"
            )
        })
    {
        if el
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .any(|a| a.value().name() == "td" || a.value().name() == "th")
        {
            continue;
        }
        let own: String = el
            .children()
            .filter_map(|c| c.value().as_text().map(|t| t.to_string()))
            .collect();
        doc.outside.push_str(&own);
        doc.outside.push('\n');
    }
    doc
}

fn html_runs(el: scraper::ElementRef, rules: &[Rule], inh: &Inherited, runs: &mut Vec<Run>) {
    for child in el.children() {
        if let Some(t) = child.value().as_text() {
            push_run(
                runs,
                Run {
                    text: transformed(t, inh.transform.as_deref()),
                    ..inh.run.clone()
                },
            );
        } else if let Some(e) = scraper::ElementRef::wrap(child) {
            match e.value().name() {
                "br" => push_run(
                    runs,
                    Run {
                        text: " ".into(),
                        ..inh.run.clone()
                    },
                ),
                "img" => push_run(
                    runs,
                    Run {
                        text: e.value().attr("alt").unwrap_or("").to_string(),
                        ..inh.run.clone()
                    },
                ),
                "style" | "script" => {}
                name => {
                    let mut inner = inh.clone();
                    apply_text(&declared(e, rules), name, &mut inner);
                    html_runs(e, rules, &inner, runs);
                }
            }
        }
    }
}

// ─── LaTeX ───

/// Character state while interpreting LaTeX source.
#[derive(Debug, Clone)]
struct TexState {
    run: Run,
}

fn tex_group(chars: &[char], i: &mut usize) -> String {
    // `{…}` at chars[*i] (after optional spaces); returns the inside.
    while *i < chars.len() && chars[*i] == ' ' {
        *i += 1;
    }
    if *i >= chars.len() || chars[*i] != '{' {
        // An unbraced single token.
        let s = chars.get(*i).map(|c| c.to_string()).unwrap_or_default();
        *i += 1;
        return s;
    }
    let mut depth = 0;
    let start = *i + 1;
    while *i < chars.len() {
        match chars[*i] {
            '\\' => *i += 1,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    *i += 1;
                    return chars[start..*i - 1].iter().collect();
                }
            }
            _ => {}
        }
        *i += 1;
    }
    chars[start..].iter().collect()
}

fn tex_optional(chars: &[char], i: &mut usize) -> Option<String> {
    if chars.get(*i) != Some(&'[') {
        return None;
    }
    let start = *i + 1;
    while *i < chars.len() && chars[*i] != ']' {
        *i += 1;
    }
    *i += 1;
    Some(chars[start..*i - 1].iter().collect())
}

fn tex_color(model: Option<&str>, spec: &str) -> Option<Rgb> {
    match model {
        Some("HTML") => hex(spec),
        _ => match spec {
            "red" => Some((255, 0, 0)),
            "blue" => Some((0, 0, 255)),
            "black" => None,
            _ => None,
        },
    }
}

/// Interpret cell content: the macros the LaTeX writer emits. `fill` receives
/// `\cellcolor`, `align` a `\multicolumn`'s alignment.
fn tex_runs(src: &str, st: &mut TexState, out: &mut Vec<Run>, fill: &mut Option<Rgb>) {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let text = |out: &mut Vec<Run>, st: &TexState, s: &str| {
        push_run(
            out,
            Run {
                text: s.to_string(),
                ..st.run.clone()
            },
        )
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => {
                let inner = tex_group(&chars, &mut i);
                let mut inner_st = st.clone();
                tex_runs(&inner, &mut inner_st, out, fill);
                continue;
            }
            '}' => {}
            '~' => text(out, st, " "),
            '\\' => {
                i += 1;
                let mut name = String::new();
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    name.push(chars[i]);
                    i += 1;
                }
                if name.is_empty() {
                    // Escaped symbol, or `\\` (a line break).
                    let sym = chars.get(i).copied().unwrap_or(' ');
                    i += 1;
                    let s = if sym == '\\' {
                        " ".to_string()
                    } else {
                        sym.to_string()
                    };
                    text(out, st, &s);
                    // `\\[2pt]`
                    let _ = tex_optional(&chars, &mut i);
                    continue;
                }
                // A space after a control word is part of it.
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                let scoped = |st: &TexState,
                              f: &dyn Fn(&mut TexState),
                              i: &mut usize,
                              out: &mut Vec<Run>,
                              fill: &mut Option<Rgb>| {
                    let inner = tex_group(&chars, i);
                    let mut s = st.clone();
                    f(&mut s);
                    tex_runs(&inner, &mut s, out, fill);
                };
                match name.as_str() {
                    "textbf" => scoped(st, &|s| s.run.bold = true, &mut i, out, fill),
                    "textit" | "emph" => scoped(st, &|s| s.run.italic = true, &mut i, out, fill),
                    "underline" | "uline" => {
                        scoped(st, &|s| s.run.underline = true, &mut i, out, fill)
                    }
                    "sout" | "st" => scoped(st, &|s| s.run.strike = true, &mut i, out, fill),
                    "texttt" => scoped(
                        st,
                        &|s| s.run.family = Some("monospace".into()),
                        &mut i,
                        out,
                        fill,
                    ),
                    "textsuperscript" => {
                        scoped(st, &|s| s.run.superscript = true, &mut i, out, fill)
                    }
                    "MakeUppercase" | "uppercase" => {
                        let inner = tex_group(&chars, &mut i);
                        let mut runs = Vec::new();
                        tex_runs(&inner, &mut st.clone(), &mut runs, fill);
                        for mut r in runs {
                            r.text = r.text.to_uppercase();
                            push_run(out, r);
                        }
                    }
                    "bfseries" => st.run.bold = true,
                    "itshape" => st.run.italic = true,
                    "ttfamily" => st.run.family = Some("monospace".into()),
                    "textcolor" => {
                        let model = tex_optional(&chars, &mut i);
                        let spec = tex_group(&chars, &mut i);
                        let color = tex_color(model.as_deref(), &spec);
                        scoped(st, &|s| s.run.color = color, &mut i, out, fill);
                    }
                    "color" => {
                        let model = tex_optional(&chars, &mut i);
                        let spec = tex_group(&chars, &mut i);
                        st.run.color = tex_color(model.as_deref(), &spec);
                    }
                    "cellcolor" => {
                        let model = tex_optional(&chars, &mut i);
                        let spec = tex_group(&chars, &mut i);
                        *fill = tex_color(model.as_deref(), &spec);
                    }
                    "fontsize" => {
                        let size = tex_group(&chars, &mut i);
                        let _ = tex_group(&chars, &mut i);
                        st.run.size_pt = size.trim_end_matches("pt").trim().parse().ok();
                    }
                    "selectfont" => {}
                    "tiny" => st.run.size_pt = Some(5.0),
                    "scriptsize" => st.run.size_pt = Some(7.0),
                    "footnotesize" => st.run.size_pt = Some(8.0),
                    "small" => st.run.size_pt = Some(9.0),
                    "normalsize" => st.run.size_pt = Some(10.0),
                    "large" => st.run.size_pt = Some(12.0),
                    "Large" => st.run.size_pt = Some(14.4),
                    "LARGE" => st.run.size_pt = Some(17.28),
                    "huge" => st.run.size_pt = Some(20.74),
                    "Huge" => st.run.size_pt = Some(24.88),
                    "textbackslash" => text(out, st, "\\"),
                    "textless" => text(out, st, "<"),
                    "textgreater" => text(out, st, ">"),
                    "textbar" => text(out, st, "|"),
                    "textasciitilde" => text(out, st, "~"),
                    "textasciicircum" => text(out, st, "^"),
                    "newline" | "linebreak" => text(out, st, " "),
                    "multirow" => {
                        let _ = tex_group(&chars, &mut i);
                        let _ = tex_optional(&chars, &mut i);
                        let _ = tex_group(&chars, &mut i);
                        let _ = tex_optional(&chars, &mut i);
                        let inner = tex_group(&chars, &mut i);
                        tex_runs(&inner, &mut st.clone(), out, fill);
                    }
                    "begin" => {
                        // A nested tabular: its rows are lines of this cell.
                        let _env = tex_group(&chars, &mut i);
                        let _ = tex_optional(&chars, &mut i);
                        let _ = tex_group(&chars, &mut i);
                    }
                    "end" => {
                        let _ = tex_group(&chars, &mut i);
                    }
                    _ => {}
                }
                continue;
            }
            '\n' => text(out, st, " "),
            c => text(out, st, &c.to_string()),
        }
        i += 1;
    }
}

/// Split at `sep` characters at brace depth 0, outside nested environments.
fn tex_split<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let bytes = s.as_bytes();
    let (mut depth, mut env, mut start, mut i) = (0i32, 0i32, 0usize, 0usize);
    while i < bytes.len() {
        if s[i..].starts_with("\\begin{") {
            env += 1;
        } else if s[i..].starts_with("\\end{") {
            env -= 1;
        }
        match bytes[i] {
            b'\\' if !s[i..].starts_with(sep) => {
                i += 2;
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && env == 0 && s[i..].starts_with(sep) {
            parts.push(&s[start..i]);
            i += sep.len();
            start = i;
            continue;
        }
        i += 1;
    }
    parts.push(&s[start..]);
    parts
}

/// Read LaTeX source as the writer emits it (source-level: the compile oracle
/// checks that it typesets). Columns' alignment comes from the column spec or a
/// cell's `\multicolumn`; text outside the tabular (titles, notes) is
/// `outside`. Rules (`\toprule`, `\hline`…) are table rules, not cell borders.
pub fn latex(src: &str) -> Doc {
    let mut doc = Doc::default();
    let base = TexState {
        run: Run {
            size_pt: Some(10.0),
            ..Run::default()
        },
    };
    let begin = ["\\begin{tabular}{", "\\begin{longtable}{"]
        .iter()
        .filter_map(|b| src.find(b).map(|i| (i, b.len())))
        .min();
    let Some((start, blen)) = begin else {
        let mut runs = Vec::new();
        tex_runs(src, &mut base.clone(), &mut runs, &mut None);
        doc.outside = runs.iter().map(|r| r.text.as_str()).collect();
        return doc;
    };
    let chars: Vec<char> = src[start + blen - 1..].chars().collect();
    let mut k = 0;
    let spec = tex_group(&chars, &mut k);
    let end = ["\n\\end{tabular}", "\n\\end{longtable}"]
        .iter()
        .filter_map(|e| src[start..].find(e).map(|i| start + i))
        .min()
        .unwrap_or(src.len());
    let body_start = start + blen - 1 + chars[..k].iter().collect::<String>().len();
    let body = &src[body_start..end];
    let mut before = Vec::new();
    tex_runs(&src[..start], &mut base.clone(), &mut before, &mut None);
    let mut after = Vec::new();
    tex_runs(&src[end..], &mut base.clone(), &mut after, &mut None);
    doc.outside = before
        .iter()
        .chain(&after)
        .map(|r| r.text.as_str())
        .collect();
    // Column alignments.
    let mut aligns = Vec::new();
    let sc: Vec<char> = spec.chars().collect();
    let mut j = 0;
    while j < sc.len() {
        match sc[j] {
            'l' => aligns.push(Align::Left),
            'c' => aligns.push(Align::Center),
            'r' => aligns.push(Align::Right),
            'p' | 'm' | 'b' => {
                aligns.push(Align::Justify);
                j += 1;
                let _ = tex_group(&sc, &mut j);
                continue;
            }
            '@' | '>' | '<' | '!' => {
                j += 1;
                let _ = tex_group(&sc, &mut j);
                continue;
            }
            _ => {}
        }
        j += 1;
    }
    for line in tex_split(body, "\\\\") {
        let line = line.trim();
        // Drop rules and longtable markers.
        let mut l = line.to_string();
        for rule in [
            "\\toprule",
            "\\midrule",
            "\\bottomrule",
            "\\hline",
            "\\endhead",
            "\\endfirsthead",
            "\\addlinespace",
        ] {
            l = l.replace(rule, "");
        }
        while let Some(p) = l.find("\\cmidrule") {
            let rest: Vec<char> = l[p + 9..].chars().collect();
            let mut q = 0;
            let _ = (rest.first() == Some(&'(')).then(|| {
                while q < rest.len() && rest[q] != ')' {
                    q += 1;
                }
                q += 1;
            });
            let _ = tex_group(&rest, &mut q);
            let consumed: String = rest[..q].iter().collect();
            l = format!("{}{}", &l[..p], &l[p + 9 + consumed.len()..]);
        }
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        let mut col = 0;
        for cell in tex_split(l, "&") {
            let cell = cell.trim();
            let mut fill = None;
            let mut runs = Vec::new();
            let mut align = aligns.get(col).copied();
            let mut span = 1;
            let content = if let Some(rest) = cell.strip_prefix("\\multicolumn") {
                let rc: Vec<char> = rest.chars().collect();
                let mut q = 0;
                span = tex_group(&rc, &mut q).trim().parse().unwrap_or(1);
                let a = tex_group(&rc, &mut q);
                align = match a.trim() {
                    "c" => Some(Align::Center),
                    "r" => Some(Align::Right),
                    "l" => Some(Align::Left),
                    _ => Some(Align::Justify),
                };
                tex_group(&rc, &mut q)
            } else {
                cell.to_string()
            };
            tex_runs(&content, &mut base.clone(), &mut runs, &mut fill);
            col += span;
            doc.cells.push(finish(Look {
                runs,
                fill,
                align,
                ..Look::default()
            }));
        }
    }
    doc
}
