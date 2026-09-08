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

use crate::{chunk::Chunk, chunk_mesher::mesh_chunk, utils::{CardinalDirection, ISizeVec3Range, Vec3Range}};

mod chunk;
mod chunk_mesher;
mod resource_location;
mod utils;
mod proc_gen;

mod data_extractor;

struct App {
    vsync: bool,
    material: Option<MaterialHandle<engine::material::Rotating>>,
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::ResumeCtx<'_>) -> Result<()> {
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
    let mcdata = data_extractor::MinecraftData::read()?;

    let mut chunks = HashMap::<ISizeVec3, Chunk>::new();
    let generator = proc_gen::Generator::new(0);

    tracing::info!("Generating start chunks");
    for p in ISizeVec3Range(ISizeVec3::new(-2, -2, -8), ISizeVec3::new(2, 2, 8)) {
        let chunk = generator.generate_chunk(glam::ISizeVec3::ZERO);
        chunks.insert(p, chunk);
    }
    tracing::info!("Meshing start chunks");

    #[expect(clippy::iter_over_hash_type, reason = "order is not observable")]
    for (&p, chunk) in &chunks {
        let Some(neighbors) = CardinalDirection::VALUES.try_map(|direction| {
            let new_pos = p + direction;
            chunks.get(&new_pos)
        }).map(EnumMap::from_array)
        else { continue };
        mesh_chunk(&mcdata, chunk, neighbors);
    }
    tracing::info!("Finished");

    if false {
        engine::start(App {
            vsync: true,
            material: None,
        })?;
    }

    Ok(())
}
