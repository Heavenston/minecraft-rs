use std::{collections::HashMap, io::BufReader, path::{Path, PathBuf}};
use anyhow::{Context, Result};
use itertools::Itertools;

use crate::resource_location::ResourceLocation;

mod blockstate;
mod model;

const ASSETS_BASE_PATH: &str = "./minecraft_resources/assets";

fn read_blockstates() -> Result<HashMap<ResourceLocation, blockstate::BlockState>> {
    let dir = std::fs::read_dir(Path::new(ASSETS_BASE_PATH).join("minecraft/blockstates"))?;

    let mut blockstates = HashMap::new();
    for entry in dir {
        let entry = entry?;
        let file = BufReader::new(std::fs::File::open(entry.path())?);
        match serde_json::from_reader::<_, blockstate::BlockState>(file) {
            Ok(bs) => {
                let filename = entry.file_name().to_string_lossy().to_string();
                let filename = filename.split('.').next().unwrap_or_default();
                let Some(resource_location) = ResourceLocation::from_parts("minecraft", filename)
                else {
                    tracing::warn!(file_path = ?entry.path(), filename, "Invalid resource location name for blockstate");
                    continue;
                };
                blockstates.insert(resource_location, bs);
            },
            Err(error) => tracing::warn!(path = ?entry.path(), %error, "Could not read blockstate file"),
        }
    }
    Ok(blockstates)
}

#[derive(Default)]
struct ModelStore {
    models: HashMap<ResourceLocation, model::Model>
}

impl ModelStore {
    fn read_model(&mut self, location: &ResourceLocation) -> Result<&model::Model> {
        if let Some(model) = self.models.get(location) {
            return Ok(model);
        }
        let path = Path::new(ASSETS_BASE_PATH).join(location.namespace()).join("models").join(location.path()).with_extension("json");
        let file = BufReader::new(std::fs::File::open(&path).with_context(|| format!("Reading file {path:?}"))?);
        let model = serde_json::from_reader::<_, model::Model>(file)?;
        Ok(self.models.entry(location.clone()).insert_entry(model).into_mut())
    }
}

pub fn run() -> Result<()> {
    let blockstates = read_blockstates()?;
    let mut model_store = ModelStore::default();

    for (l, blockstate) in &blockstates {
        let blockstate::BlockState::Variants { variants } = blockstate
        else { continue };
        let Ok(blockstate::ModelChoice::Single(state_model)) = variants.values().exactly_one()
        else { continue };
        let model = match model_store.read_model(&state_model.model) {
            Ok(model) => model,
            Err(error) => {
                tracing::warn!(location = %state_model.model, ?error, "Could not read model file");
                continue;
            },
        };
        // if model.parent.as_ref().is_some_and(|p| p.as_str() == "minecraft:block/cube_all") {
        //     println!("{l} - {model:#?}");
        // }
    }
    
    Ok(())
}
