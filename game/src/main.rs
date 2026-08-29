use anyhow::Result;
use winit::event_loop::EventLoop;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let event_loop = EventLoop::with_user_event().build()?;
    let mut engine = engine::Engine::new();
    event_loop.run_app(&mut engine)?;
    Ok(())
}
