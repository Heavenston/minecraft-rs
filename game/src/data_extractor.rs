use std::{io::BufReader, path::Path};
use anyhow::{Context as _, Result};
use itertools::Itertools as _;

use crate::resource_location::{ResourceLocation, ResourceLocationMap};

pub mod blockstate;
pub mod model;

const ASSETS_BASE_PATH: &str = "./minecraft_resources/assets";

fn read_folder<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>, namespace: &str) -> Result<ResourceLocationMap<T>> {
    let path = path.as_ref();
    let entries = dirwalk::WalkBuilder::new(path)
        .extensions(["json"])
        .iter()?;

    let mut result = ResourceLocationMap::default();
    for entry in entries {
        let entry = entry?;
        if entry.is_dir || entry.is_hidden { continue }
        let file_path = path.join(&entry.relative_path);

        let file = BufReader::new(std::fs::File::open(&file_path)?);
        match serde_json::from_reader::<_, T>(file) {
            Ok(value) => {
                let filename = entry.relative_path.trim_end_matches(".json").to_string();
                let Some(resource_location) = ResourceLocation::new(&format!("{namespace}:{filename}"))
                else {
                    tracing::warn!(?file_path, filename, "Invalid resource location name");
                    continue;
                };
                result.insert(resource_location, value);
            },
            Err(error) => tracing::warn!(?file_path, %error, "Could not read file"),
        }
    }
    Ok(result)
}

pub struct MinecraftData {
    blockstates: ResourceLocationMap<blockstate::BlockState>,
    models: ResourceLocationMap<model::Model>,
}
static_assertions::assert_impl_all!(MinecraftData: Send, Sync);

impl MinecraftData {
    #[expect(clippy::single_call_fn, reason = "I sure hope this is created once")]
    pub fn read() -> Result<Self> {
        let mut blockstates = ResourceLocationMap::<blockstate::BlockState>::default();
        let mut models = ResourceLocationMap::<model::Model>::default();
        for subfolder in std::fs::read_dir(ASSETS_BASE_PATH)? {
            let subfolder = subfolder?;
            if !subfolder.file_type()?.is_dir() { continue }
            match subfolder.file_name().into_string() {
                Err(str) => {
                    tracing::warn!(?str, "Folder in assets has invalid characters");
                },
                Ok(s) if !ResourceLocation::check_namespace(&s) => {
                    tracing::warn!(namespace = s, "Folder in assets is not a valid namespace");
                },
                Ok(namespace) => {
                    blockstates.extend(read_folder(subfolder.path().join("blockstates"), &namespace)?);
                    models.extend(read_folder(subfolder.path().join("models"), &namespace)?);
                },
            }
        }
        tracing::info!(blockstate_count = blockstates.len(), model_count = models.len(), namespaces = ?blockstates.keys().chain(models.keys()).copied().map(ResourceLocation::namespace).unique().collect_vec(), "Exaction finished");
        Ok(Self { blockstates, models })
    }

    pub fn blockstate(&self, location: ResourceLocation) -> &blockstate::BlockState {
        &self.blockstates[&location]
    }

    pub fn model(&self, location: ResourceLocation) -> &model::Model {
        self.models.get(&location).unwrap_or_else(|| panic!("Could not find block model {location}"))
    }

    pub fn read_texture(location: ResourceLocation) -> Result<image::DynamicImage> {
        let file_path = Path::new(ASSETS_BASE_PATH).join(location.namespace()).join("textures").join(location.path()).with_extension("png");
        let file = BufReader::new(std::fs::File::open(&file_path).with_context(|| format!("reading file at {}", file_path.display()))?);
        Ok(image::load(file, image::ImageFormat::Png)?)
    }
}
