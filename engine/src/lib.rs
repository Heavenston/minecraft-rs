mod render_graph_nodes;
pub mod world;
pub mod material;
use std::sync::Arc;

pub use harness::{ Renderer, renderer, InputsState, KeyCode };
pub use wgpu;

use anyhow::Result;
use parking_lot::RwLock;

use crate::{render_graph_nodes::RenderPassConfig, world::World};

pub struct ResumeCtx<'a> {
    pub renderer: &'a mut Renderer,
    pub world: &'a mut World,
}

pub struct Ctx<'a> {
    pub inputs_state: &'a mut InputsState,
    pub renderer: &'a mut Renderer,
    pub world: &'a mut World,
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
}

impl<A: App> harness::App for HarnessApp<A> {
    fn resume(&mut self, renderer: &mut Renderer) -> anyhow::Result<()> {
        render_graph_nodes::register(renderer.render_graph());

        self.app.resume(&mut ResumeCtx {
            renderer,
            world: &mut self.world.write(),
        })
    }

    fn update(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        {
            let mut world = self.world.write();
            world.update_render_graph(ctx.renderer.render_graph());
        
            self.app.update(Ctx {
                inputs_state: ctx.inputs_state,
                renderer: ctx.renderer,
                world: &mut world,
            })?;
        }

        ctx.renderer.render_graph().set_input::<render_graph_nodes::WorldResource>(self.world.read_arc());

        Ok(())
    }
}

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    harness::start(HarnessApp {
        app,
        world: Arc::<RwLock<World>>::default(),
    })?;
    Ok(())
}
