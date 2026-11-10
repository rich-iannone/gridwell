//! Structural oracle for RTF: groups balance, and every color index used (`\cfN`,
//! `\clcbpatN`, `\brdrcfN`) exists in the color table written in the header. The color table
//! is written before anything that uses it, so a color first seen late (a styled
//! run, a title, a label, a note) must still have been registered up front.

use gridwell_ir::{StyleDef, Table};
use gridwell_testkit::examples::{font_families, styled_merge};
use gridwell_testkit::{cell_content, examples, labeled_group, row, styled, text, TableBuilder};
use gridwell_writer_rtf::render_rtf;

/// Group depth never negative and back to zero at the end (escaped braces skipped).
fn balanced(rtf: &str) -> bool {
    let mut depth = 0i64;
    let mut chars = rtf.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Numbers following a control word, e.g. all N in `\cfN`.
fn indices(rtf: &str, word: &str) -> Vec<usize> {
    rtf.match_indices(word)
        .filter_map(|(i, _)| {
            let digits: String = rtf[i + word.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .collect()
}

fn check(name: &str, t: &Table) {
    let rtf = render_rtf(t).unwrap();
    assert!(balanced(&rtf), "{name}: unbalanced groups\n{rtf}");
    let table = rtf
        .split("{\\colortbl ;")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    let colors = table.matches(';').count();
    for word in ["\\cf", "\\clcbpat", "\\brdrcf"] {
        for i in indices(&rtf, word) {
            assert!(
                i >= 1 && i <= colors,
                "{name}: {word}{i} but {colors} colors\n{rtf}"
            );
        }
    }
}

#[test]
fn corpus_is_well_formed() {
    for ex in examples() {
        check(ex.name, &ex.table());
    }
}

#[test]
fn colours_first_used_outside_cells_are_registered() {
    let style = |color: &str, bg: Option<&str>| StyleDef {
        color: Some(color.into()),
        background_color: bg.map(Into::into),
        font_weight: Some("bold".into()),
        ..Default::default()
    };
    let mut t = TableBuilder::new(1)
        .title("Title")
        .subtitle("Sub")
        .style_def("t", style("#110000", None))
        .style_def("s", style("#220000", None))
        .style_def("l", style("#330000", Some("#440000")))
        .style_def("n", style("#550000", None))
        .style_def("run", style("#660000", None))
        .group(
            labeled_group(
                "Group",
                vec![row(vec![cell_content(vec![
                    text("a "),
                    styled("run", "run"),
                ])])],
            )
            .label_style("l"),
        )
        .footnote("f", "1", "note")
        .source_note("src")
        .build();
    let h = t.header.as_mut().unwrap();
    h.title.as_mut().unwrap().style_id = Some("t".into());
    h.subtitle.as_mut().unwrap().style_id = Some("s".into());
    let f = t.footer.as_mut().unwrap();
    f.footnotes[0].style_id = Some("n".into());
    f.source_notes[0].style_id = Some("n".into());
    check("styled lines", &t);
    let rtf = render_rtf(&t).unwrap();
    for (r, word) in [
        (17, "title"),
        (34, "subtitle"),
        (51, "label"),
        (68, "label fill"),
        (85, "note"),
        (102, "run"),
    ] {
        assert!(
            rtf.contains(&format!("\\red{r}\\green0\\blue0;")),
            "{word} color missing\n{rtf}"
        );
    }
    assert!(rtf.contains("{\\b\\ab\\cf"), "{rtf}");
}

/// The cell definitions of each row (`\trowd` … first newline), split at `\cellx`.
fn cell_defs(rtf: &str) -> Vec<Vec<String>> {
    rtf.split("\\trowd")
        .skip(1)
        .map(|r| {
            let defs = r.split('\n').next().unwrap();
            defs.split("\\cellx").map(str::to_string).collect()
        })
        .collect()
}

#[test]
fn merged_cells_are_outlined_once() {
    let t = styled_merge();
    check("styled merge", &t);
    let rtf = render_rtf(&t).unwrap();
    let rows = cell_defs(&rtf);
    // Header, group label, then the two rows the merge spans.
    let sides = |d: &str| ["t", "r", "b", "l"].map(|s| d.contains(&format!("\\clbrdr{s}")));
    assert_eq!(sides(&rows[2][0]), [true, true, false, true], "{rtf}");
    assert_eq!(sides(&rows[3][0]), [false, true, true, true], "{rtf}");
    // The plain cells beside it have no borders.
    assert_eq!(sides(&rows[2][1]), [false; 4], "{rtf}");
    assert_eq!(sides(&rows[3][1]), [false; 4], "{rtf}");
    // 4.5pt is 90 twips, past `\brdrw`'s 75: drawn as `\brdrth` at half.
    assert!(rows[2][0].contains("\\brdrth\\brdrw45\\brdrcf"), "{rtf}");
    // The label's color is registered though nothing else uses it.
    assert!(
        rows[1][0].contains("\\clbrdrt\\brdrdash\\brdrw15\\brdrcf"),
        "{rtf}"
    );
    assert!(rtf.contains("\\red0\\green0\\blue187;"), "{rtf}");
}

#[test]
fn every_row_is_placed_alike() {
    // Every row at RTF's default place (left, not indented), where the title and
    // notes start: a reader that places the table by one row (LibreOffice
    // centers the whole table when a row is `\trqc`) or row by row (Word) puts
    // it in one piece, under its title.
    for ex in examples() {
        let rtf = render_rtf(&ex.table()).unwrap();
        let rows = cell_defs(&rtf);
        assert!(!rows.is_empty(), "{}", ex.name);
        for row in &rows {
            for word in ["\\trq", "\\trleft"] {
                assert!(
                    !row[0].contains(word),
                    "{}: a row placed by {word}: {}",
                    ex.name,
                    row[0]
                );
            }
        }
    }
}

#[test]
fn every_piece_of_a_merged_cell_is_aligned() {
    let rtf = render_rtf(&styled_merge()).unwrap();
    let rows = cell_defs(&rtf);
    // After the merge flag, before the borders (top, left, bottom, right), as
    // the spec orders a cell definition; no other cell is aligned.
    assert!(
        rows[2][0].starts_with("\\clvmgf\\clvertalb\\clbrdrt"),
        "{rtf}"
    );
    assert!(
        rows[3][0].starts_with("\\clvmrg\\clvertalb\\clbrdrl"),
        "{rtf}"
    );
    let aligned = rows
        .iter()
        .flatten()
        .filter(|d| d.contains("\\clvertal"))
        .count();
    assert_eq!(aligned, 2, "{rtf}");
}

#[test]
fn fonts_are_registered_once_with_their_class() {
    let mut t = font_families();
    // Fonts match regardless of case: `georgia` is Georgia's entry.
    t.styles.defs.get_mut("generic").unwrap().font_family = Some("georgia, sans-serif".into());
    let rtf = render_rtf(&t).unwrap();
    check("font-families", &t);
    // In first-use order (the title's mono first); the class comes from the
    // list's generic family, the first style registering a font giving it.
    assert!(
        rtf.contains("{\\fonttbl{\\f0 Arial;}{\\f1\\fmodern Fira Mono;}{\\f2\\froman Georgia;}}"),
        "{rtf}"
    );
    assert!(rtf.contains("{\\f1 c}"), "{rtf}");
    // The table entry, then cells a and d.
    let georgia = indices(&rtf, "\\f").into_iter().filter(|&i| i == 2).count();
    assert_eq!(georgia, 3, "{rtf}");
}
