//! Images: one embedded as a PNG or JPEG `data:` URI is a `\pict` group, its
//! bytes in hex, drawn at its size (from the IR, or its own) with its alt text
//! as the picture's description. RTF has no GIF or SVG picture: those, and an
//! image linked by URL or path, show their alt text, and are warned of.

use gridwell_core::Image;
use gridwell_ir::content::ContentNode;
use gridwell_ir::Table;
use gridwell_testkit::images::{FLAG_SVG, LOGO_GIF, LOGO_PNG, REMOTE_PNG, WIDE_JPEG};
use gridwell_testkit::{cell_content, image, row, sized_image, text, TableBuilder};
use gridwell_writer_rtf::{render_rtf, unsupported};

fn one_row(content: Vec<Vec<ContentNode>>) -> Table {
    TableBuilder::new(content.len() as u32)
        .body(vec![row(content.into_iter().map(cell_content).collect())])
        .build()
}

/// A `\pict` group: its blip, `\picw`/`\pich`, `\picwgoal`/`\pichgoal`, its
/// description and its bytes.
#[derive(Debug, PartialEq)]
struct Pict {
    blip: String,
    px: (u32, u32),
    twips: (u32, u32),
    alt: Option<String>,
    data: Vec<u8>,
}

/// The end of the group opened at `start` (escaped braces skipped).
fn group_end(rtf: &str, start: usize) -> usize {
    let mut depth = 0;
    let mut chars = rtf[start..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return start + i;
                }
            }
            _ => {}
        }
    }
    panic!("unclosed group at {start}");
}

fn number(group: &str, word: &str) -> u32 {
    let at = group
        .find(word)
        .unwrap_or_else(|| panic!("{word} in {group}"))
        + word.len();
    group[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

fn picts(rtf: &str) -> Vec<Pict> {
    rtf.match_indices("{\\pict")
        .map(|(start, _)| {
            let group = &rtf[start + 1..group_end(rtf, start)];
            let alt = group.find("{\\*\\picprop").map(|p| {
                let prop = &group[p..group_end(group, p) + 1];
                assert!(prop.contains("{\\sn wzDescription}"), "{prop}");
                let sv = prop.find("{\\sv ").unwrap();
                prop[sv + 5..group_end(prop, sv)].to_string()
            });
            let body = match alt {
                Some(_) => &group[group_end(group, group.find("{\\*").unwrap()) + 1..],
                None => &group["\\pict".len()..],
            };
            let blip = body[1..].split('\\').next().unwrap().to_string();
            // The hex follows the last control word.
            let hex: String = body[body.find('\n').unwrap()..]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            Pict {
                blip,
                px: (number(body, "\\picw"), number(body, "\\pich")),
                twips: (number(body, "\\picwgoal"), number(body, "\\pichgoal")),
                alt,
                data: (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect(),
            }
        })
        .collect()
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

#[test]
fn png_and_jpeg_are_embedded_at_their_size() {
    let t = one_row(vec![
        vec![image(LOGO_PNG, "png")],
        vec![text("Made by "), image(WIDE_JPEG, "jpeg"), text(" us")],
    ]);
    let rtf = render_rtf(&t).unwrap();
    let pict = |blip: &str, px, twips, alt: &str, src| Pict {
        blip: blip.into(),
        px,
        twips,
        alt: Some(alt.into()),
        data: bytes(src),
    };
    // At 0.75pt a pixel: 18pt square; 30×15pt.
    assert_eq!(
        picts(&rtf),
        [
            pict("pngblip", (24, 24), (360, 360), "png", LOGO_PNG),
            pict("jpegblip", (40, 20), (600, 300), "jpeg", WIDE_JPEG),
        ]
    );
    // The image sits between its text.
    assert!(rtf.contains("Made by {\\pict"), "{rtf}");
    assert_eq!(warned(&t), 0);
}

#[test]
fn a_set_size_is_used_and_em_follows_the_text() {
    let t = one_row(vec![
        vec![sized_image(LOGO_PNG, "both", Some("32px"), Some("16px"))],
        vec![sized_image(WIDE_JPEG, "width", Some("20pt"), None)],
        vec![sized_image(LOGO_PNG, "em", None, Some("2em"))],
    ]);
    let twips: Vec<(u32, (u32, u32))> = picts(&render_rtf(&t).unwrap())
        .into_iter()
        .map(|p| (p.px.0, p.twips))
        .collect();
    assert_eq!(
        twips,
        [
            // Stretched to both sides, 24×12pt; its pixels are its own.
            (24, (480, 240)),
            // The other side keeps the aspect ratio.
            (40, (400, 200)),
            // Of the table's text size, 12pt.
            (24, (480, 480)),
        ]
    );
}

#[test]
fn alt_text_is_escaped_and_none_leaves_no_description() {
    let t = one_row(vec![
        vec![image(LOGO_PNG, "a {b} \\c é")],
        vec![image(LOGO_PNG, "")],
        vec![ContentNode::Image {
            src: LOGO_PNG.into(),
            alt: None,
            width: None,
            height: None,
        }],
    ]);
    let alts: Vec<Option<String>> = picts(&render_rtf(&t).unwrap())
        .into_iter()
        .map(|p| p.alt)
        .collect();
    assert_eq!(alts, [Some("a \\{b\\} \\\\c \\u233?".into()), None, None]);
}

#[test]
fn gif_svg_and_links_show_their_alt_text_and_are_warned_of() {
    let t = one_row(vec![
        vec![image(LOGO_GIF, "gif")],
        vec![image(FLAG_SVG, "svg")],
        vec![image(REMOTE_PNG, "remote")],
        vec![image("logo.png", "path")],
        vec![image(LOGO_PNG, "png")],
    ]);
    let rtf = render_rtf(&t).unwrap();
    assert_eq!(picts(&rtf).len(), 1, "{rtf}");
    for alt in ["gif", "svg", "remote", "path"] {
        assert!(rtf.contains(&format!(" {alt}\\cell")), "{alt}\n{rtf}");
    }
    assert_eq!(warned(&t), 4);
}
