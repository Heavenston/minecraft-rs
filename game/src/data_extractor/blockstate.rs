#![allow(dead_code, reason = "follows schema, even if not everything is used")]

use std::collections::HashMap;
use serde::Deserialize;

use crate::{resource_location::ResourceLocation, utils::{Axis, CardinalDirection, GridAngle}};

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

impl ModelChoice {
    pub fn as_slice(&self) -> &[Model] {
        match self {
            Self::Single(model) => std::slice::from_ref(model),
            Self::Multiple(models) => models,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub struct ModelRotation {
    #[serde(default)]
    pub x: GridAngle,
    #[serde(default)]
    pub y: GridAngle,
    #[serde(default)]
    pub z: GridAngle,
}

impl ModelRotation {
    pub const ZERO: Self = Self {
        x: GridAngle::Zero,
        y: GridAngle::Zero,
        z: GridAngle::Zero,
    };

    pub fn rotate_with_uv(self, mut direction: CardinalDirection) -> (CardinalDirection, GridAngle) {
        let mut uv_rotation = GridAngle::Zero;
        if direction.axis() == Axis::X {
            uv_rotation += self.x;
        }
        direction = direction.rotate(Axis::X, self.x);

        if direction.axis() == Axis::Y {
            uv_rotation += self.y;
        }
        direction = direction.rotate(Axis::Y, self.y);

        if direction.axis() == Axis::Z {
            uv_rotation += self.z;
        }
        direction = direction.rotate(Axis::Z, self.z);

        (direction, uv_rotation)
    }
}

#[derive(Debug, Deserialize)]
pub struct Model {
    #[serde(rename = "model")]
    pub location: ResourceLocation,
    #[serde(default, flatten)]
    pub rotation: ModelRotation,
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
