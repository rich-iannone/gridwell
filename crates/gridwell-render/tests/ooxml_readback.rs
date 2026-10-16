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

/// Every part of `bytes` (a zip), by name.
fn package_parts(bytes: Vec<u8>) -> std::collections::BTreeMap<String, String> {
    use gridwell_ooxml::zip::ZipArchive;
    use std::io::{Cursor, Read};
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut part = zip.by_index(i).unwrap();
            let mut xml = String::new();
            part.read_to_string(&mut xml).unwrap();
            (part.name().to_string(), xml)
        })
        .collect()
}

/// Each `<tag …>` start tag in `xml`, as its attributes.
fn tags(xml: &str, tag: &str) -> Vec<std::collections::HashMap<String, String>> {
    let mut out = Vec::new();
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().unwrap() {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == tag.as_bytes() => out
                .push(
                    e.attributes()
                        .map(|a| {
                            let a = a.unwrap();
                            (
                                String::from_utf8(a.key.local_name().as_ref().to_vec()).unwrap(),
                                a.unescape_value().unwrap().into_owned(),
                            )
                        })
                        .collect(),
                ),
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

/// `target` relative to the folder of `source` (`a/b/c.xml`), as a part name.
fn resolve_target(source: &str, target: &str) -> String {
    let mut path: Vec<&str> = match target.strip_prefix('/') {
        Some(_) => vec![],
        None => source.split('/').collect::<Vec<_>>()[..source.matches('/').count()].to_vec(),
    };
    for seg in target.trim_start_matches('/').split('/') {
        match seg {
            ".." => {
                path.pop();
            }
            "." => {}
            s => path.push(s),
        }
    }
    path.join("/")
}

#[test]
fn every_package_is_typed_and_linked() {
    // Open Packaging Conventions: every part has its own content type, every
    // relationship points at a part that exists, and every part is reachable
    // from the package's root relationships.
    for ex in gridwell_testkit::examples() {
        let t = ex.table();
        for format in ["docx", "xlsx", "pptx"] {
            let what = format!("{} {format}", ex.name);
            let parts = package_parts(
                gridwell_render::render(&t, format, None)
                    .unwrap()
                    .into_bytes(),
            );
            let types = &parts["[Content_Types].xml"];
            let overrides: std::collections::HashMap<String, String> = tags(types, "Override")
                .into_iter()
                .map(|a| (a["PartName"].clone(), a["ContentType"].clone()))
                .collect();
            for name in parts.keys() {
                if name == "[Content_Types].xml" || name.ends_with(".rels") {
                    continue;
                }
                let ct = overrides
                    .get(&format!("/{name}"))
                    .unwrap_or_else(|| panic!("{what}: {name} has no content type"));
                assert!(ct.starts_with("application/vnd."), "{what}: {name}: {ct}");
            }
            for part in overrides.keys() {
                assert!(
                    parts.contains_key(&part[1..]),
                    "{what}: content type for missing {part}"
                );
            }
            // Follow relationships from the root.
            let mut seen = std::collections::BTreeSet::new();
            let mut todo = vec![String::new()];
            while let Some(source) = todo.pop() {
                let rels = match source.rsplit_once('/') {
                    Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
                    None if source.is_empty() => "_rels/.rels".into(),
                    None => format!("_rels/{source}.rels"),
                };
                let Some(xml) = parts.get(&rels) else {
                    continue;
                };
                for rel in tags(xml, "Relationship") {
                    if rel.get("TargetMode").map(String::as_str) == Some("External") {
                        continue;
                    }
                    let target = resolve_target(&source, &rel["Target"]);
                    assert!(
                        parts.contains_key(&target),
                        "{what}: {rels} points at missing {target}"
                    );
                    if seen.insert(target.clone()) {
                        todo.push(target);
                    }
                }
            }
            for name in parts.keys() {
                if name == "[Content_Types].xml" || name.ends_with(".rels") {
                    continue;
                }
                assert!(seen.contains(name), "{what}: {name} is not linked");
            }
        }
    }
}

#[test]
fn pptx_package_has_the_parts_presentationml_requires() {
    // Keynote and PowerPoint refuse a package without a theme, a master color
    // map, or the presentation's notes size (all required by the schema).
    let t = gridwell_testkit::examples()[0].table();
    let parts = package_parts(gridwell_writer_pptx::render_pptx(&t).unwrap());
    let child_order = |xml: &str, root: &str| -> Vec<String> {
        let mut reader = Reader::from_str(xml);
        let (mut depth, mut out) = (0, Vec::new());
        loop {
            match reader.read_event().unwrap() {
                Event::Start(e) => {
                    if depth == 1 {
                        out.push(String::from_utf8(e.local_name().as_ref().to_vec()).unwrap());
                    }
                    depth += 1;
                }
                Event::Empty(e) if depth == 1 => {
                    out.push(String::from_utf8(e.local_name().as_ref().to_vec()).unwrap())
                }
                Event::End(_) => depth -= 1,
                Event::Eof => break,
                _ => {}
            }
        }
        assert!(xml.contains(&format!("<p:{root} ")), "{xml}");
        out
    };

    // CT_Presentation: … sldMasterIdLst, …, sldIdLst, sldSz, notesSz (required).
    let pres = child_order(&parts["ppt/presentation.xml"], "presentation");
    let at = |name: &str| pres.iter().position(|c| c == name);
    assert!(
        at("sldSz").is_some() && at("notesSz") == at("sldSz").map(|i| i + 1),
        "{pres:?}"
    );
    let notes = &tags(&parts["ppt/presentation.xml"], "notesSz")[0];
    assert!(notes["cx"].parse::<u64>().unwrap() > 0 && notes["cy"].parse::<u64>().unwrap() > 0);

    // CT_SlideMaster: cSld, clrMap (required), …, sldLayoutIdLst. The master
    // paints a background (Keynote shows black without one).
    assert!(
        parts["ppt/slideMasters/slideMaster1.xml"].contains("<p:cSld><p:bg>"),
        "master has no background"
    );
    let master = child_order(&parts["ppt/slideMasters/slideMaster1.xml"], "sldMaster");
    assert_eq!(master[..2], ["cSld", "clrMap"], "{master:?}");
    let clr_map = &tags(&parts["ppt/slideMasters/slideMaster1.xml"], "clrMap")[0];
    for key in [
        "bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ] {
        assert!(clr_map.contains_key(key), "clrMap lacks {key}");
    }

    // The master's theme.
    let rels = tags(
        &parts["ppt/slideMasters/_rels/slideMaster1.xml.rels"],
        "Relationship",
    );
    let theme_rel = rels
        .iter()
        .find(|r| r["Type"].ends_with("/relationships/theme"))
        .expect("master has a theme relationship");
    let theme_name = resolve_target("ppt/slideMasters/slideMaster1.xml", &theme_rel["Target"]);
    let theme = &parts[&theme_name];
    let types = tags(&parts["[Content_Types].xml"], "Override");
    assert!(
        types
            .iter()
            .any(|o| o["PartName"] == format!("/{theme_name}")
                && o["ContentType"] == "application/vnd.openxmlformats-officedocument.theme+xml"),
        "{types:?}"
    );
    // CT_BaseStyles: a color scheme with all twelve colors, a font scheme, and a
    // format scheme whose four lists each hold at least three styles.
    for color in [
        "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ] {
        assert_eq!(tags(theme, color).len(), 1, "theme lacks {color}");
    }
    for font in ["majorFont", "minorFont"] {
        assert_eq!(tags(theme, font).len(), 1, "theme lacks {font}");
    }
    let count_in = |list: &str| {
        let start = theme
            .find(&format!("<a:{list}>"))
            .unwrap_or_else(|| panic!("no {list}"));
        let end = theme.find(&format!("</a:{list}>")).unwrap();
        let inner = &theme[start + list.len() + 4..end];
        child_order(&format!("<p:x >{inner}</p:x>"), "x").len()
    };
    for list in [
        "fillStyleLst",
        "lnStyleLst",
        "effectStyleLst",
        "bgFillStyleLst",
    ] {
        assert!(count_in(list) >= 3, "{list} has {} styles", count_in(list));
    }
}
