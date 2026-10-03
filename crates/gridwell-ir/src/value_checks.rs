//! INVALID_COLOR and INVALID_LENGTH: every colour and length in the IR parses, and
//! each length uses a form that makes sense for its field (no `fr` padding, no
//! negative widths, …).

use crate::cell::Row;
use crate::content::ContentNode;
use crate::style::StyleDef;
use crate::validation::{ValidationError, ValidationRule};
use crate::Table;
use gridwell_core::{Color, FontSize, Length};

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
                let bf = |name: &str| f(&format!("border.{side}.{name}"));
                self.length(b.width.as_ref(), &bf("width"), LengthKind::BORDER, at);
                self.color(b.color.as_ref(), &bf("color"), at);
            }
        }
    }

    fn content(&mut self, nodes: &[ContentNode], field: &str, at: At) {
        for (i, node) in nodes.iter().enumerate() {
            if let ContentNode::Image { width, height, .. } = node {
                let f = |name: &str| format!("{field}[{i}].{name}");
                self.length(width.as_ref(), &f("width"), LengthKind::SIZE, at);
                self.length(height.as_ref(), &f("height"), LengthKind::SIZE, at);
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
                self.content(&cell.content, "cell content", at);
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
            ck.content(&t.content, "header.title.content", at);
        }
        if let Some(t) = &header.subtitle {
            ck.content(&t.content, "header.subtitle.content", at);
        }
        for (i, line) in header.extra_lines.iter().enumerate() {
            ck.content(
                &line.content,
                &format!("header.extra_lines[{i}].content"),
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
            ck.content(&label.content, "label.content", at);
        }
        ck.rows(&group.rows, "tbody", g);
        ck.rows(&group.summary_rows, "tbody_summary", g);
    }

    if let Some(footer) = &table.footer {
        let at = At::section("footer");
        for (i, n) in footer.footnotes.iter().enumerate() {
            ck.content(&n.content, &format!("footer.footnotes[{i}].content"), at);
        }
        for (i, n) in footer.source_notes.iter().enumerate() {
            ck.content(&n.content, &format!("footer.source_notes[{i}].content"), at);
        }
    }
}
