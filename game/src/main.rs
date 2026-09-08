#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(const_ops)]
#![feature(const_convert)]
#![feature(const_cmp)]
#![feature(const_index)]
#![feature(integer_widen_truncate)]
#![feature(array_try_map)]

#![allow(dead_code, reason = "in development")]
#![allow(unused_imports, reason = "in development")]
#![allow(clippy::single_call_fn, reason = "in development")]

use std::collections::HashMap;

use anyhow::Result;
use engine::{wgpu, world::MaterialHandle};
use enum_map::EnumMap;
use glam::{ISizeVec3, Vec4};
use itertools::Itertools as _;

use crate::{chunk::Chunk, chunk_mesher::mesh_chunk, data_extractor::MinecraftData, utils::{CardinalDirection, ISizeVec3Range, Vec3Range}};

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
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::ResumeCtx<'_>) -> Result<()> {
        tracing::info!("Meshing start chunks");
        #[expect(clippy::iter_over_hash_type, reason = "order is not observable")]
        for (&p, chunk) in &self.chunks {
            let Some(neighbors) = CardinalDirection::VALUES.try_map(|direction| {
                let new_pos = p + direction;
                self.chunks.get(&new_pos)
            }).map(EnumMap::from_array)
            else { continue };
            mesh_chunk(&self.mc_data, chunk, neighbors);
        }

        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        let material = engine::material::Rotating::new(ctx, engine::material::RotatingConfig {
            color: Vec4::new(1., 1., 1., 1.),
            speed: 60.,
        });
        self.material = Some(ctx.world.add_material(material));

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
    })?;

    Ok(())
}
