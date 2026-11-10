//! `color: false` is plain text: no escape codes at all, and otherwise exactly
//! the colored output — same characters, widths and layout — so a `.txt` file of a
//! table matches what a terminal shows, minus the styling.

use gridwell_testkit::examples;
use gridwell_writer_ansi::{AnsiConfig, AnsiWriter};

/// Remove the escapes the writer emits: SGR sequences (`ESC [ … m`) and OSC 8
/// hyperlinks (`ESC ] 8 ; ; URL ESC \\`).
fn strip_escapes(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            for d in chars.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else if c == '\x1b' && chars.peek() == Some(&']') {
            while let Some(d) = chars.next() {
                if d == '\x1b' && chars.next_if_eq(&'\\').is_some() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn color_off_is_the_colored_output_without_escapes() {
    let mut styled = 0;
    for ex in examples() {
        let table = ex.table();
        for (box_drawing, background_colors) in [(true, false), (true, true), (false, false)] {
            let config = |color| AnsiConfig {
                box_drawing,
                background_colors,
                color,
                ..AnsiConfig::default()
            };
            let colored = AnsiWriter::with_config(config(true))
                .render(&table)
                .unwrap();
            let plain = AnsiWriter::with_config(config(false))
                .render(&table)
                .unwrap();
            assert!(
                !plain.contains('\x1b'),
                "{}: escape in plain output:\n{plain}",
                ex.name
            );
            assert_eq!(plain, strip_escapes(&colored), "{}", ex.name);
            styled += usize::from(colored != plain);
        }
    }
    // The comparison means something only if most examples have styling.
    assert!(styled > 50, "only {styled} styled outputs");
}

#[test]
fn color_is_on_by_default() {
    assert!(AnsiConfig::default().color);
}
