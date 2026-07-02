//! Shared test corpus for gridwell.
//!
//! [`examples`] returns the canonical set of example tables used by:
//! - every writer's snapshot tests (textual regression gate), and
//! - the visual harness `xtask` (corpus generation + gallery + image diff).
//!
//! Each [`Example`] carries a stable `name` (a kebab-case slug) used for
//! snapshot names, corpus filenames, and gallery cells, plus a [`Category`] and
//! feature [`tag`](Example::tags)s for grouping and filtering.
//!
//! Tables are constructed with the [`builder`] DSL, which auto-derives row
//! counts and column specs so every example is *valid* by construction (see the
//! `all_examples_validate` test).

pub mod builder;
pub mod examples;

pub use builder::*;

use gridwell_ir::Table;

/// Broad grouping for an example, used in the gallery's table of contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Structure,
    Spans,
    Groups,
    Footnotes,
    Styling,
    Widths,
    Content,
    Alignment,
    I18n,
    Realism,
}

impl Category {
    /// A lowercase, stable slug for the category.
    pub fn slug(self) -> &'static str {
        match self {
            Category::Structure => "structure",
            Category::Spans => "spans",
            Category::Groups => "groups",
            Category::Footnotes => "footnotes",
            Category::Styling => "styling",
            Category::Widths => "widths",
            Category::Content => "content",
            Category::Alignment => "alignment",
            Category::I18n => "i18n",
            Category::Realism => "realism",
        }
    }
}

/// A single IR feature an example exercises. Used for filtering in the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Header,
    Spanner,
    Colspan,
    Rowspan,
    Stub,
    RowGroup,
    Summary,
    Footnote,
    SourceNote,
    Striping,
    Borders,
    Colors,
    StyleComposition,
    Conditional,
    ColumnWidth,
    HiddenColumn,
    StyledText,
    LineBreak,
    Image,
    RawHtml,
    TypedValue,
    Alignment,
    Cjk,
    Rtl,
    Emoji,
    Wide,
    Tall,
}

impl Feature {
    /// A lowercase, stable slug for the feature.
    pub fn slug(self) -> &'static str {
        match self {
            Feature::Header => "header",
            Feature::Spanner => "spanner",
            Feature::Colspan => "colspan",
            Feature::Rowspan => "rowspan",
            Feature::Stub => "stub",
            Feature::RowGroup => "row-group",
            Feature::Summary => "summary",
            Feature::Footnote => "footnote",
            Feature::SourceNote => "source-note",
            Feature::Striping => "striping",
            Feature::Borders => "borders",
            Feature::Colors => "colors",
            Feature::StyleComposition => "style-composition",
            Feature::Conditional => "conditional",
            Feature::ColumnWidth => "column-width",
            Feature::HiddenColumn => "hidden-column",
            Feature::StyledText => "styled-text",
            Feature::LineBreak => "line-break",
            Feature::Image => "image",
            Feature::RawHtml => "raw-html",
            Feature::TypedValue => "typed-value",
            Feature::Alignment => "alignment",
            Feature::Cjk => "cjk",
            Feature::Rtl => "rtl",
            Feature::Emoji => "emoji",
            Feature::Wide => "wide",
            Feature::Tall => "tall",
        }
    }
}

/// A named, categorized example table.
pub struct Example {
    /// Stable kebab-case slug (unique across the corpus).
    pub name: &'static str,
    /// Broad grouping.
    pub category: Category,
    /// One-line human description.
    pub description: &'static str,
    /// IR features this example exercises.
    pub tags: &'static [Feature],
    /// Constructs the table (called lazily so building the registry is cheap).
    pub build: fn() -> Table,
}

impl Example {
    /// Build this example's table.
    pub fn table(&self) -> Table {
        (self.build)()
    }
}

/// The canonical corpus, ordered by category then definition order.
pub fn examples() -> Vec<Example> {
    examples::all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn corpus_has_at_least_50_examples() {
        assert!(
            examples().len() >= 50,
            "expected >= 50 examples, found {}",
            examples().len()
        );
    }

    #[test]
    fn example_names_are_unique() {
        let mut seen = HashSet::new();
        for ex in examples() {
            assert!(seen.insert(ex.name), "duplicate example name: {}", ex.name);
        }
    }

    #[test]
    fn example_names_are_kebab_slugs() {
        for ex in examples() {
            assert!(
                ex.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "example name is not a kebab slug: {}",
                ex.name
            );
        }
    }

    #[test]
    fn all_examples_validate() {
        for ex in examples() {
            let table = ex.table();
            let errors = table.validate();
            assert!(
                errors.is_empty(),
                "example '{}' has validation errors: {:#?}",
                ex.name,
                errors
            );
        }
    }

    #[test]
    fn all_examples_round_trip_json() {
        for ex in examples() {
            let table = ex.table();
            let json = table.to_json().expect("serialize");
            let reparsed = Table::from_json(&json)
                .unwrap_or_else(|e| panic!("example '{}' failed to reparse: {e}", ex.name));
            assert!(
                reparsed.validate().is_empty(),
                "example '{}' invalid after round-trip",
                ex.name
            );
        }
    }
}
