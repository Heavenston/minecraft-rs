#![feature(macro_metavar_expr)]

mod render_graph_nodes;
pub mod world;
use std::sync::Arc;

use crevice::std140::AsStd140;
pub use harness::{ Renderer, InputsState };

use anyhow::Result;
use parking_lot::RwLock;

use crate::{render_graph_nodes::RenderPassConfig, world::{GPUWorld, World, WorldUniformBuffer}};

pub struct Ctx<'a> {
    pub inputs_state: &'a mut InputsState,
    pub renderer: &'a mut Renderer,
}

pub trait App: 'static {
    fn resume(&mut self, renderer: &mut Renderer) -> Result<()> {
        let _ = renderer;
        Ok(())
    }

    fn update(&mut self, ctx: Ctx<'_>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
}

struct HarnessApp<A: App> {
    app: A,
    world: Arc<RwLock<World>>,
    gpu_world: Option<Arc<RwLock<GPUWorld>>>,
}

impl<A: App> harness::App for HarnessApp<A> {
    fn resume(&mut self, renderer: &mut Renderer) -> anyhow::Result<()> {
        render_graph_nodes::register(renderer.render_graph());

        let world_uniform = renderer.device().create_buffer(&wgpu::wgt::BufferDescriptor {
            label: Some("World uniform buffer"),
            size: WorldUniformBuffer::std140_size_static() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        self.gpu_world = Some(Arc::new(RwLock::new(GPUWorld {
            staging_belt: wgpu::util::StagingBelt::new(renderer.device().clone(), 128),
            world_uniform,
        })));

        self.app.resume(renderer)
    }

    fn update(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        let gpu_world = self.gpu_world.as_ref().unwrap();

        ctx.renderer.render_graph().set_input::<render_graph_nodes::RenderPassConfigResource>(RenderPassConfig {
            clear_color: wgpu::Color::RED,
        });
        ctx.renderer.render_graph().set_input::<render_graph_nodes::WorldResource>(self.world.read_arc());
        ctx.renderer.render_graph().set_input::<render_graph_nodes::GPUWorldResource>(gpu_world.write_arc());
        self.app.update(Ctx {
            inputs_state: ctx.inputs_state,
            renderer: ctx.renderer,
        })
    }
}

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    harness::start(HarnessApp {
        app,
        world: Default::default(),
        gpu_world: None,
    })?;
    Ok(())
}
