#![allow(dead_code)]

use anyhow::Result;
use engine::wgpu;
use glam::Vec4;

mod chunk;
mod resource_location;

struct App {
    vsync: bool,
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::ResumeCtx<'_>) -> Result<()> {
        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        let material = engine::material::Rotating::new(ctx, engine::material::RotatingConfig {
            color: Vec4::new(1., 1., 1., 1.),
            speed: 60.,
        });
        ctx.world.add_material(material);

        ctx.renderer.render_graph().set_input::<engine::renderer::resources::PresentMode>(wgpu::PresentMode::AutoVsync);
        
        Ok(())
    }

    fn update(&mut self, ctx: engine::Ctx<'_>) -> Result<()> {
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyV) {
            if self.vsync {
                self.vsync = false;
                println!("DISABLE");
                ctx.renderer.render_graph().set_input::<engine::renderer::resources::PresentMode>(wgpu::PresentMode::AutoNoVsync);
            }
            else {
                self.vsync = true;
                println!("ENABLE");
                ctx.renderer.render_graph().set_input::<engine::renderer::resources::PresentMode>(wgpu::PresentMode::AutoVsync);
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    engine::start(App {
        vsync: true,
    })
}
