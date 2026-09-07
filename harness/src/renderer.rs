#![allow(clippy::all, reason = "fixing errors first")]

mod graph_nodes;

use std::sync::Arc;
use anyhow::Result;

use render_graph::RenderGraph;

pub mod resources {
    use render_graph::graph_resource as res;
    res!(pub struct WindowSize((u32, u32)); permanent);
    res!(pub struct Device(pub wgpu::Device); permanent);
    res!(pub struct Queue(pub wgpu::Queue); permanent);
    res!(pub struct SurfaceTexture(pub wgpu::SurfaceTexture));

    res!(pub(crate) struct BorrowedSurfaceTexture(pub wgpu::SurfaceTexture));
    res!(pub struct SurfaceTextureView(pub wgpu::TextureView));
    res!(pub struct FrameCommandEncoder(pub wgpu::CommandEncoder));
    res!(pub struct FrameCommandEncoderSubmitted(pub ()));
    res!(pub struct SurfacePresented(pub ()));
    res!(#[derive(Default)] pub struct ComputingFrame(pub ()));
    res!(pub struct FrameFinished(pub ()); permanent);
}
use resources as res;

#[expect(clippy::single_call_fn, reason = "only used when creating graph")]
fn define_inputs(graph: &mut RenderGraph) {
    graph.define_input::<res::WindowSize>();
    graph.define_input::<res::Device>();
    graph.define_input::<res::Queue>();

    graph.define_input::<res::SurfaceTexture>();
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,

    render_graph: RenderGraph,
    window: Arc<winit::window::Window>,
}

impl Renderer {
    pub async fn new(display_handle: winit::event_loop::OwnedDisplayHandle, window: Arc<winit::window::Window>) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: Some(Box::new(display_handle)),
        });
        
        let surface = instance.create_surface(Arc::clone(&window)).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: true,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::IMMEDIATES,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                required_limits: wgpu::Limits {
                    max_immediate_size: 128,
                    ..wgpu::Limits::default()
                },
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result in all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let size = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };

        let mut render_graph = RenderGraph::new();
        define_inputs(&mut render_graph);
        graph_nodes::register(&mut render_graph);

        render_graph.set_input::<res::Device>(device.clone());
        render_graph.set_input::<res::Queue>(queue.clone());

        Ok(Self {
            surface,
            device,
            queue,
            surface_config,
            is_surface_configured: false,

            render_graph,
            window,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn render_graph(&mut self) -> &mut RenderGraph {
        &mut self.render_graph
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
            self.is_surface_configured = true;
        }
    }

    pub fn render(&mut self) -> anyhow::Result<()> {
        self.render_graph.prepare_run();

        self.window.request_redraw();

        // We can't render unless the surface is configured
        if !self.is_surface_configured {
            return Ok(());
        }
        
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                surface_texture
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => {
                // Skip this frame
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                // You could recreate the devices and all resources
                // created with it here, but we'll just bail
                anyhow::bail!("Lost device");
            }
        };

        self.render_graph.set_input::<res::SurfaceTexture>(output);
        let () = self.render_graph.compute::<res::FrameFinished>();

        Ok(())
    }
}
