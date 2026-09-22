#![expect(unused_imports, reason = "wip")]

use std::{cell::RefCell, collections::{HashMap, HashSet, VecDeque}, iter::zip, sync::Arc, time::Instant};

use anyhow::{Context as _, Result};
use crossbeam_channel::Receiver;
use engine::{MaterialHandle, MaterialStore, wgpu::{self, util::DeviceExt as _}};
use enum_map::EnumMap;
use glam::{ISizeVec3, U8Vec4, USizeVec3, Vec3, Vec4, Vec4Swizzles as _};
use image::{EncodableLayout as _, Pixel as _};
use itertools::Itertools as _;
use ordermap::{OrderMap, OrderSet};
use parking_lot::RwLock;
use render_graph::RenderGraph;

use crate::{
    ToChunkThreadMessage, chunk::{CHUNK_SIZE, Chunk}, chunk_mesher::{ChunkMesher, ChunkTransparencyMode}, data_extractor::MinecraftData, materials::{ self, chunk::ChunkMaterial, chunk_full_face::ChunkFullFaceMaterial }, resource_location::{ResourceLocation, ResourceLocationMap}, utils::{CardinalDirection, TwentySixDirection}, world::SharedWorldRef
};

const MIPMAP_LEVELS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkFullFaceMaterialKey {
    transparency: ChunkTransparencyMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkMaterialKey {
    transparency: ChunkTransparencyMode,
}

struct MeshingState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    world: SharedWorldRef,
    texture: wgpu::Texture,
    texture_layers: EnumMap<u8, Option<ResourceLocation>>,
    mesher: ChunkMesher,
    mcdata: Arc<MinecraftData>,
    materials: Arc<RwLock<engine::MaterialStore>>,
    chunk_full_face_materials: HashMap<ChunkFullFaceMaterialKey, MaterialHandle<ChunkFullFaceMaterial>>,
    chunk_materials: HashMap<ChunkMaterialKey, MaterialHandle<ChunkMaterial>>,
}

impl MeshingState {
    fn get_chunk_full_face_material(&mut self, materials: &mut MaterialStore, transparency: ChunkTransparencyMode) -> MaterialHandle<ChunkFullFaceMaterial> {
        let key = ChunkFullFaceMaterialKey { transparency };
        if let Some(&material) = self.chunk_full_face_materials.get(&key) {
            return material;
        }

        let material = materials.add_material(ChunkFullFaceMaterial::new(materials::chunk_full_face::ChunkRenderConfig {
            texture: self.texture.clone(),
            transparency,
        }));
        self.chunk_full_face_materials.insert(key, material);

        material
    }

    fn get_chunk_material(&mut self, materials: &mut MaterialStore, transparency: ChunkTransparencyMode) -> MaterialHandle<ChunkMaterial> {
        let key = ChunkMaterialKey { transparency };
        if let Some(&material) = self.chunk_materials.get(&key) {
            return material;
        }

        let material = materials.add_material(ChunkMaterial::new(materials::chunk::RenderConfig {
            texture: self.texture.clone(),
            transparency,
        }));
        self.chunk_materials.insert(key, material);

        material
    }

    fn mesh_chunk(&mut self, chunk_pos: ISizeVec3) -> bool {
        let world = self.world.read();
        let Some(chunk) = world.get_chunk(chunk_pos).cloned()
        else { return false };
        let Ok(neighbors) = EnumMap::<TwentySixDirection, _>::try_from_fn(|direction| world.get_chunk(chunk_pos + direction).cloned().ok_or(())) else { return false };
        drop(world);

        let mesh = self.mesher.mesh_chunk(chunk_pos, &chunk, EnumMap::from_fn(|dir| &*neighbors[dir]));
        let quad_data = mesh.quad_submeshes.iter()
            .map(|submesh| materials::chunk_full_face::ChunkRenderData::upload(&self.device, chunk_pos, submesh))
            .collect_vec();
        let normal_data = mesh.submeshes.iter()
            .map(|submesh| materials::chunk::ChunkRenderData::upload(&self.device, chunk_pos, submesh))
            .collect_vec();

        let mut materials = self.materials.write_arc();
        #[expect(clippy::iter_over_hash_type, reason = "order does not matter")]
        for &material in self.chunk_full_face_materials.values() {
            materials.get_material_mut(material).unwrap().remove_chunk(chunk_pos);
        }
        #[expect(clippy::iter_over_hash_type, reason = "order does not matter")]
        for &material in self.chunk_materials.values() {
            materials.get_material_mut(material).unwrap().remove_chunk(chunk_pos);
        }

        for (submesh, chunk_render_data) in zip(&mesh.quad_submeshes, quad_data) {
            let material = self.get_chunk_full_face_material(&mut materials, submesh.transparency);
            materials.get_material_mut(material).unwrap().insert_chunk(chunk_render_data);
        }
        for (submesh, chunk_render_data) in zip(&mesh.submeshes, normal_data) {
            let material = self.get_chunk_material(&mut materials, submesh.transparency);
            materials.get_material_mut(material).unwrap().insert_chunk(chunk_render_data);
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
            let mut premimage = texture_data.image.clone();
            for p in premimage.pixels_mut() {
                let v = U8Vec4::from_array(p.0);
                let result = ((v.xyz().as_u16vec3() * u16::from(v.w)) / 255u16).as_u8vec3();
                p.0[0..3].copy_from_slice(&result.to_array());
            }
            let premimage = premimage;
            for level in 1..MIPMAP_LEVELS {
                let nimage = image::imageops::resize(&premimage, texture_data.image.width()/2u32.pow(level), texture_data.image.height()/2u32.pow(level), image::imageops::FilterType::Gaussian);
                self.queue.write_texture(wgpu::TexelCopyTextureInfoBase {
                    texture: &self.texture,
                    mip_level: level,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i.try_into().unwrap() },
                    aspect: wgpu::TextureAspect::All,
                }, nimage.as_bytes(), wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(nimage.width() * 4),
                    rows_per_image: None,
                }, wgpu::Extent3d {
                    width: nimage.width(),
                    height: nimage.height(),
                    depth_or_array_layers: 1,
                });
            }
        }
    }
}

struct State {
    generator: crate::proc_gen::Generator,
    store: MeshingState,

    mesh_queue: OrderSet<ISizeVec3>,
}

impl State {
    fn gen_chunk(&mut self, chunk_pos: ISizeVec3) {
        let chunk = self.generator.generate_chunk(chunk_pos);
        self.store.world.write().set_chunk(chunk_pos, Arc::new(chunk));
        self.mesh_queue.insert(chunk_pos);
    }

    fn drain_mesh_queue(&mut self) {
        self.mesh_queue.retain(|&pos| !self.store.mesh_chunk(pos));
    }
}

pub fn chunk_thread(seed: u64, receiver: &Receiver<ToChunkThreadMessage>, mcdata: Arc<MinecraftData>, world: SharedWorldRef, device: wgpu::Device, queue: wgpu::Queue, materials: Arc<RwLock<engine::MaterialStore>>) {
    let mut gen_queue = VecDeque::<ISizeVec3>::new();

    let texture = device.create_texture(&wgpu::wgt::TextureDescriptor {
        label: Some("Blocks texture"),
        size: wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 256 },
        mip_level_count: MIPMAP_LEVELS,
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
            world,
            texture,
            texture_layers: EnumMap::default(),
            mesher: ChunkMesher::new(Arc::clone(&mcdata)),
            mcdata,
            chunk_full_face_materials: Default::default(),
            chunk_materials: Default::default(),
            materials,
        },

        mesh_queue: OrderSet::new(),
    };

    let mut distance = 0isize;
    #[expect(clippy::infinite_loop, reason = "intended")]
    loop {
        if let Some(chunk_pos) = gen_queue.pop_front() {
            state.gen_chunk(chunk_pos);
        }

        if gen_queue.is_empty() && distance < 20 {
            tracing::info!(distance);
            for y in -5isize..5isize {
                for dx in -distance..=distance {
                    gen_queue.push_back(ISizeVec3::new(dx, y, -distance));
                    gen_queue.push_back(ISizeVec3::new(dx, y, distance));
                }
                for dz in (-distance + 1)..distance {
                    gen_queue.push_back(ISizeVec3::new(-distance, y, dz));
                    gen_queue.push_back(ISizeVec3::new(distance, y, dz));
                }
            }
            distance += 1;
        }

        state.drain_mesh_queue();
        state.store.update_textures();

        if let Some(msg) = if !gen_queue.is_empty() { receiver.try_recv().ok() } else { receiver.recv().ok() } {
            match msg {
                ToChunkThreadMessage::RemeshChunk(pos) => {
                    state.mesh_queue.insert(pos);
                },
            }
        }
    }
}
