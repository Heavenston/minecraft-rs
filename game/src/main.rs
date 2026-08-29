use anyhow::Result;
use winit::event_loop::EventLoop;
use tracing::info;

struct App {
    
}

impl engine::App for App {
    fn resume(&mut self, render_world: &mut engine::RenderWorld) -> Result<()> {
        render_world.use_middleware::<engine::render_middlewares::Clear>()
            .set_color(glam::Vec4::new(1., 0., 0., 1.));
        Ok(())
    }

    fn update(&mut self, ctx: engine::Ctx<'_>) -> Result<()> {
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyW) {
            info!("Key W just pressed");
            ctx.render_world.use_middleware::<engine::render_middlewares::Clear>()
                .set_color(glam::Vec4::new(0., 1., 0., 1.));
        }
        if ctx.inputs_state.just_released(engine::KeyCode::KeyW) {
            info!("Key W just released");
            ctx.render_world.use_middleware::<engine::render_middlewares::Clear>()
                .set_color(glam::Vec4::new(1., 0., 0., 1.));
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
