use serde::Deserialize;
use std::collections::HashMap;

use crate::resource_location::ResourceLocation;

#[derive(Debug, Deserialize)]
pub struct Model {
    pub parent: Option<ResourceLocation>,

    /// None preserves inheritance; defaults to true when resolved.
    pub ambientocclusion: Option<bool>,

    #[serde(default)]
    pub display: HashMap<DisplayPosition, DisplayTransform>,

    #[serde(default)]
    pub textures: HashMap<String, Texture>,

    /// None inherits parent elements; Some replaces them, even if empty.
    pub elements: Option<Vec<Element>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayPosition {
    ThirdpersonRighthand,
    ThirdpersonLefthand,
    FirstpersonRighthand,
    FirstpersonLefthand,
    Gui,
    Head,
    Ground,
    Fixed,
    OnShelf,
}

#[derive(Debug, Deserialize)]
pub struct DisplayTransform {
    #[serde(default)]
    pub rotation: [f32; 3],

    #[serde(default)]
    pub translation: [f32; 3],

    #[serde(default = "unit_scale")]
    pub scale: [f32; 3],
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Texture {
    /// Resource location or a reference such as "#side".
    Reference(ResourceLocation),
    Detailed {
        sprite: String,

        #[serde(default)]
        force_translucent: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<ElementRotation>,

    #[serde(default = "default_true")]
    pub shade: bool,

    pub shade_direction_override: Option<Direction>,

    #[serde(default)]
    pub light_emission: u8,

    #[serde(default)]
    pub faces: HashMap<Direction, Face>,
}

#[derive(Debug, Deserialize)]
pub struct ElementRotation {
    pub origin: [f32; 3],

    #[serde(default)]
    pub x: f32,

    #[serde(default)]
    pub y: f32,

    #[serde(default)]
    pub z: f32,

    pub axis: Option<Axis>,
    pub angle: Option<f32>,

    #[serde(default)]
    pub rescale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

#[derive(Debug, Deserialize)]
pub struct Face {
    /// None means UVs must be generated from the element's position.
    pub uv: Option<[f32; 4]>,

    pub texture: String,
    pub cullface: Option<Direction>,

    #[serde(default)]
    pub rotation: u16,

    #[serde(default = "default_tintindex")]
    pub tintindex: i32,
}

fn unit_scale() -> [f32; 3] {
    [1.0; 3]
}

fn default_true() -> bool {
    true
}

fn default_tintindex() -> i32 {
    -1
}
