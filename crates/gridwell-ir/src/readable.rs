//! The table as this gridwell reads it: what it doesn't read left out, so a writer
//! sees only what it knows.
//!
//! A table of a newer minor IR version may hold what that version added: a
//! keyword value, a length unit, a color form or a link scheme this gridwell
//! doesn't know. They aren't validation errors there (they are in a table of this
//! or an older version); [`Table::readable`] leaves each out and lists it in
//! [`Readable::ignored`]. In any version, a content node of a type this gridwell
//! doesn't know is drawn as its `fallback` (or as nothing), and a conditional style
//! whose selector has a field it doesn't read applies to no row.

use crate::cell::Row;
use crate::content::{ContentNode, UnknownNode};
use crate::keywords::Keyword;
use crate::style::{Border, StyleDef};
use crate::validation::{cell_place, keyword_problem, place, ValidationRule};
use crate::value_checks::LengthKind;
use crate::value_checks::{color_problem, font_size_problem, href_message, length_problem};
use crate::Table;
use std::borrow::Cow;
use std::fmt;

/// A table as [`Table::readable`] reads it.
#[derive(Debug, Clone)]
pub struct Readable<'a> {
    /// The table without what this gridwell doesn't read: borrowed when that's
    /// nothing.
    pub table: Cow<'a, Table>,
    /// What was left out that the table's author may want to know of, in document
    /// order.
    pub ignored: Vec<Ignored>,
}

/// Something in a valid table that [`Table::readable`] left out. A warning, not an
/// error: the table renders without it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ignored {
    /// A value of a newer minor IR version that this gridwell doesn't know: in a
    /// table of this version, it would break `rule`, with `message`.
    Value {
        /// The rule it would break.
        rule: ValidationRule,
        /// The error it would be, as validation words it.
        message: String,
        /// What's drawn instead: `"it is left out"`, `"the default, left, is
        /// used"`, ….
        consequence: String,
    },
    /// A conditional style whose selector has fields this gridwell doesn't read
    /// (a newer IR version may narrow selectors with them): it applies to no row.
    Selector {
        /// Its index in `styles.conditionals`.
        index: usize,
        /// The fields, in the order written.
        fields: Vec<String>,
    },
}

impl Ignored {
    /// The rule an ignored value reports under.
    pub const IGNORED_VALUE: &'static str = "IGNORED_VALUE";
    /// The rule an ignored selector reports under.
    pub const IGNORED_SELECTOR: &'static str = "IGNORED_SELECTOR";

    /// The rule it reports under, as validation errors report theirs.
    pub fn rule(&self) -> &'static str {
        match self {
            Ignored::Value { .. } => Self::IGNORED_VALUE,
            Ignored::Selector { .. } => Self::IGNORED_SELECTOR,
        }
    }
}

impl fmt::Display for Ignored {
    /// `[IGNORED_VALUE] config.dir is "auto", which is not one of: ltr, rtl; it is
    /// left out`, or `[IGNORED_SELECTOR] styles.conditionals[0].selector has a field
    /// this gridwell doesn't read ("col_parity"); the conditional style applies to
    /// no row`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ignored::Value {
                message,
                consequence,
                ..
            } => write!(f, "[{}] {message}; {consequence}", self.rule()),
            Ignored::Selector { index, fields } => {
                let names: Vec<String> = fields.iter().map(|n| format!("{n:?}")).collect();
                write!(
                    f,
                    "[{}] styles.conditionals[{index}].selector has {} this gridwell \
                     doesn't read ({}); {NO_ROW}",
                    self.rule(),
                    if fields.len() == 1 {
                        "a field"
                    } else {
                        "fields"
                    },
                    names.join(", ")
                )
            }
        }
    }
}

const LEFT_OUT: &str = "it is left out";
const NO_ROW: &str = "the conditional style applies to no row";
const NO_LINK: &str = "the text is drawn without a link";
const NO_TYPED_VALUE: &str = "the typed value is left out";

impl Table {
    /// The table as this gridwell reads it: borrowed when it reads all of it.
    ///
    /// A table of a newer minor IR version may hold what that version added: a
    /// keyword value, a length unit, a color form or a link scheme this gridwell
    /// doesn't know. They aren't validation errors there; each is left out (an
    /// optional field unset, any other its default, a link's text drawn without
    /// it) and listed in [`Readable::ignored`]. In any version, a content node of a
    /// type this gridwell doesn't know is replaced by its `fallback` (or removed),
    /// and a conditional style whose selector has a field this gridwell doesn't
    /// read is removed and listed.
    ///
    /// For a valid table: [`validated`](Self::validated) calls it.
    pub fn readable(&self) -> Readable<'_> {
        let newer = self.is_newer_minor();
        let has_unknown = || {
            self.contents()
                .any(|c| c.iter().any(|n| matches!(n, ContentNode::Unknown(_))))
        };
        let other_fields = self
            .styles
            .conditionals
            .iter()
            .any(|c| !c.selector.other.is_empty());
        if !newer && !other_fields && !has_unknown() {
            return Readable {
                table: Cow::Borrowed(self),
                ignored: Vec::new(),
            };
        }
        let mut table = self.clone();
        let mut reader = Reader {
            newer,
            ignored: Vec::new(),
            changed: false,
        };
        reader.table(&mut table);
        Readable {
            table: match reader.changed {
                true => Cow::Owned(table),
                false => Cow::Borrowed(self),
            },
            ignored: reader.ignored,
        }
    }
}

/// Leaves out of a table what this gridwell doesn't read.
struct Reader {
    /// The table is a newer minor IR version.
    newer: bool,
    ignored: Vec<Ignored>,
    /// Whether anything was left out (an unknown node without a fallback is, and
    /// isn't listed).
    changed: bool,
}

impl Reader {
    fn ignore(&mut self, rule: ValidationRule, message: String, consequence: impl Into<String>) {
        self.changed = true;
        self.ignored.push(Ignored::Value {
            rule,
            message,
            consequence: consequence.into(),
        });
    }

    /// An optional keyword this gridwell doesn't know is left out.
    fn keyword<K: Keyword>(&mut self, slot: &mut Option<K>, field: impl FnOnce() -> String) {
        if !self.newer {
            return;
        }
        if let Some(message) = slot.as_ref().and_then(|v| keyword_problem(v, field)) {
            *slot = None;
            self.ignore(ValidationRule::UnknownValue, message, LEFT_OUT);
        }
    }

    /// A keyword with a default that this gridwell doesn't know is its default.
    fn keyword_or_default<K: Keyword + Default>(
        &mut self,
        slot: &mut K,
        field: impl FnOnce() -> String,
    ) {
        if !self.newer {
            return;
        }
        if let Some(message) = keyword_problem(slot, field) {
            *slot = K::default();
            let consequence = format!("the default, {}, is used", slot.as_str());
            self.ignore(ValidationRule::UnknownValue, message, consequence);
        }
    }

    /// Whether a selector's keyword is one this gridwell doesn't know (and so the
    /// conditional is dropped).
    fn unknown_selector<K: Keyword>(
        &mut self,
        value: Option<&K>,
        field: impl FnOnce() -> String,
    ) -> bool {
        match value.and_then(|v| keyword_problem(v, field)) {
            Some(message) => {
                self.ignore(ValidationRule::UnknownValue, message, NO_ROW);
                true
            }
            None => false,
        }
    }

    fn color(&mut self, slot: &mut Option<String>, field: impl FnOnce() -> String) {
        if !self.newer {
            return;
        }
        if let Some(message) = slot.as_deref().and_then(|v| color_problem(v, field)) {
            *slot = None;
            self.ignore(ValidationRule::InvalidColor, message, LEFT_OUT);
        }
    }

    fn length(
        &mut self,
        slot: &mut Option<String>,
        kind: LengthKind,
        field: impl FnOnce() -> String,
    ) {
        if !self.newer {
            return;
        }
        if let Some(message) = slot.as_deref().and_then(|v| length_problem(v, kind, field)) {
            *slot = None;
            self.ignore(ValidationRule::InvalidLength, message, LEFT_OUT);
        }
    }

    fn font_size(&mut self, slot: &mut Option<String>, field: impl FnOnce() -> String) {
        if !self.newer {
            return;
        }
        if let Some(message) = slot.as_deref().and_then(|v| font_size_problem(v, field)) {
            *slot = None;
            self.ignore(ValidationRule::InvalidLength, message, LEFT_OUT);
        }
    }

    fn table(&mut self, t: &mut Table) {
        let cfg = &mut t.config;
        self.length(&mut cfg.table_width, LengthKind::SIZE, || {
            "config.table_width".into()
        });
        self.length(&mut cfg.container_width, LengthKind::SIZE, || {
            "config.container_width".into()
        });
        self.length(&mut cfg.container_height, LengthKind::SIZE, || {
            "config.container_height".into()
        });
        self.keyword(&mut cfg.container_overflow, || {
            "config.container_overflow".into()
        });
        self.keyword(&mut cfg.dir, || "config.dir".into());
        self.keyword_or_default(&mut cfg.page_break_mode, || "config.page_break_mode".into());
        if let Some(rules) = &mut cfg.rules {
            for (name, rule) in rules.iter_mut() {
                if let Some(rule) = rule {
                    self.border(rule, &format!("config.rules.{name}"));
                }
            }
        }

        for (i, col) in t.column_spec.iter_mut().enumerate() {
            let f = |name: &str| format!("column_spec[{i}].{name}");
            self.keyword_or_default(&mut col.align, || f("align"));
            if self.newer {
                let field = || f("width");
                if let Some(message) = length_problem(&col.width, LengthKind::COLUMN_WIDTH, field) {
                    col.width = "auto".to_string();
                    let consequence = "the default, auto, is used";
                    self.ignore(ValidationRule::InvalidLength, message, consequence);
                }
            }
            self.length(&mut col.min_width, LengthKind::EXTENT, || f("min_width"));
            self.length(&mut col.max_width, LengthKind::EXTENT, || f("max_width"));
        }

        let palette = &mut t.styles;
        for (id, def) in &mut palette.defs {
            self.style(def, &format!("styles.defs.{id}"));
        }
        for (id, comp) in &mut palette.compositions {
            self.style(
                &mut comp.overrides,
                &format!("styles.compositions.{id}.overrides"),
            );
        }
        let mut kept = Vec::with_capacity(palette.conditionals.len());
        for (i, mut cond) in std::mem::take(&mut palette.conditionals)
            .into_iter()
            .enumerate()
        {
            let at = format!("styles.conditionals[{i}].selector");
            let sel = &cond.selector;
            if !sel.other.is_empty() {
                self.changed = true;
                self.ignored.push(Ignored::Selector {
                    index: i,
                    fields: sel.other.keys().cloned().collect(),
                });
                continue;
            }
            if self.newer
                && (self.unknown_selector(sel.row_parity.as_ref(), || format!("{at}.row_parity"))
                    | self.unknown_selector(sel.scope.as_ref(), || format!("{at}.scope")))
            {
                continue;
            }
            self.style(&mut cond.style, &format!("styles.conditionals[{i}].style"));
            kept.push(cond);
        }
        palette.conditionals = kept;

        if let Some(header) = &mut t.header {
            if let Some(line) = &mut header.title {
                self.content(&mut line.content, &|p| format!("header.title.{p}"));
            }
            if let Some(line) = &mut header.subtitle {
                self.content(&mut line.content, &|p| format!("header.subtitle.{p}"));
            }
            for (i, line) in header.extra_lines.iter_mut().enumerate() {
                self.content(&mut line.content, &|p| {
                    format!("header.extra_lines[{i}].{p}")
                });
            }
        }

        self.rows(&mut t.table.thead.rows, "thead", None);
        for (g, group) in t.table.tbody.iter_mut().enumerate() {
            self.keyword_or_default(&mut group.summary_placement, || {
                format!("the summary_placement of tbody group {g}")
            });
            if let Some(label) = &mut group.label {
                self.content(&mut label.content, &|p| {
                    format!("{p} of the label of tbody group {g}")
                });
            }
            let g = Some(g as u32);
            self.rows(&mut group.rows, "tbody", g);
            self.rows(&mut group.summary_rows, "tbody_summary", g);
        }

        if let Some(footer) = &mut t.footer {
            for (i, n) in footer.footnotes.iter_mut().enumerate() {
                self.content(&mut n.content, &|p| format!("footer.footnotes[{i}].{p}"));
            }
            for (i, n) in footer.source_notes.iter_mut().enumerate() {
                self.content(&mut n.content, &|p| format!("footer.source_notes[{i}].{p}"));
            }
        }
    }

    /// `path` names the style (`styles.defs.x`).
    fn style(&mut self, def: &mut StyleDef, path: &str) {
        let f = |name: &str| format!("{path}.{name}");
        self.font_size(&mut def.font_size, || f("font_size"));
        self.keyword(&mut def.font_weight, || f("font_weight"));
        self.keyword(&mut def.font_style, || f("font_style"));
        self.color(&mut def.color, || f("color"));
        self.color(&mut def.background_color, || f("background_color"));
        self.keyword(&mut def.text_align, || f("text_align"));
        self.keyword(&mut def.vertical_align, || f("vertical_align"));
        self.keyword(&mut def.text_transform, || f("text_transform"));
        self.keyword(&mut def.text_decoration, || f("text_decoration"));
        self.keyword(&mut def.white_space, || f("white_space"));
        if let Some(p) = &mut def.padding {
            for (side, v) in [
                ("top", &mut p.top),
                ("right", &mut p.right),
                ("bottom", &mut p.bottom),
                ("left", &mut p.left),
            ] {
                self.length(v, LengthKind::EXTENT, || f(&format!("padding.{side}")));
            }
        }
        if let Some(border) = &mut def.border {
            for (side, b) in [
                ("top", &mut border.top),
                ("right", &mut border.right),
                ("bottom", &mut border.bottom),
                ("left", &mut border.left),
            ] {
                if let Some(b) = b {
                    self.border(b, &f(&format!("border.{side}")));
                }
            }
        }
        self.length(&mut def.indent, LengthKind::INDENT, || f("indent"));
        self.keyword(&mut def.word_break, || f("word_break"));
        self.keyword(&mut def.overflow, || f("overflow"));
        self.keyword(&mut def.text_overflow, || f("text_overflow"));
        self.length(&mut def.min_width, LengthKind::EXTENT, || f("min_width"));
        self.length(&mut def.max_width, LengthKind::EXTENT, || f("max_width"));
    }

    /// `path` names the border (`config.rules.top`).
    fn border(&mut self, b: &mut Border, path: &str) {
        let f = |name: &str| format!("{path}.{name}");
        self.length(&mut b.width, LengthKind::BORDER, || f("width"));
        self.keyword(&mut b.style, || f("style"));
        self.color(&mut b.color, || f("color"));
    }

    /// `field` makes a field's name from its path in the content
    /// (`content[0].href`).
    fn content(&mut self, nodes: &mut Vec<ContentNode>, field: &dyn Fn(&str) -> String) {
        self.nodes(nodes, "content", field);
    }

    /// [`content`](Self::content) for the nodes at `path` (`content`,
    /// `content[0].fallback`); an unknown node is replaced by its fallback, or
    /// removed.
    fn nodes(&mut self, nodes: &mut Vec<ContentNode>, path: &str, field: &dyn Fn(&str) -> String) {
        let unknown = |n: &ContentNode| matches!(n, ContentNode::Unknown(_));
        if !self.newer && !nodes.iter().any(unknown) {
            return;
        }
        let mut out = Vec::with_capacity(nodes.len());
        for (i, mut node) in std::mem::take(nodes).into_iter().enumerate() {
            let f = |name: &str| field(&format!("{path}[{i}].{name}"));
            match &mut node {
                ContentNode::Image { width, height, .. } => {
                    self.length(width, LengthKind::SIZE, || f("width"));
                    self.length(height, LengthKind::SIZE, || f("height"));
                }
                ContentNode::Text { href, .. } | ContentNode::StyledText { href, .. } => {
                    if self.newer {
                        let problem = href.as_deref().and_then(|h| href_message(h, || f("href")));
                        if let Some((message, true)) = problem {
                            *href = None;
                            self.ignore(ValidationRule::InvalidHref, message, NO_LINK);
                        }
                    }
                }
                ContentNode::Unknown(UnknownNode { fallback, .. }) => {
                    self.changed = true;
                    if let Some(mut fallback) = fallback.take() {
                        self.nodes(&mut fallback, &format!("{path}[{i}].fallback"), field);
                        out.extend(fallback);
                    }
                    continue;
                }
                _ => {}
            }
            out.push(node);
        }
        *nodes = out;
    }

    fn rows(&mut self, rows: &mut [Row], section: &str, row_group: Option<u32>) {
        for (r, row) in rows.iter_mut().enumerate() {
            self.keyword(&mut row.role, || {
                format!("the role of {} row {r}", place(section, row_group))
            });
            for (c, cell) in row.cells.iter_mut().enumerate() {
                let of = |path: &str| format!("{path} of {}", cell_place(section, row_group, r, c));
                let the = |field: &str| {
                    format!("the {field} of {}", cell_place(section, row_group, r, c))
                };
                self.keyword(&mut cell.scope, || the("scope"));
                if self.newer {
                    let problem = cell
                        .typed_value
                        .as_ref()
                        .and_then(|tv| keyword_problem(&tv.value_type, || the("typed_value.type")));
                    if let Some(message) = problem {
                        cell.typed_value = None;
                        self.ignore(ValidationRule::UnknownValue, message, NO_TYPED_VALUE);
                    }
                }
                self.content(&mut cell.content, &of);
            }
        }
    }
}
