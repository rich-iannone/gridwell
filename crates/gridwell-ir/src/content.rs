use serde::{Deserialize, Serialize};

/// A content node — rich inline content for cells, titles, footnotes, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentNode {
    Text {
        value: String,
    },
    StyledText {
        value: String,
        #[serde(default)]
        style_id: Option<String>,
    },
    LineBreak {},
    FootnoteMark {
        #[serde(rename = "ref")]
        reference: String,
        mark_text: String,
    },
    Image {
        src: String,
        #[serde(default)]
        alt: Option<String>,
        #[serde(default)]
        width: Option<String>,
        #[serde(default)]
        height: Option<String>,
    },
    Raw {
        format: String,
        value: String,
    },
    /// Catch-all for unknown/future content node types (open enum).
    #[serde(other)]
    Unknown,
}
