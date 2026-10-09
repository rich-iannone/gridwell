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
//! Values are parsed once here: colours, lengths and font sizes that don't parse and
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
}

/// One resolved border edge. Present only when the edge is drawn: `none` and
/// `hidden` resolve to no border at all.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBorder {
    pub style: BorderStyle,
    /// `None`: the format's default width (CSS's is `medium`; writers use 1px).
    pub width: Option<Length>,
    /// `None`: the text colour (CSS `currentColor`).
    pub color: Option<Color>,
}

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

    /// Italic or oblique.
    pub fn is_italic(&self) -> bool {
        self.font_style.as_ref().is_some_and(FontStyle::is_italic)
    }

    /// The text colour, if any and not fully transparent.
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

    /// The font size in points. Relative sizes (`em`, `%`, `smaller`, …) resolve
    /// against `base_pt`, the size the target format uses when none is set.
    /// `None` when unset, or when the size is not a usable positive number.
    pub fn size_pt(&self, base_pt: f64) -> Option<f64> {
        self.font_size
            .as_ref()?
            .to_pt(base_pt, base_pt)
            .filter(|pt| pt.is_finite() && *pt > 0.0)
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
