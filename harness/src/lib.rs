#![recursion_limit = "256"]

pub mod renderer;
mod winit_loop_handler;
mod inputs_state;

pub use renderer::Renderer;
pub use inputs_state::*;

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

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    let mut engine = winit_loop_handler::WinitLoopHandler::new(app);
    event_loop.run_app(&mut engine)?;
    Ok(())
}
