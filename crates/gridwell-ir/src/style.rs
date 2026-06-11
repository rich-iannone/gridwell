use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The style palette: definitions, compositions, and conditionals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StylePalette {
    #[serde(default)]
    pub defs: HashMap<String, StyleDef>,
    #[serde(default)]
    pub compositions: HashMap<String, StyleComposition>,
    #[serde(default)]
    pub conditionals: Vec<ConditionalStyle>,
}

/// A style definition (all fields optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleDef {
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<String>,
    #[serde(default)]
    pub font_weight: Option<String>,
    #[serde(default)]
    pub font_style: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub text_align: Option<String>,
    #[serde(default)]
    pub vertical_align: Option<String>,
    #[serde(default)]
    pub text_transform: Option<String>,
    #[serde(default)]
    pub text_decoration: Option<String>,
    #[serde(default)]
    pub white_space: Option<String>,
    #[serde(default)]
    pub padding: Option<Padding>,
    #[serde(default)]
    pub border: Option<BorderSet>,
    #[serde(default)]
    pub indent: Option<String>,
    #[serde(default)]
    pub word_break: Option<String>,
    #[serde(default)]
    pub overflow: Option<String>,
    #[serde(default)]
    pub text_overflow: Option<String>,
    #[serde(default)]
    pub min_width: Option<String>,
    #[serde(default)]
    pub max_width: Option<String>,
}

/// Padding (four sides).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Padding {
    #[serde(default)]
    pub top: Option<String>,
    #[serde(default)]
    pub right: Option<String>,
    #[serde(default)]
    pub bottom: Option<String>,
    #[serde(default)]
    pub left: Option<String>,
}

/// A set of borders (four sides).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderSet {
    #[serde(default)]
    pub top: Option<Border>,
    #[serde(default)]
    pub right: Option<Border>,
    #[serde(default)]
    pub bottom: Option<Border>,
    #[serde(default)]
    pub left: Option<Border>,
}

/// A single border edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Border {
    #[serde(default)]
    pub width: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

