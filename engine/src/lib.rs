#![feature(macro_metavar_expr)]

pub mod renderer;
pub use renderer::Renderer;
mod engine;
pub use engine::{ App, Ctx };
pub mod render_graph;
mod inputs_state;
pub use inputs_state::*;

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    let mut engine = engine::Engine::new(app);
    event_loop.run_app(&mut engine)?;
    Ok(())
}
