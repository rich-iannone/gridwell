//! Images: one alone in its cell or line, embedded as a PNG, JPEG or GIF
//! `data:` URI, is drawn over the cell at its size, placed as the cell's text
//! would be; the row grows to hold it and an auto-width column widens to fit
//! it. Anything else shows its alt text, and is warned of.

#[allow(dead_code)]
mod support;

use std::io::Read;

use gridwell_ir::{Cell, StyleDef, Table};
use gridwell_layout::unsupported::Place;
use gridwell_testkit::images::{FLAG_SVG, LOGO_GIF, LOGO_PNG, REMOTE_PNG, WIDE_JPEG};
use gridwell_testkit::{
    cell, cell_content, column, image, row, sized_image, text, CellExt, ColumnExt, TableBuilder,
};
use gridwell_writer_xlsx::{render_xlsx, unsupported, XlsxWriter};
use support::{attr, find_text};

/// EMU per pixel.
const EMU: u64 = 9525;

/// A drawn image's anchor: its top-left cell (0-based) and offsets in it, its
/// size (all in px), and its alt text.
#[derive(Debug, PartialEq)]
struct Anchor {
    col: u64,
    col_off: u64,
    row: u64,
    row_off: u64,
    size: (u64, u64),
    alt: Option<String>,
}

fn value(xml: &str, tag: &str) -> u64 {
    let start = xml.find(&format!("<xdr:{tag}>")).unwrap() + tag.len() + 6;
    xml[start..start + xml[start..].find('<').unwrap()]
        .parse()
        .unwrap()
}

fn anchors(t: &Table) -> Vec<Anchor> {
    let drawing = XlsxWriter::new().render_drawing_xml(t).unwrap();
    drawing
        .split("<xdr:twoCellAnchor")
        .skip(1)
        .map(|a| {
            let from = &a[a.find("<xdr:from>").unwrap()..a.find("</xdr:from>").unwrap()];
            let ext = &a[a.find("<a:ext ").unwrap()..];
            let ext = &ext[..ext.find("/>").unwrap()];
            let emu = |name| attr(ext, name).unwrap().parse::<u64>().unwrap();
            let px = |emu: u64| {
                assert_eq!(emu % EMU, 0, "{a}");
                emu / EMU
            };
            let pr = &a[a.find("<xdr:cNvPr ").unwrap()..];
            Anchor {
                col: value(from, "col"),
                col_off: px(value(from, "colOff")),
                row: value(from, "row"),
                row_off: px(value(from, "rowOff")),
                size: (px(emu("cx")), px(emu("cy"))),
                alt: attr(&pr[..pr.find('>').unwrap()], "descr").map(str::to_string),
            }
        })
        .collect()
}

fn media(t: &Table) -> Vec<String> {
    let bytes = render_xlsx(t).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut names: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with("xl/media/"))
        .map(str::to_string)
        .collect();
    names.sort();
    for n in &names {
        let mut data = Vec::new();
        zip.by_name(n).unwrap().read_to_end(&mut data).unwrap();
        assert!(!data.is_empty(), "{n}");
    }
    names
}

/// A table whose head row is `A`, `B`, … and whose one body row is `cells`.
fn body(cells: Vec<Cell>) -> Table {
    let n = cells.len();
    TableBuilder::new(n as u32)
        .head(row((0..n)
            .map(|i| cell(&((b'A' + i as u8) as char).to_string()))
            .collect()))
        .body(vec![row(cells)])
        .build()
}

fn img(src: &str, alt: &str, w: &str, h: &str) -> Cell {
    cell_content(vec![sized_image(src, alt, Some(w), Some(h))])
}

fn row_height(sheet: &str, r: usize) -> Option<f64> {
    let start = sheet.find(&format!("<row r=\"{r}\""))?;
    let tag = &sheet[start..start + sheet[start..].find('>').unwrap()];
    attr(tag, "ht").map(|h| h.parse().unwrap())
}

/// The image places `unsupported` warns of.
fn warned(t: &Table) -> Vec<Place> {
    unsupported(t)
        .into_iter()
        .find(|u| u.feature == "image")
        .map_or(vec![], |u| u.places)
}

#[test]
fn a_lone_image_is_drawn_at_its_size_and_its_row_holds_it() {
    let t = body(vec![img(LOGO_PNG, "logo", "32px", "40px"), cell("x")]);
    let a = anchors(&t);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!((a[0].col, a[0].row), (0, 1));
    assert_eq!(a[0].size, (32, 40));
    assert_eq!(a[0].alt.as_deref(), Some("logo"));
    // 40px and 2px above and below: 44px, 33pt.
    let sheet = XlsxWriter::new().render_sheet_xml(&t).unwrap();
    assert_eq!(row_height(&sheet, 2), Some(33.0), "{sheet}");
    // The cell holds no text.
    let sst = XlsxWriter::new().render_shared_strings_xml(&t).unwrap();
    assert!(!sst.contains("logo"), "{sst}");
    assert_eq!(media(&t), ["xl/media/image1.png"]);
    assert!(warned(&t).is_empty());
}

#[test]
fn an_image_without_a_size_has_its_own_and_em_follows_the_font() {
    let t = body(vec![
        cell_content(vec![image(LOGO_PNG, "logo")]),
        cell_content(vec![sized_image(LOGO_PNG, "em", Some("2em"), None)]),
    ]);
    let sizes: Vec<(u64, u64)> = anchors(&t).iter().map(|a| a.size).collect();
    // 24px; 2em of 11pt is 22pt, about 29px, and keeps the logo square.
    assert_eq!(sizes, [(24, 24), (29, 29)]);
}

#[test]
fn png_jpeg_and_gif_are_drawn_svg_and_links_show_their_alt_text() {
    let t = body(vec![
        cell_content(vec![image(LOGO_PNG, "png")]),
        cell_content(vec![image(WIDE_JPEG, "jpeg")]),
        cell_content(vec![image(LOGO_GIF, "gif")]),
        cell_content(vec![image(FLAG_SVG, "svg")]),
        cell_content(vec![image(REMOTE_PNG, "remote")]),
    ]);
    let alts: Vec<Option<String>> = anchors(&t).into_iter().map(|a| a.alt).collect();
    assert_eq!(
        alts,
        [Some("png".into()), Some("jpeg".into()), Some("gif".into())]
    );
    assert_eq!(
        media(&t),
        [
            "xl/media/image1.png",
            "xl/media/image2.jpeg",
            "xl/media/image3.gif"
        ]
    );
    let sizes: Vec<(u64, u64)> = anchors(&t).iter().map(|a| a.size).collect();
    assert_eq!(sizes, [(24, 24), (40, 20), (24, 24)]);
    let (sheet, _, sst) = support::render(&t);
    assert_eq!(find_text(&sheet, &sst, "svg"), "D2");
    assert_eq!(find_text(&sheet, &sst, "remote"), "E2");
    let cell = |col| Place::Cell {
        section: gridwell_layout::SectionKind::Body,
        group: Some(0),
        row: 0,
        col,
    };
    assert_eq!(warned(&t), [cell(3), cell(4)]);
}

#[test]
fn an_image_beside_text_or_in_a_footnote_shows_its_alt_text() {
    let mut t = TableBuilder::new(1)
        .body(vec![row(vec![cell_content(vec![
            text("Made by "),
            image(LOGO_PNG, "logo"),
        ])])])
        .footnote("n", "1", "note")
        .build();
    t.footer.as_mut().unwrap().footnotes[0].content = vec![image(LOGO_PNG, "foot")];
    assert!(anchors(&t).is_empty());
    let (sheet, _, sst) = support::render(&t);
    find_text(&sheet, &sst, "Made by logo");
    assert!(sst.contains("foot"), "{sst}");
    let places = warned(&t);
    assert_eq!(places.len(), 2, "{places:?}");
    assert_eq!(places[1], Place::Footnote(0));
}

#[test]
fn blank_text_around_an_image_leaves_it_alone() {
    let t = body(vec![cell_content(vec![
        text(" "),
        image(LOGO_PNG, "logo"),
        text("\n"),
    ])]);
    assert_eq!(anchors(&t).len(), 1);
    assert!(warned(&t).is_empty());
}

#[test]
fn an_image_is_placed_as_its_cells_text() {
    // Columns 84px wide (the default 12 characters); the row is 44px tall
    // (the 40px image in column D).
    let t = TableBuilder::new(4)
        .style_def(
            "top",
            StyleDef {
                vertical_align: Some("top".into()),
                ..Default::default()
            },
        )
        .style_def(
            "middle",
            StyleDef {
                vertical_align: Some("middle".into()),
                ..Default::default()
            },
        )
        .columns(vec![
            column("a", "A"),
            column("b", "B").align("center"),
            column("c", "C").align("right"),
            column("d", "D"),
        ])
        .body(vec![row(vec![
            img(LOGO_PNG, "left", "24px", "24px"),
            img(LOGO_PNG, "center", "24px", "24px").style("top"),
            img(LOGO_PNG, "right", "24px", "24px").style("middle"),
            img(LOGO_PNG, "tall", "24px", "40px"),
        ])])
        .build();
    let places: Vec<(u64, u64, u64)> = anchors(&t)
        .iter()
        .map(|a| (a.col, a.col_off, a.row_off))
        .collect();
    assert_eq!(
        places,
        [
            // Left, at the bottom (Excel's default): 2px in, 44 - 24 - 2 down.
            (0, 2, 18),
            // Centered across 84px; at the top.
            (1, 30, 2),
            // Right, 2px from the edge; centered down.
            (2, 58, 10),
            (3, 2, 2),
        ]
    );
}

#[test]
fn an_auto_width_column_widens_to_fit_an_image_and_a_fixed_one_does_not() {
    let table = |widths: [Option<&str>; 3], images: bool| {
        let cols = ["a", "b", "c"]
            .into_iter()
            .zip(widths)
            .map(|(id, w)| match w {
                Some(w) => column(id, id).width(w),
                None => column(id, id),
            })
            .collect();
        let cells = if images {
            vec![
                img(LOGO_PNG, "wide", "200px", "20px"),
                img(LOGO_PNG, "fixed", "200px", "20px"),
                cell("x"),
            ]
        } else {
            vec![cell("a"), cell("b"), cell("x")]
        };
        TableBuilder::new(3)
            .columns(cols)
            .body(vec![row(cells)])
            .build()
    };
    let widths = |t: &Table| -> Vec<String> {
        let sheet = XlsxWriter::new().render_sheet_xml(t).unwrap();
        sheet
            .split("<col ")
            .skip(1)
            .map(|c| attr(&format!(" {c}"), "width").unwrap().to_string())
            .collect()
    };
    let t = table([None, Some("50px"), None], true);
    // The image's 200px and 2px on either side; the fixed width; the default.
    let expected = table([Some("204px"), Some("50px"), None], false);
    assert_eq!(widths(&t), widths(&expected));
    assert_ne!(widths(&t)[0], widths(&expected)[2]);
    // 2px in on either side of the widened column.
    assert_eq!(anchors(&t)[0].col_off, 2);
}

#[test]
fn a_lone_image_in_a_title_or_source_note_is_drawn_on_its_line() {
    let mut t = TableBuilder::new(2)
        .title("Title")
        .head(row(vec![cell("A"), cell("B")]))
        .body(vec![row(vec![cell("a"), cell("b")])])
        .source_note("note")
        .build();
    t.header.as_mut().unwrap().title.as_mut().unwrap().content =
        vec![sized_image(LOGO_PNG, "t", None, Some("40px"))];
    t.footer.as_mut().unwrap().source_notes[0].content = vec![image(LOGO_PNG, "n")];
    let rows: Vec<(u64, Option<String>)> =
        anchors(&t).into_iter().map(|a| (a.row, a.alt)).collect();
    // The title is row 1; the note follows the head, the body and a blank row.
    assert_eq!(rows, [(0, Some("t".into())), (4, Some("n".into()))]);
    let sheet = XlsxWriter::new().render_sheet_xml(&t).unwrap();
    assert_eq!(row_height(&sheet, 1), Some(33.0), "{sheet}");
    assert!(warned(&t).is_empty());
}

#[test]
fn a_zero_sized_image_draws_nothing_and_a_huge_one_stays_in_bounds() {
    let t = body(vec![
        img(LOGO_PNG, "none", "0px", "0px"),
        img(LOGO_PNG, "huge", "100000px", "100000px"),
    ]);
    let a = anchors(&t);
    assert_eq!(a.len(), 1, "{a:?}");
    // Clamped at 14,400pt: 19,200px. The row stops at Excel's 409pt (which
    // the sheet writes as the whole pixels below it, 545px or 408.75pt).
    assert_eq!(a[0].size, (19_200, 19_200));
    let sheet = XlsxWriter::new().render_sheet_xml(&t).unwrap();
    assert_eq!(row_height(&sheet, 2), Some(408.75), "{sheet}");
    let sst = XlsxWriter::new().render_shared_strings_xml(&t).unwrap();
    assert!(!sst.contains("none"), "{sst}");
}

#[test]
fn the_same_image_twice_is_stored_once() {
    let t = body(vec![
        cell_content(vec![image(LOGO_PNG, "one")]),
        cell_content(vec![image(LOGO_PNG, "two")]),
    ]);
    assert_eq!(anchors(&t).len(), 2);
    assert_eq!(media(&t), ["xl/media/image1.png"]);
}

/// `bytes` as a `data:` URI.
fn uri(bytes: &[u8]) -> String {
    format!(
        "data:application/octet-stream;base64,{}",
        gridwell_core::image::base64(bytes)
    )
}

#[test]
fn an_image_excel_would_misread_shows_its_alt_text() {
    // A whole JPEG that starts with a marker without a length (TEM), which
    // rust_xlsxwriter reads as having one: the next marker, FFE1, 65,505
    // bytes on, at byte 65,509. Sized so that it then reads past the end.
    let app1 = [&[0xFF, 0xE1, 0xFF, 0xC1][..], &[0; 65_471]].concat();
    let jpeg = [
        &[0xFF, 0xD8, 0xFF, 0x01][..],
        &app1,
        &[0xFF, 0xC0, 0, 11, 8, 0, 20, 0, 40, 1, 1, 0x11, 0],
        &[0xFF, 0xDA, 0, 8, 1, 1, 0, 0, 0x3F, 0, 1, 1, 1, 1, 1, 1],
        &[0xFF, 0xD9],
    ]
    .concat();
    assert_eq!(jpeg.len(), 65_510);
    let src = uri(&jpeg);
    let image = gridwell_core::Image::from_src(&src).unwrap();
    assert_eq!(image.natural_px, Some((40.0, 20.0)));
    let t = body(vec![img(&src, "misread", "24px", "24px")]);
    assert!(anchors(&t).is_empty());
    let (sheet, _, sst) = support::render(&t);
    assert_eq!(find_text(&sheet, &sst, "misread"), "A2");
    assert_eq!(warned(&t).len(), 1);
}
