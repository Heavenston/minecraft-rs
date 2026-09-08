use std::{collections::HashMap, io::BufReader};
use anyhow::Result;
use itertools::Itertools;

use crate::resource_location::ResourceLocation;

mod blockstate;

fn read_blockstates() -> Result<HashMap<ResourceLocation, blockstate::BlockState>> {
    let dir = std::fs::read_dir("minecraft_resources/assets/minecraft/blockstates")?;

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

pub fn run() -> Result<()> {
    let blockstates = read_blockstates()?;

    println!("{:?}", blockstates.keys().cloned().collect_vec());
    
    Ok(())
}
