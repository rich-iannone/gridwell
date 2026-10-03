//! Readers for HTML (html5ever), XLSX (calamine), RTF and the Pandoc JSON AST.

use std::io::Cursor;

use calamine::{Data, Reader as _, Xlsx};
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;

use super::Grid;

/// Place cells with spans into a grid, row by row, HTML-table style: each cell
/// goes to the first free position of its row (positions taken by rowspans from
/// above are skipped).
fn place(rows: Vec<Vec<(String, usize, usize)>>) -> Grid {
    let mut grid: Vec<Vec<Option<String>>> = vec![Vec::new(); rows.len()];
    for (r, cells) in rows.into_iter().enumerate() {
        let mut c = 0;
        for (text, colspan, rowspan) in cells {
            while grid[r].get(c).is_some_and(Option::is_some) {
                c += 1;
            }
            for rr in r..(r + rowspan.max(1)).min(grid.len()) {
                let row = &mut grid[rr];
                if row.len() < c + colspan.max(1) {
                    row.resize(c + colspan.max(1), None);
                }
                for slot in &mut row[c..c + colspan.max(1)] {
                    *slot = Some(text.clone());
                }
            }
            c += colspan.max(1);
        }
    }
    grid.into_iter()
        .map(|r| r.into_iter().map(Option::unwrap_or_default).collect())
        .collect()
}

// ─── HTML ───

/// Text of an element: text nodes and image alt text, in document order.
fn element_text(e: ElementRef) -> String {
    let mut out = String::new();
    for node in e.descendants() {
        match node.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(el) if el.name() == "img" => {
                out.push_str(el.attr("alt").unwrap_or(""));
            }
            _ => {}
        }
    }
    out
}

/// The grid of the (first) table in an HTML fragment, parsed by html5ever.
pub fn html_grid(html: &str) -> Grid {
    let doc = Html::parse_fragment(html);
    let tr = Selector::parse("table > thead > tr, table > tbody > tr").unwrap();
    let cell = Selector::parse(":scope > th, :scope > td").unwrap();
    let rows = doc
        .select(&tr)
        .map(|row| {
            row.select(&cell)
                .map(|c| {
                    let span =
                        |a: &str| c.value().attr(a).and_then(|v| v.parse().ok()).unwrap_or(1);
                    (element_text(c), span("colspan"), span("rowspan"))
                })
                .collect()
        })
        .collect();
    place(rows)
}

// ─── XLSX ───

/// The grid of the table in an .xlsx file, read with calamine: `skip` rows
/// (header lines) then `rows` × `width` cells, with merged ranges expanded.
/// Numeric cells come back as their number's decimal form.
pub fn xlsx_grid(bytes: &[u8], skip: usize, rows: usize, width: usize) -> Grid {
    let mut book: Xlsx<_> =
        Xlsx::new(Cursor::new(bytes.to_vec())).expect("calamine opens the workbook");
    book.load_merged_regions().expect("merged regions");
    let range = book.worksheet_range("Table").expect("sheet 'Table'");
    let at = |r: usize, c: usize| -> String {
        match range.get_value((r as u32, c as u32)) {
            Some(Data::String(s)) => s.clone(),
            Some(Data::Float(f)) => f.to_string(),
            Some(Data::Int(i)) => i.to_string(),
            Some(Data::Bool(b)) => b.to_string(),
            Some(Data::Empty) | None => String::new(),
            Some(other) => other.to_string(),
        }
    };
    let mut grid: Grid = (0..rows)
        .map(|r| (0..width).map(|c| at(skip + r, c)).collect())
        .collect();
    for (_, _, dims) in book.merged_regions_by_sheet("Table") {
        let (r0, c0) = (dims.start.0 as usize, dims.start.1 as usize);
        let (r1, c1) = (dims.end.0 as usize, dims.end.1 as usize);
        let v = at(r0, c0);
        // Merged rows/columns that fall inside the table area.
        let first = r0.max(skip) - skip;
        let last = (r1 + 1).saturating_sub(skip).min(rows);
        for grid_row in grid.iter_mut().take(last).skip(first) {
            for slot in grid_row.iter_mut().take((c1 + 1).min(width)).skip(c0) {
                slot.clone_from(&v);
            }
        }
    }
    grid
}

// ─── RTF ───

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String, Option<i32>),
    Text(char),
    Open,
    Close,
}

fn tokenize(rtf: &str) -> Vec<Tok> {
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
                // Escaped symbol: \\ \{ \}
                out.push(Tok::Text(c));
            }
            '\r' | '\n' => {}
            c => out.push(Tok::Text(c)),
        }
        i += 1;
    }
    out
}

/// One cell definition of an RTF row: right edge and vertical-merge role.
struct CellDef {
    right: i32,
    continues: bool,
}

/// The grid of the table in an RTF document: rows from `\trowd … \row`, cells from
/// `\cellx` boundaries (a cell spans every column boundary it crosses, over the
/// union of all rows' boundaries) and `\clvmrg` continuations. `None` if some
/// column boundary never appears in any row, so columns can't be told apart.
pub fn rtf_grid(rtf: &str, width: usize) -> Option<Grid> {
    let toks = tokenize(rtf);
    let mut rows: Vec<(Vec<CellDef>, Vec<String>)> = Vec::new();
    let mut defs: Vec<CellDef> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut continues = false;
    let mut in_row = false;
    let mut depth = 0;
    let mut skip_depth: Option<i32> = None;
    let mut pending_unicode_skip = false;
    // `\uN` code units, decoded together so surrogate pairs survive.
    let mut units: Vec<u16> = Vec::new();
    let flush = |units: &mut Vec<u16>, text: &mut String| {
        if !units.is_empty() {
            text.push_str(&String::from_utf16_lossy(units));
            units.clear();
        }
    };
    for t in toks {
        if let Some(d) = skip_depth {
            match t {
                Tok::Open => depth += 1,
                Tok::Close => {
                    depth -= 1;
                    if depth < d {
                        skip_depth = None;
                    }
                }
                _ => {}
            }
            continue;
        }
        match t {
            Tok::Open => depth += 1,
            Tok::Close => depth -= 1,
            Tok::Word(w, n) => match w.as_str() {
                "fonttbl" | "colortbl" => skip_depth = Some(depth),
                "trowd" => {
                    units.clear();
                    in_row = true;
                    defs.clear();
                    texts.clear();
                    text.clear();
                    continues = false;
                }
                "clvmrg" => continues = true,
                "cellx" => {
                    defs.push(CellDef {
                        right: n.unwrap_or(0),
                        continues,
                    });
                    continues = false;
                }
                "cell" if in_row => {
                    flush(&mut units, &mut text);
                    texts.push(std::mem::take(&mut text));
                }
                "row" if in_row => {
                    rows.push((std::mem::take(&mut defs), std::mem::take(&mut texts)));
                    in_row = false;
                }
                "tab" => {
                    flush(&mut units, &mut text);
                    text.push(' ');
                }
                "u" => {
                    if let Some(n) = n {
                        // Only table text matters; titles and notes are outside rows.
                        if in_row {
                            units.push(n as i16 as u16);
                        }
                        pending_unicode_skip = true;
                        continue;
                    }
                }
                _ => {}
            },
            Tok::Text(c) => {
                if pending_unicode_skip && c == '?' {
                    pending_unicode_skip = false;
                    continue;
                }
                if in_row {
                    flush(&mut units, &mut text);
                    text.push(c);
                }
            }
        }
        pending_unicode_skip = false;
    }
    let mut bounds: Vec<i32> = rows
        .iter()
        .flat_map(|(d, _)| d.iter().map(|d| d.right))
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    if bounds.len() != width {
        return None;
    }
    let col_of = |right: i32| bounds.iter().position(|&b| b == right).unwrap();
    let mut grid: Grid = Vec::new();
    for (defs, texts) in rows {
        let mut out = vec![String::new(); width];
        let mut left = 0;
        for (def, text) in defs.iter().zip(texts) {
            let end = col_of(def.right);
            for (c, slot) in out.iter_mut().enumerate().take(end + 1).skip(left) {
                *slot = if def.continues {
                    grid.last()
                        .map(|r: &Vec<String>| r[c].clone())
                        .unwrap_or_default()
                } else {
                    text.clone()
                };
            }
            left = end + 1;
        }
        grid.push(out);
    }
    Some(grid)
}

// ─── Pandoc AST ───

/// Plain text of Pandoc inlines/blocks (any JSON below a cell).
fn pandoc_text(v: &Value, out: &mut String) {
    match v {
        Value::Object(o) => match o.get("t").and_then(Value::as_str) {
            Some("Str") => out.push_str(o["c"].as_str().unwrap_or("")),
            Some("Space") | Some("SoftBreak") => out.push(' '),
            Some("LineBreak") => {}
            // Image: [attr, alt inlines, target]
            Some("Image") => pandoc_text(&o["c"][1], out),
            Some("RawInline") | Some("RawBlock") => out.push_str(o["c"][1].as_str().unwrap_or("")),
            _ => {
                if let Some(c) = o.get("c") {
                    pandoc_text(c, out);
                }
            }
        },
        Value::Array(a) => a.iter().for_each(|x| pandoc_text(x, out)),
        _ => {}
    }
}

fn find_table(v: &Value) -> Option<&Value> {
    match v {
        Value::Object(o) if o.get("t").and_then(Value::as_str) == Some("Table") => Some(v),
        Value::Object(o) => o.values().find_map(find_table),
        Value::Array(a) => a.iter().find_map(find_table),
        _ => None,
    }
}

/// The grid of the first Table block in Pandoc JSON (a block, a Quarto `Div`, or a
/// whole document): head rows, then each body's intermediate head rows and body
/// rows. The table foot (notes) is not part of the grid.
pub fn pandoc_grid(json: &str) -> Grid {
    let v: Value = serde_json::from_str(json).expect("pandoc JSON");
    let Some(table) = find_table(&v) else {
        return Vec::new();
    };
    let c = &table["c"];
    let row_cells = |row: &Value| -> Vec<(String, usize, usize)> {
        row[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| {
                let mut text = String::new();
                pandoc_text(&cell[4], &mut text);
                let n = |i: usize| cell[i].as_u64().unwrap_or(1) as usize;
                (text, n(3), n(2))
            })
            .collect()
    };
    let mut sections: Vec<Vec<Vec<(String, usize, usize)>>> = Vec::new();
    sections.push(c[3][1].as_array().unwrap().iter().map(row_cells).collect());
    for body in c[4].as_array().unwrap() {
        sections.push(body[2].as_array().unwrap().iter().map(row_cells).collect());
        sections.push(body[3].as_array().unwrap().iter().map(row_cells).collect());
    }
    // Spans never cross sections, so each is placed on its own.
    sections.into_iter().flat_map(place).collect()
}
