#![recursion_limit = "256"]

#![feature(specialization)]
#![feature(impl_restriction)]
#![feature(const_closures)]
#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(const_ops)]
#![feature(const_convert)]
#![feature(const_cmp)]
#![feature(const_index)]

#![allow(clippy::single_call_fn, reason = "in development")]
#![allow(incomplete_features, reason = "specialization")]

use std::{f32::consts::{PI, TAU}, sync::Arc, time::Instant};

use anyhow::Result;
use crossbeam_channel::Sender;
use engine::wgpu;
use glam::{Vec3, Vec3Swizzles as _, Vec4};
use render_graph::RenderGraph;

use crate::data_extractor::MinecraftData;

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

#[expect(clippy::struct_excessive_bools, reason = "todo")]
struct App {
    vsync: bool,
    enable_wireframe: bool,
    pause_clipping: bool,

    camera_autorotate: bool,
    camera_position: Vec3,

    mc_data: Arc<MinecraftData>,
    previous_update: Option<Instant>,
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
                chunk_thread::chunk_thread(0, &to_chunk_thread_receiver, mcdata, device, queue, materials);
            });
        }
        self.to_chunk_thread = Some(to_chunk_thread);

        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        self.set_enable_wireframe(ctx.renderer.render_graph(), self.enable_wireframe);
        self.set_enable_vsync(ctx.renderer.render_graph(), self.vsync);
        
        Ok(())
    }

    fn update(&mut self, ctx: &mut engine::Ctx<'_>) -> Result<()> {
        let delta_t = self.previous_update.map_or(f32::INFINITY, |i| i.elapsed().as_secs_f32());
        self.previous_update = Some(Instant::now());

        let window_size = ctx.renderer.window().outer_size();
        #[expect(clippy::cast_precision_loss, reason = "")]
        let aspect_ratio = window_size.width as f32 / window_size.height as f32;

        if self.camera_autorotate {
            self.camera_position = self.camera_position.rotate_y(delta_t * TAU * (1. / 14.));
        }

        {
            let target_transform = glam::camera::rh::view::look_at_mat4(self.camera_position + Vec3::ONE / 2., Vec3::ONE / 2., Vec3::Y).inverse_or_zero();
            ctx.world.camera_transform = target_transform + (ctx.world.camera_transform - target_transform) * f32::exp2(-delta_t / 0.05);
        }
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
        let speed_mult = if ctx.inputs_state.pressed(engine::KeyCode::ShiftLeft) {
            2.
        } else {
            1.
        };
        let rotate_speed = PI * (1. / 3.) * speed_mult;
        if ctx.inputs_state.pressed(engine::KeyCode::ArrowUp) {
            self.camera_position = self.camera_position.rotate_towards(Vec3::Y, delta_t * rotate_speed);
        }
        if ctx.inputs_state.pressed(engine::KeyCode::ArrowDown) {
            self.camera_position = self.camera_position.rotate_towards(Vec3::NEG_Y, delta_t * rotate_speed);
        }
        if ctx.inputs_state.pressed(engine::KeyCode::ArrowLeft) {
            self.camera_position = self.camera_position.rotate_y(delta_t *-rotate_speed);
        }
        if ctx.inputs_state.pressed(engine::KeyCode::ArrowRight) {
            self.camera_position = self.camera_position.rotate_y(delta_t * rotate_speed);
        }

        let mut dist_diff = 0.;
        if ctx.inputs_state.pressed(engine::KeyCode::KeyO) {
            dist_diff += 1.;
        }
        if ctx.inputs_state.pressed(engine::KeyCode::KeyL) {
            dist_diff -= 1.;
        }
        if dist_diff != 0. {
            let (camera_pos_norm, camera_distance) = self.camera_position.normalize_and_length();
            self.camera_position = camera_pos_norm * (dist_diff * delta_t * 50.).mul_add(speed_mult, camera_distance);
        }

        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyR) {
            self.camera_autorotate = !self.camera_autorotate;
        }
        if ctx.inputs_state.just_pressed(engine::KeyCode::KeyF) {
            let facing = ctx.world.camera_transform.transform_vector3(Vec3::NEG_Z).xz().round();
            tracing::info!(%facing, camera_position = %self.camera_position, camera_dist = self.camera_position.length());
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

        camera_autorotate: false,
        camera_position: Vec3::new(80., 25., 80.),

        mc_data,
        previous_update: None,
        to_chunk_thread: None,
    })?;

    Ok(())
}
