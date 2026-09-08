#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(const_ops)]
#![feature(const_convert)]
#![feature(const_cmp)]
#![feature(const_index)]
#![feature(array_try_map)]

#![allow(dead_code, reason = "in development")]
#![allow(unused_imports, reason = "in development")]
#![allow(clippy::single_call_fn, reason = "in development")]

use std::{cell::RefCell, collections::HashMap};

use anyhow::{Context, Result};
use engine::{wgpu::{self, util::DeviceExt}, world::MaterialHandle};
use enum_map::EnumMap;
use glam::{ISizeVec3, Vec4};
use image::EncodableLayout;
use itertools::Itertools as _;

use crate::{chunk::{CHUNK_SIZE, Chunk}, chunk_mesher::{ChunkSubMesh, mesh_chunk}, data_extractor::MinecraftData, materials::{ChunkMaterial, ChunkRenderData}, resource_location::ResourceLocation, utils::{CardinalDirection, ISizeVec3Range, Vec3Range}};

mod chunk;
mod chunk_mesher;
mod resource_location;
mod utils;
mod proc_gen;
mod materials;

mod data_extractor;

struct App {
    vsync: bool,
    material: Option<MaterialHandle<engine::material::Rotating>>,

    mc_data: MinecraftData,
    chunks: HashMap<ISizeVec3, Chunk>,
    generator: proc_gen::Generator,

    chunk_materials: RefCell<HashMap<ResourceLocation, MaterialHandle<ChunkMaterial>>>,
}

impl App {
    fn get_chunk_material(&self, ctx: &mut engine::ResumeCtx<'_>, location: &ResourceLocation) -> Result<MaterialHandle<ChunkMaterial>> {
        if let Some(&material) = self.chunk_materials.borrow().get(location) {
            return Ok(material);
        }

        let image = self.mc_data.read_texture(location).with_context(|| format!("reading mc texture {location}"))?.to_rgba8();
        let texture = ctx.renderer.device().create_texture_with_data(ctx.renderer.queue(), &wgpu::wgt::TextureDescriptor {
            label: Some(location.as_str()),
            size: wgpu::Extent3d { width: image.width(), height: image.height(), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }, wgpu::wgt::TextureDataOrder::LayerMajor, image.as_bytes());

        let material = ctx.world.add_material(ChunkMaterial::new(texture));
        self.chunk_materials.borrow_mut().insert(location.clone(), material);

        Ok(material)
    }
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::ResumeCtx<'_>) -> Result<()> {
        tracing::info!("Meshing start chunks");

        #[expect(clippy::iter_over_hash_type, reason = "order is not observable")]
        for (&chunk_position, chunk) in &self.chunks {
            let Some(neighbors) = CardinalDirection::VALUES.try_map(|direction| {
                let new_pos = chunk_position + direction;
                self.chunks.get(&new_pos)
            }).map(EnumMap::from_array)
            else { continue };
            let mesh = mesh_chunk(&self.mc_data, chunk, neighbors);
            for submesh in mesh.sub_meshes {
                let buffer = ctx.renderer.device().create_buffer(&wgpu::wgt::BufferDescriptor {
                    label: Some("chunk instance data"),
                    size: (submesh.instances.len() * 4).try_into().unwrap(),
                    usage: wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: true,
                });
                let padded_data = submesh.instances.iter().flat_map(|&a| [a, 0]).collect_vec();
                buffer.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(bytemuck::cast_slice::<_, u8>(&padded_data));
                buffer.unmap();
                let material = self.get_chunk_material(ctx, submesh.texture)?;
                let material = ctx.world.get_material_mut(material).unwrap();
                material.chunk_list = material.chunk_list.iter().cloned().chain([ChunkRenderData {
                    direction: submesh.face,
                    position: (chunk_position * CHUNK_SIZE.as_isizevec3()).as_vec3(),
                    vertex_buffer: buffer,
                }]).collect();
            }
        }

        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        ctx.renderer.render_graph().set_input::<engine::renderer::resources::PresentMode>(wgpu::PresentMode::AutoVsync);
        
        Ok(())
    }

    fn update(&mut self, ctx: engine::Ctx<'_>) -> Result<()> {
        let Some(material) = self.material
        else { return Ok(()); };

        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyV) {
            if self.vsync {
                self.vsync = false;
                println!("DISABLE");
                ctx.renderer.render_graph().set_input::<engine::renderer::resources::PresentMode>(wgpu::PresentMode::AutoNoVsync);
            }
            else {
                self.vsync = true;
                println!("ENABLE");
                ctx.renderer.render_graph().set_input::<engine::renderer::resources::PresentMode>(wgpu::PresentMode::AutoVsync);
            }
        }

        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyC) {
            ctx.world.get_material_mut(material).unwrap().set_config(engine::material::RotatingConfig {
                color: Vec4::new(1., 0., 0., 1.),
                speed: 0.5,
            });
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    tracing::info!("Extracting minecraft data");
    let mc_data = data_extractor::MinecraftData::read()?;

    let mut chunks = HashMap::<ISizeVec3, Chunk>::new();
    let generator = proc_gen::Generator::new(0);

    tracing::info!("Generating start chunks");
    for p in ISizeVec3Range(ISizeVec3::new(-2, -8, -2), ISizeVec3::new(2, 8, 2)) {
        chunks.insert(p, generator.generate_chunk(p));
    }
    tracing::info!("Finished");

    engine::start(App {
        vsync: true,
        material: None,

        mc_data,
        chunks,
        generator,

        chunk_materials: RefCell::new(HashMap::new()),
    })?;

    Ok(())
}
