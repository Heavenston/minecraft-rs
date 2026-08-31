use crate::render_graph::RenderGraph;

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderStage {
    None,
    Background,
    Content3D,
    Content2D,
    UI,
}

pub struct RenderWorld {
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_graph: RenderGraph,

    surface_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
}

impl RenderWorld {
    pub(crate) fn new(device: wgpu::Device, queue: wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        Self {
            device,
            queue,
            render_graph: RenderGraph::new(),

            surface_format,
            width: 0,
            height: 0,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn render_target_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    pub(crate) fn set_render_target_size(&mut self, (width, height): (u32, u32)) {
        self.width = width;
        self.height = height;
    }

    pub fn render_target_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn render_graph(&self) -> &RenderGraph {
        &self.render_graph
    }

    pub fn render_graph_mut(&mut self) -> &mut RenderGraph {
        &mut self.render_graph
    }
}
