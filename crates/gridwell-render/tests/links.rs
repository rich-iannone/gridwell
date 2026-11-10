//! Links (a text run's `href`) in every format: each link of the `links` example
//! reaches every format's output, written as `link_url` writes it; an `href`
//! that isn't a link (in IR that wasn't validated) reaches none; a link's URL
//! can't break out of where a format puts it; and the OOXML formats write each
//! URL once, as it is, in their relationships.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use gridwell_core::link::link_url;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{Table, ValidationRule};
use gridwell_render::{render, OutputKind, REGISTRY};
use gridwell_testkit::{cell, examples, labeled_group, raw, row, RowExt, TableBuilder};
use quick_xml::events::Event;

/// In every `href` that isn't a link, so that none may reach an output.
const SECRET: &str = "GW_SECRET_4b1";

/// A format's output as text: an OOXML package's parts, each read whole.
fn text_of(format: &str, bytes: Vec<u8>) -> String {
    parts(format, bytes).into_values().collect()
}

/// A format's output by part: a text format's is one part, `""`.
fn parts(format: &str, bytes: Vec<u8>) -> BTreeMap<String, String> {
    if REGISTRY
        .iter()
        .any(|w| w.name() == format && w.kind() == OutputKind::Text)
    {
        return BTreeMap::from([(String::new(), String::from_utf8(bytes).unwrap())]);
    }
    let mut zip = gridwell_ooxml::zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut part = Vec::new();
        file.read_to_end(&mut part).unwrap();
        out.insert(
            file.name().to_string(),
            String::from_utf8_lossy(&part).into_owned(),
        );
    }
    out
}

/// A table rendered by the format's own entry point, which doesn't validate it.
fn unvalidated(format: &str, t: &Table) -> Vec<u8> {
    fn ok<T: Into<Vec<u8>>, E: std::fmt::Display>(format: &str, r: Result<T, E>) -> Vec<u8> {
        r.map(Into::into)
            .unwrap_or_else(|e| panic!("{format}: {e}"))
    }
    match format {
        "html" => ok(format, gridwell_writer_html::render_html(t)),
        "latex" => ok(format, gridwell_writer_latex::render_latex(t)),
        "typst" => ok(format, gridwell_writer_typst::render_typst(t)),
        "rtf" => ok(format, gridwell_writer_rtf::render_rtf(t)),
        "svg" => ok(format, gridwell_writer_svg::render_svg(t)),
        "ansi" => ok(format, gridwell_writer_ansi::render_ansi(t)),
        "pandoc" => ok(format, gridwell_writer_pandoc::render_pandoc(t)),
        "quarto" => ok(format, gridwell_writer_quarto::render_quarto(t)),
        "docx" => ok(format, gridwell_writer_docx::render_docx(t)),
        "xlsx" => ok(format, gridwell_writer_xlsx::render_xlsx(t)),
        "pptx" => ok(format, gridwell_writer_pptx::render_pptx(t)),
        other => panic!("no entry point for {other}"),
    }
}

/// A URL as `format` writes it: XML formats escape `&`, LaTeX `%`, `#` and `&`.
fn written(format: &str, url: &str) -> String {
    match format {
        "html" | "svg" | "docx" | "xlsx" | "pptx" => url.replace('&', "&amp;"),
        "latex" => url
            .replace('%', "\\%")
            .replace('#', "\\#")
            .replace('&', "\\&"),
        _ => url.to_string(),
    }
}

/// The `href`s of the `links` example, as it holds them.
fn example_hrefs() -> Vec<String> {
    let t = examples()
        .into_iter()
        .find(|e| e.name == "links")
        .unwrap()
        .table();
    let mut out = Vec::new();
    let v = serde_json::to_value(&t).unwrap();
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(h) = m.get("href").and_then(|h| h.as_str()) {
                    out.push(h.to_string());
                }
                m.values().for_each(|c| walk(c, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    walk(&v, &mut out);
    out
}

#[test]
fn every_format_writes_every_link_of_the_links_example() {
    let t = examples()
        .into_iter()
        .find(|e| e.name == "links")
        .unwrap()
        .table();
    let hrefs = example_hrefs();
    assert_eq!(hrefs.len(), 4, "{hrefs:?}");
    for w in REGISTRY {
        let out = text_of(w.name(), render(&t, w.name(), None).unwrap().into_bytes());
        for href in &hrefs {
            let url = link_url(href).unwrap();
            // XLSX keeps the part after a `#` apart (its `location`).
            let url = match w.name() {
                "xlsx" => url.split('#').next().unwrap().to_string(),
                _ => url,
            };
            let url = written(w.name(), &url);
            assert!(out.contains(&url), "{}: no {url}", w.name());
        }
    }
}

/// `href`s that aren't links, each holding [`SECRET`].
fn not_links() -> Vec<String> {
    [
        "javascript:{s}()",
        "JaVaScRiPt:{s}()",
        "data:text/html,<script>{s}</script>",
        "file:///etc/{s}",
        "vbscript:{s}",
        "/{s}",
        "{s}.example.com",
        "https:{s}",
        "https:///{s}",
        " https://{s}.org",
        "java\tscript:{s}",
        "https://{s}.org/\u{7}",
    ]
    .iter()
    .map(|h| h.replace("{s}", SECRET))
    .collect()
}

/// A table with `run` in every place content is (header lines, head cells, a
/// group label, body and summary cells, a footnote and a source note), and how
/// many content lists it is in.
fn everywhere(run: ContentNode) -> (Table, usize) {
    let mut t = TableBuilder::new(2)
        .title("t")
        .subtitle("s")
        .extra_line("e")
        .stub_cols(1)
        .head(row(vec![cell("a"), cell("b")]).role("column_label"))
        .group(
            labeled_group("g", vec![row(vec![cell("x"), cell("1")])]).summary(vec![row(vec![
                cell("Total"),
                cell("1"),
            ])
            .role("summary")]),
        )
        .footnote("f1", "1", "a note")
        .source_note("source")
        .build();
    let mut v = serde_json::to_value(&t).unwrap();
    fn add(v: &mut serde_json::Value, run: &serde_json::Value, n: &mut usize) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, c) in m.iter_mut() {
                    match c {
                        serde_json::Value::Array(nodes) if k == "content" => {
                            nodes.push(run.clone());
                            *n += 1;
                        }
                        _ => add(c, run, n),
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|c| add(c, run, n)),
            _ => {}
        }
    }
    let mut n = 0;
    add(&mut v, &serde_json::to_value(run).unwrap(), &mut n);
    t = serde_json::from_value(v).unwrap();
    (t, n)
}

#[test]
fn an_href_that_isnt_a_link_reaches_no_output() {
    for href in not_links() {
        let runs = [
            ContentNode::Text {
                value: "shown".into(),
                href: Some(href.clone()),
            },
            ContentNode::StyledText {
                value: "shown".into(),
                style_id: None,
                href: Some(href.clone()),
            },
        ];
        for run in runs {
            let (t, lists) = everywhere(run);
            assert_eq!(lists, 12);
            let errors = t.validate();
            // One per run, and nothing else.
            assert_eq!(errors.len(), lists, "{href:?}: {errors:?}");
            assert!(
                errors.iter().all(|e| e.rule == ValidationRule::InvalidHref),
                "{href:?}: {errors:?}"
            );
            for w in REGISTRY {
                assert!(render(&t, w.name(), None).is_err(), "{}", w.name());
                let out = text_of(w.name(), unvalidated(w.name(), &t));
                assert!(!out.contains(SECRET), "{} wrote {href:?}", w.name());
                assert!(
                    out.contains("shown"),
                    "{}: the run's text is gone",
                    w.name()
                );
            }
        }
    }
}

#[test]
fn raw_html_links_that_arent_links_reach_no_output() {
    let html = format!(r#"<a href="javascript:{SECRET}()">shown</a>"#);
    let (t, _) = everywhere(raw("html", &html));
    assert!(t.validate().is_empty());
    for w in REGISTRY {
        // HTML writes raw HTML as it is (the trust boundary) unless `raw` is off.
        let options = serde_json::json!({ "raw": false });
        let out = text_of(
            w.name(),
            render(&t, w.name(), Some(&options)).unwrap().into_bytes(),
        );
        assert!(!out.contains(SECRET), "{} with raw off", w.name());
        assert!(out.contains("shown"), "{}", w.name());
        if !["html", "pandoc", "quarto"].contains(&w.name()) {
            let out = text_of(w.name(), render(&t, w.name(), None).unwrap().into_bytes());
            assert!(!out.contains(SECRET), "{}", w.name());
        }
    }
}

#[test]
fn a_links_url_cant_break_out_of_where_a_format_puts_it() {
    // A link (an https URL with a host), but with the characters each format's
    // syntax ends a URL or a string with.
    let href = format!(
        r#"https://x.org/"><script>{SECRET}</script>'")]}}{{\par\href{{a}}{{b}}#link("c")[d]"#
    );
    let url = link_url(&href).unwrap();
    for c in ['"', '<', '>', '\\', '{', '}'] {
        assert!(!url.contains(c), "{url}");
    }
    let (t, _) = everywhere(ContentNode::Text {
        value: "shown".into(),
        href: Some(href.clone()),
    });
    assert!(t.validate().is_empty(), "{:?}", t.validate());
    for w in REGISTRY {
        let out = text_of(w.name(), render(&t, w.name(), None).unwrap().into_bytes())
            // An XML format may write `'` as a character reference.
            .replace("&apos;", "'")
            .replace("&#39;", "'");
        assert!(!out.contains("<script>"), "{}", w.name());
        assert!(!out.contains("\\par\\href"), "{}", w.name());
        assert!(!out.contains("\")[#"), "{}", w.name());
        let url = match w.name() {
            "xlsx" => url.split('#').next().unwrap().to_string(),
            _ => url.clone(),
        };
        assert!(
            out.contains(&written(w.name(), &url)),
            "{}: no {url}",
            w.name()
        );
    }
}

/// The `Target` of every external relationship in a package, by its part.
fn external_targets(parts: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, xml) in parts.iter().filter(|(n, _)| n.ends_with(".rels")) {
        let mut reader = quick_xml::Reader::from_str(xml);
        loop {
            match reader.read_event().unwrap() {
                Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"Relationship" => {
                    let attr = |k: &str| {
                        e.attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == k.as_bytes())
                            .map(|a| a.unescape_value().unwrap().into_owned())
                    };
                    if attr("TargetMode").as_deref() == Some("External") {
                        out.push((name.clone(), attr("Target").unwrap()));
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    out
}

#[test]
fn ooxml_formats_write_each_url_as_it_is_in_their_relationships() {
    let t = examples()
        .into_iter()
        .find(|e| e.name == "links")
        .unwrap()
        .table();
    let mut urls: Vec<String> = example_hrefs()
        .iter()
        .map(|h| link_url(h).unwrap())
        .collect();
    urls.sort();
    for format in ["docx", "pptx", "xlsx"] {
        let parts = parts(format, render(&t, format, None).unwrap().into_bytes());
        let mut targets: Vec<String> = external_targets(&parts)
            .into_iter()
            .map(|(_, t)| t)
            .collect();
        targets.sort();
        let want: Vec<String> = urls
            .iter()
            .map(|u| match format {
                // XLSX's relationship holds the URL before its `#`; the rest is
                // the hyperlink's `location`.
                "xlsx" => u.split('#').next().unwrap().to_string(),
                _ => u.clone(),
            })
            .collect();
        assert_eq!(targets, want, "{format}");
        if format == "xlsx" {
            let sheet = &parts["xl/worksheets/sheet1.xml"];
            assert!(sheet.contains(r#"location="top""#), "{sheet}");
            // Nothing escaped twice.
            assert!(!sheet.contains("%2525"), "{sheet}");
        }
        // Escaped once: `%25` stays one `%`, `%C3` stays as it is.
        for (_, target) in external_targets(&parts) {
            assert!(
                !target.contains("%2525") && !target.contains("%25C3"),
                "{format}: {target}"
            );
        }
    }
}

#[test]
fn ooxml_formats_write_a_url_without_the_escapes_their_libraries_look_for_as_it_is() {
    // rust_xlsxwriter escapes a URL's `%` again, and its `[` and `]`, unless the
    // URL has one of a few escapes (`%25`, `%20`, …): these have none.
    for href in [
        "https://example.org/caf%C3%A9",
        "https://example.org/a[1]?b=%41",
    ] {
        let (t, _) = everywhere(ContentNode::Text {
            value: "shown".into(),
            href: Some(href.into()),
        });
        for format in ["docx", "pptx", "xlsx"] {
            let parts = parts(format, render(&t, format, None).unwrap().into_bytes());
            let targets = external_targets(&parts);
            assert!(!targets.is_empty(), "{format}");
            for (part, target) in targets {
                assert_eq!(target, href, "{format} {part}");
            }
        }
    }
}
