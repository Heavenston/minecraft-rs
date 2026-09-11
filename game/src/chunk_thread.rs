#![expect(unused_imports, reason = "wip")]

use std::{cell::RefCell, collections::{HashMap, HashSet}, sync::Arc, time::Instant};

use anyhow::{Context as _, Result};
use engine::{MaterialHandle, wgpu::{self, util::DeviceExt as _}};
use enum_map::EnumMap;
use glam::{ISizeVec3, USizeVec3, Vec3, Vec4};
use image::{EncodableLayout as _, Pixel as _};
use ordermap::OrderMap;
use parking_lot::RwLock;
use render_graph::RenderGraph;

use crate::{chunk::{CHUNK_SIZE, Chunk}, chunk_mesher::mesh_chunk, data_extractor::MinecraftData, materials::{ChunkMaterial, ChunkRenderData, ChunkTransparencyMode}, resource_location::{ResourceLocation, ResourceLocationMap}, utils::{CardinalDirection, ISizeVec3Range}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkMaterialKey {
    texture_location: ResourceLocation,
    transparency: ChunkTransparencyMode,
}

#[derive(Debug, Clone)]
struct TextureData {
    texture: wgpu::Texture,
    present_transparency: ChunkTransparencyMode,
}

struct MeshingState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    mcdata: Arc<MinecraftData>,
    materials: Arc<RwLock<engine::MaterialStore>>,
    textures: ResourceLocationMap<TextureData>,
    chunk_materials: HashMap<ChunkMaterialKey, MaterialHandle<ChunkMaterial>>,
    chunks: HashMap<ISizeVec3, Chunk>,
}

impl MeshingState {
    fn get_texture(&mut self, location: ResourceLocation) -> Result<TextureData> {
        if let Some(texture) = self.textures.get(&location).cloned() {
            return Ok(texture);
        }

        let mut image = MinecraftData::read_texture(location).with_context(|| format!("reading mc texture {location}"))?;
        image.apply_color_space(image::metadata::Cicp::SRGB, image::ConvertColorOptions::default())?;
        let image = image.to_rgba8();

        let has_transparent = image.pixels().any(|p| p.alpha() == 0);
        let has_translucent = image.pixels().any(|p| (1..u8::MAX).contains(&p.alpha()));

        let texture = self.device.create_texture_with_data(&self.queue, &wgpu::wgt::TextureDescriptor {
            label: Some(location.as_str()),
            size: wgpu::Extent3d { width: image.width(), height: image.height(), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }, wgpu::wgt::TextureDataOrder::LayerMajor, image.as_bytes());

        let data = TextureData {
            texture,
            present_transparency: if has_translucent {
                ChunkTransparencyMode::Translucent
            } else if has_transparent {
                ChunkTransparencyMode::Cutout
            } else {
                ChunkTransparencyMode::Opaque
            },
        };
        self.textures.insert(location, data.clone());

        Ok(data)
    }

    fn get_chunk_material(&mut self, texture_location: ResourceLocation, force_translucent: bool) -> Result<MaterialHandle<ChunkMaterial>> {
        let TextureData { texture, present_transparency } = self.get_texture(texture_location)?;
        let transparency = if force_translucent {
            ChunkTransparencyMode::Translucent
        } else {
            present_transparency
        };
        let key = ChunkMaterialKey { texture_location, transparency };
        if let Some(&material) = self.chunk_materials.get(&key) {
            return Ok(material);
        }

        let material = self.materials.write().add_material(ChunkMaterial::new(crate::materials::ChunkRenderConfig {
            texture,
            transparency,
        }));
        self.chunk_materials.insert(key, material);

        Ok(material)
    }

    fn mesh_chunk(&mut self, chunk_pos: ISizeVec3) -> Result<bool> {
        let chunk = &self.chunks[&chunk_pos];
        let Ok(neighbors) = EnumMap::<CardinalDirection, _>::try_from_fn(|direction| {
            let new_pos = chunk_pos + direction;
            self.chunks.get(&new_pos).ok_or(())
        }) else { return Ok(false) };

        let mesh = crate::chunk_mesher::mesh_chunk(&self.mcdata, chunk, neighbors);
        for submesh in mesh.quad_submeshes {
            let buffer = self.device.create_buffer(&wgpu::wgt::BufferDescriptor {
                label: Some(&format!("chunk,{chunk_pos},{:?},{}", submesh.direction, submesh.texture)),
                size: (submesh.instances.len() * 4).try_into().unwrap(),
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: true,
            });
            buffer.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(bytemuck::cast_slice::<_, u8>(&submesh.instances));
            buffer.unmap();
            let material = self.get_chunk_material(submesh.texture, submesh.force_translucent)?;

            let mut materials = self.materials.write();
            let material = materials.get_material_mut(material).unwrap();
            material.chunk_list = material.chunk_list.iter().cloned().chain([ChunkRenderData {
                direction: submesh.direction,
                position: (chunk_pos * CHUNK_SIZE.as_isizevec3()).as_vec3(),
                vertex_buffer: buffer,
            }]).collect();
        }

        Ok(true)
    }
}

struct State {
    generator: crate::proc_gen::Generator,
    store: MeshingState,

    mesh_queue: Vec<ISizeVec3>,
}

impl State {
    fn gen_chunk(&mut self, chunk_pos: ISizeVec3) {
        if self.store.chunks.contains_key(&chunk_pos) { return }
        self.store.chunks.insert(chunk_pos, self.generator.generate_chunk(chunk_pos));
        self.mesh_queue.push(chunk_pos);
        self.mesh_queue.retain(|&pos| !self.store.mesh_chunk(pos).unwrap());
    }
}

pub fn chunk_thread(seed: u64, mcdata: Arc<MinecraftData>, device: wgpu::Device, queue: wgpu::Queue, materials: Arc<RwLock<engine::MaterialStore>>) {
    let mut state = State {
        generator: crate::proc_gen::Generator::new(seed),
        store: MeshingState {
            device,
            queue,
            mcdata,
            textures: Default::default(),
            chunk_materials: Default::default(),
            materials,
            chunks: Default::default(),
        },

        mesh_queue: vec![],
    };

    for distance in 0..15isize {
        tracing::info!("{:2}/15", distance);
        for y in -4isize..4isize {
            for dx in -distance..=distance {
                state.gen_chunk(ISizeVec3::new(dx, y, -distance));
                state.gen_chunk(ISizeVec3::new(dx, y, distance));
            }
            for dz in (-distance + 1)..distance {
                state.gen_chunk(ISizeVec3::new(-distance, y, dz));
                state.gen_chunk(ISizeVec3::new(distance, y, dz));
            }
        }
    }
}
