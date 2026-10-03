//! Structural oracle for RTF: groups balance, and every colour index used (`\cfN`,
//! `\clcbpatN`) exists in the colour table written in the header. The colour table
//! is written before anything that uses it, so a colour first seen late (a styled
//! run, a title, a label, a note) must still have been registered up front.

use gridwell_ir::{StyleDef, Table};
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
    for word in ["\\cf", "\\clcbpat"] {
        for i in indices(&rtf, word) {
            assert!(
                i >= 1 && i <= colors,
                "{name}: {word}{i} but {colors} colours\n{rtf}"
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
            "{word} colour missing\n{rtf}"
        );
    }
    assert!(rtf.contains("{\\b\\cf"), "{rtf}");
}
