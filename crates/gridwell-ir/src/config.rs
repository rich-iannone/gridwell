use serde::{Deserialize, Serialize};

use crate::keywords::{Overflow, PageBreakMode, TextDirection};
use crate::style::Border;

/// Table-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub table_cols: u32,
    #[serde(default)]
    pub header_rows: u32,
    #[serde(default)]
    pub body_rows: u32,
    #[serde(default)]
    pub stub_cols: u32,

    #[serde(default)]
    pub row_striping: bool,
    #[serde(default)]
    pub row_striping_include_stub: bool,
    #[serde(default)]
    pub row_striping_include_body: bool,
    #[serde(default)]
    pub column_labels_hidden: bool,

    #[serde(default)]
    pub table_width: Option<String>,
    #[serde(default)]
    pub container_width: Option<String>,
    #[serde(default)]
    pub container_height: Option<String>,
    #[serde(default)]
    pub container_overflow: Option<Overflow>,

    #[serde(default = "default_locale")]
    pub locale: String,
    /// The language of the table's text, a BCP 47 tag (`ar`, `fr-CA`).
    #[serde(default)]
    pub lang: Option<String>,
    /// The direction its columns and text run in.
    #[serde(default)]
    pub dir: Option<TextDirection>,

    #[serde(default)]
    pub page_break_mode: PageBreakMode,

    #[serde(default)]
    pub aria_label: Option<String>,
    #[serde(default)]
    pub aria_describedby: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,

    /// The table's id (an HTML id: not empty, no whitespace), as `gt(id =)` and
    /// `GT(id=)`. The HTML writer uses it unless its `table_id` option is set.
    #[serde(default)]
    pub id: Option<String>,
    /// The table's rules, each replacing one of the default look's.
    #[serde(default)]
    pub rules: Option<Rules>,
}

/// The table's rules (`config.rules`). Each replaces a rule of the default look:
/// a field it sets replaces that field of the default rule, and a `style` of
/// `none` or `hidden` removes the rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rules {
    /// Above the heading (its first line), when the table has one.
    #[serde(default)]
    pub title: Option<Border>,
    /// The table's top: under the heading, above the column labels.
    #[serde(default)]
    pub top: Option<Border>,
    /// Under the column labels.
    #[serde(default)]
    pub column_labels: Option<Border>,
    /// Under each spanner label.
    #[serde(default)]
    pub spanners: Option<Border>,
    /// Above and below each group label.
    #[serde(default)]
    pub group: Option<Border>,
    /// Between rows (above each cell outside the head).
    #[serde(default)]
    pub rows: Option<Border>,
    /// After the stub (right of it, left in a right-to-left table).
    #[serde(default)]
    pub stub: Option<Border>,
    /// Between a group's summary rows and its data rows.
    #[serde(default)]
    pub summary: Option<Border>,
    /// Between the grand summary rows and the rows next to them.
    #[serde(default)]
    pub grand_summary: Option<Border>,
    /// The table's bottom: above the footer.
    #[serde(default)]
    pub bottom: Option<Border>,
    /// Below the footer (its last note), when the table has one.
    #[serde(default)]
    pub notes: Option<Border>,
}

impl Rules {
    /// Every rule with its name, in the order declared.
    pub fn iter(&self) -> [(&'static str, &Option<Border>); 11] {
        [
            ("title", &self.title),
            ("top", &self.top),
            ("column_labels", &self.column_labels),
            ("spanners", &self.spanners),
            ("group", &self.group),
            ("rows", &self.rows),
            ("stub", &self.stub),
            ("summary", &self.summary),
            ("grand_summary", &self.grand_summary),
            ("bottom", &self.bottom),
            ("notes", &self.notes),
        ]
    }
}

fn default_locale() -> String {
    "en-US".to_string()
}
