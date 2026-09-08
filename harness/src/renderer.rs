#![allow(clippy::all, reason = "fixing errors first")]

mod graph_nodes;

use std::sync::Arc;
use anyhow::Result;

use render_graph::RenderGraph;

pub mod resources {
    use render_graph::graph_resource as res;
    res!(pub struct Instance(pub wgpu::Instance); permanent);
    res!(pub struct Surface(pub wgpu::Surface<'static>); permanent);
    res!(pub struct Device(pub wgpu::Device); permanent);
    res!(pub struct Queue(pub wgpu::Queue); permanent);
    res!(pub struct SurfaceTexture(pub wgpu::SurfaceTexture));

    res!(pub struct WindowSize(pub (u32, u32)); permanent);
    res!(pub struct PresentMode(pub wgpu::PresentMode); permanent);
    res!(pub struct SurfaceConfiguration(pub wgpu::SurfaceConfiguration); permanent);

    res!(pub(crate) struct ConfiguredSurface(pub ()); permanent);

    res!(pub struct FrameSubmitList(pub Vec<wgpu::CommandBuffer>));
    res!(pub struct FrameSubmitListSubmitted(pub ()); unordered);

    res!(pub(crate) struct BorrowedSurfaceTexture(pub wgpu::SurfaceTexture));
    res!(pub struct SurfaceTextureView(pub wgpu::TextureView));
    res!(pub struct FrameCommandEncoder(pub wgpu::CommandEncoder); unordered);
    res!(pub struct FrameCommandEncoderSubmitted(pub ()));
    res!(pub struct SurfacePresented(pub ()));
    res!(#[derive(Default)] pub struct ComputingFrame(pub ()); unordered);
    res!(pub struct FrameFinished(pub ()); permanent);
}
use resources as res;

#[expect(clippy::single_call_fn, reason = "only used when creating graph")]
fn define_inputs(graph: &mut RenderGraph) {
    graph.define_input::<res::Instance>();
    graph.define_input::<res::Device>();
    graph.define_input::<res::Queue>();

    graph.define_input::<res::WindowSize>();
    graph.define_input::<res::PresentMode>();

    graph.define_input::<res::Surface>();
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,

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

        let mut render_graph = RenderGraph::new();
        define_inputs(&mut render_graph);
        graph_nodes::register(&mut render_graph);

        render_graph.set_input::<res::Instance>(instance.clone());
        render_graph.set_input::<res::Surface>(surface);
        render_graph.set_input::<res::Device>(device.clone());
        render_graph.set_input::<res::Queue>(queue.clone());

        let size = window.outer_size();
        render_graph.set_input::<res::WindowSize>((size.width, size.height));
        render_graph.set_input::<res::PresentMode>(wgpu::PresentMode::AutoVsync);

        Ok(Self {
            device,
            queue,

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
        wgpu::TextureFormat::Bgra8UnormSrgb
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.render_graph.set_input::<res::WindowSize>((width, height));
    }

    pub fn render(&mut self) -> anyhow::Result<()> {
        self.render_graph.prepare_run();
        self.window.request_redraw();
        let () = self.render_graph.compute::<res::FrameFinished>();
        Ok(())
    }
}
