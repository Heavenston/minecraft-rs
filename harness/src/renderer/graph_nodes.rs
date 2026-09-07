use super::resources as res;

#[expect(clippy::single_call_fn, reason = "only called when creating graph")]
pub(super) fn register(graph: &mut render_graph::RenderGraph) {
    render_graph::node_helper!(into graph;
        CreateSurfaceConfiguration
        (window_size: ref res::WindowSize, &present_mode: ref res::PresentMode) -> (res::SurfaceConfiguration) {
            assert_ne!(window_size.0, 0, "Window width cannot be 0");
            assert_ne!(window_size.1, 0, "Window height cannot be 0");
            OutputValue(wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                width: window_size.0,
                height: window_size.1,
                present_mode,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
                color_space: wgpu::SurfaceColorSpace::Auto,
            })  
        };

        ConfigureSurface
        (device: ref res::Device, surface: ref res::Surface, surface_config: ref res::SurfaceConfiguration) -> (default res::ConfiguredSurface) {
            surface.configure(device, surface_config);
        };

        AcquireSurfaceTexture
        (surface: ref res::Surface, _: ref res::ConfiguredSurface) -> (default res::ComputingFrame, res::SurfaceTexture) {
            let output = match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
                wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                    surface_texture
                }
                wgpu::CurrentSurfaceTexture::Timeout
                | wgpu::CurrentSurfaceTexture::Occluded
                | wgpu::CurrentSurfaceTexture::Validation => {
                    // Skip this frame
                    todo!()
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    todo!()
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    // You could recreate the devices and all resources
                    // created with it here, but we'll just bail
                    panic!("Lost device");
                }
            };
            OutputValue(output)
        };

        EndFrame
        (_: res::ComputingFrame) -> (default res::FrameFinished);

        EndFrameMustHavePresented
        (_: res::ComputingFrame, _: res::SurfacePresented) -> (default res::ComputingFrame);

        EndFrameMustHaveSubmitted
        (_: res::ComputingFrame, _: res::FrameSubmitListSubmitted) -> (default res::ComputingFrame);

        CreateSurfaceTextureView
        (surface_texture: res::SurfaceTexture) -> (res::BorrowedSurfaceTexture, res::SurfaceTextureView)
        {
            let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("surface texture view"),
                ..wgpu::TextureViewDescriptor::default()
            });
            OutputValue(surface_texture,view)
        };

        DestroySurfaceTextureView
        (_: res::SurfaceTextureView, surface_texture: res::BorrowedSurfaceTexture) -> (res::SurfaceTexture)
        { OutputValue(surface_texture) };

        CreateFrameCommandEncoder
        (device: ref res::Device, _: ref res::ComputingFrame) -> (res::FrameCommandEncoder) {
            OutputValue(
                device.create_command_encoder(&wgpu::wgt::CommandEncoderDescriptor { label: Some("frame command encoder") })
            )
        };

        CreateFrameSubmitList
        () -> (default res::FrameSubmitList);

        SubmitFrameSubmitList (
            queue: ref res::Queue, list: res::FrameSubmitList,
        ) -> (default res::FrameSubmitListSubmitted) {
            queue.submit(list);
        };
        
        FinishFrameCommandEncoder (mut list: res::FrameSubmitList, command_encoder: res::FrameCommandEncoder) -> (res::FrameSubmitList) {
            list.push(command_encoder.finish());
            OutputValue(list)
        };

        // Define that FrameSubmitListSubmitted must have happen before SurfaceTexture is destroyed
        SubmitListBorrowsSurfaceTexture(_: ref res::FrameSubmitListSubmitted, _: ref res::SurfaceTexture) -> ();

        PresentSurface
        (queue: ref res::Queue, output: res::SurfaceTexture) -> (default res::SurfacePresented)
        { queue.present(output); };
    );
}

