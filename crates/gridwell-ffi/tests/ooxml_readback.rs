//! Read-back oracle for the OOXML writers: parse the DOCX, PPTX and XLSX output,
//! rebuild the logical grid from the format's own merge markup (`gridSpan` /
//! `vMerge` in DOCX; `gridSpan` / `rowSpan` / `hMerge` / `vMerge` in PPTX;
//! `<mergeCell>` in XLSX), and compare it position by position with the grid
//! computed directly from the IR: every visible position must show the text of
//! the cell covering it.
//!
//! Scenarios: every tiling of small grids placed in the head, a labelled body
//! group, and (one-row tilings) a summary row, under every set of hidden columns
//! that leaves at least one visible.

use std::collections::BTreeMap;

use gridwell_ir::content::ContentNode;
use gridwell_ir::{Row, Table};
use gridwell_testkit::spans::tilings;
use gridwell_testkit::{cell, column, group, labeled_group, row, ColumnExt, TableBuilder};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// A logical grid: one text per visible position, row by row.
type Grid = Vec<Vec<String>>;

fn text_of(content: &[ContentNode]) -> String {
    content
        .iter()
        .map(|n| match n {
            ContentNode::Text { value } => value.as_str(),
            _ => "",
        })
        .collect()
}

/// The expected grid, from the IR alone (no layout crate): each origin covers its
/// rectangle; hidden columns are then dropped.
fn expected(table: &Table) -> Grid {
    let hidden: Vec<bool> = table.column_spec.iter().map(|c| c.hidden).collect();
    let visible = hidden.iter().filter(|h| !**h).count();
    let section = |rows: &[Row]| -> Grid {
        let width = hidden.len();
        let mut grid = vec![vec![String::new(); width]; rows.len()];
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.is_placeholder {
                    continue;
                }
                let text = text_of(&cell.content);
                for grid_row in grid.iter_mut().skip(r).take(cell.rowspan as usize) {
                    for slot in grid_row.iter_mut().skip(c).take(cell.colspan as usize) {
                        slot.clone_from(&text);
                    }
                }
            }
        }
        grid.into_iter()
            .map(|r| {
                r.into_iter()
                    .zip(&hidden)
                    .filter(|(_, h)| !**h)
                    .map(|(t, _)| t)
                    .collect()
            })
            .collect()
    };
    let mut out = Vec::new();
    if !table.config.column_labels_hidden {
        out.extend(section(&table.table.thead.rows));
    }
    for g in &table.table.tbody {
        if let Some(label) = &g.label {
            out.push(vec![text_of(&label.content); visible]);
        }
        out.extend(section(&g.rows));
        out.extend(section(&g.summary_rows));
    }
    out
}

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name.as_bytes())
        .map(|a| String::from_utf8(a.value.into_owned()).unwrap())
}

/// One parsed table cell: its attributes and text.
#[derive(Debug, Default, Clone)]
struct RawCell {
    attrs: BTreeMap<String, String>,
    text: String,
}

/// Parse table rows: `row_tag` elements containing `cell_tag` elements, with the
/// text of every `text_tag` inside a cell concatenated.
fn parse_rows(xml: &str, row_tag: &str, cell_tag: &str, text_tag: &str) -> Vec<Vec<RawCell>> {
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

fn span(c: &RawCell, key: &str) -> usize {
    c.attrs.get(key).and_then(|v| v.parse().ok()).unwrap_or(1)
}

fn docx_grid(xml: &str) -> Grid {
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
fn pptx_grid(xml: &str) -> Grid {
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

/// "B3" → (1, 3).
fn cell_ref(r: &str) -> (usize, usize) {
    let letters: String = r.chars().take_while(|c| c.is_ascii_uppercase()).collect();
    let col = letters
        .bytes()
        .fold(0usize, |n, b| n * 26 + (b - b'A' + 1) as usize)
        - 1;
    (col, r[letters.len()..].parse().unwrap())
}

fn xlsx_grid(xml: &str, width: usize) -> Grid {
    let mut cells: BTreeMap<(usize, usize), String> = BTreeMap::new();
    let mut max_row = 0;
    for raw in parse_rows(xml, "row", "c", "t") {
        for c in raw {
            let (col, r) = cell_ref(&c.attrs["r"]);
            max_row = max_row.max(r);
            cells.insert((r, col), c.text.clone());
        }
    }
    // Merged ranges show the top-left cell's value everywhere.
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().unwrap() {
            Event::Empty(e) if e.local_name().as_ref() == b"mergeCell" => {
                let range = attr(&e, "ref").unwrap();
                let (a, b) = range.split_once(':').unwrap();
                let ((c0, r0), (c1, r1)) = (cell_ref(a), cell_ref(b));
                let v = cells.get(&(r0, c0)).cloned().unwrap_or_default();
                for r in r0..=r1 {
                    for c in c0..=c1 {
                        assert!(
                            (r, c) == (r0, c0) || !cells.contains_key(&(r, c)),
                            "value inside merge {range} at {r},{c}"
                        );
                        cells.insert((r, c), v.clone());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    (1..=max_row)
        .map(|r| {
            (0..width)
                .map(|c| cells.get(&(r, c)).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}

fn scenarios() -> Vec<(String, Table)> {
    let plain = |p: &str, n: usize| row((0..n).map(|i| cell(&format!("{p}{i}"))).collect());
    let mut out = Vec::new();
    for (rows, cols) in [(1, 3), (2, 3), (3, 3), (2, 4), (1, 4)] {
        for t in tilings(rows, cols) {
            for mask in 0u32..(1 << cols) - 1 {
                let columns: Vec<_> = (0..cols)
                    .map(|i| {
                        let c = column(&format!("c{i}"), "");
                        if mask & (1 << i) != 0 {
                            c.hidden()
                        } else {
                            c
                        }
                    })
                    .collect();
                let n = cols as u32;
                let name = format!("{} mask {mask:b}", t.ascii());
                out.push((
                    format!("body {name}"),
                    TableBuilder::new(n)
                        .columns(columns.clone())
                        .head(plain("h", cols))
                        .group(labeled_group("G", t.to_rows()))
                        .group(group(vec![plain("z", cols)]))
                        .build(),
                ));
                let mut head = TableBuilder::new(n).columns(columns.clone());
                for r in t.to_rows() {
                    head = head.head(r);
                }
                out.push((
                    format!("head {name}"),
                    head.body(vec![plain("b", cols)]).build(),
                ));
                if rows == 1 {
                    out.push((
                        format!("summary {name}"),
                        TableBuilder::new(n)
                            .columns(columns)
                            .stub_cols(1)
                            .head(plain("h", cols))
                            .group(group(vec![plain("b", cols)]).summary(t.to_rows()))
                            .build(),
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn ooxml_grids_read_back_as_the_ir_grid() {
    let mut n = 0;
    for (name, table) in scenarios() {
        assert!(table.validate().is_empty(), "{name}");
        let want = expected(&table);
        let width = want[0].len();

        let docx = gridwell_writer_docx::DocxWriter::new()
            .render_document_xml(&table)
            .unwrap();
        assert_eq!(docx_grid(&docx), want, "docx {name}\n{docx}");

        let pptx = gridwell_writer_pptx::PptxWriter::new()
            .render_slide_xml(&table)
            .unwrap();
        assert_eq!(pptx_grid(&pptx), want, "pptx {name}\n{pptx}");

        let xlsx = gridwell_writer_xlsx::XlsxWriter::new()
            .render_sheet_xml(&table)
            .unwrap();
        assert_eq!(xlsx_grid(&xlsx, width), want, "xlsx {name}\n{xlsx}");
        n += 1;
    }
    assert!(n > 9_000, "only {n} scenarios");
}

#[test]
fn xlsx_styles_part_is_well_formed_and_indexed() {
    // Every `s` index used by the sheet exists in cellXfs, for the whole corpus.
    for ex in gridwell_testkit::examples() {
        let t = ex.table();
        let w = gridwell_writer_xlsx::XlsxWriter::new();
        let sheet = w.render_sheet_xml(&t).unwrap();
        let styles = w.render_styles_xml(&t).unwrap();
        let mut reader = Reader::from_str(&styles);
        let mut xfs = None;
        let mut in_cell_xfs = false;
        loop {
            match reader.read_event().expect("well-formed styles.xml") {
                Event::Start(e) if e.local_name().as_ref() == b"cellXfs" => {
                    in_cell_xfs = true;
                    xfs = Some((attr(&e, "count").unwrap().parse::<usize>().unwrap(), 0));
                }
                Event::Start(e) | Event::Empty(e)
                    if in_cell_xfs && e.local_name().as_ref() == b"xf" =>
                {
                    xfs.as_mut().unwrap().1 += 1;
                }
                Event::End(e) if e.local_name().as_ref() == b"cellXfs" => in_cell_xfs = false,
                Event::Eof => break,
                _ => {}
            }
        }
        let (count, actual) = xfs.expect("cellXfs");
        assert_eq!(count, actual, "{}", ex.name);
        for raw in parse_rows(&sheet, "row", "c", "t") {
            for c in raw {
                let s = c.attrs.get("s").map_or(0, |s| s.parse().unwrap());
                assert!(s < count, "{}: s={s} of {count}", ex.name);
            }
        }
    }
}
