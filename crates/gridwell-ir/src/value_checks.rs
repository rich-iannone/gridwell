//! INVALID_COLOR and INVALID_LENGTH: every color and length in the IR parses, and
//! each length uses a form that makes sense for its field (no `fr` padding, no
//! negative widths, …). INVALID_LANG: `config.lang` is a well-formed language tag.
//! INVALID_HREF: every link is one gridwell writes. INVALID_TABLE_ID: `config.id`
//! is an HTML id.

use crate::cell::Row;
use crate::content::ContentNode;
use crate::style::Border;
use crate::style::StyleDef;
use crate::validation::{cell_place, ValidationError, ValidationRule};
use crate::Table;
use gridwell_core::link::href_problem;
use gridwell_core::{Color, FontSize, LanguageTag, Length};

/// Which length forms a field accepts beyond plain non-negative absolute/relative
/// lengths (`px pt em rem in cm mm`).
#[derive(Clone, Copy)]
struct LengthKind {
    percent: bool,
    fr: bool,
    auto: bool,
    negative: bool,
}

impl LengthKind {
    /// Column widths: anything but negative.
    const COLUMN_WIDTH: Self = Self {
        percent: true,
        fr: true,
        auto: true,
        negative: false,
    };
    /// Table, container and image sizes.
    const SIZE: Self = Self {
        fr: false,
        ..Self::COLUMN_WIDTH
    };
    /// min/max widths and padding.
    const EXTENT: Self = Self {
        auto: false,
        ..Self::SIZE
    };
    /// `text-indent` may be negative (a hanging indent).
    const INDENT: Self = Self {
        negative: true,
        ..Self::EXTENT
    };
    /// Border widths: CSS has no percentage border width.
    const BORDER: Self = Self {
        percent: false,
        ..Self::EXTENT
    };

    fn expected(self) -> String {
        let mut forms = vec!["a non-negative length (px, pt, em, rem, in, cm, mm)"];
        if self.negative {
            forms[0] = "a length (px, pt, em, rem, in, cm, mm)";
        }
        if self.percent {
            forms.push("a percentage");
        }
        if self.fr {
            forms.push("a fraction (fr)");
        }
        if self.auto {
            forms.push("auto");
        }
        forms.join(", or ")
    }

    /// `None` if `length` is acceptable, else what's wrong with it.
    fn problem(self, length: &Length) -> Option<&'static str> {
        match length {
            Length::Auto if !self.auto => Some("auto is not allowed here"),
            Length::Fr(_) if !self.fr => Some("fr units are not allowed here"),
            Length::Percent(_) if !self.percent => Some("percentages are not allowed here"),
            l if l.is_negative() && !self.negative => Some("it is negative"),
            _ => None,
        }
    }
}

/// Where a value sits, for the error's location fields.
#[derive(Clone, Copy)]
struct At<'a> {
    section: &'a str,
    row_group: Option<u32>,
    row: Option<u32>,
    col: Option<u32>,
}

impl<'a> At<'a> {
    fn section(section: &'a str) -> Self {
        Self {
            section,
            row_group: None,
            row: None,
            col: None,
        }
    }

    fn error(self, rule: ValidationRule, message: String) -> ValidationError {
        ValidationError {
            rule,
            section: self.section.to_string(),
            row_group: self.row_group,
            row: self.row,
            col: self.col,
            message,
        }
    }
}

struct Checker<'e> {
    errors: &'e mut Vec<ValidationError>,
}

impl Checker<'_> {
    fn color(&mut self, value: Option<&String>, field: &str, at: At) {
        let Some(v) = value else { return };
        if v.parse::<Color>().is_err() {
            self.errors.push(at.error(
                ValidationRule::InvalidColor,
                format!(
                    "{field} is \"{v}\", which is not a color (expected a CSS named color, \
                     #RGB/#RGBA/#RRGGBB/#RRGGBBAA, rgb()/rgba(), hsl()/hsla(), or transparent)"
                ),
            ));
        }
    }

    fn length(&mut self, value: Option<&String>, field: &str, kind: LengthKind, at: At) {
        let Some(v) = value else { return };
        let why = match v.parse::<Length>() {
            Ok(l) => match kind.problem(&l) {
                Some(why) => why,
                None => return,
            },
            Err(_) => "it does not parse",
        };
        self.errors.push(at.error(
            ValidationRule::InvalidLength,
            format!(
                "{field} is \"{v}\", which is not valid here: {why} (expected {})",
                kind.expected()
            ),
        ));
    }

    fn font_size(&mut self, value: Option<&String>, field: &str, at: At) {
        let Some(v) = value else { return };
        if v.parse::<FontSize>().is_err() {
            self.errors.push(at.error(
                ValidationRule::InvalidLength,
                format!(
                    "{field} is \"{v}\", which is not a font size (expected a non-negative \
                     length, a percentage, or one of: xx-small, x-small, small, medium, \
                     large, x-large, xx-large, xxx-large, smaller, larger)"
                ),
            ));
        }
    }

    fn style(&mut self, def: &StyleDef, path: &str) {
        let at = At::section("styles");
        let f = |name: &str| format!("{path}.{name}");
        self.color(def.color.as_ref(), &f("color"), at);
        self.color(def.background_color.as_ref(), &f("background_color"), at);
        self.font_size(def.font_size.as_ref(), &f("font_size"), at);
        self.length(def.indent.as_ref(), &f("indent"), LengthKind::INDENT, at);
        self.length(
            def.min_width.as_ref(),
            &f("min_width"),
            LengthKind::EXTENT,
            at,
        );
        self.length(
            def.max_width.as_ref(),
            &f("max_width"),
            LengthKind::EXTENT,
            at,
        );
        if let Some(p) = &def.padding {
            for (side, v) in [
                ("top", &p.top),
                ("right", &p.right),
                ("bottom", &p.bottom),
                ("left", &p.left),
            ] {
                self.length(
                    v.as_ref(),
                    &f(&format!("padding.{side}")),
                    LengthKind::EXTENT,
                    at,
                );
            }
        }
        if let Some(border) = &def.border {
            for (side, b) in [
                ("top", &border.top),
                ("right", &border.right),
                ("bottom", &border.bottom),
                ("left", &border.left),
            ] {
                let Some(b) = b else { continue };
                self.border(b, &f(&format!("border.{side}")), at);
            }
        }
    }

    /// A border's width and color; `path` names the border.
    fn border(&mut self, b: &Border, path: &str, at: At) {
        let f = |name: &str| format!("{path}.{name}");
        self.length(b.width.as_ref(), &f("width"), LengthKind::BORDER, at);
        self.color(b.color.as_ref(), &f("color"), at);
    }

    /// Check the image nodes and links of `nodes`; `field` makes a field's name
    /// from its path in the content (`content[0].width`).
    fn content(&mut self, nodes: &[ContentNode], field: &dyn Fn(&str) -> String, at: At) {
        for (i, node) in nodes.iter().enumerate() {
            let f = |name: &str| field(&format!("content[{i}].{name}"));
            match node {
                ContentNode::Image { width, height, .. } => {
                    self.length(width.as_ref(), &f("width"), LengthKind::SIZE, at);
                    self.length(height.as_ref(), &f("height"), LengthKind::SIZE, at);
                }
                ContentNode::Text {
                    href: Some(href), ..
                }
                | ContentNode::StyledText {
                    href: Some(href), ..
                } => {
                    if let Some(why) = href_problem(href) {
                        self.errors.push(at.error(
                            ValidationRule::InvalidHref,
                            format!(
                                "{} is {href:?}, which is not a link gridwell writes: {why} \
                                 (expected an http, https or mailto URL)",
                                f("href")
                            ),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn rows(&mut self, rows: &[Row], section: &str, row_group: Option<u32>) {
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                let at = At {
                    section,
                    row_group,
                    row: Some(r as u32),
                    col: Some(c as u32),
                };
                let of = |path: &str| format!("{path} of {}", cell_place(section, row_group, r, c));
                self.content(&cell.content, &of, at);
            }
        }
    }
}

pub(crate) fn validate_values(table: &Table, errors: &mut Vec<ValidationError>) {
    let mut ck = Checker { errors };

    let config = At::section("config");
    let cfg = &table.config;
    ck.length(
        cfg.table_width.as_ref(),
        "config.table_width",
        LengthKind::SIZE,
        config,
    );
    for (field, v) in [
        ("config.container_width", &cfg.container_width),
        ("config.container_height", &cfg.container_height),
    ] {
        ck.length(v.as_ref(), field, LengthKind::SIZE, config);
    }
    if let Some(lang) = &cfg.lang {
        if let Err(why) = LanguageTag::parse(lang) {
            ck.errors.push(config.error(
                ValidationRule::InvalidLang,
                format!("config.lang is \"{lang}\", which is not a language tag: {why}"),
            ));
        }
    }

    if let Some(id) = &cfg.id {
        if id.is_empty() || id.chars().any(char::is_whitespace) {
            ck.errors.push(config.error(
                ValidationRule::InvalidTableId,
                format!(
                    "config.id is {id:?}, which is not an HTML id (it must not be empty \
                     or have whitespace)"
                ),
            ));
        }
    }
    if let Some(rules) = &cfg.rules {
        for (name, rule) in rules.iter() {
            if let Some(rule) = rule {
                ck.border(rule, &format!("config.rules.{name}"), config);
            }
        }
    }

    for (i, col) in table.column_spec.iter().enumerate() {
        let at = At {
            col: Some(i as u32),
            ..At::section("column_spec")
        };
        let f = |name: &str| format!("column_spec[{i}].{name}");
        ck.length(Some(&col.width), &f("width"), LengthKind::COLUMN_WIDTH, at);
        ck.length(
            col.min_width.as_ref(),
            &f("min_width"),
            LengthKind::EXTENT,
            at,
        );
        ck.length(
            col.max_width.as_ref(),
            &f("max_width"),
            LengthKind::EXTENT,
            at,
        );
    }

    // Sorted ids so error order is deterministic (the palette is a HashMap).
    let palette = &table.styles;
    let mut ids: Vec<&String> = palette.defs.keys().collect();
    ids.sort();
    for id in ids {
        ck.style(&palette.defs[id], &format!("styles.defs.{id}"));
    }
    let mut ids: Vec<&String> = palette.compositions.keys().collect();
    ids.sort();
    for id in ids {
        ck.style(
            &palette.compositions[id].overrides,
            &format!("styles.compositions.{id}.overrides"),
        );
    }
    for (i, cond) in palette.conditionals.iter().enumerate() {
        ck.style(&cond.style, &format!("styles.conditionals[{i}].style"));
    }

    if let Some(header) = &table.header {
        let at = At::section("header");
        if let Some(t) = &header.title {
            ck.content(&t.content, &|p| format!("header.title.{p}"), at);
        }
        if let Some(t) = &header.subtitle {
            ck.content(&t.content, &|p| format!("header.subtitle.{p}"), at);
        }
        for (i, line) in header.extra_lines.iter().enumerate() {
            ck.content(
                &line.content,
                &|p| format!("header.extra_lines[{i}].{p}"),
                at,
            );
        }
    }

    ck.rows(&table.table.thead.rows, "thead", None);
    for (g, group) in table.table.tbody.iter().enumerate() {
        let g = Some(g as u32);
        if let Some(label) = &group.label {
            let at = At {
                row_group: g,
                ..At::section("tbody_label")
            };
            let of = |p: &str| {
                format!(
                    "{p} of the label of tbody group {}",
                    at.row_group.unwrap_or_default()
                )
            };
            ck.content(&label.content, &of, at);
        }
        ck.rows(&group.rows, "tbody", g);
        ck.rows(&group.summary_rows, "tbody_summary", g);
    }

    if let Some(footer) = &table.footer {
        let at = At::section("footer");
        for (i, n) in footer.footnotes.iter().enumerate() {
            ck.content(&n.content, &|p| format!("footer.footnotes[{i}].{p}"), at);
        }
        for (i, n) in footer.source_notes.iter().enumerate() {
            ck.content(&n.content, &|p| format!("footer.source_notes[{i}].{p}"), at);
        }
    }
}
