#![recursion_limit = "256"]

#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(const_ops)]
#![feature(const_convert)]
#![feature(const_cmp)]
#![feature(const_index)]

#![allow(clippy::single_call_fn, reason = "in development")]

use std::{sync::Arc, time::Instant};

use anyhow::Result;
use crossbeam_channel::Sender;
use engine::wgpu;
use glam::{Vec3, Vec4};
use render_graph::RenderGraph;

use crate::data_extractor::MinecraftData ;

mod chunk;
mod chunk_mesher;
mod resource_location;
mod utils;
mod proc_gen;
mod materials;
mod chunk_thread;

mod data_extractor;

enum ToChunkThreadMessage {
    RegenWithSeed(u64),
}

struct App {
    vsync: bool,
    enable_wireframe: bool,
    pause_clipping: bool,

    mc_data: Arc<MinecraftData>,
    start: Instant,
    to_chunk_thread: Option<Sender<ToChunkThreadMessage>>,
}

impl App {
    fn set_enable_wireframe(&mut self, render_graph: &mut RenderGraph, val: bool) {
        render_graph.set_input::<materials::EnableWireframes>(val);
        self.enable_wireframe = val;
    }

    fn set_enable_vsync(&mut self, render_graph: &mut RenderGraph, enable: bool) {
        self.vsync = enable;
        let present_mode = if enable {
            wgpu::PresentMode::AutoVsync
        }
        else {
            wgpu::PresentMode::AutoNoVsync
        };
        render_graph.set_input::<engine::renderer::resources::PresentMode>(present_mode);
    }
}

impl engine::App for App {
    fn resume(&mut self, ctx: &mut engine::Ctx<'_>) -> Result<()> {
        let (to_chunk_thread, to_chunk_thread_receiver) = crossbeam_channel::unbounded();
        {
            let mcdata = Arc::clone(&self.mc_data);
            let device = ctx.renderer.device().clone();
            let queue = ctx.renderer.queue().clone();
            let materials = Arc::clone(ctx.materials_arc);
            std::thread::spawn(move || {
                chunk_thread::chunk_thread(0, to_chunk_thread_receiver, mcdata, device, queue, materials);
            });
        }
        self.to_chunk_thread = Some(to_chunk_thread);

        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        self.set_enable_wireframe(ctx.renderer.render_graph(), self.enable_wireframe);
        self.set_enable_vsync(ctx.renderer.render_graph(), self.vsync);
        
        Ok(())
    }

    fn update(&mut self, ctx: &mut engine::Ctx<'_>) -> Result<()> {
        let time = self.start.elapsed().as_secs_f32();

        let window_size = ctx.renderer.window().outer_size();
        #[expect(clippy::cast_precision_loss, reason = "")]
        let aspect_ratio = window_size.width as f32 / window_size.height as f32;

        let distance = 80.;
        let height = 25.;
        ctx.world.camera_transform = glam::camera::rh::view::look_at_mat4(Vec3::new((time / 2.).cos() * distance, height, (time / 2.).sin() * distance) + Vec3::ONE/2., Vec3::ONE/2., Vec3::Y).inverse_or_zero();
        ctx.world.camera_projection = glam::camera::rh::proj::directx::perspective(50f32.to_radians(), aspect_ratio, 0.01, 1_000.);
        if !self.pause_clipping {
            ctx.world.camera_clip_transform = ctx.world.camera_transform;
        }

        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyV) {
            self.set_enable_vsync(ctx.renderer.render_graph(), !self.vsync);
            tracing::info!(enabled = self.vsync, "Changed vsync state");
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyW) {
            self.set_enable_wireframe(ctx.renderer.render_graph(), !self.enable_wireframe);
            tracing::info!(enabled = self.enable_wireframe, "Changed enable wireframe state");
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyG) {
            ctx.renderer.render_graph().clear_cache();
            tracing::info!("Cleared render graph cache");
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyC) {
            self.pause_clipping = !self.pause_clipping;
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyS) {
            let seed = rand::random::<u64>();
            tracing::info!(seed, "Changind seed");
            self.to_chunk_thread.as_ref().unwrap().send(ToChunkThreadMessage::RegenWithSeed(seed)).unwrap();
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let mc_data = Arc::new(data_extractor::MinecraftData::read()?);

    engine::start(App {
        vsync: true,
        enable_wireframe: false,
        pause_clipping: false,

        mc_data,
        start: Instant::now(),
        to_chunk_thread: None,
    })?;

    Ok(())
}
