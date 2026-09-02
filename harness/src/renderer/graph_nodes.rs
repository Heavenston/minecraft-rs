use super::resources as res;

pub(super) fn register(graph: &mut render_graph::RenderGraph) {
    graph.define_input::<res::Device>();
    graph.define_input::<res::Queue>();
    graph.define_input::<res::SurfaceTexture>();

    graph.push_node(CreateSurfaceTextureView);
    graph.push_node(CreateFrameCommandEncoder);
    graph.push_node(SubmitFrameCommandEncoder);
    graph.push_node(PresentSurface);
}

struct CreateSurfaceTextureView;
impl render_graph::GraphNode for CreateSurfaceTextureView {
    render_graph::declare_graph_deps!((ref res::SurfaceTexture,) -> (res::SurfaceTextureView,));
    fn run(&mut self, (output,): Self::Inputs<'_>) -> Self::Outputs {
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        (view,)
    }
}

struct CreateFrameCommandEncoder;
impl render_graph::GraphNode for CreateFrameCommandEncoder {
    render_graph::declare_graph_deps!((ref res::Device,) -> (res::FrameCommandEncoder,));
    fn run(&mut self, (device,): Self::Inputs<'_>) -> Self::Outputs {
        let encoder = device.create_command_encoder(&wgpu::wgt::CommandEncoderDescriptor { label: Some("frame command encoder") });
        (encoder,)
    }
}

struct SubmitFrameCommandEncoder;
impl render_graph::GraphNode for SubmitFrameCommandEncoder {
    // We borrow SurfaceTextureView to prevent the surface texture from being presented
    // until after this node
    render_graph::declare_graph_deps!((res::FrameCommandEncoder, ref res::Queue, ref res::SurfaceTextureView,) -> (res::FrameCommandEncoderSubmitted,));
    fn run(&mut self, (command_encoder, queue, _): Self::Inputs<'_>) -> Self::Outputs {
        queue.submit(std::iter::once(command_encoder.finish()));
        ((),)
    }
}

struct PresentSurface;
impl render_graph::GraphNode for PresentSurface {
    // We consume SurfaceTextureView to make sure it is not in use anymore
    render_graph::declare_graph_deps!((res::SurfaceTexture, res::SurfaceTextureView, ref res::Queue,) -> (res::SurfacePrensented,));
    fn run(&mut self, (output,_,queue): Self::Inputs<'_>) -> Self::Outputs {
        queue.present(output);
        ((),)
    }
}
