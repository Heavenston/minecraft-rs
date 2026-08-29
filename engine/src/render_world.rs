
pub struct RenderWorld {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl RenderWorld {
    pub(crate) fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            device,
            queue,
        }
    }
}
