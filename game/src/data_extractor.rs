use std::{collections::HashMap, io::BufReader, path::{Path, PathBuf}};
use anyhow::{Context as _, Result};
use itertools::Itertools as _;

use crate::resource_location::ResourceLocation;

pub mod blockstate;
pub mod model;

const ASSETS_BASE_PATH: &str = "./minecraft_resources/assets";

fn read_folder<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>, namespace: &str, path_prefix: &str) -> Result<HashMap<ResourceLocation, T>> {
    let dir = std::fs::read_dir(Path::new(ASSETS_BASE_PATH).join(path))?;

    let mut result = HashMap::new();
    for entry in dir {
        let entry = entry?;
        let file = BufReader::new(std::fs::File::open(entry.path())?);
        match serde_json::from_reader::<_, T>(file) {
            Ok(value) => {
                let filename = entry.file_name().to_string_lossy().to_string();
                let filename = filename.split('.').next().unwrap_or_default();
                let Some(resource_location) = ResourceLocation::from_parts(namespace, format!("{path_prefix}{filename}"))
                else {
                    tracing::warn!(file_path = ?entry.path(), filename, "Invalid resource location name");
                    continue;
                };
                result.insert(resource_location, value);
            },
            Err(error) => tracing::warn!(path = ?entry.path(), %error, "Could not read file"),
        }
    }
    Ok(result)
}

pub struct MinecraftData {
    blockstates: HashMap<ResourceLocation, blockstate::BlockState>,
    models: HashMap<ResourceLocation, model::Model>,
}

impl MinecraftData {
    #[expect(clippy::single_call_fn, reason = "I sure hope this is created once")]
    pub fn read() -> Result<Self> {
        let blockstates = read_folder("minecraft/blockstates", "minecraft", "")?;
        let models = read_folder("minecraft/models/block", "minecraft", "block/")?;
        tracing::info!(blockstate_count = blockstates.len(), model_count = models.len(), "Exaction finished");
        Ok(Self { blockstates, models })
    }

    pub fn blockstate(&self, location: &ResourceLocation) -> &blockstate::BlockState {
        &self.blockstates[location]
    }

    pub fn model(&self, location: &ResourceLocation) -> &model::Model {
        &self.models[location]
    }

    pub fn read_texture(&self, location: &ResourceLocation) -> Result<image::DynamicImage> {
        let file_path = Path::new(ASSETS_BASE_PATH).join(location.namespace()).join("textures").join(location.path()).with_extension("png");
        let file = BufReader::new(std::fs::File::open(&file_path).with_context(|| format!("reading file at {}", file_path.display()))?);
        Ok(image::load(file, image::ImageFormat::Png)?)
    }
}
