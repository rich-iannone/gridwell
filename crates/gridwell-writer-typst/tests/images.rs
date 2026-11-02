//! Images: one embedded as a PNG, JPEG, GIF or SVG `data:` URI is drawn inline
//! at its size (from the IR, or its own); one linked by URL or path shows its
//! alt text, and is warned of. Typst reads them (in 0.12 as in later versions):
//! compiled with a show rule that records each image it draws.
//!
//! Compiling is skipped when `typst` is missing, unless `GRIDWELL_REQUIRE_TYPST`
//! is set.

use std::process::Command;

use gridwell_ir::Table;
use gridwell_testkit::images::{FLAG_SVG, LOGO_GIF, LOGO_PNG, REMOTE_PNG, WIDE_JPEG};
use gridwell_testkit::{cell_content, image, row, sized_image, text, TableBuilder};
use gridwell_writer_typst::{render_typst, unsupported};
use serde_json::Value;

fn one_row(content: Vec<Vec<gridwell_ir::content::ContentNode>>) -> Table {
    TableBuilder::new(content.len() as u32)
        .body(vec![row(content.into_iter().map(cell_content).collect())])
        .build()
}

fn typst_available() -> bool {
    if Command::new("typst").arg("--version").output().is_ok() {
        return true;
    }
    if std::env::var_os("GRIDWELL_REQUIRE_TYPST").is_some_and(|v| !v.is_empty()) {
        panic!("typst not found but GRIDWELL_REQUIRE_TYPST is set");
    }
    eprintln!("skipping: typst not found");
    false
}

/// Each image Typst draws for `t`: its format, width, height and alt text.
fn drawn(t: &Table) -> Vec<(String, String, String, String)> {
    let dir = std::env::temp_dir().join(format!("gridwell-typst-images-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!(
        "{:x}.typ",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let doc = format!(
        "#show image: it => [#metadata((format: it.format, width: repr(it.width.length), \
         height: repr(it.height.length), alt: it.alt)) <gw-image>] + it\n{}",
        render_typst(t).unwrap()
    );
    std::fs::write(&file, doc).unwrap();
    let out = Command::new("typst")
        .arg("query")
        .arg(&file)
        .args(["<gw-image>", "--field", "value"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let values: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    let s = |v: &Value| v.as_str().unwrap_or("none").to_string();
    values
        .iter()
        .map(|v| {
            (
                s(&v["format"]),
                s(&v["width"]),
                s(&v["height"]),
                s(&v["alt"]),
            )
        })
        .collect()
}

#[test]
fn each_format_is_embedded_with_its_size() {
    let t = one_row(vec![
        vec![image(LOGO_PNG, "png")],
        vec![image(WIDE_JPEG, "jpeg")],
        vec![image(LOGO_GIF, "gif")],
        vec![image(FLAG_SVG, "svg")],
    ]);
    let src = render_typst(&t).unwrap();
    for format in ["png", "jpg", "gif", "svg"] {
        assert!(src.contains(&format!("format: \"{format}\"")), "{format}");
    }
    assert!(unsupported(&t).is_empty());
    if !typst_available() {
        return;
    }
    let row = |f: &str, w: &str, h: &str, alt: &str| {
        (f.to_string(), w.to_string(), h.to_string(), alt.to_string())
    };
    // Their own sizes, at 0.75pt a pixel: 24×24, 40×20, 24×24, 30×15.
    assert_eq!(
        drawn(&t),
        [
            row("png", "18pt", "18pt", "png"),
            row("jpg", "30pt", "15pt", "jpeg"),
            row("gif", "18pt", "18pt", "gif"),
            row("svg", "22.5pt", "11.25pt", "svg"),
        ]
    );
}

#[test]
fn a_set_size_is_used_and_em_follows_the_text() {
    let t = one_row(vec![
        vec![sized_image(LOGO_PNG, "both", Some("32px"), Some("16px"))],
        vec![sized_image(WIDE_JPEG, "width", Some("20pt"), None)],
        vec![sized_image(LOGO_PNG, "em", None, Some("2em"))],
        vec![
            text("Made by "),
            sized_image(LOGO_GIF, "inline", Some("1em"), None),
            text(" us"),
        ],
    ]);
    if !typst_available() {
        return;
    }
    let sizes: Vec<(String, String)> = drawn(&t).into_iter().map(|(_, w, h, _)| (w, h)).collect();
    let pt = |w: &str, h: &str| (w.to_string(), h.to_string());
    assert_eq!(
        sizes,
        [
            // Stretched to both sides.
            pt("24pt", "12pt"),
            // The other side keeps the aspect ratio.
            pt("20pt", "10pt"),
            // Of the table's text size, 11pt.
            pt("22pt", "22pt"),
            pt("11pt", "11pt"),
        ]
    );
}

#[test]
fn a_linked_image_shows_its_alt_text_and_is_warned_of() {
    let t = one_row(vec![
        vec![image(REMOTE_PNG, "Remote logo")],
        vec![gridwell_ir::content::ContentNode::Image {
            src: "logo.png".into(),
            alt: None,
            width: None,
            height: None,
        }],
        // Markup in alt text is text.
        vec![image("x.png", "*not bold* #x")],
    ]);
    let src = render_typst(&t).unwrap();
    assert!(!src.contains("image("), "{src}");
    assert!(src.contains("Remote logo"), "{src}");
    assert!(src.contains("[image]"), "{src}");
    assert!(src.contains("\\*not bold\\* \\#x"), "{src}");
    let image = unsupported(&t)
        .into_iter()
        .find(|u| u.feature == "image")
        .unwrap();
    assert_eq!(image.places.len(), 3);
    if typst_available() {
        assert!(drawn(&t).is_empty());
    }
}
