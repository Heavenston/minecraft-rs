#![allow(dead_code)]

use anyhow::Result;
use glam::Vec4;

mod chunk;
mod resource_location;

struct App {
    
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::ResumeCtx<'_>) -> Result<()> {
        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        let material = engine::material::Rotating::new(ctx, engine::material::RotatingConfig {
            color: Vec4::new(1., 1., 1., 1.),
            speed: 1.,
        });
        ctx.world.add_material(material);
        Ok(())
    }
    // fn resume(&mut self, renderer: &mut engine::Renderer, world: &mut engine::world::World) -> Result<()> {
        // world.clear_color = Vec4::new(0., 0., 0., 1.);
        // world.add_material(engine::material::Rotating::new(renderer, engine::material::RotatingConfig {
        //     color: todo!(),
        //     speed: todo!(),
        // }));

    //     Ok(())
    // }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    engine::start(App { })
}
