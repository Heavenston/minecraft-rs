use crevice::std140::AsStd140;
use glam::Mat4;

#[derive(Default)]
pub struct World {
    pub camera_transform: Mat4,
    pub camera_projection: Mat4,
}

#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, AsStd140)]
#[repr(C)]
pub struct WorldUniformBuffer {
    pub view_projection_matrix: Mat4,
}

#[derive(Debug)]
pub(crate) struct GPUWorld {
    pub staging_belt: wgpu::util::StagingBelt,
    pub world_uniform: wgpu::Buffer,

    pub world_bind_group: wgpu::BindGroup,
}
