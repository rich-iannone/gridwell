use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// A content node — rich inline content for cells, titles, footnotes, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
// The derived impls are the known types' (as inherent functions); the trait impls
// below route a node of any other type to `Unknown`.
#[serde(remote = "Self")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentNode {
    Text {
        value: String,
        /// A link: an `http`, `https` or `mailto` URL (see
        /// [`gridwell_core::link`]). Left out when there is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        href: Option<String>,
    },
    StyledText {
        value: String,
        #[serde(default)]
        style_id: Option<String>,
        /// As [`ContentNode::Text`]'s.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        href: Option<String>,
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
    /// Markup in `format`, which that format's writer copies into its output
    /// unchecked: a trust boundary. Rendering with the `raw` option off (in
    /// `gridwell-render`) writes none of it.
    Raw {
        format: String,
        value: String,
    },
    /// A node of a type this gridwell doesn't know (the content-node enum is open,
    /// so a newer minor IR version can add types). It is drawn as nothing, and
    /// written back whole: every field, `type` included.
    #[serde(skip)]
    Unknown(Map<String, Value>),
}

impl ContentNode {
    /// The `type` of every node this gridwell knows.
    pub const KNOWN_TYPES: [&'static str; 6] = [
        "text",
        "styled_text",
        "line_break",
        "footnote_mark",
        "image",
        "raw",
    ];
}

impl Serialize for ContentNode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ContentNode::Unknown(node) => node.serialize(serializer),
            known => ContentNode::serialize(known, serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ContentNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let node = Map::<String, Value>::deserialize(deserializer)?;
        match node.get("type") {
            Some(Value::String(t)) if Self::KNOWN_TYPES.contains(&t.as_str()) => {
                ContentNode::deserialize(Value::Object(node)).map_err(D::Error::custom)
            }
            Some(Value::String(_)) => Ok(ContentNode::Unknown(node)),
            Some(_) => Err(D::Error::custom("a content node's `type` must be a string")),
            None => Err(D::Error::missing_field("type")),
        }
    }
}
