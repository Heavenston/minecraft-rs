pub mod render_graph_nodes;
mod world;
mod material;
mod material_store;

pub use world::{ WorldUniform, Std140WorldUniform, World };
pub use material::{ Material, RenderGraphWrapper };
pub use material_store::{ MaterialStore, MaterialHandle };
pub use harness::{ Renderer, renderer, InputsState, KeyCode };
use render_graph_nodes::RenderPassConfig;
pub use wgpu;

use std::sync::Arc;
use anyhow::Result;
use parking_lot::RwLock;

pub struct Ctx<'a> {
    pub inputs_state: &'a mut InputsState,
    pub renderer: &'a mut Renderer,
    pub world: &'a mut World,
    pub materials: &'a mut MaterialStore,
    pub materials_arc: &'a Arc<RwLock<MaterialStore>>,
}

pub trait App: 'static {
    fn resume(&mut self, ctx: &mut Ctx<'_>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    fn update(&mut self, ctx: &mut Ctx<'_>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
}

struct HarnessApp<A: App> {
    app: A,
    world: Arc<RwLock<World>>,
    materials: Arc<RwLock<MaterialStore>>,
}

impl<A: App> harness::App for HarnessApp<A> {
    fn resume(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        render_graph_nodes::register(ctx.renderer.render_graph());

        self.app.resume(&mut Ctx {
            inputs_state: ctx.inputs_state,
            renderer: ctx.renderer,
            world: &mut self.world.write(),
            materials: &mut self.materials.write(),
            materials_arc: &self.materials,
        })?;
        Ok(())
    }

    fn update(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        {
            let mut world = self.world.write();
            let mut material_store = self.materials.write();

            world.update_render_graph(ctx.renderer.render_graph());
            material_store.update_render_graph(ctx.renderer.render_graph());

            self.app.update(&mut Ctx {
                inputs_state: ctx.inputs_state,
                renderer: ctx.renderer,
                world: &mut world,
                materials: &mut material_store,
                materials_arc: &self.materials,
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
        materials: Arc::<RwLock<MaterialStore>>::default(),
    })?;
    Ok(())
}
