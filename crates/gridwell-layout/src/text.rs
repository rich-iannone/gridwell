//! Content as plain text, for formats (or attributes) without inline markup, and
//! with `text_transform` applied, for formats without a text transform of their own.

use std::borrow::Cow;

use gridwell_ir::content::ContentNode;
use gridwell_ir::TextTransform;

/// The text of `nodes`: text and styled text verbatim, footnote marks inline, images
/// as their alt text, line breaks as `line_break`. Raw nodes (format-specific
/// markup) and unknown nodes contribute nothing.
pub fn plain_text(nodes: &[ContentNode], line_break: &str) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value } | ContentNode::StyledText { value, .. } => {
                out.push_str(value)
            }
            ContentNode::LineBreak {} => out.push_str(line_break),
            ContentNode::FootnoteMark { mark_text, .. } => out.push_str(mark_text),
            ContentNode::Image { alt, .. } => out.push_str(alt.as_deref().unwrap_or("")),
            ContentNode::Raw { .. } | ContentNode::Unknown(_) => {}
        }
    }
    out
}

/// `nodes` with `text_transform` applied to their text: each run takes its own
/// style's transform (`run`, given a styled run's style id), else `outer` (the
/// cell's or line's), as CSS inherits it; `none` turns it off. Borrowed when no
/// run is transformed.
///
/// `capitalize` uppercases each word's first letter: a letter with only
/// punctuation since the last whitespace, line break or the start (`(it's)` →
/// `(It's)`, `well-known` → `Well-known`), counted across runs, so a run starting
/// mid-word isn't capitalized. Footnote marks, images and raw markup are kept as
/// written.
pub fn transform_case<'a>(
    nodes: &'a [ContentNode],
    outer: Option<&TextTransform>,
    run: impl Fn(&str) -> Option<TextTransform>,
) -> Cow<'a, [ContentNode]> {
    let transforms: Vec<Option<TextTransform>> = nodes
        .iter()
        .map(|n| match n {
            ContentNode::Text { .. } => outer.cloned(),
            ContentNode::StyledText { style_id, .. } => style_id
                .as_deref()
                .and_then(&run)
                .or_else(|| outer.cloned()),
            _ => None,
        })
        .collect();
    let active = |t: &Option<TextTransform>| t.as_ref().is_some_and(|t| *t != TextTransform::None);
    if !transforms.iter().any(active) {
        return Cow::Borrowed(nodes);
    }
    let mut in_word = false;
    nodes
        .iter()
        .zip(&transforms)
        .map(|(node, t)| match node {
            ContentNode::Text { value } => ContentNode::Text {
                value: case(value, t.as_ref(), &mut in_word),
            },
            ContentNode::StyledText { value, style_id } => ContentNode::StyledText {
                value: case(value, t.as_ref(), &mut in_word),
                style_id: style_id.clone(),
            },
            ContentNode::LineBreak {} => {
                in_word = false;
                node.clone()
            }
            _ => node.clone(),
        })
        .collect()
}

/// The Unicode superscript form of `c` (`2` → `²`): digits, `+ - − = ( )`, and
/// every lowercase ASCII letter but `q`.
pub fn superscript_char(c: char) -> Option<char> {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    const LETTERS: [char; 26] = [
        'ᵃ', 'ᵇ', 'ᶜ', 'ᵈ', 'ᵉ', 'ᶠ', 'ᵍ', 'ʰ', 'ⁱ', 'ʲ', 'ᵏ', 'ˡ', 'ᵐ', 'ⁿ', 'ᵒ', 'ᵖ', 'q', 'ʳ',
        'ˢ', 'ᵗ', 'ᵘ', 'ᵛ', 'ʷ', 'ˣ', 'ʸ', 'ᶻ',
    ];
    Some(match c {
        '0'..='9' => DIGITS[c as usize - '0' as usize],
        'q' => return None,
        'a'..='z' => LETTERS[c as usize - 'a' as usize],
        '+' => '⁺',
        '-' | '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        _ => return None,
    })
}

/// The Unicode subscript form of `c` (`2` → `₂`): digits, `+ - − = ( )`, and
/// the lowercase ASCII letters that have one.
pub fn subscript_char(c: char) -> Option<char> {
    const DIGITS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
    Some(match c {
        '0'..='9' => DIGITS[c as usize - '0' as usize],
        '+' => '₊',
        '-' | '−' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

/// One run's text under transform `t`; `in_word` carries word state across runs.
fn case(s: &str, t: Option<&TextTransform>, in_word: &mut bool) -> String {
    let mut capitalized = String::with_capacity(s.len());
    for c in s.chars() {
        if !*in_word && c.is_alphabetic() && t == Some(&TextTransform::Capitalize) {
            capitalized.extend(c.to_uppercase());
        } else {
            capitalized.push(c);
        }
        if c.is_whitespace() {
            *in_word = false;
        } else if c.is_alphanumeric() {
            *in_word = true;
        }
    }
    match t {
        // Whole strings, for context-dependent mappings (a final sigma).
        Some(TextTransform::Uppercase) => s.to_uppercase(),
        Some(TextTransform::Lowercase) => s.to_lowercase(),
        _ => capitalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_kind() {
        let nodes: Vec<ContentNode> = serde_json::from_value(serde_json::json!([
            { "type": "text", "value": "a " },
            { "type": "styled_text", "value": "b", "style_id": "s" },
            { "type": "footnote_mark", "ref": "f", "mark_text": "1" },
            { "type": "line_break" },
            { "type": "image", "src": "x.png", "alt": "logo" },
            { "type": "image", "src": "y.png" },
            { "type": "raw", "format": "html", "value": "<b>raw</b>" },
            { "type": "something_new" }
        ]))
        .unwrap();
        assert_eq!(plain_text(&nodes, " / "), "a b1 / logo");
        assert_eq!(plain_text(&nodes, "\n"), "a b1\nlogo");
        assert_eq!(plain_text(&[], " "), "");
    }

    fn transformed(nodes: serde_json::Value, outer: Option<TextTransform>) -> Vec<ContentNode> {
        let nodes: Vec<ContentNode> = serde_json::from_value(nodes).unwrap();
        let run = |id: &str| match id {
            "up" => Some(TextTransform::Uppercase),
            "none" => Some(TextTransform::None),
            _ => None,
        };
        transform_case(&nodes, outer.as_ref(), run).into_owned()
    }

    fn texts(nodes: &[ContentNode]) -> Vec<&str> {
        nodes
            .iter()
            .filter_map(|n| match n {
                ContentNode::Text { value } | ContentNode::StyledText { value, .. } => {
                    Some(value.as_str())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn transforms_inherit_and_runs_override() {
        let nodes = serde_json::json!([
            { "type": "text", "value": "straße " },
            { "type": "styled_text", "value": "keep", "style_id": "none" },
            { "type": "styled_text", "value": " plain", "style_id": "other" },
            { "type": "footnote_mark", "ref": "f", "mark_text": "a" }
        ]);
        let out = transformed(nodes.clone(), Some(TextTransform::Uppercase));
        assert_eq!(texts(&out), ["STRASSE ", "keep", " PLAIN"]);
        assert!(matches!(&out[3], ContentNode::FootnoteMark { mark_text, .. } if mark_text == "a"));
        // A run's own transform applies without the outer one.
        let nodes = serde_json::json!([
            { "type": "text", "value": "a " },
            { "type": "styled_text", "value": "b", "style_id": "up" }
        ]);
        assert_eq!(texts(&transformed(nodes, None)), ["a ", "B"]);
        let lower = serde_json::json!([{ "type": "text", "value": "ΟΔΟΣ ΑΒ" }]);
        assert_eq!(
            texts(&transformed(lower, Some(TextTransform::Lowercase))),
            ["οδος αβ"]
        );
    }

    #[test]
    fn capitalize_finds_word_starts_across_runs() {
        let nodes = serde_json::json!([
            { "type": "text", "value": "(it's) well-known 2nd" },
            { "type": "line_break" },
            { "type": "text", "value": "new line, ab" },
            { "type": "styled_text", "value": "cd ef", "style_id": "other" }
        ]);
        let out = transformed(nodes, Some(TextTransform::Capitalize));
        assert_eq!(
            texts(&out),
            ["(It's) Well-known 2nd", "New Line, Ab", "cd Ef"]
        );
    }

    #[test]
    fn untransformed_content_is_borrowed() {
        let nodes: Vec<ContentNode> = serde_json::from_value(serde_json::json!([
            { "type": "text", "value": "a" },
            { "type": "styled_text", "value": "b", "style_id": "none" }
        ]))
        .unwrap();
        let none = |_: &str| Some(TextTransform::None);
        assert!(matches!(
            transform_case(&nodes, None, none),
            Cow::Borrowed(_)
        ));
        let off = Some(TextTransform::None);
        assert!(matches!(
            transform_case(&nodes, off.as_ref(), |_| None),
            Cow::Borrowed(_)
        ));
        let up = Some(TextTransform::Uppercase);
        assert!(matches!(
            transform_case(&nodes, up.as_ref(), none),
            Cow::Owned(_)
        ));
    }
}
