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
    /// so a newer minor IR version can add types). It is drawn as its `fallback`,
    /// or as nothing without one, and written back whole: every field, `type`
    /// included.
    #[serde(skip)]
    Unknown(UnknownNode),
}

/// A content node of a type this gridwell doesn't know ([`ContentNode::Unknown`]).
#[derive(Debug, Clone)]
pub struct UnknownNode {
    /// Its fields as written, `type` included; `fallback` too, when it is `null`.
    pub fields: Map<String, Value>,
    /// What a reader that doesn't know the type draws in its place (`fallback`):
    /// content nodes, checked as any content is. `None` when there is none (or it
    /// is `null`): then the node is drawn as nothing.
    pub fallback: Option<Vec<ContentNode>>,
}

impl UnknownNode {
    /// Its `type`.
    pub fn type_name(&self) -> &str {
        self.fields
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

impl ContentNode {
    /// Every node of `nodes`, in order, each followed by the nodes of its
    /// `fallback` (and theirs).
    pub fn walk<'a>(nodes: &'a [ContentNode], f: &mut impl FnMut(&'a ContentNode)) {
        for node in nodes {
            f(node);
            if let ContentNode::Unknown(UnknownNode {
                fallback: Some(fallback),
                ..
            }) = node
            {
                Self::walk(fallback, f);
            }
        }
    }
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
            ContentNode::Unknown(node) => {
                use serde::ser::SerializeMap;
                let n = node.fields.len() + usize::from(node.fallback.is_some());
                let mut map = serializer.serialize_map(Some(n))?;
                for (k, v) in &node.fields {
                    map.serialize_entry(k, v)?;
                }
                if let Some(fallback) = &node.fallback {
                    map.serialize_entry("fallback", fallback)?;
                }
                map.end()
            }
            known => ContentNode::serialize(known, serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ContentNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut node = Map::<String, Value>::deserialize(deserializer)?;
        match node.get("type") {
            Some(Value::String(t)) if Self::KNOWN_TYPES.contains(&t.as_str()) => {
                ContentNode::deserialize(Value::Object(node)).map_err(D::Error::custom)
            }
            Some(Value::String(t)) => {
                let fallback = match node.get("fallback") {
                    None | Some(Value::Null) => None,
                    Some(_) => {
                        let what = format!("the fallback of a content node of type {t:?}");
                        let v = node.remove("fallback").unwrap_or_default();
                        let nodes = Vec::<ContentNode>::deserialize(v)
                            .map_err(|e| D::Error::custom(format!("{what}: {e}")))?;
                        Some(nodes)
                    }
                };
                Ok(ContentNode::Unknown(UnknownNode {
                    fields: node,
                    fallback,
                }))
            }
            Some(_) => Err(D::Error::custom("a content node's `type` must be a string")),
            None => Err(D::Error::missing_field("type")),
        }
    }
}
