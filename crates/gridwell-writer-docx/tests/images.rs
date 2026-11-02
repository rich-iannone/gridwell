//! Images: one embedded as a PNG, JPEG or GIF `data:` URI is an inline drawing
//! at its size (from the IR, or its own), its alt text the drawing's
//! description, its bytes a media part (each image once) related to the
//! document. Word needs a bitmap beside an SVG, which the writer can't make:
//! an SVG, and an image linked by URL or path, shows its alt text, and is
//! warned of.

use std::collections::BTreeMap;
use std::io::Read;

use gridwell_core::Image;
use gridwell_ir::content::ContentNode;
use gridwell_ir::Table;
use gridwell_testkit::images::{FLAG_SVG, LOGO_GIF, LOGO_PNG, REMOTE_PNG, WIDE_JPEG};
use gridwell_testkit::{cell_content, image, row, sized_image, text, TableBuilder};
use gridwell_writer_docx::{render_docx, unsupported};

/// EMU per point.
const EMU: i64 = 12_700;

fn one_row(content: Vec<Vec<ContentNode>>) -> Table {
    TableBuilder::new(content.len() as u32)
        .body(vec![row(content.into_iter().map(cell_content).collect())])
        .build()
}

/// The value of the first attribute `name` in `xml`.
fn attr<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = xml.find(&key)? + key.len();
    Some(&xml[start..start + xml[start..].find('"')?])
}

/// A drawing: its size in points, its `docPr` id and description, and the
/// part it shows.
#[derive(Debug, PartialEq)]
struct Drawing {
    size: (f64, f64),
    id: u32,
    alt: Option<String>,
    part: String,
}

struct Docx {
    document: String,
    drawings: Vec<Drawing>,
    /// Each media part's bytes.
    media: BTreeMap<String, Vec<u8>>,
    content_types: String,
}

fn read(t: &Table) -> Docx {
    let bytes = render_docx(t).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let names: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with("word/media/"))
        .map(str::to_string)
        .collect();
    let mut part = |name: &str| {
        let mut data = Vec::new();
        zip.by_name(name).unwrap().read_to_end(&mut data).unwrap();
        data
    };
    let text = |data: Vec<u8>| String::from_utf8(data).unwrap();
    let document = text(part("word/document.xml"));
    let rels = text(part("word/_rels/document.xml.rels"));
    let content_types = text(part("[Content_Types].xml"));
    let target = |id: &str| {
        let rel = rels
            .split("<Relationship ")
            .find(|r| r.starts_with(&format!("Id=\"{id}\" ")))
            .unwrap_or_else(|| panic!("no {id} in {rels}"));
        assert!(
            attr(rel, "Type").unwrap().ends_with("/relationships/image"),
            "{rel}"
        );
        format!("word/{}", attr(rel, "Target").unwrap())
    };
    let drawings: Vec<Drawing> = document
        .split("<w:drawing>")
        .skip(1)
        .map(|d| {
            let d = &d[..d.find("</w:drawing>").unwrap()];
            let extent = &d[d.find("<wp:extent ").unwrap()..];
            let pr = &d[d.find("<wp:docPr ").unwrap()..];
            let pr = &pr[..pr.find("/>").unwrap()];
            let pt = |name| attr(extent, name).unwrap().parse::<i64>().unwrap() as f64 / EMU as f64;
            // The picture's own size and description match the drawing's.
            let xfrm = &d[d.find("<a:ext ").unwrap()..];
            assert_eq!(attr(xfrm, "cx"), attr(extent, "cx"), "{d}");
            assert_eq!(attr(xfrm, "cy"), attr(extent, "cy"), "{d}");
            let cnv = &d[d.find("<pic:cNvPr ").unwrap()..];
            assert_eq!(
                attr(&cnv[..cnv.find("/>").unwrap()], "descr"),
                attr(pr, "descr")
            );
            Drawing {
                size: (pt("cx"), pt("cy")),
                id: attr(pr, "id").unwrap().parse().unwrap(),
                alt: attr(pr, "descr").map(str::to_string),
                part: target(attr(d, "r:embed").unwrap()),
            }
        })
        .collect();
    let media = names.into_iter().map(|n| (n.clone(), part(&n))).collect();
    Docx {
        document,
        drawings,
        media,
        content_types,
    }
}

fn warned(t: &Table) -> usize {
    unsupported(t)
        .into_iter()
        .find(|u| u.feature == "image")
        .map_or(0, |u| u.places.len())
}

fn bytes(src: &str) -> Vec<u8> {
    Image::from_src(src).unwrap().data
}

fn drawing(size: (f64, f64), id: u32, alt: &str, part: &str) -> Drawing {
    Drawing {
        size,
        id,
        alt: Some(alt.into()),
        part: part.into(),
    }
}

#[test]
fn png_jpeg_and_gif_are_embedded_at_their_size() {
    let t = one_row(vec![
        vec![image(LOGO_PNG, "png")],
        vec![text("Made by "), image(WIDE_JPEG, "jpeg"), text(" us")],
        vec![image(LOGO_GIF, "gif")],
    ]);
    let docx = read(&t);
    // At 0.75pt a pixel: 18pt square; 30×15pt.
    assert_eq!(
        docx.drawings,
        [
            drawing((18.0, 18.0), 1, "png", "word/media/image1.png"),
            drawing((30.0, 15.0), 2, "jpeg", "word/media/image2.jpeg"),
            drawing((18.0, 18.0), 3, "gif", "word/media/image3.gif"),
        ]
    );
    assert_eq!(
        docx.media,
        BTreeMap::from([
            ("word/media/image1.png".into(), bytes(LOGO_PNG)),
            ("word/media/image2.jpeg".into(), bytes(WIDE_JPEG)),
            ("word/media/image3.gif".into(), bytes(LOGO_GIF)),
        ])
    );
    for (ext, mime) in [
        ("png", "image/png"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
    ] {
        assert!(
            docx.content_types
                .contains(&format!("Extension=\"{ext}\" ContentType=\"{mime}\"")),
            "{}",
            docx.content_types
        );
    }
    // Each drawing is a run of its paragraph, between its text.
    assert!(
        docx.document
            .contains("Made by </w:t></w:r><w:r><w:drawing>"),
        "{}",
        docx.document
    );
    assert_eq!(warned(&t), 0);
}

#[test]
fn a_set_size_is_used_and_em_follows_the_text() {
    let t = one_row(vec![
        vec![sized_image(LOGO_PNG, "both", Some("32px"), Some("16px"))],
        vec![sized_image(WIDE_JPEG, "width", Some("20pt"), None)],
        vec![sized_image(LOGO_PNG, "em", None, Some("2em"))],
    ]);
    let sizes: Vec<(f64, f64)> = read(&t).drawings.iter().map(|d| d.size).collect();
    assert_eq!(
        sizes,
        [
            // Stretched to both sides.
            (24.0, 12.0),
            // The other side keeps the aspect ratio.
            (20.0, 10.0),
            // Of the document's text size, 11pt.
            (22.0, 22.0),
        ]
    );
}

#[test]
fn the_same_image_is_stored_once_and_each_drawing_has_its_own_id() {
    let mut t = TableBuilder::new(2)
        .title("T")
        .body(vec![row(vec![
            cell_content(vec![image(LOGO_PNG, "one")]),
            cell_content(vec![image(LOGO_PNG, "two")]),
        ])])
        .footnote("n", "1", "note")
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().content = vec![image(LOGO_PNG, "title")];
    t.footer.as_mut().unwrap().footnotes[0].content = vec![image(LOGO_GIF, "note")];
    let docx = read(&t);
    let png = "word/media/image1.png";
    assert_eq!(
        docx.drawings,
        [
            drawing((18.0, 18.0), 1, "title", png),
            drawing((18.0, 18.0), 2, "one", png),
            drawing((18.0, 18.0), 3, "two", png),
            drawing((18.0, 18.0), 4, "note", "word/media/image2.gif"),
        ]
    );
    assert_eq!(docx.media.len(), 2);
}

#[test]
fn alt_text_is_escaped_and_none_leaves_no_description() {
    let t = one_row(vec![
        vec![image(LOGO_PNG, "a < b & \"c\"")],
        vec![ContentNode::Image {
            src: LOGO_PNG.into(),
            alt: None,
            width: None,
            height: None,
        }],
    ]);
    let alts: Vec<Option<String>> = read(&t).drawings.into_iter().map(|d| d.alt).collect();
    assert_eq!(alts, [Some("a &lt; b &amp; &quot;c&quot;".into()), None]);
}

#[test]
fn svg_and_links_show_their_alt_text_and_are_warned_of() {
    let t = one_row(vec![
        vec![image(FLAG_SVG, "svg")],
        vec![image(REMOTE_PNG, "remote")],
        vec![image("logo.png", "path")],
        vec![text("x")],
    ]);
    let docx = read(&t);
    assert!(docx.drawings.is_empty(), "{}", docx.document);
    assert!(docx.media.is_empty());
    assert!(
        !docx.content_types.contains("image/"),
        "{}",
        docx.content_types
    );
    for alt in ["svg", "remote", "path"] {
        assert!(
            docx.document.contains(&format!(">{alt}</w:t>")),
            "{alt}\n{}",
            docx.document
        );
    }
    assert_eq!(warned(&t), 3);
}
