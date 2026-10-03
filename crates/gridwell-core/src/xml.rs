//! XML text escaping shared by the XML-based writers (DOCX, PPTX, XLSX, SVG).

/// Escape `s` for XML text or a double- or single-quoted attribute, and drop the
/// characters XML 1.0 forbids even as references: C0 controls other than tab,
/// newline and carriage return, and the non-characters U+FFFE and U+FFFF. (A
/// single such character makes Word, Excel and PowerPoint refuse the file.)
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if !is_xml_char(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// The XML 1.0 `Char` production (Rust `char`s are never surrogates).
pub fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{FFFD}' | '\u{10000}'..)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_and_quotes() {
        assert_eq!(
            escape(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn drops_characters_xml_forbids() {
        assert_eq!(escape("a\u{0}b\u{1}c\u{8}d\u{b}e\u{c}f\u{1f}g"), "abcdefg");
        assert_eq!(escape("x\u{FFFE}y\u{FFFF}z"), "xyz");
        assert_eq!(escape("t\tn\nr\r"), "t\tn\nr\r");
        assert_eq!(escape("é😀\u{FFFD}\u{E000}"), "é😀\u{FFFD}\u{E000}");
        // DEL and C1 controls are legal XML 1.0 characters.
        assert_eq!(escape("\u{7f}\u{85}"), "\u{7f}\u{85}");
    }
}
