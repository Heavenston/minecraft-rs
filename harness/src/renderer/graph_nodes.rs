use super::resources as res;

pub(super) fn register(graph: &mut render_graph::RenderGraph) {
    macro_rules! node {
        () => {};
        ($name: ident($($arg:tt: $(ref $marker:vis)?$input:ty),*$(,)?) -> ($($output:tt)*) $body: block;$($rest:tt)*) => {
            mod ${ concat($name, _module) } {
                #[allow(unused, reason = "macro-generated")]
                #[expect(clippy::allow_attributes, reason = "")]
                use super::*;

                render_graph::input_bundle!(struct Input($($(ref${ ignore($marker) })?$input),*));
                render_graph::output_bundle!(struct Output($($output)*));
                pub struct $name;
                impl render_graph::GraphNode for $name {
                    type InputBundle = Input;
                    type OutputBundle = Output;
                    fn run(&mut self, InputValue($($arg),*): InputValue) -> OutputValue {
                        $body
                    }
                }
            }
            use ${ concat($name, _module) }::$name;
            graph.push_node($name);
            node!($($rest)*)
        };
    }

    node!(
        BeginFrame
        () -> (res::ComputingFrame)
        { OutputValue(()) };

        EndFrame
        (_: res::ComputingFrame) -> ()
        { OutputValue() };

        EndFrameMustHavePresented
        (_: res::ComputingFrame, _: res::SurfacePresented) -> (res::ComputingFrame)
        { OutputValue(()) };

        EndFrameMustHaveSubmitted
        (_: res::ComputingFrame, _: res::FrameCommandEncoderSubmitted) -> (res::ComputingFrame)
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

