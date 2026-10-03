//! XML readers for the OOXML parts: rebuild a table's logical grid from the format's
//! own merge markup.

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::Grid;

pub fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name.as_bytes())
        .map(|a| String::from_utf8(a.value.into_owned()).unwrap())
}

/// One parsed table cell: its attributes and text.
#[derive(Debug, Default, Clone)]
pub struct RawCell {
    pub attrs: BTreeMap<String, String>,
    pub text: String,
}

/// Parse table rows: `row_tag` elements containing `cell_tag` elements, with the
/// text of every `text_tag` inside a cell concatenated.
pub fn parse_rows(xml: &str, row_tag: &str, cell_tag: &str, text_tag: &str) -> Vec<Vec<RawCell>> {
    let mut reader = Reader::from_str(xml);
    let mut rows: Vec<Vec<RawCell>> = Vec::new();
    let mut cell: Option<RawCell> = None;
    let mut in_text = false;
    let start = |e: &BytesStart, rows: &mut Vec<Vec<RawCell>>, cell: &mut Option<RawCell>| {
        let name = e.local_name();
        let name = std::str::from_utf8(name.as_ref()).unwrap();
        if name == row_tag {
            rows.push(Vec::new());
        } else if name == cell_tag {
            let attrs = e
                .attributes()
                .flatten()
                .map(|a| {
                    (
                        String::from_utf8(a.key.local_name().as_ref().to_vec()).unwrap(),
                        String::from_utf8(a.value.into_owned()).unwrap(),
                    )
                })
                .collect();
            *cell = Some(RawCell {
                attrs,
                text: String::new(),
            });
        } else if let Some(c) = cell.as_mut() {
            // Child property elements (DOCX keeps gridSpan / vMerge in <w:tcPr>).
            if name == "gridSpan" || name == "vMerge" {
                c.attrs
                    .insert(name.to_string(), attr(e, "val").unwrap_or_default());
            }
        }
    };
    loop {
        match reader.read_event().expect("well-formed XML") {
            Event::Start(e) => {
                start(&e, &mut rows, &mut cell);
                in_text = e.local_name().as_ref() == text_tag.as_bytes();
            }
            Event::Empty(e) => {
                let name = e.local_name();
                start(&e, &mut rows, &mut cell);
                if name.as_ref() == cell_tag.as_bytes() {
                    rows.last_mut().unwrap().push(cell.take().unwrap());
                }
            }
            Event::Text(t) if in_text => {
                if let Some(c) = cell.as_mut() {
                    c.text.push_str(&t.unescape().unwrap());
                }
            }
            Event::End(e) => {
                in_text = false;
                if e.local_name().as_ref() == cell_tag.as_bytes() {
                    rows.last_mut().unwrap().push(cell.take().unwrap());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    rows
}

pub fn span(c: &RawCell, key: &str) -> usize {
    c.attrs.get(key).and_then(|v| v.parse().ok()).unwrap_or(1)
}

pub fn docx_grid(xml: &str) -> Grid {
    let mut grid: Grid = Vec::new();
    for raw in parse_rows(xml, "tr", "tc", "t") {
        let mut out = Vec::new();
        for c in raw {
            let n = span(&c, "gridSpan");
            // `<w:vMerge/>` (no val, or "continue") continues the cell above.
            let text = match c.attrs.get("vMerge").map(String::as_str) {
                Some("") | Some("continue") => {
                    grid.last().expect("vMerge in first row")[out.len()].clone()
                }
                _ => c.text.clone(),
            };
            out.extend(std::iter::repeat_n(text, n));
        }
        grid.push(out);
    }
    grid
}

/// The PPTX grid, checking DrawingML's merge rules on the way: an origin's
/// `gridSpan` / `rowSpan` are followed by exactly that many `hMerge` / `vMerge`
/// cells, and a `vMerge` cell is `hMerge` exactly when the cell above it is.
pub fn pptx_grid(xml: &str) -> Grid {
    let raw = parse_rows(xml, "tr", "tc", "t");
    let flag = |c: &RawCell, k: &str| c.attrs.contains_key(k);
    for (r, row) in raw.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if flag(cell, "hMerge") || flag(cell, "vMerge") {
                if flag(cell, "vMerge") {
                    let above = &raw[r - 1][c];
                    assert_eq!(
                        flag(cell, "hMerge"),
                        flag(above, "hMerge"),
                        "hMerge mismatch at {r},{c}"
                    );
                }
                continue;
            }
            let (cs, rs) = (span(cell, "gridSpan"), span(cell, "rowSpan"));
            for (rr, covered_row) in raw.iter().enumerate().skip(r).take(rs) {
                for (cc, k) in covered_row.iter().enumerate().skip(c).take(cs) {
                    if (rr, cc) == (r, c) {
                        continue;
                    }
                    assert_eq!(
                        flag(k, "hMerge"),
                        cc > c,
                        "hMerge at {rr},{cc} under origin {r},{c}"
                    );
                    assert_eq!(
                        flag(k, "vMerge"),
                        rr > r,
                        "vMerge at {rr},{cc} under origin {r},{c}"
                    );
                }
            }
            if let Some(next) = row.get(c + cs) {
                assert!(!flag(next, "hMerge"), "stray hMerge at {r},{}", c + cs);
            }
            if let Some(below) = raw.get(r + rs).and_then(|row| row.get(c)) {
                assert!(!flag(below, "vMerge"), "stray vMerge at {},{c}", r + rs);
            }
        }
    }
    let mut grid: Grid = Vec::new();
    for row in raw {
        let mut out: Vec<String> = Vec::new();
        for c in row {
            let text = if flag(&c, "vMerge") {
                grid.last().expect("vMerge in first row")[out.len()].clone()
            } else if flag(&c, "hMerge") {
                out.last().expect("hMerge in first column").clone()
            } else {
                c.text.clone()
            };
            out.push(text);
        }
        grid.push(out);
    }
    grid
}
