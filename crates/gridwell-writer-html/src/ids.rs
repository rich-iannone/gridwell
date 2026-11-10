//! Element ids, made as Great Tables makes them: valid, unique within the table,
//! and prefixed by the table's id when it has one (`table_id`), so they are also
//! unique on a page holding several tables.
//!
//! Header cells (the stubhead, column labels, spanners, row-group labels, stub
//! cells and summary-row labels) and the title and subtitle get ids, and each
//! body cell lists its headers' ids in its `headers` attribute.

use std::collections::HashSet;

use gridwell_ir::content::ContentNode;

/// Makes ids that are valid and unique within a table. A repeated id gets a
/// numeric suffix (`num`, `num-2`, `num-3`).
pub struct IdRegistry {
    /// `{table_id}-` (sanitized), or nothing.
    pub prefix: String,
    used: HashSet<String>,
}

impl IdRegistry {
    /// A registry for a table with id `table_id`; `reserved` ids (the
    /// footnotes') are never made. Nor is the table's own: an id made is its prefix
    /// and more, or, when the table's id sanitizes to nothing, unlike it, sanitized.
    pub fn new<'s>(table_id: Option<&str>, reserved: impl IntoIterator<Item = &'s str>) -> Self {
        let prefix = table_id.map(sanitize_id).unwrap_or_default();
        let prefix = if prefix.is_empty() {
            prefix
        } else {
            format!("{prefix}-")
        };
        let used = reserved.into_iter().map(str::to_string).collect();
        Self { prefix, used }
    }

    /// A new id from `base` (any text), else `fallback` when nothing of it is left.
    pub fn make(&mut self, base: &str, fallback: &str) -> String {
        let name = match sanitize_id(base) {
            n if n.is_empty() => fallback.to_string(),
            n => n,
        };
        let mut candidate = format!("{}{name}", self.prefix);
        let mut n = 2;
        while self.used.contains(&candidate) {
            candidate = format!("{}{name}-{n}", self.prefix);
            n += 1;
        }
        self.used.insert(candidate.clone());
        candidate
    }
}

/// The CSS selector for the element with id `id` (any text): `#` and the id,
/// escaped as `CSS.escape` does, except that every ASCII character other than
/// letters, digits, `_` and `-` is a hex escape, so the selector can't close the
/// `<style>` element it's written into.
pub fn id_selector(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 1);
    out.push('#');
    for (i, c) in id.chars().enumerate() {
        // An ident can't start with a digit, or `-` and a digit, or be just `-`.
        let leading_digit = c.is_ascii_digit() && (i == 0 || (i == 1 && id.starts_with('-')));
        let plain = c.is_ascii_alphanumeric() || c == '_' || c == '-' || !c.is_ascii();
        if c == '\0' {
            out.push('\u{FFFD}');
        } else if leading_digit || id == "-" || !plain {
            out.push_str(&format!("\\{:x} ", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// Text as an id: each run of characters other than word characters (letters,
/// digits, `_`), `.`, `:` and `-` becomes one `-`, and leading and trailing `-`
/// go. The result needs no escaping in an attribute.
pub fn sanitize_id(text: &str) -> String {
    let keep = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '-');
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if keep(c) {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// The text of a label, for its id: its text runs, with raw HTML's tags removed
/// and its character references decoded. Footnote marks and images are left out.
pub fn id_text(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. } => {
                out.push_str(value)
            }
            ContentNode::LineBreak {} => out.push(' '),
            ContentNode::Raw { format, value } if format == "html" => {
                out.push_str(&unescape_html(&strip_tags(value)))
            }
            _ => {}
        }
    }
    out
}

/// Whether a label shows nothing (no nodes, or only blank text and line breaks),
/// so it labels nothing and gets no id.
pub fn is_blank(nodes: &[ContentNode]) -> bool {
    nodes.iter().all(|node| match node {
        ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. } => {
            value.trim().is_empty()
        }
        ContentNode::LineBreak {} => true,
        _ => false,
    })
}

/// `html` without its tags (`<…>`).
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        match rest[open..].find('>') {
            // `<>` is not a tag.
            Some(1) | None => {
                out.push('<');
                rest = &rest[open + 1..];
            }
            Some(close) => rest = &rest[open + close + 1..],
        }
    }
    out.push_str(rest);
    out
}

/// `text` with its character references decoded: the numeric ones and the few
/// named ones markup commonly holds. Any other `&` is kept as it is.
fn unescape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => {
                    let n = match name.strip_prefix('#') {
                        Some(hex) if hex.starts_with(['x', 'X']) => {
                            u32::from_str_radix(&hex[1..], 16).ok()
                        }
                        Some(dec) => dec.parse().ok(),
                        None => None,
                    };
                    n.and_then(char::from_u32)
                }
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `html` with the ids defined in each of its SVGs prefixed (`{prefix}-{id}`, the
/// prefix sanitized), along with the references to them in the SVG (`url(#…)`,
/// `href="#…"`): cells holding the same SVG (a flag's clip path, a plot's fill
/// pattern) then don't repeat an id, and a reference can't resolve to another
/// cell's definition.
pub fn prefix_svg_ids(html: &str, prefix: &str) -> String {
    let prefix = sanitize_id(prefix);
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = svg_start(rest) {
        let Some(len) = rest[start..].find("</svg>") else {
            break;
        };
        let end = start + len + "</svg>".len();
        out.push_str(&rest[..start]);
        out.push_str(&prefix_ids(&rest[start..end], &prefix));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Where the first `<svg` tag starts (not `<svgfoo`).
fn svg_start(html: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = html[from..].find("<svg") {
        let at = from + i;
        let next = html[at + 4..].chars().next();
        if !next.is_some_and(is_word_char) {
            return Some(at);
        }
        from = at + 4;
    }
    None
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The value of an attribute or reference that starts `open` at the start of `s`
/// and ends at `close`, and the length of the whole.
fn quoted<'s>(s: &'s str, open: &str, close: char) -> Option<(&'s str, usize)> {
    let body = s.strip_prefix(open)?;
    let end = body.find(close)?;
    Some((&body[..end], open.len() + end + close.len_utf8()))
}

/// One SVG with the ids it defines (`id="…"`) and the references to them prefixed.
fn prefix_ids(svg: &str, prefix: &str) -> String {
    let mut ids = HashSet::new();
    let mut prev = None;
    for (i, c) in svg.char_indices() {
        if c == 'i' && !prev.is_some_and(is_word_char) {
            if let Some((id, _)) = quoted(&svg[i..], "id=\"", '"') {
                if !id.is_empty() {
                    ids.insert(id);
                }
            }
        }
        prev = Some(c);
    }
    if ids.is_empty() {
        return svg.to_string();
    }
    let mut out = String::with_capacity(svg.len() + ids.len() * (prefix.len() + 1));
    let mut prev = None;
    let mut i = 0;
    while i < svg.len() {
        let s = &svg[i..];
        let id_attr = if prev.is_some_and(is_word_char) {
            None
        } else {
            quoted(s, "id=\"", '"').map(|(id, n)| ("id=\"", id, "\"", n))
        };
        let hit = id_attr
            .or_else(|| quoted(s, "url(#", ')').map(|(id, n)| ("url(#", id, ")", n)))
            .or_else(|| quoted(s, "href=\"#", '"').map(|(id, n)| ("href=\"#", id, "\"", n)))
            .filter(|(_, id, _, _)| ids.contains(id));
        if let Some((open, id, close, n)) = hit {
            out.push_str(&format!("{open}{prefix}-{id}{close}"));
            prev = close.chars().last();
            i += n;
            continue;
        }
        let c = s.chars().next().expect("not at the end");
        out.push(c);
        prev = Some(c);
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sanitized_text() {
        assert_eq!(sanitize_id("Sales (2024)"), "Sales-2024");
        assert_eq!(sanitize_id("  a  b  "), "a-b");
        assert_eq!(sanitize_id("x.y:z-w_v"), "x.y:z-w_v");
        assert_eq!(sanitize_id("Größe €"), "Größe");
        assert_eq!(sanitize_id("\"><script>"), "script");
        assert_eq!(sanitize_id("!!!"), "");
    }

    #[test]
    fn repeated_ids_get_a_suffix() {
        let mut r = IdRegistry::new(None, ["fn1"]);
        assert_eq!(r.make("num", "column"), "num");
        assert_eq!(r.make("num", "column"), "num-2");
        assert_eq!(r.make("num", "column"), "num-3");
        assert_eq!(r.make("num-2", "column"), "num-2-2");
        assert_eq!(r.make("", "column"), "column");
        assert_eq!(r.make("?", "column"), "column-2");
        assert_eq!(r.make("fn1", "column"), "fn1-2");
    }

    #[test]
    fn a_table_id_prefixes_every_id() {
        let mut r = IdRegistry::new(Some("my table"), []);
        assert_eq!(r.prefix, "my-table-");
        assert_eq!(r.make("title", "title"), "my-table-title");
        assert_eq!(r.make("title", "title"), "my-table-title-2");
        let mut r = IdRegistry::new(Some("t-a"), []);
        assert_eq!(r.prefix, "t-a-");
        assert_eq!(r.make("a", "column"), "t-a-a");
        let mut r = IdRegistry::new(Some("!"), []);
        assert_eq!(r.prefix, "");
        assert_eq!(r.make("a", "column"), "a");
    }

    #[test]
    fn id_selectors_escape_what_an_ident_cannot_hold() {
        assert_eq!(id_selector("sales-2024"), "#sales-2024");
        assert_eq!(id_selector("1a"), "#\\31 a");
        assert_eq!(id_selector("-1"), "#-\\31 ");
        assert_eq!(id_selector("a1"), "#a1");
        assert_eq!(id_selector("-"), "#\\2d ");
        assert_eq!(id_selector("--x"), "#--x");
        assert_eq!(id_selector("a.b:c"), "#a\\2e b\\3a c");
        assert_eq!(id_selector("é€"), "#é€");
        assert_eq!(id_selector("a\0b"), "#a\u{FFFD}b");
        assert_eq!(id_selector("</style>"), "#\\3c \\2f style\\3e ");
    }

    #[test]
    fn label_text_leaves_out_markup_and_marks() {
        let nodes: Vec<ContentNode> = serde_json::from_value(serde_json::json!([
            { "type": "text", "value": "Mean" },
            { "type": "line_break" },
            { "type": "raw", "format": "html", "value": "<b>&lt;5&gt;</b> &#956;g&#x2F;L &bogus; a<>b" },
            { "type": "raw", "format": "latex", "value": "\\textbf{x}" },
            { "type": "footnote_mark", "ref": "f", "mark_text": "1" },
        ]))
        .unwrap();
        assert_eq!(id_text(&nodes), "Mean <5> μg/L &bogus; a<>b");
    }

    #[test]
    fn svg_ids_are_prefixed_with_their_references() {
        let html = r##"<span id="keep"><svg viewBox="0 0 1 1"><defs><clipPath id="c"><rect/></clipPath><linearGradient id="g"/></defs><rect clip-path="url(#c)" fill="url(#g)" data-id="d"/><use xlink:href="#c"/><use href="#other"/><a href="#g"/></svg></span><svg><g id="c"/></svg><svgx id="n"></svgx>"##;
        assert_eq!(
            prefix_svg_ids(html, "t a-x-0"),
            r##"<span id="keep"><svg viewBox="0 0 1 1"><defs><clipPath id="t-a-x-0-c"><rect/></clipPath><linearGradient id="t-a-x-0-g"/></defs><rect clip-path="url(#t-a-x-0-c)" fill="url(#t-a-x-0-g)" data-id="t-a-x-0-d"/><use xlink:href="#t-a-x-0-c"/><use href="#other"/><a href="#t-a-x-0-g"/></svg></span><svg><g id="t-a-x-0-c"/></svg><svgx id="n"></svgx>"##
        );
        // No SVG, or one without ids, or one left open: unchanged.
        for html in ["<b id=\"x\">x</b>", "<svg><rect/></svg>", "<svg id=\"x\">"] {
            assert_eq!(prefix_svg_ids(html, "p"), html);
        }
    }
}
