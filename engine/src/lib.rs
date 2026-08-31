#![feature(macro_metavar_expr)]

mod render_graph_nodes;
pub mod world;
pub use harness::{ Renderer, InputsState };

use anyhow::Result;

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
}

impl<A: App> harness::App for HarnessApp<A> {
    fn resume(&mut self, renderer: &mut Renderer) -> anyhow::Result<()> {
        render_graph_nodes::register(renderer.render_graph());
        self.app.resume(renderer)
    }

    fn update(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        self.app.update(Ctx {
            inputs_state: ctx.inputs_state,
            renderer: ctx.renderer,
        })
    }
}

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    harness::start(HarnessApp {
        app,
    })?;
    Ok(())
}
