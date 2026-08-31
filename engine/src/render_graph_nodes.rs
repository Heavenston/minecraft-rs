use harness::renderer::resources as render_res;
use render_graph::{ GraphNode, RenderGraph, declare_graph_deps, graph_resource };

pub(super) fn register(graph: &mut RenderGraph) {
    graph.define_input::<RenderPassConfigResource>();

    graph.push_node::<StartRenderPass>();
    graph.push_node::<EndRenderPass>();
}

pub(super) struct RenderPassConfig {
    pub clear_color: wgpu::Color,
}

graph_resource!(pub(super) struct RenderPassConfigResource(pub(super) RenderPassConfig));
graph_resource!(struct RenderPass(wgpu::RenderPass<'static>));
graph_resource!(struct RenderPassCommandEncoder(wgpu::CommandEncoder));

struct StartRenderPass;
impl GraphNode for StartRenderPass {
    declare_graph_deps!((render_res::FrameCommandEncoder,RenderPassConfigResource,ref render_res::SurfaceTextureView,) -> (RenderPass,RenderPassCommandEncoder,));
    fn run((mut command_encoder, config, texture_view): Self::Inputs<'_>) -> Self::Outputs {
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
    fn run((render_pass, command_encoder): Self::Inputs<'_>) -> Self::Outputs {
        drop(render_pass);
        (command_encoder,)
    }
}
