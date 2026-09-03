use super::resources as res;

pub(super) fn register(graph: &mut render_graph::RenderGraph) {
    graph.push_node(BeginFrame);
    graph.push_node(EndFrame);
    let a = graph.push_node(EndFrameMustHaveSubmitted);
    let b = graph.push_node(EndFrameMustHavePresented);
    graph.define_explicit_ordering(b, a);

    graph.push_node(CreateSurfaceTextureView);
    graph.push_node(DestroySurfaceTextureView);
    graph.push_node(CreateFrameCommandEncoder);
    graph.push_node(SubmitFrameCommandEncoder);
    graph.push_node(PresentSurface);
}

struct BeginFrame;
impl render_graph::GraphNode for BeginFrame {
    render_graph::declare_graph_deps!(() -> (res::ComputingFrame,));
    fn run(&mut self, _: Self::Inputs<'_>) -> Self::Outputs { Default::default() }
}

struct EndFrame;
impl render_graph::GraphNode for EndFrame {
    render_graph::declare_graph_deps!((res::ComputingFrame,) -> (res::FrameFinished,));
    fn run(&mut self, _: Self::Inputs<'_>) -> Self::Outputs { Default::default() }
}

struct EndFrameMustHavePresented;
impl render_graph::GraphNode for EndFrameMustHavePresented {
    render_graph::declare_graph_deps!((res::ComputingFrame, res::SurfacePresented,) -> (res::ComputingFrame,));
    fn run(&mut self, _: Self::Inputs<'_>) -> Self::Outputs { Default::default() }
}

struct EndFrameMustHaveSubmitted;
impl render_graph::GraphNode for EndFrameMustHaveSubmitted {
    render_graph::declare_graph_deps!((res::ComputingFrame, res::FrameCommandEncoderSubmitted,) -> (res::ComputingFrame,));
    fn run(&mut self, _: Self::Inputs<'_>) -> Self::Outputs { Default::default() }
}

struct CreateSurfaceTextureView;
impl render_graph::GraphNode for CreateSurfaceTextureView {
    render_graph::declare_graph_deps!((res::SurfaceTexture,) -> (res::BorrowedSurfaceTexture,res::SurfaceTextureView,));
    fn run(&mut self, (surface_texture,): Self::Inputs<'_>) -> Self::Outputs {
        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        (surface_texture,view,)
    }
}

struct DestroySurfaceTextureView;
impl render_graph::GraphNode for DestroySurfaceTextureView {
    render_graph::declare_graph_deps!((res::SurfaceTextureView,res::BorrowedSurfaceTexture,) -> (res::SurfaceTexture,));
    fn run(&mut self, (_,surface_texture,): Self::Inputs<'_>) -> Self::Outputs {
        (surface_texture,)
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
    render_graph::declare_graph_deps!((res::SurfaceTexture,ref res::Queue,) -> (res::SurfacePresented,));
    fn run(&mut self, (output,queue): Self::Inputs<'_>) -> Self::Outputs {
        queue.present(output);
        ((),)
    }
}
