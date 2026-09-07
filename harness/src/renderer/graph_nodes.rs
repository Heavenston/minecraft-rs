use super::resources as res;

#[expect(clippy::single_call_fn, reason = "only called when creating graph")]
pub(super) fn register(graph: &mut render_graph::RenderGraph) {
    node!(in graph;
        BeginFrame
        () -> (res::ComputingFrame)
        { OutputValue(()) };

        EndFrame
        ((): res::ComputingFrame) -> ()
        { OutputValue() };

        EndFrameMustHavePresented
        ((): res::ComputingFrame, (): res::SurfacePresented) -> (res::ComputingFrame)
        { OutputValue(()) };

        EndFrameMustHaveSubmitted
        ((): res::ComputingFrame, (): res::FrameCommandEncoderSubmitted) -> (res::ComputingFrame)
        { OutputValue(()) };

        CreateSurfaceTextureView
        (surface_texture: res::SurfaceTexture) -> (res::BorrowedSurfaceTexture, res::SurfaceTextureView)
        {
            let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
            OutputValue(surface_texture,view)
        };

        DestroySurfaceTextureView
        (_: res::SurfaceTextureView, surface_texture: res::BorrowedSurfaceTexture) -> (res::BorrowedSurfaceTexture)
        { OutputValue(surface_texture) };

        CreateFrameCommandEncoder
        (device: ref res::Device) -> (res::FrameCommandEncoder) {
            OutputValue(
                device.create_command_encoder(&wgpu::wgt::CommandEncoderDescriptor { label: Some("frame command encoder") })
            )
        };

        SubmitFrameCommandEncoder
        (queue: ref res::Queue, command_encoder: res::FrameCommandEncoder, _: ref res::SurfaceTextureView) -> (res::FrameCommandEncoderSubmitted)
        {
            queue.submit(std::iter::once(command_encoder.finish()));
            OutputValue(())
        };

        PresentSurface
        (queue: ref res::Queue, output: res::SurfaceTexture) -> (res::SurfacePresented)
        {
            queue.present(output);
            OutputValue(())
        };
    );
}

