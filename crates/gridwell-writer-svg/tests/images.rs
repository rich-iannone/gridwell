//! Images: one embedded as a `data:` URI (PNG, JPEG, GIF or SVG) is an
//! `<image>` at its size (from the IR, or its own), standing on the baseline of
//! its line, which grows to hold it, between the text around it (mirrored
//! right to left); its alt text is its `<title>`. One linked by URL or path,
//! which a self-contained drawing shouldn't load, shows its alt text, and is
//! warned of. `rsvg-convert` draws them (skipped when it's missing, unless
//! `GRIDWELL_REQUIRE_RSVG` is set).

use std::process::Command;

use gridwell_core::Image;
use gridwell_ir::content::ContentNode;
use gridwell_ir::{Table, TextDirection};
use gridwell_testkit::images::{FLAG_SVG, LOGO_GIF, LOGO_PNG, REMOTE_PNG, WIDE_JPEG};
use gridwell_testkit::{cell_content, image, row, sized_image, text, TableBuilder};
use gridwell_writer_svg::{render_svg, unsupported};

fn one_row(content: Vec<Vec<ContentNode>>) -> Table {
    TableBuilder::new(content.len() as u32)
        .body(vec![row(content.into_iter().map(cell_content).collect())])
        .build()
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(tag[start..start + tag[start..].find('"')?].to_string())
}

fn num(tag: &str, name: &str) -> f64 {
    attr(tag, name).unwrap().parse().unwrap()
}

#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    href: String,
    title: Option<String>,
}

fn images(svg: &str) -> Vec<Drawn> {
    svg.split("<image ")
        .skip(1)
        .map(|i| {
            let tag = format!(" {}", &i[..i.find('>').unwrap()]);
            let title = (!tag.ends_with('/')).then(|| {
                let t = &i[i.find("<title>").unwrap() + 7..];
                t[..t.find("</title>").unwrap()].to_string()
            });
            Drawn {
                x: num(&tag, "x"),
                y: num(&tag, "y"),
                width: num(&tag, "width"),
                height: num(&tag, "height"),
                href: attr(&tag, "href").unwrap(),
                title,
            }
        })
        .collect()
}

/// Each `<text>`: its text, `x`, `y`, `textLength` and whether it's `rtl`.
fn texts(svg: &str) -> Vec<(String, f64, f64, f64, bool)> {
    svg.split("<text ")
        .skip(1)
        .map(|t| {
            let tag = format!(" {}", &t[..t.find('>').unwrap()]);
            let body = &t[t.find('>').unwrap() + 1..t.find("</text>").unwrap()];
            let mut text = String::new();
            let mut in_tag = false;
            for c in body.chars() {
                match c {
                    '<' => in_tag = true,
                    '>' => in_tag = false,
                    c if !in_tag => text.push(c),
                    _ => {}
                }
            }
            (
                text,
                num(&tag, "x"),
                num(&tag, "y"),
                num(&tag, "textLength"),
                attr(&tag, "direction").as_deref() == Some("rtl"),
            )
        })
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.011
}

fn warned(t: &Table) -> usize {
    unsupported(t)
        .into_iter()
        .find(|u| u.feature == "image")
        .map_or(0, |u| u.places.len())
}

fn uri(src: &str) -> String {
    Image::from_src(src).unwrap().data_uri()
}

#[test]
fn each_format_is_drawn_at_its_size() {
    let t = one_row(vec![
        vec![image(LOGO_PNG, "png")],
        vec![image(WIDE_JPEG, "jpeg")],
        vec![image(LOGO_GIF, "gif")],
        vec![image(FLAG_SVG, "svg & co")],
    ]);
    let svg = render_svg(&t).unwrap();
    let drawn: Vec<(f64, f64, String, Option<String>)> = images(&svg)
        .into_iter()
        .map(|d| (d.width, d.height, d.href, d.title))
        .collect();
    let d = |w, h, src, alt: &str| (w, h, uri(src), Some(alt.to_string()));
    assert_eq!(
        drawn,
        [
            d(24.0, 24.0, LOGO_PNG, "png"),
            d(40.0, 20.0, WIDE_JPEG, "jpeg"),
            d(24.0, 24.0, LOGO_GIF, "gif"),
            d(30.0, 15.0, FLAG_SVG, "svg &amp; co"),
        ]
    );
    // The SVG's own markup is in its (base64) URI, not the drawing's.
    assert!(!svg.contains("<rect width=\"30\""), "{svg}");
    assert_eq!(warned(&t), 0);
}

#[test]
fn a_set_size_is_used_and_em_follows_the_text() {
    let mut t = one_row(vec![
        vec![sized_image(LOGO_PNG, "both", Some("32px"), Some("16px"))],
        vec![sized_image(WIDE_JPEG, "width", Some("30pt"), None)],
        vec![sized_image(LOGO_PNG, "em", None, Some("2em"))],
        vec![sized_image(LOGO_PNG, "big em", Some("1em"), None)],
    ]);
    t.styles.defs.insert(
        "big".into(),
        serde_json::from_value(serde_json::json!({ "font_size": "20px" })).unwrap(),
    );
    t.table.tbody[0].rows[0].cells[3].style_id = Some("big".into());
    let sizes: Vec<(f64, f64)> = images(&render_svg(&t).unwrap())
        .iter()
        .map(|d| (d.width, d.height))
        .collect();
    assert_eq!(
        sizes,
        [
            // Stretched to both sides.
            (32.0, 16.0),
            // The other side keeps the aspect ratio.
            (40.0, 20.0),
            // Of the text's size: the table's 16px, or its cell's.
            (32.0, 32.0),
            (20.0, 20.0),
        ]
    );
}

/// The rules' rects (x, y, width, height): above and below the one row.
fn rules(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.split("<rect ")
        .skip(1)
        .map(|r| {
            let tag = format!(" {}", &r[..r.find("/>").unwrap()]);
            (
                num(&tag, "x"),
                num(&tag, "y"),
                num(&tag, "width"),
                num(&tag, "height"),
            )
        })
        .collect()
}

#[test]
fn a_picture_stands_on_the_baseline_and_its_line_grows_to_hold_it() {
    let tall = |src: &str| {
        one_row(vec![vec![
            text("ab "),
            sized_image(src, "tall", Some("10px"), Some("60px")),
            text(" cd"),
        ]])
    };
    let svg = render_svg(&tall(LOGO_PNG)).unwrap();
    let [img] = &images(&svg)[..] else {
        panic!("{svg}")
    };
    let texts = texts(&svg);
    assert_eq!(texts.len(), 2, "{svg}");
    // Text, picture and text share one baseline.
    for (_, _, y, _, _) in &texts {
        assert!(close(img.y + img.height, *y), "{svg}");
    }
    // The line holds the picture above its baseline (and the text's descent
    // below): between the rules, and the row taller than with the alt text.
    let [top, bottom] = rules(&svg)[..] else {
        panic!("{svg}")
    };
    assert!(
        img.y >= top.1 + top.3 && img.y + img.height < bottom.1,
        "{svg}"
    );
    let alt = render_svg(&tall(REMOTE_PNG)).unwrap();
    assert!(bottom.1 > rules(&alt)[1].1 + 40.0, "{svg}\n{alt}");
}

/// The texts and pictures of a cell `Made by <logo> us`, in order: each one's
/// left and right; and the table's right edge.
fn spans(dir: Option<TextDirection>) -> (Vec<(f64, f64)>, f64) {
    let t = TableBuilder::new(1)
        .config(|c| c.dir = dir)
        .body(vec![row(vec![cell_content(vec![
            text("Made by "),
            image(LOGO_PNG, "logo"),
            text(" us"),
        ])])])
        .build();
    let svg = render_svg(&t).unwrap();
    let mut spans: Vec<(f64, f64)> = texts(&svg)
        .into_iter()
        .map(|(_, x, _, len, rtl)| if rtl { (x - len, x) } else { (x, x + len) })
        .collect();
    let img = &images(&svg)[0];
    spans.insert(1, (img.x, img.x + img.width));
    let right = rules(&svg).iter().map(|r| r.0 + r.2).fold(0.0, f64::max);
    (spans, right)
}

#[test]
fn a_picture_sits_between_its_text_mirrored_right_to_left() {
    let (ltr, right) = spans(None);
    // The cell is wide enough for the line, padding included.
    assert!(ltr[2].1 + 5.0 <= right + 0.011, "{ltr:?} {right}");
    // Each starts where the last ended.
    assert!(close(ltr[0].1, ltr[1].0), "{ltr:?}");
    assert!(close(ltr[1].1, ltr[2].0), "{ltr:?}");
    assert!(close(ltr[1].1 - ltr[1].0, 24.0));
    // Right to left, the first is rightmost: the line (still at its cell's
    // left, as alignment is physical) is the mirror image.
    let (rtl, _) = spans(Some(TextDirection::Rtl));
    let edges = ltr[0].0 + ltr[2].1;
    for (l, r) in ltr.iter().zip(&rtl) {
        assert!(
            close(edges - l.1, r.0) && close(edges - l.0, r.1),
            "{ltr:?}\n{rtl:?}"
        );
    }
}

#[test]
fn pictures_side_by_side_are_drawn_each_on_its_own() {
    let t = one_row(vec![vec![image(LOGO_PNG, "a"), image(LOGO_PNG, "a")]]);
    let svg = render_svg(&t).unwrap();
    let xs: Vec<f64> = images(&svg).iter().map(|d| d.x).collect();
    assert_eq!(xs.len(), 2, "{svg}");
    assert!(close(xs[1] - xs[0], 24.0), "{xs:?}");
}

#[test]
fn a_linked_image_shows_its_alt_text_and_is_warned_of() {
    let t = one_row(vec![
        vec![image(REMOTE_PNG, "remote")],
        vec![image("logo.png", "path")],
        vec![sized_image(LOGO_PNG, "", Some("0px"), Some("0px"))],
    ]);
    let svg = render_svg(&t).unwrap();
    assert!(images(&svg).is_empty(), "{svg}");
    assert!(
        !svg.contains("example.com") && !svg.contains("logo.png"),
        "{svg}"
    );
    let shown: Vec<String> = texts(&svg).into_iter().map(|t| t.0).collect();
    assert!(
        shown.contains(&"remote".into()) && shown.contains(&"path".into()),
        "{shown:?}"
    );
    assert_eq!(warned(&t), 2);
}

#[test]
fn rsvg_draws_the_pictures() {
    if Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_err()
    {
        if std::env::var_os("GRIDWELL_REQUIRE_RSVG").is_some_and(|v| !v.is_empty()) {
            panic!("rsvg-convert not found but GRIDWELL_REQUIRE_RSVG is set");
        }
        eprintln!("skipping: rsvg-convert not found");
        return;
    }
    let t = one_row(vec![
        vec![sized_image(FLAG_SVG, "svg", Some("40px"), Some("20px"))],
        vec![sized_image(WIDE_JPEG, "jpeg", Some("40px"), Some("20px"))],
        vec![sized_image(LOGO_PNG, "png", Some("40px"), Some("40px"))],
        vec![sized_image(LOGO_GIF, "gif", Some("40px"), Some("40px"))],
    ]);
    let svg = render_svg(&t).unwrap();
    let dir = std::env::temp_dir().join(format!("gridwell-svg-images-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (svg_file, png_file) = (dir.join("t.svg"), dir.join("t.png"));
    std::fs::write(&svg_file, &svg).unwrap();
    let out = Command::new("rsvg-convert")
        .arg("-o")
        .arg(&png_file)
        .arg(&svg_file)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let png = image::open(&png_file).unwrap().to_rgb8();
    let at = |d: &Drawn, fx: f64, fy: f64| {
        png.get_pixel((d.x + d.width * fx) as u32, (d.y + d.height * fy) as u32)
            .0
    };
    let near = |p: [u8; 3], q: [u8; 3]| p.iter().zip(q).all(|(a, b)| a.abs_diff(b) <= 24);
    let drawn = images(&svg);
    // The flag's red; the JPEG's flat orange-red; the logo's blue corner and
    // white dot, as a PNG and as a GIF.
    assert!(
        near(at(&drawn[0], 0.5, 0.5), [0xcc, 0x33, 0x33]),
        "{:?}",
        at(&drawn[0], 0.5, 0.5)
    );
    assert!(
        near(at(&drawn[1], 0.5, 0.5), [0xd6, 0x49, 0x23]),
        "{:?}",
        at(&drawn[1], 0.5, 0.5)
    );
    for d in &drawn[2..] {
        assert!(
            near(at(d, 0.1, 0.1), [0x25, 0x63, 0xeb]),
            "{:?}",
            at(d, 0.1, 0.1)
        );
        assert!(
            near(at(d, 0.5, 0.5), [0xff, 0xff, 0xff]),
            "{:?}",
            at(d, 0.5, 0.5)
        );
    }
}
