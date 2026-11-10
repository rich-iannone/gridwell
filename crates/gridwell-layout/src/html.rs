//! `raw` HTML as content every format can draw.
//!
//! A `raw` node is written only by the format it names. HTML is the markup table
//! content is most often written in (gt's and Great Tables' `html()` and `md()`
//! produce it), and other writers would draw nothing for it; [`lower`] turns its
//! markup into the content they all draw: text, styled runs, line breaks and
//! images.
//!
//! - **Inline formatting.** `b`/`strong`, `i`/`em`/`cite`/`var`/`dfn`,
//!   `u`/`ins`, `s`/`strike`/`del`, `code`/`kbd`/`samp`/`tt` (monospace),
//!   `small`/`big`, `mark`, `q` (quotes), `font` (`color`, `face`), headings and
//!   `th` (bold), and these properties of a `style` attribute: `color`,
//!   `background-color`, `font-weight`, `font-style`, `font-size`, `font-family`,
//!   `text-decoration` and `text-transform`. One decoration line is drawn: the
//!   innermost.
//! - **Superscripts and subscripts** (`sup`, `sub`) in Unicode characters
//!   (`m<sup>2</sup>` → `m²`), when every character has one; as written otherwise.
//! - **Lines.** `br`, and the edges of blocks (`p`, `div`, `li`, `tr`, headings,
//!   …), break lines; list items start with a bullet, or their number in an `ol`;
//!   `pre` keeps its spaces and line breaks. Elsewhere, white space collapses as
//!   in a browser.
//! - **Links.** The text of an `a` whose `href` is a link (an `http`, `https`
//!   or `mailto` URL; see [`gridwell_core::link`]) links to it. Any other `href`
//!   (`javascript:`, a relative path) leaves the text as it is.
//! - **Images.** An `img` becomes an image node with its `src`, `alt`, `width`
//!   and `height`. Writers draw only `data:` URIs (see the schema docs); nothing
//!   is fetched.
//! - **Everything else keeps its text** (unknown tags), except what a
//!   browser doesn't show: comments, `script`, `style`, `head`, `title`,
//!   `template`, `svg`, and elements with `hidden` or `display: none`.
//! - Character references (named, decimal and hexadecimal) are decoded.
//!
//! Lowering never fails or panics, on any input, and takes linear time.
//!
//! A lowered run's style id is [`STYLE_PREFIX`] followed by its CSS declarations
//! (`\0html:font-weight:bold;`); [`style_def`] reads it back, so a writer resolves
//! it like any other id ([`ResolvedTable::style`](crate::ResolvedTable::style)).
//!
//! For IR you don't trust, [`without_raw`] leaves no raw markup to write as it
//! is: it lowers raw HTML for every format, HTML included, and drops raw markup
//! for any other.

use std::borrow::Cow;

use gridwell_ir::cell::Row;
use gridwell_ir::content::ContentNode;
use gridwell_ir::style::StyleDef;
use gridwell_ir::{FontStyle, FontWeight, Table, TextDecoration, TextTransform};

use crate::text::{subscript_char, superscript_char};

/// The start of a lowered run's style id. A NUL can't be typed in an id by
/// accident, so the ids don't meet the table's own.
pub const STYLE_PREFIX: &str = "\u{0}html:";

/// Deeper elements are read as text with their parent's style.
const MAX_DEPTH: usize = 128;
/// Attributes of a tag past these are ignored.
const MAX_ATTRS: usize = 64;

/// `content` with each `raw` HTML node turned into text, styled runs, line breaks
/// and images (see the module docs). Borrowed when there is none, or when the
/// content also has raw markup for another format: then it holds a version for
/// each format, and a writer takes its own.
pub fn lower(content: &[ContentNode]) -> Cow<'_, [ContentNode]> {
    let raw_in = |html: bool| {
        content
            .iter()
            .any(|n| matches!(n, ContentNode::Raw { format, .. } if is_html(format) == html))
    };
    if !raw_in(true) || raw_in(false) {
        return Cow::Borrowed(content);
    }
    let mut out: Vec<ContentNode> = Vec::with_capacity(content.len());
    for (i, node) in content.iter().enumerate() {
        match node {
            ContentNode::Raw { format, value } if is_html(format) => {
                let line_start = matches!(out.last(), None | Some(ContentNode::LineBreak {}));
                let space = matches!(
                    out.last(),
                    Some(ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. })
                        if value.ends_with(is_space)
                );
                let at_end = matches!(content.get(i + 1), None | Some(ContentNode::LineBreak {}));
                let mut l = Lowerer::new(&mut out, line_start, space);
                l.run(value);
                l.finish(at_end);
            }
            _ => out.push(node.clone()),
        }
    }
    Cow::Owned(out)
}

/// `table` with no `raw` node left, for IR you don't trust: raw HTML lowered
/// (as [`lower`] lowers it, whatever other raw markup the content holds), raw
/// markup for any other format dropped. Borrowed when the table has no raw node.
/// Every writer takes the result as it takes any table: a lowered run's style id
/// resolves as [`style_def`] reads it.
pub fn without_raw(table: &Table) -> Cow<'_, Table> {
    let is_raw = |n: &ContentNode| matches!(n, ContentNode::Raw { .. });
    if !contents(table).any(|c| c.iter().any(is_raw)) {
        return Cow::Borrowed(table);
    }
    let mut out = table.clone();
    for content in contents_mut(&mut out) {
        if content.iter().any(is_raw) {
            content.retain(|n| !matches!(n, ContentNode::Raw { format, .. } if !is_html(format)));
            if let Cow::Owned(lowered) = lower(content) {
                *content = lowered;
            }
        }
    }
    Cow::Owned(out)
}

/// Every content list of a table: header lines, cells, group labels and notes.
fn contents(t: &Table) -> impl Iterator<Item = &[ContentNode]> {
    fn cells(rows: &[Row]) -> impl Iterator<Item = &[ContentNode]> {
        rows.iter()
            .flat_map(|r| r.cells.iter().map(|c| &c.content[..]))
    }
    let header = t.header.iter().flat_map(|h| {
        let lines = h.title.iter().chain(&h.subtitle).chain(&h.extra_lines);
        lines.map(|l| &l.content[..])
    });
    let head = cells(&t.table.thead.rows);
    let body = t.table.tbody.iter().flat_map(move |g| {
        let label = g.label.iter().map(|l| &l.content[..]);
        label.chain(cells(&g.rows)).chain(cells(&g.summary_rows))
    });
    let footer = t.footer.iter().flat_map(|f| {
        let notes = f.footnotes.iter().map(|n| &n.content[..]);
        notes.chain(f.source_notes.iter().map(|n| &n.content[..]))
    });
    header.chain(head).chain(body).chain(footer)
}

/// [`contents`], to change.
fn contents_mut(t: &mut Table) -> impl Iterator<Item = &mut Vec<ContentNode>> {
    fn cells(rows: &mut [Row]) -> impl Iterator<Item = &mut Vec<ContentNode>> {
        rows.iter_mut()
            .flat_map(|r| r.cells.iter_mut().map(|c| &mut c.content))
    }
    let header = t.header.iter_mut().flat_map(|h| {
        let lines = h
            .title
            .iter_mut()
            .chain(&mut h.subtitle)
            .chain(&mut h.extra_lines);
        lines.map(|l| &mut l.content)
    });
    let head = cells(&mut t.table.thead.rows);
    let body = t.table.tbody.iter_mut().flat_map(|g| {
        let label = g.label.iter_mut().map(|l| &mut l.content);
        label
            .chain(cells(&mut g.rows))
            .chain(cells(&mut g.summary_rows))
    });
    let footer = t.footer.iter_mut().flat_map(|f| {
        let notes = f.footnotes.iter_mut().map(|n| &mut n.content);
        notes.chain(f.source_notes.iter_mut().map(|n| &mut n.content))
    });
    header.chain(head).chain(body).chain(footer)
}

/// The style a lowered run's id (`\0html:` and declarations) describes; `None`
/// for any other id.
pub fn style_def(id: &str) -> Option<StyleDef> {
    let decls = id.strip_prefix(STYLE_PREFIX)?;
    let mut css = Css::default();
    css.declare(decls);
    Some(css.to_def())
}

fn is_html(format: &str) -> bool {
    format.eq_ignore_ascii_case("html")
}

/// HTML's white space (not U+00A0, which doesn't collapse).
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

// ─── Styles ───

/// The CSS properties a run can carry, as written. Values never hold `;`.
#[derive(Debug, Clone, Default, PartialEq)]
struct Css {
    font_weight: Option<String>,
    font_style: Option<String>,
    font_size: Option<String>,
    font_family: Option<String>,
    color: Option<String>,
    background_color: Option<String>,
    text_decoration: Option<String>,
    text_transform: Option<String>,
}

impl Css {
    fn slot(&mut self, prop: &str) -> Option<&mut Option<String>> {
        Some(match prop {
            "font-weight" => &mut self.font_weight,
            "font-style" => &mut self.font_style,
            "font-size" => &mut self.font_size,
            "font-family" => &mut self.font_family,
            "color" => &mut self.color,
            "background-color" => &mut self.background_color,
            "text-decoration" | "text-decoration-line" => &mut self.text_decoration,
            "text-transform" => &mut self.text_transform,
            _ => return None,
        })
    }

    fn set(&mut self, prop: &str, value: &str) {
        let value = value.trim();
        if value.is_empty() || value.contains([';', '\0']) {
            return;
        }
        if let Some(slot) = self.slot(prop) {
            *slot = Some(value.to_string());
        }
    }

    /// Apply declarations (`color: red; font-weight: bold`); unknown properties
    /// are ignored, a later declaration wins. Whether one says `display: none`.
    fn declare(&mut self, decls: &str) -> bool {
        let mut hidden = false;
        for decl in decls.split(';') {
            let Some((prop, value)) = decl.split_once(':') else {
                continue;
            };
            let prop = prop.trim().to_ascii_lowercase();
            let value = value.trim();
            let value = value
                .strip_suffix("!important")
                .map_or(value, str::trim_end);
            if prop == "display" {
                hidden = value.eq_ignore_ascii_case("none");
            } else {
                self.set(&prop, value);
            }
        }
        hidden
    }

    /// The run's style id: `None` for no style.
    fn id(&self) -> Option<String> {
        let props = [
            ("font-weight", &self.font_weight),
            ("font-style", &self.font_style),
            ("font-size", &self.font_size),
            ("font-family", &self.font_family),
            ("color", &self.color),
            ("background-color", &self.background_color),
            ("text-decoration", &self.text_decoration),
            ("text-transform", &self.text_transform),
        ];
        let mut id = String::from(STYLE_PREFIX);
        for (prop, value) in props {
            if let Some(v) = value {
                id.push_str(prop);
                id.push(':');
                id.push_str(v);
                id.push(';');
            }
        }
        (id.len() > STYLE_PREFIX.len()).then_some(id)
    }

    fn to_def(&self) -> StyleDef {
        // The decoration line among the values (`underline dotted red`).
        let decoration = self.text_decoration.as_deref().and_then(|v| {
            v.split_whitespace()
                .map(TextDecoration::parse)
                .find(|d| !matches!(d, TextDecoration::Unknown(_)))
        });
        StyleDef {
            font_weight: self.font_weight.as_deref().map(FontWeight::parse),
            font_style: self.font_style.as_deref().map(FontStyle::parse),
            font_size: self.font_size.clone(),
            font_family: self.font_family.clone(),
            color: self.color.clone(),
            background_color: self.background_color.clone(),
            text_decoration: decoration,
            text_transform: self.text_transform.as_deref().map(TextTransform::parse),
            ..StyleDef::default()
        }
    }
}

// ─── Elements ───

/// What a browser draws nothing of.
const HIDDEN: &[&str] = &["head", "template", "svg"];
/// Elements whose content is text up to their end tag, never shown.
const RAW_TEXT: &[&str] = &["script", "style", "title", "textarea", "xmp", "noembed"];
/// Elements with no content (and no end tag).
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Elements on lines of their own.
const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "caption",
    "center",
    "dd",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "legend",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "tfoot",
    "thead",
    "tr",
    "ul",
];
/// Elements a second one of closes (`<li>a<li>b`).
const SELF_CLOSING: &[&str] = &["p", "li", "dt", "dd", "tr", "td", "th", "option"];

/// The style an element itself gives its content.
fn element_css(name: &str, attrs: &[(String, String)], css: &mut Css) {
    let (prop, value) = match name {
        "b" | "strong" | "th" => ("font-weight", "bold"),
        "i" | "em" | "cite" | "var" | "dfn" => ("font-style", "italic"),
        "u" | "ins" => ("text-decoration", "underline"),
        "s" | "strike" | "del" => ("text-decoration", "line-through"),
        "code" | "kbd" | "samp" | "tt" | "pre" => ("font-family", "monospace"),
        "small" => ("font-size", "smaller"),
        "big" => ("font-size", "larger"),
        "mark" => {
            css.set("color", "black");
            ("background-color", "yellow")
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let size = ["2em", "1.5em", "1.17em", "1em", "0.83em", "0.67em"]
                [usize::from(name.as_bytes()[1] - b'1')];
            css.set("font-size", size);
            ("font-weight", "bold")
        }
        "font" => {
            for (attr, prop) in [("color", "color"), ("face", "font-family")] {
                if let Some(v) = attr_value(attrs, attr) {
                    css.set(prop, v);
                }
            }
            return;
        }
        _ => return,
    };
    css.set(prop, value);
}

fn attr_value<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// An image dimension: a bare number is in pixels, as in HTML.
fn dimension(v: &str) -> String {
    let v = v.trim();
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
        format!("{v}px")
    } else {
        v.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Script {
    Super,
    Sub,
}

/// An open element.
struct Open {
    name: String,
    /// The style of its content (its ancestors' and its own).
    css: Css,
    id: Option<String>,
    /// The link of its content: its own, if it is an `a`, or its parent's.
    href: Option<String>,
    hidden: bool,
    pre: bool,
    /// A list: `Some(n)` for an `ol` (the last number given), `None` for a `ul`.
    list: Option<Option<u32>>,
    /// A `sup` or `sub`, and where its content starts in the output.
    script: Option<(Script, usize)>,
}

struct Lowerer<'o> {
    out: &'o mut Vec<ContentNode>,
    /// The first node of this raw markup's: earlier ones are left as they are.
    base: usize,
    stack: Vec<Open>,
    /// Elements open beyond [`MAX_DEPTH`], not tracked.
    untracked: usize,
    /// Nothing drawn on the current line yet.
    line_start: bool,
    /// The last character drawn is a collapsible space.
    space: bool,
    /// A block edge was passed: the next thing drawn starts a line.
    pending_break: bool,
    /// The next text starts a run of its own.
    split: bool,
    /// Right after `<pre>`, whose first line break is dropped.
    pre_start: bool,
}

impl<'o> Lowerer<'o> {
    fn new(out: &'o mut Vec<ContentNode>, line_start: bool, space: bool) -> Self {
        let base = out.len();
        Self {
            out,
            base,
            stack: Vec::new(),
            untracked: 0,
            line_start,
            space,
            pending_break: false,
            split: false,
            pre_start: false,
        }
    }

    fn top(&self) -> Option<&Open> {
        self.stack.last()
    }

    fn hidden(&self) -> bool {
        self.top().is_some_and(|o| o.hidden)
    }

    fn run(&mut self, html: &str) {
        // ASCII lowercase keeps every byte where it was: for finding end tags.
        let lower = html.to_ascii_lowercase();
        let bytes = html.as_bytes();
        let mut i = 0;
        while i < html.len() {
            let Some(lt) = html[i..].find('<').map(|p| i + p) else {
                self.text(&html[i..]);
                break;
            };
            self.text(&html[i..lt]);
            i = lt;
            let next = bytes.get(i + 1).copied();
            match next {
                Some(b'!') if html[i..].starts_with("<!--") => {
                    i = html[i + 4..]
                        .find("-->")
                        .map_or(html.len(), |p| i + 4 + p + 3);
                }
                Some(b'!' | b'?') => i = skip_past(html, i, b'>'),
                Some(b'/') if bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) => {
                    let (name, end) = tag_name(html, i + 2);
                    let name = name.to_ascii_lowercase();
                    i = skip_past(html, end, b'>');
                    self.end_tag(&name);
                }
                Some(b'/') => i = skip_past(html, i, b'>'),
                Some(c) if c.is_ascii_alphabetic() => {
                    let Some((name, attrs, end)) = start_tag(html, i + 1) else {
                        // A tag cut off by the end of the markup: a browser
                        // shows none of it.
                        break;
                    };
                    i = end;
                    if RAW_TEXT.contains(&name.as_str()) {
                        let close = format!("</{name}");
                        i = lower[i..].find(&close).map_or(html.len(), |p| i + p);
                    } else {
                        self.start_tag(&name, &attrs);
                    }
                }
                _ => {
                    self.text("<");
                    i += 1;
                }
            }
        }
    }

    fn start_tag(&mut self, name: &str, attrs: &[(String, String)]) {
        if SELF_CLOSING.contains(&name) {
            // Within the same list or table only.
            let open = self
                .stack
                .iter()
                .rev()
                .take_while(|o| o.list.is_none() && o.name != "table")
                .position(|o| o.name == name);
            if let Some(p) = open {
                self.close(self.stack.len() - 1 - p);
            }
        }
        let block = BLOCK.contains(&name);
        if block {
            self.pending_break = true;
        }
        match name {
            "br" => {
                if !self.hidden() {
                    self.flush_break();
                    self.line_break();
                }
                return;
            }
            "img" => {
                if !self.hidden() {
                    self.image(attrs);
                }
                return;
            }
            "td" | "th" => self.whitespace(),
            "li" => self.list_marker(),
            _ => {}
        }
        if VOID.contains(&name) {
            return;
        }
        if self.stack.len() >= MAX_DEPTH {
            self.untracked += 1;
            return;
        }
        let parent = self.top();
        let mut css = parent.map(|o| o.css.clone()).unwrap_or_default();
        element_css(name, attrs, &mut css);
        let mut hidden = parent.is_some_and(|o| o.hidden)
            || HIDDEN.contains(&name)
            || attr_value(attrs, "hidden").is_some();
        if let Some(style) = attr_value(attrs, "style") {
            hidden |= css.declare(style);
        }
        let pre = parent.is_some_and(|o| o.pre) || name == "pre";
        let href = if name == "a" {
            attr_value(attrs, "href")
                .map(str::trim)
                .filter(|h| gridwell_core::link::is_link(h))
                .map(str::to_string)
        } else {
            parent.and_then(|o| o.href.clone())
        };
        let script = match name {
            "sup" => Some(Script::Super),
            "sub" => Some(Script::Sub),
            _ => None,
        }
        .map(|s| (s, self.out.len()));
        if script.is_some() {
            self.split = true;
        }
        self.pre_start = name == "pre";
        let list = match name {
            "ol" => Some(Some(
                attr_value(attrs, "start")
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(1)
                    .saturating_sub(1),
            )),
            "ul" | "menu" => Some(None),
            _ => None,
        };
        let id = css.id();
        self.stack.push(Open {
            name: name.to_string(),
            css,
            id,
            href,
            hidden,
            pre,
            list,
            script,
        });
        if name == "q" {
            self.text_raw("\u{201C}");
        }
    }

    fn end_tag(&mut self, name: &str) {
        if BLOCK.contains(&name) {
            self.pending_break = true;
        }
        if self.untracked > 0 {
            // An end tag inside untracked elements closes one of them, as far as
            // the depth goes.
            self.untracked -= 1;
            return;
        }
        if let Some(at) = self.stack.iter().rposition(|o| o.name == name) {
            if name == "q" && !self.hidden() {
                self.text_raw("\u{201D}");
            }
            self.close(at);
        }
    }

    /// Close the elements from `at` up, innermost first.
    fn close(&mut self, at: usize) {
        while self.stack.len() > at {
            let open = self.stack.pop().expect("stack is longer than `at`");
            if let Some((script, start)) = open.script {
                self.scripted(script, start);
                self.split = true;
            }
        }
    }

    /// Redraw the runs from `start` in superscript or subscript characters, if
    /// every character has one.
    fn scripted(&mut self, script: Script, start: usize) {
        let map = match script {
            Script::Super => superscript_char,
            Script::Sub => subscript_char,
        };
        let convert = |s: &str| -> Option<String> {
            s.chars()
                .map(|c| if c == ' ' { Some(c) } else { map(c) })
                .collect()
        };
        let Some(nodes) = self.out.get(start..) else {
            return;
        };
        let mut converted = Vec::with_capacity(nodes.len());
        for node in nodes {
            match node {
                ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. } => {
                    match convert(value) {
                        Some(v) => converted.push(v),
                        None => return,
                    }
                }
                _ => return,
            }
        }
        for (node, v) in self.out[start..].iter_mut().zip(converted) {
            if let ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. } = node {
                *value = v;
            }
        }
    }

    /// A list item's bullet, or its number in an `ol`.
    fn list_marker(&mut self) {
        let list = self.stack.iter_mut().rev().find_map(|o| o.list.as_mut());
        let marker = match list {
            Some(Some(n)) => {
                *n = n.saturating_add(1);
                format!("{n}. ")
            }
            _ => "\u{2022} ".to_string(),
        };
        if !self.hidden() {
            self.flush_break();
            self.text_raw(&marker);
        }
    }

    fn image(&mut self, attrs: &[(String, String)]) {
        self.flush_break();
        self.out.push(ContentNode::Image {
            src: attr_value(attrs, "src").unwrap_or("").trim().to_string(),
            alt: attr_value(attrs, "alt").map(str::to_string),
            width: attr_value(attrs, "width").map(dimension),
            height: attr_value(attrs, "height").map(dimension),
        });
        self.line_start = false;
        self.space = false;
    }

    /// Markup text: character references decoded, white space collapsed (or kept,
    /// in `pre`).
    fn text(&mut self, s: &str) {
        if s.is_empty() || self.hidden() {
            return;
        }
        let decoded = decode_entities(s);
        let pre = self.top().is_some_and(|o| o.pre);
        for c in decoded.chars() {
            let pre_start = std::mem::take(&mut self.pre_start);
            if pre {
                match c {
                    '\n' if pre_start => {}
                    '\n' => {
                        self.flush_break();
                        self.line_break();
                    }
                    '\r' => self.pre_start = pre_start,
                    c => self.char(c),
                }
            } else if is_space(c) {
                self.whitespace();
            } else {
                self.char(c);
            }
        }
    }

    /// Text drawn as given (markers, quotes).
    fn text_raw(&mut self, s: &str) {
        for c in s.chars() {
            self.char(c);
        }
    }

    /// A collapsible space: none at the start of a line, after another, or at a
    /// block edge.
    fn whitespace(&mut self) {
        if self.line_start || self.space || self.pending_break || self.hidden() {
            return;
        }
        self.char(' ');
        self.space = true;
    }

    fn char(&mut self, c: char) {
        if self.hidden() {
            return;
        }
        self.flush_break();
        let top = self.stack.last();
        let id = top.and_then(|o| o.id.as_deref());
        let link = top.and_then(|o| o.href.as_ref());
        let split = std::mem::take(&mut self.split);
        let last = self.out.len().checked_sub(1).filter(|&l| l >= self.base);
        let run = last.and_then(|l| match &mut self.out[l] {
            ContentNode::Text { value, href }
                if id.is_none() && href.as_ref() == link && !split =>
            {
                Some(value)
            }
            ContentNode::StyledText {
                value,
                style_id: Some(s),
                href,
            } if id == Some(s.as_str()) && href.as_ref() == link && !split => Some(value),
            _ => None,
        });
        match run {
            Some(value) => value.push(c),
            None => {
                let href = link.cloned();
                self.out.push(match id {
                    None => ContentNode::Text {
                        value: c.into(),
                        href,
                    },
                    Some(id) => ContentNode::StyledText {
                        value: c.into(),
                        style_id: Some(id.to_string()),
                        href,
                    },
                })
            }
        }
        self.line_start = false;
        self.space = false;
    }

    fn flush_break(&mut self) {
        if std::mem::take(&mut self.pending_break) && !self.line_start {
            self.line_break();
        }
    }

    fn line_break(&mut self) {
        self.trim_space();
        self.out.push(ContentNode::LineBreak {});
        self.line_start = true;
        self.space = false;
    }

    /// Drop a collapsible space ending the line.
    fn trim_space(&mut self) {
        if !self.space {
            return;
        }
        self.space = false;
        let Some(l) = self.out.len().checked_sub(1).filter(|&l| l >= self.base) else {
            return;
        };
        if let ContentNode::Text { value, .. } | ContentNode::StyledText { value, .. } =
            &mut self.out[l]
        {
            value.pop();
            if value.is_empty() {
                self.out.pop();
            }
        }
    }

    /// End the markup. At the end of its line (`at_end`), a final space and line
    /// break are dropped, as a browser draws none; before more content, a block
    /// edge breaks the line.
    fn finish(mut self, at_end: bool) {
        self.close(0);
        if at_end {
            self.trim_space();
            if self.out.len() > self.base
                && matches!(self.out.last(), Some(ContentNode::LineBreak {}))
            {
                self.out.pop();
            }
        } else {
            self.flush_break();
        }
    }
}

// ─── Tokens ───

/// The index after the next `b` at or after `i` (the end, if none).
fn skip_past(s: &str, i: usize, b: u8) -> usize {
    s.as_bytes()[i..]
        .iter()
        .position(|&c| c == b)
        .map_or(s.len(), |p| i + p + 1)
}

/// A tag or attribute name starting at `i`, and the index after it.
fn tag_name(s: &str, i: usize) -> (&str, usize) {
    let end = s.as_bytes()[i..]
        .iter()
        .position(|&c| c.is_ascii_whitespace() || c == b'/' || c == b'>')
        .map_or(s.len(), |p| i + p);
    (&s[i..end], end)
}

/// A tag's attributes: names lowercase, values decoded.
type Attrs = Vec<(String, String)>;

/// A start tag whose name starts at `i`: its lowercase name, its attributes
/// (names lowercase, values decoded) and the index after its `>`. `None` when
/// the markup ends inside it.
fn start_tag(s: &str, i: usize) -> Option<(String, Attrs, usize)> {
    let bytes = s.as_bytes();
    let (name, mut i) = tag_name(s, i);
    let name = name.to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while bytes
            .get(i)
            .is_some_and(|c| c.is_ascii_whitespace() || *c == b'/')
        {
            i += 1;
        }
        match bytes.get(i)? {
            b'>' => return Some((name, attrs, i + 1)),
            _ => {
                let end = bytes[i..]
                    .iter()
                    .position(|&c| c.is_ascii_whitespace() || matches!(c, b'/' | b'>' | b'='))
                    .map_or(s.len(), |p| i + p)
                    // A name may start with `=`, which ends nothing.
                    .max(i + 1);
                let attr = s.get(i..end).unwrap_or("").to_ascii_lowercase();
                i = end;
                while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                    i += 1;
                }
                let mut value = String::new();
                if bytes.get(i) == Some(&b'=') {
                    i += 1;
                    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                        i += 1;
                    }
                    let raw = match bytes.get(i)? {
                        &q @ (b'"' | b'\'') => {
                            let close = bytes[i + 1..].iter().position(|&c| c == q)?;
                            let v = &s[i + 1..i + 1 + close];
                            i += close + 2;
                            v
                        }
                        _ => {
                            let end = bytes[i..]
                                .iter()
                                .position(|&c| c.is_ascii_whitespace() || c == b'>')
                                .map_or(s.len(), |p| i + p);
                            let v = &s[i..end];
                            i = end;
                            v
                        }
                    };
                    value = decode_entities(raw).into_owned();
                }
                // The first of a repeated attribute counts, as in HTML.
                if attrs.len() < MAX_ATTRS && !attrs.iter().any(|(n, _)| *n == attr) {
                    attrs.push((attr, value));
                }
            }
        }
    }
}

/// `s` with its character references decoded. One that isn't known stays as
/// written.
fn decode_entities(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        match entity(rest) {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The character a reference at the start of `s` (`&amp;`, `&#38;`, `&#x26;`)
/// stands for, and its length.
fn entity(s: &str) -> Option<(char, usize)> {
    let body = &s[1..];
    if let Some(num) = body.strip_prefix('#') {
        let (digits, radix, skip) = match num.strip_prefix(['x', 'X']) {
            Some(hex) => (hex, 16, 3),
            None => (num, 10, 2),
        };
        let len = digits
            .find(|c: char| !c.is_digit(radix))
            .unwrap_or(digits.len());
        if len == 0 {
            return None;
        }
        // More than 8 digits is past U+10FFFF (and u32) in either radix.
        let value = match len {
            ..=8 => u32::from_str_radix(&digits[..len], radix).unwrap_or(u32::MAX),
            _ => u32::MAX,
        };
        let c = match value {
            0 => '\u{FFFD}',
            v => char::from_u32(v).unwrap_or('\u{FFFD}'),
        };
        let semi = usize::from(digits[len..].starts_with(';'));
        return Some((c, skip + len + semi));
    }
    let len = body
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(body.len());
    if !body[len..].starts_with(';') {
        return None;
    }
    let c = named_entity(&body[..len])?;
    Some((c, len + 2))
}

/// U+00A0–U+00FF, in order.
const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

/// U+0391–U+03A9 and U+03B1–U+03C9, in order (U+03A2 is unassigned).
const GREEK: [&str; 25] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigmaf", "sigma", "tau", "upsilon", "phi",
    "chi", "psi", "omega",
];

const OTHER: &[(&str, char)] = &[
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200C}'),
    ("zwj", '\u{200D}'),
    ("lrm", '\u{200E}'),
    ("rlm", '\u{200F}'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("sbquo", '‚'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bdquo", '„'),
    ("dagger", '†'),
    ("Dagger", '‡'),
    ("bull", '•'),
    ("hellip", '…'),
    ("permil", '‰'),
    ("prime", '′'),
    ("Prime", '″'),
    ("lsaquo", '‹'),
    ("rsaquo", '›'),
    ("euro", '€'),
    ("trade", '™'),
    ("larr", '←'),
    ("uarr", '↑'),
    ("rarr", '→'),
    ("darr", '↓'),
    ("harr", '↔'),
    ("minus", '−'),
    ("lowast", '∗'),
    ("radic", '√'),
    ("infin", '∞'),
    ("asymp", '≈'),
    ("ne", '≠'),
    ("equiv", '≡'),
    ("le", '≤'),
    ("ge", '≥'),
    ("sum", '∑'),
    ("prod", '∏'),
    ("part", '∂'),
    ("nabla", '∇'),
    ("isin", '∈'),
    ("notin", '∉'),
    ("cap", '∩'),
    ("cup", '∪'),
    ("and", '∧'),
    ("or", '∨'),
    ("sdot", '⋅'),
    ("loz", '◊'),
    ("hearts", '♥'),
];

fn named_entity(name: &str) -> Option<char> {
    if let Some(i) = LATIN1.iter().position(|&n| n == name) {
        return char::from_u32(0xA0 + i as u32);
    }
    let lower = name.to_ascii_lowercase();
    if let Some(i) = GREEK.iter().position(|&n| n == lower) {
        let upper = name.as_bytes()[0].is_ascii_uppercase();
        // Only the first letter may be a capital, and there's no capital final sigma.
        if name[1..] != lower[1..] || (upper && lower == "sigmaf") {
            return None;
        }
        // The capitals' gap at U+03A2 is where the final sigma is.
        let base = if upper { 0x391 } else { 0x3B1 };
        return char::from_u32(base + i as u32);
    }
    OTHER.iter().find(|(n, _)| *n == name).map(|&(_, c)| c)
}

#[cfg(test)]
mod tests;
