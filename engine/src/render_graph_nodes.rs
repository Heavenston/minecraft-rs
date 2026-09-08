use std::num::NonZero;

use crevice::std140::AsStd140 as _;
use harness::renderer::resources as render_res;
use parking_lot::{ ArcRwLockReadGuard, RawRwLock, RwLock };
use render_graph::{ RenderGraph, graph_resource };

use crate::world::{World, WorldUniform};

pub struct RenderPassConfig {
    pub clear_color: wgpu::Color,
}

graph_resource!(pub struct WorldUniformBuffer(pub wgpu::Buffer); permanent);
graph_resource!(pub struct WorldBindGroupLayout(pub wgpu::BindGroupLayout); permanent);
graph_resource!(pub struct WorldBindGroup(pub wgpu::BindGroup); permanent);

graph_resource!(pub struct RenderPassConfigResource(pub RenderPassConfig));
graph_resource!(pub struct WorldResource(ArcRwLockReadGuard<RawRwLock, World>));

graph_resource!(pub struct BeforeRenderPass(pub ()));
graph_resource!(pub struct RenderPass(pub wgpu::RenderPass<'static>));
graph_resource!(pub struct RenderPassCommandEncoder(pub wgpu::CommandEncoder));

graph_resource!(pub struct StagingBelt(pub RwLock<wgpu::util::StagingBelt>); permanent);
graph_resource!(pub struct UsingStagingBelt(pub ()));

#[expect(clippy::single_call_fn, reason = "registered only in engine")]
pub(crate) fn register(graph: &mut RenderGraph) {
    graph.define_input::<RenderPassConfigResource>();
    graph.define_input::<WorldResource>();

    render_graph::node_helper!(into graph;
        CreateStagingBelt(device: ref render_res::Device) -> (StagingBelt) {
            tracing::debug!("Created a staging belt");
            OutputValue(RwLock::new(wgpu::util::StagingBelt::new(device.clone(), 128)))
        };
        CreateUsingStagingBelt(_: ref StagingBelt, _: ref render_res::ComputingFrame) -> (default UsingStagingBelt);
        FinishStagingBelt(_: UsingStagingBelt, staging_belt: ref StagingBelt, command_encoder: ref render_res::FrameCommandEncoder,) -> (default BeforeRenderPass) {
            staging_belt.write().finish_and_recall_on_submit(command_encoder);
        };

        CreateWorldUniformBuffer(device: ref render_res::Device) -> (WorldUniformBuffer) {
            OutputValue(device.create_buffer(&wgpu::wgt::BufferDescriptor {
                label: Some("World uniform buffer"),
                size: WorldUniform::std140_size_static() as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            }))
        };

        CreateWorldBindGroupLayout(device: ref render_res::Device) -> (WorldBindGroupLayout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("world bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZero::new(WorldUniform::std140_size_static() as u64).unwrap()),
                    },
                    count: None,
                }],
            }))
        };

        CreateWorldBindGroup(world_uniform_buffer: ref WorldUniformBuffer, world_bind_group_layout: ref WorldBindGroupLayout, device: ref render_res::Device,) -> (WorldBindGroup,) {
            OutputValue(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("world bind group"),
                layout: world_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: world_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                }],
            }))
        };

        WriteUniformBuffer(
            mut command_encoder: render_res::FrameCommandEncoder,
            world: ref WorldResource,
            world_uniform_buffer: ref WorldUniformBuffer,
            staging_belt: ref StagingBelt,
            _: ref UsingStagingBelt,
        ) -> (render_res::FrameCommandEncoder) {
            let data = WorldUniform {
                view_projection_matrix: world.camera_projection * world.camera_transform.inverse_or_zero(),
                time: world.created_at.elapsed().as_secs_f32(),
            }.as_std140();
            let bytes = data.as_bytes();

            staging_belt.write().write_buffer(&mut command_encoder, world_uniform_buffer, 0, NonZero::new(bytes.len() as u64).unwrap())
                .copy_from_slice(bytes);
    
            OutputValue(command_encoder)
        };

        StartRenderPass(
            config: RenderPassConfigResource,

            mut command_encoder: render_res::FrameCommandEncoder,
            world_bind_group: ref WorldBindGroup,
            texture_view: ref render_res::SurfaceTextureView,

            _: BeforeRenderPass,
        ) -> (RenderPass,RenderPassCommandEncoder) {
            let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(config.clear_color), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }).forget_lifetime();

            render_pass.set_bind_group(0, world_bind_group, &[]);

            OutputValue(render_pass,command_encoder)
        };

        EndRenderPass(render_pass: RenderPass, command_encoder: RenderPassCommandEncoder) -> (render_res::FrameCommandEncoder) {
            drop(render_pass);
            OutputValue(command_encoder)
        };
    );
}

