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
#![feature(likely_unlikely)]

#![allow(clippy::single_call_fn, reason = "in development")]
#![allow(incomplete_features, reason = "specialization")]

use std::{f32::consts::{PI, TAU}, sync::Arc, time::Instant};

use anyhow::Result;
use crossbeam_channel::Sender;
use engine::{MaterialHandle, wgpu};
use glam::{ISizeVec3, UVec2, Vec2, Vec3, Vec3Swizzles as _, Vec4};
use parking_lot::RwLock;
use render_graph::RenderGraph;

use crate::{chunk::{CHUNK_SIZE, Chunk, ChunkBlockIndex}, data_extractor::MinecraftData, resource_location::location, world::{SharedWorldRef, World}};

mod chunk;
mod chunk_mesher;
mod resource_location;
mod utils;
mod proc_gen;
mod materials;
mod chunk_thread;
mod world;

mod data_extractor;

#[derive(Debug)]
enum ToChunkThreadMessage {
    RemeshChunk(ISizeVec3),
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

    world: SharedWorldRef,

    block_selection_material: Option<MaterialHandle<materials::block_selection::BlockSelection>>,
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
            let world = Arc::clone(&self.world);
            std::thread::Builder::new()
                .name("chunk thread".to_string())
                .spawn(move || {
                    chunk_thread::chunk_thread(0, &to_chunk_thread_receiver, mcdata, world, device, queue, materials);
                }).unwrap();
        }
        self.to_chunk_thread = Some(to_chunk_thread);

        ctx.world.clear_color = Vec4::new(0., 0., 0., 1.);
        self.set_enable_wireframe(ctx.renderer.render_graph(), self.enable_wireframe);
        self.set_enable_vsync(ctx.renderer.render_graph(), self.vsync);

        self.block_selection_material = Some(ctx.materials.add_material(materials::block_selection::BlockSelection::new()));
        
        Ok(())
    }

    fn update(&mut self, ctx: &mut engine::Ctx<'_>) -> Result<()> {
        let delta_t = self.previous_update.map_or(f32::INFINITY, |i| i.elapsed().as_secs_f32());
        self.previous_update = Some(Instant::now());

        let window_size = ctx.renderer.window().inner_size();
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
            let facing_ = ctx.world.camera_transform.transform_vector3(Vec3::NEG_Z);
            let facing = facing_.xz().round();
            tracing::info!(%facing_, %facing, camera_position = %self.camera_position, camera_dist = self.camera_position.length());
        }

        'block_selection: {
            let window_size = UVec2::new(window_size.width, window_size.height).as_vec2();
            let cursor_pos = ctx.inputs_state.mouse_position().as_vec2() / window_size;
            let cursor_pos = (cursor_pos - Vec2::splat(0.5)) * Vec2::new(1., -1.) * 2.;
            let origin = ctx.world.camera_transform.transform_point3(Vec3::ZERO);

            let direction = ctx.world.camera_projection.inverse()
                .transform_point3(Vec3::new(cursor_pos.x, cursor_pos.y, 0.5));
            let direction = ctx.world.camera_transform
                .transform_vector3(direction).normalize();

            let mut world = self.world.write();
            let t = world.cast_ray(origin, direction, 250.);
            ctx.materials.get_material_mut(self.block_selection_material.unwrap()).unwrap().set_position(t.map(|t| t.pos.as_vec3()));
            let Some(result) = t
            else { break 'block_selection; };
            if ctx.inputs_state.just_pressed(engine::MouseButton::Left) {
                let pos = result.pos;
                tracing::info!(%cursor_pos, %origin, %direction, result = ?t.map(|h| h.block.id), "left click");
                world.set_block(pos, &chunk::BlockData { id: location!("air"), state: String::new() });
                let tct = self.to_chunk_thread.as_ref().unwrap();
                let cp = pos.div_euclid(CHUNK_SIZE.as_isizevec3());
                tct.send(ToChunkThreadMessage::RemeshChunk(cp)).unwrap();
                for n in Chunk::blocks_touches_neighbors(ChunkBlockIndex::from_pos_rem_euclid(pos)) {
                    tct.send(ToChunkThreadMessage::RemeshChunk(cp + n)).unwrap();
                }
            }
            else if ctx.inputs_state.just_pressed(engine::MouseButton::Right) {
                let pos = result.pos + result.normal;
                world.set_block(pos, &chunk::BlockData { id: location!("dirt"), state: String::new() });
                let tct = self.to_chunk_thread.as_ref().unwrap();
                let cp = pos.div_euclid(CHUNK_SIZE.as_isizevec3());
                tct.send(ToChunkThreadMessage::RemeshChunk(cp)).unwrap();
                for n in Chunk::blocks_touches_neighbors(ChunkBlockIndex::from_pos_rem_euclid(pos)) {
                    tct.send(ToChunkThreadMessage::RemeshChunk(cp + n)).unwrap();
                }
            }
        }

        Ok(())
    }
}

#[cfg(not(feature = "deadlock_detection"))]
fn start_deadlock_detection() { }
#[cfg(feature = "deadlock_detection")]
fn start_deadlock_detection() {
    use std::thread;
    use std::time::Duration;
    use parking_lot::deadlock;

    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            let deadlocks = deadlock::check_deadlock();
            if deadlocks.is_empty() {
                continue;
            }

            tracing::warn!("{} deadlocks detected", deadlocks.len());
            for (i, threads) in deadlocks.iter().enumerate() {
                eprintln!("Deadlock #{}", i);
                for t in threads {
                    eprintln!("Thread Id {:?}", t.thread_id());
                    eprintln!("{:#?}", t.backtrace());
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    start_deadlock_detection();
    #[cfg(feature = "tracy")]
    {
        use tracing_subscriber::layer::{ SubscriberExt, Layer };
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer()
                    .with_filter(tracing_subscriber::filter::EnvFilter::from_default_env()))
                .with(tracing_tracy::TracyLayer::default())
        ).expect("setup tracy layer");
        tracing_tracy::client::register_demangler!();
    }
    #[cfg(not(feature = "tracy"))]
    {
        tracing_subscriber::fmt::init();
    }

    let mc_data = Arc::new(data_extractor::MinecraftData::read()?);

    engine::start(App {
        vsync: true,
        enable_wireframe: false,
        pause_clipping: false,

        camera_autorotate: false,
        camera_position: Vec3::new(80., 25., 80.),

        previous_update: None,
        to_chunk_thread: None,

        world: Arc::new(RwLock::new(World::new(Arc::clone(&mc_data)))),
        mc_data,

        block_selection_material: None,
    })?;

    Ok(())
}
