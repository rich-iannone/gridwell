//! The style cascade and typed, resolved styles.
//!
//! Precedence, lowest to highest (each layer overrides only the properties it sets):
//!
//! 1. the column's default style (`column_spec[].style_id`; body and summary cells),
//! 2. row striping (`config.row_striping`),
//! 3. matching conditional styles, in definition order,
//! 4. the row's style,
//! 5. the cell's style.
//!
//! A style id names a definition or a composition (a definition plus overrides).
//! Values are parsed once here: colors, lengths and font sizes that don't parse and
//! keywords outside their allowed set resolve to `None`, so writers never see them.

use gridwell_core::{Color, FontSize, Length};
use gridwell_ir::style::{Border, BorderSet, Padding, StyleDef};
use gridwell_ir::{
    BorderStyle, FontStyle, FontWeight, HAlign, Keyword, Overflow, Table, TextDecoration,
    TextOverflow, TextTransform, VAlign, WhiteSpace, WordBreak,
};

/// The four sides of a box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sides<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T> Sides<T> {
    /// `(name, value)` for each side, top, right, bottom, left.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &T)> {
        [
            ("top", &self.top),
            ("right", &self.right),
            ("bottom", &self.bottom),
            ("left", &self.left),
        ]
        .into_iter()
    }

    /// Keep the sides `mask` marks; the others become `T::default()`.
    pub fn masked(&self, mask: Sides<bool>) -> Self
    where
        T: Clone + Default,
    {
        let keep = |on: bool, v: &T| if on { v.clone() } else { T::default() };
        Self {
            top: keep(mask.top, &self.top),
            right: keep(mask.right, &self.right),
            bottom: keep(mask.bottom, &self.bottom),
            left: keep(mask.left, &self.left),
        }
    }
}

impl Sides<bool> {
    /// All four sides.
    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };
}

/// One resolved border edge. Present only when the edge is drawn: `none` and
/// `hidden` resolve to no border at all.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBorder {
    pub style: BorderStyle,
    /// `None`: the format's default width (CSS's is `medium`; writers use 1px).
    pub width: Option<Length>,
    /// `None`: the text color (CSS `currentColor`).
    pub color: Option<Color>,
}

/// A border edge as formats without CSS borders draw it: a concrete width and,
/// where one is known, a concrete color. See [`ResolvedStyle::strokes`].
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub style: BorderStyle,
    /// Width in points, always positive and finite.
    pub width_pt: f64,
    /// The edge's color, else the text color; `None`: neither is set (the
    /// format's automatic color, black).
    pub color: Option<Color>,
}

/// Where a cell's content sits in its box, for formats with no baseline
/// alignment. See [`ResolvedStyle::cell_valign`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellVAlign {
    Top,
    Middle,
    Bottom,
}

/// The width of a border with no usable width of its own: CSS's `medium` is
/// 3px, but every writer draws 1px (0.75pt), as browsers' table rules do.
const DEFAULT_BORDER_PT: f64 = 0.75;

/// A fully parsed style: every property typed, every invalid value dropped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedStyle {
    /// The family list as written, with characters that could break out of a CSS
    /// declaration removed. Writers needing a single name take the first entry.
    pub font_family: Option<String>,
    pub font_size: Option<FontSize>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub color: Option<Color>,
    pub background_color: Option<Color>,
    pub text_align: Option<HAlign>,
    pub vertical_align: Option<VAlign>,
    pub text_transform: Option<TextTransform>,
    pub text_decoration: Option<TextDecoration>,
    pub white_space: Option<WhiteSpace>,
    pub padding: Sides<Option<Length>>,
    pub border: Sides<Option<ResolvedBorder>>,
    /// Sides whose border the style explicitly removes (`none` or `hidden`), as
    /// opposed to leaving unset. [`border`](Self::border) is `None` for both; a
    /// format with default rules of its own (HTML's base stylesheet) needs to know
    /// which is which, to remove its default too.
    pub border_cleared: Sides<bool>,
    pub indent: Option<Length>,
    pub word_break: Option<WordBreak>,
    pub overflow: Option<Overflow>,
    pub text_overflow: Option<TextOverflow>,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
}

impl ResolvedStyle {
    /// Resolve a raw style definition.
    pub fn from_def(def: &StyleDef) -> Self {
        let padding = def.padding.as_ref();
        let side = |f: fn(&Padding) -> &Option<String>| padding.and_then(|p| length(f(p)));
        let borders = def.border.as_ref();
        let edge = |f: fn(&BorderSet) -> &Option<Border>| borders.and_then(|b| border(f(b)));
        let cleared = |f: fn(&BorderSet) -> &Option<Border>| {
            borders
                .and_then(|b| f(b).as_ref())
                .and_then(|b| known(&b.style))
                .is_some_and(|s| matches!(s, BorderStyle::None | BorderStyle::Hidden))
        };
        Self {
            font_family: def.font_family.as_deref().and_then(font_family),
            font_size: def.font_size.as_deref().and_then(|s| s.parse().ok()),
            font_weight: known(&def.font_weight),
            font_style: known(&def.font_style),
            color: color(&def.color),
            background_color: color(&def.background_color),
            text_align: known(&def.text_align),
            vertical_align: known(&def.vertical_align),
            text_transform: known(&def.text_transform),
            text_decoration: known(&def.text_decoration),
            white_space: known(&def.white_space),
            padding: Sides {
                top: side(|p| &p.top),
                right: side(|p| &p.right),
                bottom: side(|p| &p.bottom),
                left: side(|p| &p.left),
            },
            border: Sides {
                top: edge(|b| &b.top),
                right: edge(|b| &b.right),
                bottom: edge(|b| &b.bottom),
                left: edge(|b| &b.left),
            },
            border_cleared: Sides {
                top: cleared(|b| &b.top),
                right: cleared(|b| &b.right),
                bottom: cleared(|b| &b.bottom),
                left: cleared(|b| &b.left),
            },
            indent: length(&def.indent),
            word_break: known(&def.word_break),
            overflow: known(&def.overflow),
            text_overflow: known(&def.text_overflow),
            min_width: length(&def.min_width),
            max_width: length(&def.max_width),
        }
    }

    /// True if no property is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Bold: `bold`, `bolder`, or a numeric weight of 600 or more.
    pub fn is_bold(&self) -> bool {
        self.font_weight.as_ref().is_some_and(FontWeight::is_bold)
    }

    /// Bold over what the format draws by default: the style's weight when
    /// it sets one (`normal` and `lighter` un-bold a bold default), else
    /// `default`.
    pub fn bold_over(&self, default: bool) -> bool {
        self.font_weight
            .as_ref()
            .map_or(default, FontWeight::is_bold)
    }

    /// Italic or oblique.
    pub fn is_italic(&self) -> bool {
        self.font_style.as_ref().is_some_and(FontStyle::is_italic)
    }

    /// The text color, if any and not fully transparent.
    pub fn paint(&self) -> Option<Color> {
        self.color.filter(|c| !c.is_transparent())
    }

    /// The background fill, if any and not fully transparent.
    pub fn fill(&self) -> Option<Color> {
        self.background_color.filter(|c| !c.is_transparent())
    }

    /// Underlined (`text_decoration: underline`).
    pub fn is_underline(&self) -> bool {
        self.text_decoration == Some(TextDecoration::Underline)
    }

    /// Struck through (`text_decoration: line-through`).
    pub fn is_strike(&self) -> bool {
        self.text_decoration == Some(TextDecoration::LineThrough)
    }

    /// The `font_family` list as font names, in order, for formats that name
    /// fonts rather than CSS families: names as written (quotes removed), and a
    /// generic family as a font that usually stands for it (`serif` → Times New
    /// Roman, `sans-serif` → Arial, `monospace` → Courier New; their `ui-` forms
    /// alike). Other generic families (`system-ui`, `cursive`, …) and vendor
    /// keywords (`-apple-system`) name no font and are left out; so are repeats.
    pub fn font_names(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for entry in self.font_family.as_deref().unwrap_or("").split(',') {
            let entry = entry.trim();
            let quoted = entry.len() >= 2
                && (entry.starts_with('"') && entry.ends_with('"')
                    || entry.starts_with('\'') && entry.ends_with('\''));
            let name = if quoted {
                // A quoted name is a family name, even `"serif"`.
                entry[1..entry.len() - 1].trim()
            } else {
                match entry.to_ascii_lowercase().as_str() {
                    "serif" | "ui-serif" => "Times New Roman",
                    "sans-serif" | "ui-sans-serif" => "Arial",
                    "monospace" | "ui-monospace" => "Courier New",
                    "system-ui" | "cursive" | "fantasy" | "math" | "emoji" | "fangsong"
                    | "ui-rounded" => continue,
                    _ if entry.starts_with('-') => continue,
                    _ => entry,
                }
            };
            if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                out.push(name);
            }
        }
        out
    }

    /// The font for formats naming one font per run: the first of
    /// [`font_names`](Self::font_names).
    pub fn font_name(&self) -> Option<&str> {
        self.font_names().first().copied()
    }

    /// The font size in points. Relative sizes (`em`, `%`, `smaller`, …) resolve
    /// against `base_pt`, the size the target format uses when none is set.
    /// `None` when unset, or when the size is not a usable positive number.
    pub fn size_pt(&self, base_pt: f64) -> Option<f64> {
        self.font_size
            .as_ref()?
            .to_pt(base_pt, base_pt)
            .filter(|pt| pt.is_finite() && *pt > 0.0)
    }

    /// The cell's vertical alignment. CSS's `baseline`, `sub` and `super` line up
    /// the first lines of a row's cells, which is where top alignment puts them
    /// when the cells share a font size, so they are `Top`.
    pub fn cell_valign(&self) -> Option<CellVAlign> {
        Some(match self.vertical_align.as_ref()? {
            VAlign::Middle => CellVAlign::Middle,
            VAlign::Bottom => CellVAlign::Bottom,
            VAlign::Top | VAlign::Baseline | VAlign::Super | VAlign::Sub => CellVAlign::Top,
            VAlign::Unknown(_) => return None,
        })
    }

    /// The border edges that draw something, for formats that need a concrete
    /// width and color per edge. A width that is unset or doesn't resolve to
    /// points (`%`, `auto`) is 1px; relative widths (`em`) resolve against this
    /// style's size, itself resolved against `base_pt`. Zero-width and fully
    /// transparent edges draw nothing, so they are `None`.
    pub fn strokes(&self, base_pt: f64) -> Sides<Option<Stroke>> {
        let font_pt = self.size_pt(base_pt).unwrap_or(base_pt);
        let stroke = |b: &Option<ResolvedBorder>| {
            let b = b.as_ref()?;
            let width_pt = match &b.width {
                Some(w) => match w.to_pt(font_pt, base_pt) {
                    Some(pt) if pt.is_finite() => pt,
                    _ => DEFAULT_BORDER_PT,
                },
                None => DEFAULT_BORDER_PT,
            };
            let color = b.color.or(self.color);
            if width_pt <= 0.0 || color.is_some_and(|c| c.is_transparent()) {
                return None;
            }
            Some(Stroke {
                style: b.style.clone(),
                width_pt,
                color,
            })
        };
        Sides {
            top: stroke(&self.border.top),
            right: stroke(&self.border.right),
            bottom: stroke(&self.border.bottom),
            left: stroke(&self.border.left),
        }
    }
}

fn known<K: Keyword + Clone>(v: &Option<K>) -> Option<K> {
    v.as_ref().filter(|k| k.is_known()).cloned()
}

fn color(v: &Option<String>) -> Option<Color> {
    v.as_deref()?.parse().ok()
}

fn length(v: &Option<String>) -> Option<Length> {
    v.as_deref()?.parse().ok()
}

fn border(b: &Option<Border>) -> Option<ResolvedBorder> {
    let b = b.as_ref()?;
    let style = known(&b.style)?;
    if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
        return None;
    }
    Some(ResolvedBorder {
        style,
        width: length(&b.width).filter(|l| !l.is_negative()),
        color: color(&b.color),
    })
}

/// Keep font names, commas, spaces, hyphens, underscores, dots and quotes; drop
/// anything that could end a CSS declaration, rule or element.
fn font_family(v: &str) -> Option<String> {
    let kept: String = v
        .chars()
        .filter(|&c| c.is_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '.' | '"' | '\''))
        .collect();
    let kept = kept.trim();
    (!kept.is_empty()).then(|| kept.to_string())
}

/// Overlay `top` onto `base`: every property `top` sets wins; padding and borders
/// merge per side.
pub(crate) fn overlay(base: &mut StyleDef, top: &StyleDef) {
    // Destructured so a new StyleDef field fails to compile until it is handled.
    let StyleDef {
        font_family,
        font_size,
        font_weight,
        font_style,
        color,
        background_color,
        text_align,
        vertical_align,
        text_transform,
        text_decoration,
        white_space,
        padding,
        border,
        indent,
        word_break,
        overflow,
        text_overflow,
        min_width,
        max_width,
    } = top;
    macro_rules! take {
        ($($f:ident),*) => { $( if $f.is_some() { base.$f = $f.clone(); } )* };
    }
    take!(
        font_family,
        font_size,
        font_weight,
        font_style,
        color,
        background_color,
        text_align,
        vertical_align,
        text_transform,
        text_decoration,
        white_space,
        indent,
        word_break,
        overflow,
        text_overflow,
        min_width,
        max_width
    );
    if let Some(p) = padding {
        let b = base.padding.get_or_insert(Padding {
            top: None,
            right: None,
            bottom: None,
            left: None,
        });
        for (dst, src) in [
            (&mut b.top, &p.top),
            (&mut b.right, &p.right),
            (&mut b.bottom, &p.bottom),
            (&mut b.left, &p.left),
        ] {
            if src.is_some() {
                *dst = src.clone();
            }
        }
    }
    if let Some(t) = border {
        let b = base.border.get_or_insert(BorderSet {
            top: None,
            right: None,
            bottom: None,
            left: None,
        });
        for (dst, src) in [
            (&mut b.top, &t.top),
            (&mut b.right, &t.right),
            (&mut b.bottom, &t.bottom),
            (&mut b.left, &t.left),
        ] {
            if src.is_some() {
                *dst = src.clone();
            }
        }
    }
}

/// Look up a style id: a definition, or a composition (its base definition with the
/// overrides laid over it). Unknown ids, and compositions whose base is missing,
/// resolve to `None`.
pub(crate) fn lookup(table: &Table, id: &str) -> Option<StyleDef> {
    let palette = &table.styles;
    if let Some(def) = palette.defs.get(id) {
        return Some(def.clone());
    }
    let comp = palette.compositions.get(id)?;
    let mut def = palette.defs.get(&comp.extends)?.clone();
    overlay(&mut def, &comp.overrides);
    Some(def)
}

/// A style id resolved on its own (header lines, labels, notes).
pub(crate) fn resolve_id(table: &Table, id: Option<&str>) -> ResolvedStyle {
    id.and_then(|id| lookup(table, id))
        .map(|d| ResolvedStyle::from_def(&d))
        .unwrap_or_default()
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn font_families_become_font_names() {
        let names = |f: &str| {
            let s = ResolvedStyle::from_def(&StyleDef {
                font_family: Some(f.into()),
                ..StyleDef::default()
            });
            s.font_names()
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("Georgia, serif"), ["Georgia", "Times New Roman"]);
        assert_eq!(
            names("'Fira Sans', \"Source Sans Pro\" ,SANS-SERIF"),
            ["Fira Sans", "Source Sans Pro", "Arial"]
        );
        assert_eq!(names("ui-monospace, monospace"), ["Courier New"]);
        assert_eq!(names("\"serif\""), ["serif"]);
        assert_eq!(
            names("system-ui, -apple-system, Segoe UI, cursive"),
            ["Segoe UI"]
        );
        assert_eq!(names("system-ui"), Vec::<String>::new());
        assert_eq!(names("Arial, arial, sans-serif"), ["Arial"]);
        assert_eq!(names(" , '' ,Menlo"), ["Menlo"]);
        let s = ResolvedStyle::from_def(&StyleDef {
            font_family: Some("cursive, Georgia".into()),
            ..StyleDef::default()
        });
        assert_eq!(s.font_name(), Some("Georgia"));
        assert_eq!(ResolvedStyle::default().font_name(), None);
    }

    #[test]
    fn removed_borders_are_told_apart_from_unset_ones() {
        let def: StyleDef = serde_json::from_value(serde_json::json!({ "border": {
            "top": { "style": "none" },
            "right": { "style": "hidden" },
            "bottom": { "style": "solid", "width": "1px" }
        }}))
        .unwrap();
        let s = ResolvedStyle::from_def(&def);
        assert!(s.border.top.is_none() && s.border.right.is_none());
        assert!(s.border.bottom.is_some() && s.border.left.is_none());
        let c = &s.border_cleared;
        assert_eq!(
            (c.top, c.right, c.bottom, c.left),
            (true, true, false, false)
        );
        assert!(
            !ResolvedStyle::from_def(&StyleDef::default())
                .border_cleared
                .top
        );
        // An unknown style is neither drawn nor removed.
        let def: StyleDef =
            serde_json::from_value(serde_json::json!({ "border": { "top": { "style": "wavy" } } }))
                .unwrap();
        let s = ResolvedStyle::from_def(&def);
        assert!(s.border.top.is_none() && !s.border_cleared.top);
    }

    fn style(f: impl FnOnce(&mut StyleDef)) -> ResolvedStyle {
        let mut d = StyleDef::default();
        f(&mut d);
        ResolvedStyle::from_def(&d)
    }

    #[test]
    fn decorations() {
        let u = style(|d| d.text_decoration = Some("underline".into()));
        let s = style(|d| d.text_decoration = Some("line-through".into()));
        let o = style(|d| d.text_decoration = Some("overline".into()));
        assert!(u.is_underline() && !u.is_strike());
        assert!(s.is_strike() && !s.is_underline());
        assert!(!o.is_underline() && !o.is_strike());
        assert!(!ResolvedStyle::default().is_underline());
    }

    #[test]
    fn strokes_have_concrete_widths_and_colors() {
        let s = style(|d| {
            d.color = Some("#336699".into());
            d.font_size = Some("20pt".into());
            d.border = Some(BorderSet {
                top: Some(Border {
                    width: Some("2px".into()),
                    style: Some("dashed".into()),
                    color: Some("#FF0000".into()),
                }),
                right: Some(Border {
                    width: None,
                    style: Some("solid".into()),
                    color: None,
                }),
                bottom: Some(Border {
                    width: Some("0.1em".into()),
                    style: Some("double".into()),
                    color: Some("transparent".into()),
                }),
                left: Some(Border {
                    width: Some("0".into()),
                    style: Some("solid".into()),
                    color: None,
                }),
            });
        });
        let k = s.strokes(12.0);
        let top = k.top.unwrap();
        assert_eq!(top.style, BorderStyle::Dashed);
        assert!((top.width_pt - 1.5).abs() < 1e-9);
        assert_eq!(top.color, Some(Color::rgb(255, 0, 0)));
        // Unset width: 1px; unset color: the text color.
        let right = k.right.unwrap();
        assert!((right.width_pt - 0.75).abs() < 1e-9);
        assert_eq!(right.color, Some(Color::rgb(0x33, 0x66, 0x99)));
        // Transparent and zero-width edges draw nothing.
        assert!(k.bottom.is_none() && k.left.is_none());
        // `em` resolves against the style's own size.
        let em = style(|d| {
            d.font_size = Some("20pt".into());
            d.border = Some(BorderSet {
                top: Some(Border {
                    width: Some("0.1em".into()),
                    style: Some("solid".into()),
                    color: None,
                }),
                right: None,
                bottom: None,
                left: None,
            });
        });
        let top = em.strokes(12.0).top.unwrap();
        assert!((top.width_pt - 2.0).abs() < 1e-9, "{top:?}");
        assert_eq!(top.color, None);
        assert_eq!(ResolvedStyle::default().strokes(12.0), Sides::default());
    }

    #[test]
    fn baseline_alignments_sit_at_the_top() {
        let v = |s: &str| style(|d| d.vertical_align = Some(s.into())).cell_valign();
        assert_eq!(v("top"), Some(CellVAlign::Top));
        assert_eq!(v("middle"), Some(CellVAlign::Middle));
        assert_eq!(v("Bottom"), Some(CellVAlign::Bottom));
        for s in ["baseline", "sub", "super"] {
            assert_eq!(v(s), Some(CellVAlign::Top), "{s}");
        }
        assert_eq!(v("center"), None);
        assert_eq!(ResolvedStyle::default().cell_valign(), None);
    }

    #[test]
    fn masking_keeps_marked_sides() {
        let s = Sides {
            top: Some(1),
            right: Some(2),
            bottom: Some(3),
            left: Some(4),
        };
        let m = s.masked(Sides {
            top: true,
            bottom: true,
            ..Sides::default()
        });
        assert_eq!(
            (m.top, m.right, m.bottom, m.left),
            (Some(1), None, Some(3), None)
        );
        assert_eq!(s.masked(Sides::ALL), s);
    }

    #[test]
    fn sizes_resolve_against_the_base() {
        let pt = |v: &str| style(|d| d.font_size = Some(v.into())).size_pt(12.0);
        for (v, want) in [
            ("20px", 15.0),
            ("9pt", 9.0),
            ("1.5em", 18.0),
            ("50%", 6.0),
            ("larger", 14.4),
            ("small", 9.75),
        ] {
            let got = pt(v).unwrap_or(f64::NAN);
            assert!((got - want).abs() < 1e-9, "{v}: {got}");
        }
        assert_eq!(pt("0"), None);
        assert_eq!(ResolvedStyle::default().size_pt(12.0), None);
    }
}
