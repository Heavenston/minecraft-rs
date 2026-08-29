use anyhow::Result;
use winit::event_loop::EventLoop;
use tracing::info;

struct App {
    
}

impl engine::App for App {
    fn update(&mut self, ctx: engine::Ctx<'_>) -> Result<()> {
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyW) {
            info!("Key W just pressed");
        }
        if ctx.inputs_state.just_released(engine::KeyCode::KeyW) {
            info!("Key W just released");
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let event_loop = EventLoop::with_user_event().build()?;
    let mut engine = engine::Engine::new(App {});
    event_loop.run_app(&mut engine)?;
    Ok(())
}
