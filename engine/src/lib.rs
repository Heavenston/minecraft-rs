#![feature(macro_metavar_expr)]

mod render_graph_nodes;
pub mod world;
pub mod material;
use std::{num::NonZero, sync::Arc};

use crevice::std140::AsStd140;
pub use harness::{ Renderer, InputsState };

use anyhow::Result;
use parking_lot::RwLock;

use crate::{render_graph_nodes::RenderPassConfig, world::{GPUWorld, World, WorldUniformBuffer}};

pub struct ResumeCtx<'a> {
    pub renderer: &'a mut Renderer,
    pub world: &'a mut World,
    pub gpu_world: &'a mut GPUWorld,
}

pub struct Ctx<'a> {
    pub inputs_state: &'a mut InputsState,
    pub renderer: &'a mut Renderer,
}

pub trait App: 'static {
    fn resume(&mut self, ctx: &mut ResumeCtx<'_>) -> Result<()> {
        let _ = ctx;
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

        let world_bind_group_layout = renderer.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZero::new(world_uniform.size()).unwrap()),
                },
                count: None,
            }],
        });
        let world_bind_group = renderer.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world bind group"),
            layout: &world_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &world_uniform,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        self.gpu_world = Some(Arc::new(RwLock::new(GPUWorld {
            world_uniform,
            world_bind_group,
            world_bind_group_layout,
        })));

        self.app.resume(&mut ResumeCtx {
            renderer,
            world: &mut *self.world.write(),
            gpu_world: &mut *self.gpu_world.as_ref().unwrap().write(),
        })
    }

    fn update(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        self.world.write().update_render_graph(ctx.renderer.render_graph());
        
        ctx.renderer.render_graph().set_input::<render_graph_nodes::WorldResource>(self.world.read_arc());
        ctx.renderer.render_graph().set_input::<render_graph_nodes::GPUWorldResource>(self.gpu_world.as_ref().unwrap().write_arc());
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
