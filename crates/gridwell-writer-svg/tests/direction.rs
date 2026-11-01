//! A right-to-left table is drawn as the mirror image of the same table left to
//! right: its columns, rules and lines of text (each `direction="rtl"`), at the
//! mirrored place. Borders and alignment are physical, so a left-aligned cell is
//! still at its cell's left, and a cell's left border on its left, wherever the
//! cell went. The table's language is the drawing's `xml:lang`.

use gridwell_ir::style::Padding;
use gridwell_ir::{Table, TextDirection};
use gridwell_testkit::examples::directed;
use gridwell_writer_svg::render_svg;

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(tag[start..start + tag[start..].find('"')?].to_string())
}

fn num(tag: &str, name: &str) -> f64 {
    attr(tag, name).unwrap().parse().unwrap()
}

/// The drawn width (the `<svg>`'s is rounded up), its rects as (x, y, width,
/// height, fill), and its texts as (text, left, right, y): where each line of
/// text is drawn, from its anchor, direction and length.
#[allow(clippy::type_complexity)]
fn drawing(
    svg: &str,
) -> (
    f64,
    Vec<(f64, f64, f64, f64, String)>,
    Vec<(String, f64, f64, f64)>,
) {
    let rects: Vec<(f64, f64, f64, f64, String)> = svg
        .split("<rect ")
        .skip(1)
        .map(|r| {
            let tag = format!(" {}", &r[..r.find("/>").unwrap()]);
            let n = |k| num(&tag, k);
            (
                n("x"),
                n("y"),
                n("width"),
                n("height"),
                attr(&tag, "fill").unwrap(),
            )
        })
        .collect();
    let width = rects.iter().map(|r| r.0 + r.2).fold(0.0, f64::max);
    let texts = svg
        .split("<text ")
        .skip(1)
        .map(|t| {
            let close = t.find('>').unwrap();
            let tag = format!(" {}", &t[..close]);
            let text = t[close + 1..t.find("</text>").unwrap()].to_string();
            let (x, len) = (num(&tag, "x"), num(&tag, "textLength"));
            let rtl = attr(&tag, "direction").as_deref() == Some("rtl");
            let (left, right) = match (attr(&tag, "text-anchor").as_deref(), rtl) {
                (Some("middle"), _) => (x - len / 2.0, x + len / 2.0),
                (Some("end"), false) | (None, true) => (x - len, x),
                (Some("end"), true) | (None, false) => (x, x + len),
                (Some(a), _) => panic!("anchor {a}"),
            };
            (text, left, right, num(&tag, "y"))
        })
        .collect();
    (width, rects, texts)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.011
}

/// `directed` without its one-sided border, and, right to left, with its
/// (physical) alignment swapped: the mirror image of itself left to right.
fn plain(dir: Option<TextDirection>) -> Table {
    let mut t = directed(dir.clone(), Some("ar"));
    if dir.is_some() {
        t.column_spec[0].align = "right".into();
        t.column_spec[1].align = "left".into();
    }
    t.styles.defs.clear();
    for g in &mut t.table.tbody {
        for r in &mut g.rows {
            for c in &mut r.cells {
                c.style_id = None;
            }
        }
    }
    t
}

#[test]
fn right_to_left_is_the_mirror_image() {
    let (w, ltr_rects, ltr_texts) = drawing(&render_svg(&plain(None)).unwrap());
    let rtl_svg = render_svg(&plain(Some(TextDirection::Rtl))).unwrap();
    let (rw, rtl_rects, rtl_texts) = drawing(&rtl_svg);
    assert!(close(w, rw));
    assert_eq!(ltr_rects.len(), rtl_rects.len());
    for l in &ltr_rects {
        // Some rect of the same kind is at the mirrored place.
        let mirrored = rtl_rects.iter().any(|r| {
            close(r.0, w - l.0 - l.2)
                && close(r.1, l.1)
                && close(r.2, l.2)
                && close(r.3, l.3)
                && r.4 == l.4
        });
        assert!(mirrored, "{l:?} not mirrored in {rtl_rects:?}");
    }
    assert_eq!(ltr_texts.len(), rtl_texts.len());
    for (l, r) in ltr_texts.iter().zip(&rtl_texts) {
        assert_eq!(l.0, r.0);
        assert!(
            close(r.1, w - l.2) && close(r.2, w - l.1) && close(r.3, l.3),
            "{l:?} → {r:?}"
        );
    }
    // Not vacuous: the stub moved to the right, the group label and note too.
    let x = |texts: &[(String, f64, f64, f64)], s: &str| texts.iter().find(|t| t.0 == s).unwrap().1;
    assert!(x(&ltr_texts, "one") < x(&ltr_texts, "2") && x(&rtl_texts, "one") > x(&rtl_texts, "2"));
    assert!(x(&rtl_texts, "Source") > w / 2.0 && x(&rtl_texts, "Group") > w / 2.0);
    assert_eq!(
        rtl_svg.matches("direction=\"rtl\"").count(),
        rtl_texts.len()
    );
    assert!(!render_svg(&plain(None)).unwrap().contains("direction="));
}

#[test]
fn alignment_is_physical() {
    // Right to left, unswapped: a left-aligned cell is still at its cell's left
    // edge, and a right-aligned one at its right.
    let (_, rects, texts) =
        drawing(&render_svg(&directed(Some(TextDirection::Rtl), None)).unwrap());
    let text = |s: &str| texts.iter().find(|t| t.0 == s).unwrap().clone();
    // The stub's (2pt) rule is between the columns, the stub to its right; a
    // cell's text is half the rule and its 5pt padding from it.
    let rule = rects.iter().find(|r| r.2 == 2.0 && r.3 > 2.0).unwrap().0 + 1.0;
    for s in ["Name", "one", "two"] {
        assert!(
            close(text(s).1, rule + 6.0),
            "{s}: {:?}, rule at {rule}",
            text(s)
        );
    }
    for s in ["Value", "2", "LEFTRED"] {
        assert!(
            close(text(s).2, rule - 6.0),
            "{s}: {:?}, rule at {rule}",
            text(s)
        );
    }
}

#[test]
fn padding_is_on_the_side_it_names() {
    // `LEFTRED` with 20px of padding on its right: right-aligned, it ends that
    // much further from its cell's right edge than `2` does, in either direction.
    let inset = |dir: Option<TextDirection>| {
        let mut t = directed(dir, None);
        t.styles.defs.get_mut("left_red").unwrap().padding = Some(Padding {
            top: None,
            right: Some("20px".into()),
            bottom: None,
            left: None,
        });
        let (_, _, texts) = drawing(&render_svg(&t).unwrap());
        let right = |s: &str| texts.iter().find(|t| t.0 == s).unwrap().2;
        right("2") - right("LEFTRED")
    };
    let ltr = inset(None);
    assert!(ltr > 10.0, "{ltr}");
    assert!(close(inset(Some(TextDirection::Rtl)), ltr));
}

#[test]
fn a_border_is_on_the_side_it_names() {
    let red = |t: &Table| {
        let svg = render_svg(t).unwrap();
        let (w, rects, texts) = drawing(&svg);
        let r = rects.into_iter().find(|r| r.4 == "#FF0000").unwrap();
        let cell = texts.into_iter().find(|t| t.0 == "LEFTRED").unwrap();
        (w, r, cell)
    };
    // Left to right, the red border is between the stub and the cell.
    let (_, r, cell) = red(&directed(None, None));
    assert!(r.0 > 0.0 && r.0 < cell.1, "{r:?} {cell:?}");
    // Right to left, the cell is on the left, and so is its border: the table's
    // left edge.
    let (_, r, cell) = red(&directed(Some(TextDirection::Rtl), None));
    assert!(close(r.0, 0.0) && r.0 < cell.1, "{r:?} {cell:?}");
}

#[test]
fn the_language_is_the_drawings() {
    let svg = render_svg(&directed(None, Some("ar"))).unwrap();
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width="));
    assert!(
        svg[..svg.find('>').unwrap()].ends_with(" xml:lang=\"ar\""),
        "{svg}"
    );
    let svg = render_svg(&directed(None, Some("a\"b"))).unwrap();
    assert!(
        svg[..svg.find('>').unwrap()].contains(" xml:lang=\"a&quot;b\""),
        "{svg}"
    );
    let svg = render_svg(&directed(Some(TextDirection::Rtl), None)).unwrap();
    assert!(!svg[..svg.find('>').unwrap()].contains("xml:lang"), "{svg}");
}
