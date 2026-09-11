#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(const_ops)]
#![feature(const_convert)]
#![feature(const_cmp)]
#![feature(const_index)]
#![feature(array_try_map)]

#![allow(clippy::single_call_fn, reason = "in development")]

use std::{cell::RefCell, collections::HashMap, time::Instant};

use anyhow::{Context as _, Result};
use engine::{wgpu::{self, util::DeviceExt as _}, world::MaterialHandle};
use enum_map::EnumMap;
use glam::{ISizeVec3, Vec3, Vec4};
use image::{EncodableLayout as _, Pixel as _};
use ordermap::OrderMap;
use render_graph::RenderGraph;

use crate::{chunk::{CHUNK_SIZE, Chunk}, chunk_mesher::mesh_chunk, data_extractor::MinecraftData, materials::{ChunkMaterial, ChunkRenderData, ChunkTransparencyMode}, resource_location::{ResourceLocation, ResourceLocationMap}, utils::{CardinalDirection, ISizeVec3Range}};

mod chunk;
mod chunk_mesher;
mod resource_location;
mod utils;
mod proc_gen;
mod materials;

mod data_extractor;

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

struct App {
    vsync: bool,
    enable_wireframe: bool,

    mc_data: MinecraftData,
    chunks: OrderMap<ISizeVec3, Chunk>,
    #[expect(dead_code, reason = "not yet used")]
    generator: proc_gen::Generator,

    start: Instant,
    textures: RefCell<ResourceLocationMap<TextureData>>,
    chunk_materials: RefCell<HashMap<ChunkMaterialKey, MaterialHandle<ChunkMaterial>>>,
}

impl App {
    fn get_texture(&self, ctx: &engine::ResumeCtx<'_>, location: ResourceLocation) -> Result<TextureData> {
        if let Some(texture) = self.textures.borrow().get(&location).cloned() {
            return Ok(texture);
        }

        let mut image = MinecraftData::read_texture(location).with_context(|| format!("reading mc texture {location}"))?;
        image.apply_color_space(image::metadata::Cicp::SRGB, image::ConvertColorOptions::default())?;
        let image = image.to_rgba8();

        let has_transparent = image.pixels().any(|p| p.alpha() == 0);
        let has_translucent = image.pixels().any(|p| (1..u8::MAX).contains(&p.alpha()));

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
        self.textures.borrow_mut().insert(location, data.clone());

        Ok(data)
    }

    fn get_chunk_material(&self, ctx: &mut engine::ResumeCtx<'_>, texture_location: ResourceLocation, force_translucent: bool) -> Result<MaterialHandle<ChunkMaterial>> {
        let TextureData { texture, present_transparency } = self.get_texture(ctx, texture_location)?;
        let transparency = if force_translucent {
            ChunkTransparencyMode::Translucent
        } else {
            present_transparency
        };
        let key = ChunkMaterialKey { texture_location, transparency };
        if let Some(&material) = self.chunk_materials.borrow().get(&key) {
            return Ok(material);
        }

        let material = ctx.world.add_material(ChunkMaterial::new(materials::ChunkRenderConfig {
            texture,
            transparency,
        }));
        self.chunk_materials.borrow_mut().insert(key, material);

        Ok(material)
    }

    fn set_enable_wireframe(&mut self, render_graph: &mut RenderGraph, val: bool) {
        render_graph.set_input::<materials::EnableWireframes>(val);
        self.enable_wireframe = val;
    }

    fn set_enable_vsync(&mut self, render_graph: &mut RenderGraph, enable: bool) {
        self.vsync = enable;
        let present_mode = if enable {
            wgpu::PresentMode::AutoVsync
        }
        else {
            wgpu::PresentMode::AutoNoVsync
        };
        render_graph.set_input::<engine::renderer::resources::PresentMode>(present_mode);
    }
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::ResumeCtx<'_>) -> Result<()> {
        tracing::info!("Meshing start chunks");

        let mut face_count: usize = 0;

        for (&chunk_position, chunk) in &self.chunks {
            let Some(neighbors) = CardinalDirection::VALUES.try_map(|direction| {
                let new_pos = chunk_position + direction;
                self.chunks.get(&new_pos)
            }).map(EnumMap::from_array)
            else { continue };
            let mesh = mesh_chunk(&self.mc_data, chunk, neighbors);
            for submesh in mesh.quad_submeshes {
                let buffer = ctx.renderer.device().create_buffer(&wgpu::wgt::BufferDescriptor {
                    label: Some(&format!("chunk,{chunk_position},{:?},{}", submesh.direction, submesh.texture)),
                    size: (submesh.instances.len() * 4).try_into().unwrap(),
                    usage: wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: true,
                });
                face_count += submesh.instances.len();
                buffer.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(bytemuck::cast_slice::<_, u8>(&submesh.instances));
                buffer.unmap();
                let material = self.get_chunk_material(ctx, submesh.texture, submesh.force_translucent)?;
                let material = ctx.world.get_material_mut(material).unwrap();
                material.chunk_list = material.chunk_list.iter().cloned().chain([ChunkRenderData {
                    direction: submesh.direction,
                    position: (chunk_position * CHUNK_SIZE.as_isizevec3()).as_vec3(),
                    vertex_buffer: buffer,
                }]).collect();
            }
        }

        tracing::info!(face_count, material_count = self.chunk_materials.borrow().len(), "Ready");

        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        self.set_enable_wireframe(ctx.renderer.render_graph(), self.enable_wireframe);
        self.set_enable_vsync(ctx.renderer.render_graph(), self.vsync);
        
        Ok(())
    }

    fn update(&mut self, ctx: engine::Ctx<'_>) -> Result<()> {
        let time = self.start.elapsed().as_secs_f32();

        let window_size = ctx.renderer.window().outer_size();
        #[expect(clippy::cast_precision_loss, reason = "")]
        let aspect_ratio = window_size.width as f32 / window_size.height as f32;

        let distance = 80.;
        let height = 25.;
        ctx.world.camera_transform = glam::camera::rh::view::look_at_mat4(Vec3::new((time / 2.).cos() * distance, height, (time / 2.).sin() * distance), Vec3::ZERO, Vec3::Y).inverse_or_zero();
        ctx.world.camera_projection = glam::camera::rh::proj::directx::perspective(50f32.to_radians(), aspect_ratio, 0.01, 1_000.);

        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyV) {
            self.set_enable_vsync(ctx.renderer.render_graph(), !self.vsync);
            tracing::info!(enabled = self.vsync, "Changed vsync state");
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyW) {
            self.set_enable_wireframe(ctx.renderer.render_graph(), !self.enable_wireframe);
            tracing::info!(enabled = self.enable_wireframe, "Changed enable wireframe state");
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyG) {
            ctx.renderer.render_graph().clear_cache();
            tracing::info!("Cleared render graph cache");
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    tracing::info!("Extracting minecraft data");
    let mc_data = data_extractor::MinecraftData::read()?;

    let mut chunks = OrderMap::<ISizeVec3, Chunk>::new();
    let generator = proc_gen::Generator::new(0);

    tracing::info!("Generating start chunks");
    for p in ISizeVec3Range(ISizeVec3::new(-6, -4, -6), ISizeVec3::new(6, 4, 6)) {
        chunks.insert(p, generator.generate_chunk(p));
    }
    tracing::info!("Finished");

    engine::start(App {
        vsync: true,
        enable_wireframe: false,

        mc_data,
        chunks,
        generator,

        start: Instant::now(),
        textures: RefCell::new(HashMap::default()),
        chunk_materials: RefCell::new(HashMap::default()),
    })?;

    Ok(())
}
