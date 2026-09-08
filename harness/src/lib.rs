#![recursion_limit = "256"]

pub mod renderer;
mod winit_loop_handler;
mod inputs_state;

pub use renderer::Renderer;
pub use inputs_state::*;

use anyhow::Result;
use winit::platform::x11::EventLoopBuilderExtX11 as _;

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
    let mut event_loop_builder = winit::event_loop::EventLoop::with_user_event();
    let event_loop_builder = if std::env::var("WINIT_BACKEND").is_ok_and(|p| p == "x11") {
        tracing::info!("WINIT_BACKEND env variable set to 'x11', using x11 backend");
        event_loop_builder.with_x11()
    } else {
        &mut event_loop_builder
    };
    let event_loop = event_loop_builder.build()?;
    let mut engine = winit_loop_handler::WinitLoopHandler::new(app);
    event_loop.run_app(&mut engine)?;
    Ok(())
}
