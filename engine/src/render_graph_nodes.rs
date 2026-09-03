use std::num::NonZero;

use crevice::std140::AsStd140;
use harness::renderer::resources as render_res;
use parking_lot::{ ArcRwLockReadGuard, RawRwLock, RwLock };
use render_graph::{ GraphNode, RenderGraph, declare_graph_deps, graph_resource };

use crate::world::{World, WorldUniform};

pub(crate) fn register(graph: &mut RenderGraph) {
    graph.define_input::<RenderPassConfigResource>();
    graph.define_input::<WorldResource>();

    graph.push_node(CreateUsingStagingBelt);
    graph.push_node(CreateStagingBelt);
    graph.push_node(FinishStagingBelt);

    graph.push_node(CreateWorldUniformBuffer);
    graph.push_node(CreateWorldBindGroup);
    graph.push_node(CreateWorldBindGroupLayout);

    graph.push_node(WriteUniformBuffer);
    graph.push_node(StartRenderPass);
    graph.push_node(EndRenderPass);
}

pub(crate) struct RenderPassConfig {
    pub clear_color: wgpu::Color,
}

graph_resource!(pub(crate) struct WorldUniformBuffer(pub wgpu::Buffer); permanent);
graph_resource!(pub(crate) struct WorldBindGroupLayout(pub wgpu::BindGroupLayout); permanent);
graph_resource!(pub(crate) struct WorldBindGroup(pub wgpu::BindGroup); permanent);

graph_resource!(pub(crate) struct RenderPassConfigResource(pub RenderPassConfig));
graph_resource!(pub(crate) struct WorldResource(ArcRwLockReadGuard<RawRwLock, World>));

graph_resource!(pub(crate) struct BeforeRenderPass(pub ()));
graph_resource!(pub(crate) struct RenderPass(pub wgpu::RenderPass<'static>));
graph_resource!(pub(crate) struct RenderPassCommandEncoder(pub wgpu::CommandEncoder));

graph_resource!(pub(crate) struct StagingBelt(pub RwLock<wgpu::util::StagingBelt>); permanent);
graph_resource!(pub(crate) struct UsingStagingBelt(pub ()));

struct CreateStagingBelt;
impl GraphNode for CreateStagingBelt {
    declare_graph_deps!((ref render_res::Device,) -> (StagingBelt,));
    fn run(&mut self, (device,): Self::Inputs<'_>) -> Self::Outputs {
        tracing::debug!("Created a staging belt");
        (RwLock::new(wgpu::util::StagingBelt::new(device.clone(), 128)),)
    }
}

struct CreateUsingStagingBelt;
impl GraphNode for CreateUsingStagingBelt {
    declare_graph_deps!((ref StagingBelt,) -> (UsingStagingBelt,));
    fn run(&mut self, _: Self::Inputs<'_>) -> Self::Outputs {
        ((),)
    }
}

struct CreateWorldUniformBuffer;
impl GraphNode for CreateWorldUniformBuffer {
    declare_graph_deps!((ref render_res::Device,) -> (WorldUniformBuffer,));
    fn run(&mut self, (device,): Self::Inputs<'_>) -> Self::Outputs {
        let world_uniform = device.create_buffer(&wgpu::wgt::BufferDescriptor {
            label: Some("World uniform buffer"),
            size: WorldUniform::std140_size_static() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        (world_uniform,)
    }
}

struct CreateWorldBindGroupLayout;
impl GraphNode for CreateWorldBindGroupLayout {
    declare_graph_deps!((ref render_res::Device,) -> (WorldBindGroupLayout,));
    fn run(&mut self, (device,): Self::Inputs<'_>) -> Self::Outputs {
        let world_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });
        (world_bind_group_layout,)
    }
}

struct CreateWorldBindGroup;
impl GraphNode for CreateWorldBindGroup {
    declare_graph_deps!((ref WorldUniformBuffer, ref WorldBindGroupLayout, ref render_res::Device,) -> (WorldBindGroup,));
    fn run(&mut self, (world_uniform_buffer, world_bind_group_layout, device,): Self::Inputs<'_>) -> Self::Outputs {
        let world_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world bind group"),
            layout: &world_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &world_uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });
        (world_bind_group,)
    }
}

struct FinishStagingBelt;
impl GraphNode for FinishStagingBelt {
    declare_graph_deps!((UsingStagingBelt,ref StagingBelt,ref render_res::FrameCommandEncoder,) -> (BeforeRenderPass,));
    fn run(&mut self, (_,staging_belt,command_encoder): Self::Inputs<'_>) -> Self::Outputs {
        staging_belt.write().finish_and_recall_on_submit(command_encoder);
        ((),)
    }
}

struct WriteUniformBuffer;
impl GraphNode for WriteUniformBuffer {
    declare_graph_deps!((render_res::FrameCommandEncoder,ref WorldResource,ref WorldUniformBuffer,ref StagingBelt,ref UsingStagingBelt,) -> (render_res::FrameCommandEncoder,));
    fn run(&mut self, (mut command_encoder,world,world_uniform_buffer,staging_belt,_): Self::Inputs<'_>) -> Self::Outputs {
        let data = WorldUniform {
            view_projection_matrix: world.camera_transform.inverse_or_zero() * world.camera_projection,
        }.as_std140();
        let bytes = data.as_bytes();

        staging_belt.write().write_buffer(&mut command_encoder, world_uniform_buffer, 0, NonZero::new(bytes.len() as u64).unwrap());
        
        (command_encoder,)
    }
}

struct StartRenderPass;
impl GraphNode for StartRenderPass {
    declare_graph_deps!((render_res::FrameCommandEncoder,RenderPassConfigResource,BeforeRenderPass,ref WorldBindGroup,ref render_res::SurfaceTextureView,) -> (RenderPass,RenderPassCommandEncoder,));
    fn run(&mut self, (mut command_encoder,config,_,world_bind_group,texture_view): Self::Inputs<'_>) -> Self::Outputs {
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

        (render_pass,command_encoder,)
    }
}

struct EndRenderPass;
impl GraphNode for EndRenderPass {
    declare_graph_deps!((RenderPass,RenderPassCommandEncoder,) -> (render_res::FrameCommandEncoder,));
    fn run(&mut self, (render_pass, command_encoder): Self::Inputs<'_>) -> Self::Outputs {
        drop(render_pass);
        (command_encoder,)
    }
}
