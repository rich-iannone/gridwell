//! An image whose data is cut short isn't one: every writer shows its alt
//! text, so none panics on it and Typst (which fails to compile a document
//! with an image it can't decode) compiles it.

mod support;

use gridwell_core::image::base64;
use gridwell_core::Image;
use gridwell_ir::content::ContentNode;
use gridwell_testkit::images::{FLAG_SVG, LOGO_GIF, LOGO_PNG, WIDE_JPEG};
use gridwell_testkit::{cell_content, row, text, TableBuilder};

#[test]
fn no_writer_panics_on_a_cut_image() {
    let mut failures = Vec::new();
    let mut typst = Vec::new();
    for src in [LOGO_PNG, WIDE_JPEG, LOGO_GIF, FLAG_SVG] {
        let image = Image::from_src(src).unwrap();
        let cut = |n: usize| ContentNode::Image {
            src: format!(
                "data:{};base64,{}",
                image.format.mime(),
                base64(&image.data[..n])
            ),
            alt: Some("cut".into()),
            width: None,
            height: None,
        };
        let n = image.data.len();
        for i in 0..n {
            let ContentNode::Image { src, .. } = cut(i) else {
                unreachable!()
            };
            assert_eq!(Image::from_src(&src), None, "{:?} cut at {i}", image.format);
        }
        // Alone in a cell (drawn where a format can), and beside text; and
        // the whole image, which is drawn.
        let cells = (0..n)
            .map(|i| cell_content(vec![cut(i)]))
            .chain((0..n).map(|i| cell_content(vec![text("t"), cut(i)])))
            .chain([cell_content(vec![cut(n)])])
            .collect();
        let mut t = TableBuilder::new(2 * n as u32 + 1)
            .title("x")
            .body(vec![row(cells)])
            .build();
        // And alone on a line.
        t.header.as_mut().unwrap().title.as_mut().unwrap().content = vec![cut(n / 2)];
        assert!(t.validate().is_empty(), "{:?}", t.validate());
        typst.push(gridwell_render::render_text(&t, "typst", None).unwrap());
        for f in gridwell_render::names() {
            match std::panic::catch_unwind(|| gridwell_render::render(&t, f, None)) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => failures.push(format!("{f}, {:?}: {e}", image.format)),
                Err(_) => failures.push(format!("{f}, {:?}: panicked", image.format)),
            }
            if std::panic::catch_unwind(|| gridwell_render::warnings(&t, Some(f))).is_err() {
                failures.push(format!("{f}, {:?}: warnings panicked", image.format));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    if support::typeset::require("typst", "GRIDWELL_REQUIRE_TYPST") {
        for grid in support::typeset::typst_grids(&typst) {
            grid.unwrap();
        }
    }
}
