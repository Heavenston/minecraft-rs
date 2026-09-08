use std::collections::HashMap;
use serde::Deserialize;

use crate::resource_location::ResourceLocation;

#[derive(Debug, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum BlockState {
    Variants {
        variants: HashMap<String, ModelChoice>,
    },
    Multipart {
        multipart: Vec<MultipartCase>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ModelChoice {
    Single(Model),
    Multiple(Vec<Model>),
}

#[derive(Debug, Deserialize)]
pub struct Model {
    pub location: ResourceLocation,
    #[serde(default)]
    pub x: u16,
    #[serde(default)]
    pub y: u16,
    #[serde(default)]
    pub z: u16,
    #[serde(default)]
    pub uvlock: bool,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
pub struct MultipartCase {
    /// When absent, the model always applies.
    #[serde(default)]
    pub when: Option<Condition>,
    pub apply: ModelChoice,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    Logical(LogicalCondition),
    /// All properties must match. Values may contain `|`.
    States(HashMap<String, String>),
}

#[derive(Debug, Deserialize)]
pub enum LogicalCondition {
    #[serde(rename = "OR")]
    Or(Vec<Condition>),

    #[serde(rename = "AND")]
    And(Vec<Condition>),
}
