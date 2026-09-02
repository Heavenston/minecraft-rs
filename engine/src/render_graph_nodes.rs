use std::num::NonZero;

use crevice::std140::AsStd140;
use harness::renderer::resources as render_res;
use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, RawRwLock};
use render_graph::{ GraphNode, RenderGraph, declare_graph_deps, graph_resource };

use crate::world::{GPUWorld, World, WorldUniformBuffer};

pub(super) fn register(graph: &mut RenderGraph) {
    graph.define_input::<RenderPassConfigResource>();
    graph.define_input::<WorldResource>();

    graph.push_node(CreateUsingStagingBelt);
    graph.push_node(WriteUniformBuffer);
    graph.push_node(FinishStagingBelt);
    graph.push_node(StartRenderPass);
    graph.push_node(EndRenderPass);
}

pub(super) struct RenderPassConfig {
    pub clear_color: wgpu::Color,
}

graph_resource!(pub(super) struct RenderPassConfigResource(pub(super) RenderPassConfig));
graph_resource!(pub(super) struct WorldResource(ArcRwLockReadGuard<RawRwLock, World>));
graph_resource!(pub(super) struct GPUWorldResource(ArcRwLockWriteGuard<RawRwLock, GPUWorld>));

graph_resource!(struct BeforeRenderPass(()));
graph_resource!(struct UsingStagingBelt(()));
graph_resource!(struct RenderPass(wgpu::RenderPass<'static>));
graph_resource!(struct RenderPassCommandEncoder(wgpu::CommandEncoder));

struct CreateUsingStagingBelt;
impl GraphNode for CreateUsingStagingBelt {
    declare_graph_deps!(() -> (UsingStagingBelt,));
    fn run(&mut self, _: Self::Inputs<'_>) -> Self::Outputs { ((),) }
}

struct WriteUniformBuffer;
impl GraphNode for WriteUniformBuffer {
    declare_graph_deps!((GPUWorldResource,render_res::FrameCommandEncoder,ref WorldResource,ref UsingStagingBelt,) -> (GPUWorldResource,render_res::FrameCommandEncoder,));
    fn run(&mut self, (mut gpu_world,mut command_encoder,world,_): Self::Inputs<'_>) -> Self::Outputs {
        let data = WorldUniformBuffer {
            view_projection_matrix: world.camera_transform.inverse_or_zero() * world.camera_projection,
        }.as_std140();
        let bytes = data.as_bytes();

        let gw = &mut *gpu_world;
        gw.staging_belt.write_buffer(&mut command_encoder, &gw.world_uniform, 0, NonZero::new(bytes.len() as u64).unwrap());
        
        (gpu_world,command_encoder,)
    }
}

struct FinishStagingBelt;
impl GraphNode for FinishStagingBelt {
    declare_graph_deps!((UsingStagingBelt,GPUWorldResource,ref render_res::FrameCommandEncoder,) -> (BeforeRenderPass,GPUWorldResource,));
    fn run(&mut self, ((), mut gpu_world, command_encoder): Self::Inputs<'_>) -> Self::Outputs {
        gpu_world.staging_belt.finish_and_recall_on_submit(command_encoder);
        ((), gpu_world)
    }
}

struct StartRenderPass;
impl GraphNode for StartRenderPass {
    declare_graph_deps!((render_res::FrameCommandEncoder,RenderPassConfigResource,BeforeRenderPass,ref render_res::SurfaceTextureView,) -> (RenderPass,RenderPassCommandEncoder,));
    fn run(&mut self, (mut command_encoder,config,_,texture_view): Self::Inputs<'_>) -> Self::Outputs {
        let render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

        (render_pass,command_encoder)
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
