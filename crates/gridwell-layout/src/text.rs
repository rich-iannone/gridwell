//! Content as plain text, for formats (or attributes) without inline markup.

use gridwell_ir::content::ContentNode;

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
            ContentNode::Raw { .. } | ContentNode::Unknown => {}
        }
    }
    out
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
}
