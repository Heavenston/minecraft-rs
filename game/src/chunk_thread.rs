#![expect(unused_imports, reason = "wip")]

use std::{cell::RefCell, collections::{HashMap, HashSet}, sync::Arc, time::Instant};

use anyhow::{Context as _, Result};
use crossbeam_channel::Receiver;
use engine::{MaterialHandle, wgpu::{self, util::DeviceExt as _}};
use enum_map::EnumMap;
use glam::{ISizeVec3, USizeVec3, Vec3, Vec4};
use image::{EncodableLayout as _, Pixel as _};
use ordermap::OrderMap;
use parking_lot::RwLock;
use render_graph::RenderGraph;

use crate::{ToChunkThreadMessage, chunk::{CHUNK_SIZE, Chunk}, chunk_mesher::{ChunkMesher, ChunkTransparencyMode}, data_extractor::MinecraftData, materials::{ChunkMaterial, ChunkRenderData}, resource_location::{ResourceLocation, ResourceLocationMap}, utils::CardinalDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkMaterialKey {
    transparency: ChunkTransparencyMode,
}

struct MeshingState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    texture_layers: EnumMap<u8, Option<ResourceLocation>>,
    mesher: ChunkMesher,
    mcdata: Arc<MinecraftData>,
    materials: Arc<RwLock<engine::MaterialStore>>,
    chunk_materials: HashMap<ChunkMaterialKey, MaterialHandle<ChunkMaterial>>,
    chunks: HashMap<ISizeVec3, Chunk>,
}

impl MeshingState {
    fn clear_all_chunks(&mut self) {
        self.chunks.clear();
        #[expect(clippy::iter_over_hash_type, reason = "me no care")]
        for &handle in self.chunk_materials.values() {
            self.materials.write().get_material_mut(handle).unwrap().chunk_list = Arc::default();
        }
    }

    fn get_chunk_material(&mut self, transparency: ChunkTransparencyMode) -> MaterialHandle<ChunkMaterial> {
        let key = ChunkMaterialKey { transparency };
        if let Some(&material) = self.chunk_materials.get(&key) {
            return material;
        }

        let material = self.materials.write().add_material(ChunkMaterial::new(crate::materials::ChunkRenderConfig {
            texture: self.texture.clone(),
            transparency,
        }));
        self.chunk_materials.insert(key, material);

        material
    }

    fn mesh_chunk(&mut self, chunk_pos: ISizeVec3) -> bool {
        let chunk = &self.chunks[&chunk_pos];
        let Ok(neighbors) = EnumMap::<CardinalDirection, _>::try_from_fn(|direction| {
            let new_pos = chunk_pos + direction;
            self.chunks.get(&new_pos).ok_or(())
        }) else { return false };

        let mesh = self.mesher.mesh_chunk(chunk_pos, chunk, neighbors);
        for submesh in mesh.quad_submeshes {
            let buffer = self.device.create_buffer(&wgpu::wgt::BufferDescriptor {
                label: Some(&format!("chunk,{chunk_pos},{:?}", submesh.direction)),
                size: (submesh.instances.len() * 4).try_into().unwrap(),
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: true,
            });
            buffer.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(bytemuck::cast_slice::<_, u8>(&submesh.instances));
            buffer.unmap();
            let material = self.get_chunk_material(submesh.transparency);

            let mut materials = self.materials.write();
            let material = materials.get_material_mut(material).unwrap();
            material.chunk_list = material.chunk_list.iter().cloned().chain([ChunkRenderData {
                direction: submesh.direction,
                position: (chunk_pos * CHUNK_SIZE.as_isizevec3()).as_vec3(),
                vertex_buffer: buffer,
            }]).collect();
        }

        true
    }

    fn update_textures(&mut self) {
        for (i, (expected, current)) in self.mesher.textures().zip(self.texture_layers.values_mut()).enumerate() {
            if *current == Some(expected) { continue }
            *current = Some(expected);
            let texture_data = self.mcdata.texture(expected);
            assert_eq!(texture_data.image.width(), 16, "Only 16x16 images supported");
            assert_eq!(texture_data.image.height(), 16, "Only 16x16 images supported");
            self.queue.write_texture(wgpu::TexelCopyTextureInfoBase {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: i.try_into().unwrap() },
                aspect: wgpu::TextureAspect::All,
            }, texture_data.image.as_bytes(), wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16 * 4),
                rows_per_image: None,
            }, wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            });
        }
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
        self.mesh_queue.retain(|&pos| !self.store.mesh_chunk(pos));
    }
}

pub fn chunk_thread(seed: u64, receiver: Receiver<ToChunkThreadMessage>, mcdata: Arc<MinecraftData>, device: wgpu::Device, queue: wgpu::Queue, materials: Arc<RwLock<engine::MaterialStore>>) {
    let texture = device.create_texture(&wgpu::wgt::TextureDescriptor {
        label: Some("Blocks texture"),
        size: wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 256 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let mut state = State {
        generator: crate::proc_gen::Generator::new(seed),
        store: MeshingState {
            device,
            queue,
            texture,
            texture_layers: EnumMap::default(),
            mesher: ChunkMesher::new(Arc::clone(&mcdata)),
            mcdata,
            chunk_materials: Default::default(),
            materials,
            chunks: Default::default(),
        },

        mesh_queue: vec![],
    };

    let mut distance = 0isize;
    #[expect(clippy::infinite_loop, reason = "intended")]
    loop {
        tracing::info!(distance);
        for y in -4isize..4isize {
            for dx in -distance..=distance {
                state.gen_chunk(ISizeVec3::new(dx, y, -distance));
                state.gen_chunk(ISizeVec3::new(dx, y, distance));
            }
            for dz in (-distance + 1)..distance {
                state.gen_chunk(ISizeVec3::new(-distance, y, dz));
                state.gen_chunk(ISizeVec3::new(distance, y, dz));
            }
            state.store.update_textures();
        }
        distance += 1;

        while let Ok(msg) = receiver.try_recv() {
            match msg {
                ToChunkThreadMessage::RegenWithSeed(new_seed) => {
                    state.generator = crate::proc_gen::Generator::new(new_seed);
                    state.mesh_queue.clear();
                    state.store.clear_all_chunks();
                    distance = 0;
                },
            }
        }
    }
}
