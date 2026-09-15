use std::{io::BufReader, path::Path};
use anyhow::Result;
use image::{Pixel as _, RgbaImage};
use itertools::Itertools as _;

use crate::resource_location::{ResourceLocation, ResourceLocationMap};

pub mod blockstate;
pub mod model;

const ASSETS_BASE_PATH: &str = "./minecraft_resources/assets";

fn read_folder<T, F>(path: impl AsRef<Path>, namespace: &str, extension: &str, mut parser: F) -> Result<ResourceLocationMap<T>>
    where F: FnMut(BufReader<std::fs::File>) -> Result<T>,
{
    let path = path.as_ref();
    if !std::fs::exists(path)? { return Ok(Default::default()); }
    let entries = dirwalk::WalkBuilder::new(path)
        .extensions([extension])
        .iter()?;

    let mut result = ResourceLocationMap::default();
    for entry in entries {
        let entry = entry?;
        if entry.is_dir || entry.is_hidden { continue }
        let file_path = path.join(&entry.relative_path);

        let file = BufReader::new(std::fs::File::open(&file_path)?);
        match parser(file) {
            Ok(value) => {
                let filename = entry.relative_path.trim_end_matches(&format!(".{extension}")).to_string();
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

pub struct TextureInfo {
    pub image: RgbaImage,
    /// Wether the image contains any completely transparent pixels.
    pub has_transparent: bool,
    /// Wether the image contains any partially transparent pixels.
    pub has_translucent: bool,
}

impl TextureInfo {
    fn from_file(file: BufReader<std::fs::File>) -> Result<Self> {
        let mut image = image::load(file, image::ImageFormat::Png)?;
        image.apply_color_space(image::metadata::Cicp::SRGB, image::ConvertColorOptions::default())?;
        let image = image.to_rgba8();

        let has_transparent = image.pixels().any(|p| p.alpha() == 0);
        let has_translucent = image.pixels().any(|p| (1..u8::MAX).contains(&p.alpha()));
        
        Ok(Self {
            image,
            has_transparent,
            has_translucent,
        })
    }
}

pub struct MinecraftData {
    blockstates: ResourceLocationMap<blockstate::BlockState>,
    models: ResourceLocationMap<model::Model>,
    textures: ResourceLocationMap<TextureInfo>,
}
static_assertions::assert_impl_all!(MinecraftData: Send, Sync);

impl MinecraftData {
    #[expect(clippy::single_call_fn, reason = "I sure hope this is created once")]
    pub fn read() -> Result<Self> {
        let mut blockstates = ResourceLocationMap::<blockstate::BlockState>::default();
        let mut models = ResourceLocationMap::<model::Model>::default();
        let mut textures = ResourceLocationMap::<TextureInfo>::default();
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
                    blockstates.extend(read_folder(subfolder.path().join("blockstates"), &namespace, "json", |p| Ok(serde_json::from_reader::<_,blockstate::BlockState>(p)?))?);
                    models.extend(read_folder(subfolder.path().join("models"), &namespace, "json", |p| Ok(serde_json::from_reader::<_,model::Model>(p)?))?);
                    textures.extend(read_folder(subfolder.path().join("textures"), &namespace, "png", TextureInfo::from_file)?);
                },
            }
        }
        tracing::info!(
            blockstate_count = blockstates.len(),
            model_count = models.len(),
            texture_count = textures.len(),
            block_texture_count = textures.keys().filter(|key| key.path().starts_with("block/")).count(),
            namespaces = ?blockstates.keys().chain(models.keys()).chain(textures.keys()).copied().map(ResourceLocation::namespace).unique().collect_vec(),
            "Exaction finished",
        );
        Ok(Self { blockstates, models, textures })
    }

    pub fn blockstate(&self, location: ResourceLocation) -> &blockstate::BlockState {
        &self.blockstates[&location]
    }

    pub fn model(&self, location: ResourceLocation) -> &model::Model {
        self.models.get(&location).unwrap_or_else(|| panic!("Could not find block model {location}"))
    }

    pub fn texture(&self, location: ResourceLocation) -> &TextureInfo {
        self.textures.get(&location).unwrap_or_else(|| panic!("Could not find texture {location}"))
    }
}
