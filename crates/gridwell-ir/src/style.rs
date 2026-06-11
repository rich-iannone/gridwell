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

