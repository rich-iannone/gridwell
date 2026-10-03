//! OOXML structure checks beyond the grid (the grid itself is checked for every
//! format in `grid_invariant.rs`): every cell-format index in XLSX resolves.

use quick_xml::events::Event;
use quick_xml::Reader;

mod support;
use support::ooxml::{attr, parse_rows};

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
        // Every xf's font/fill/border index exists.
        for (table, attr_name) in [
            ("fonts", "fontId"),
            ("fills", "fillId"),
            ("borders", "borderId"),
        ] {
            let n: usize = styles
                .split(&format!("<{table} count=\""))
                .nth(1)
                .and_then(|r| r.split('"').next())
                .and_then(|c| c.parse().ok())
                .unwrap_or_else(|| panic!("{}: no {table} count", ex.name));
            for (i, _) in styles.match_indices(&format!("{attr_name}=\"")) {
                let v: usize = styles[i + attr_name.len() + 2..]
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(v < n, "{}: {attr_name}={v} of {n}", ex.name);
            }
        }
        for raw in parse_rows(&sheet, "row", "c", "t") {
            for c in raw {
                let s = c.attrs.get("s").map_or(0, |s| s.parse().unwrap());
                assert!(s < count, "{}: s={s} of {count}", ex.name);
            }
        }
    }
}

#[test]
fn every_part_of_every_package_is_well_formed_xml() {
    use gridwell_ooxml::zip::ZipArchive;
    use std::io::{Cursor, Read};
    for ex in gridwell_testkit::examples() {
        let t = ex.table();
        for format in ["docx", "xlsx", "pptx"] {
            let bytes = gridwell_render::render(&t, format, None)
                .unwrap()
                .into_bytes();
            let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
            assert_eq!(zip.by_index(0).unwrap().name(), "[Content_Types].xml");
            for i in 0..zip.len() {
                let mut part = zip.by_index(i).unwrap();
                let name = part.name().to_string();
                let mut xml = String::new();
                part.read_to_string(&mut xml).unwrap();
                // One root element, every tag closed, nothing after the root.
                let mut reader = Reader::from_str(&xml);
                reader.config_mut().check_end_names = true;
                let (mut depth, mut roots) = (0i32, 0);
                loop {
                    match reader.read_event() {
                        Ok(Event::Start(_)) => {
                            if depth == 0 {
                                roots += 1;
                            }
                            depth += 1;
                        }
                        Ok(Event::Empty(_)) if depth == 0 => roots += 1,
                        Ok(Event::End(_)) => depth -= 1,
                        Ok(Event::Eof) => break,
                        Ok(_) => {}
                        Err(e) => panic!("{} {format} {name}: {e}", ex.name),
                    }
                }
                assert_eq!((depth, roots), (0, 1), "{} {format} {name}", ex.name);
            }
        }
    }
}
