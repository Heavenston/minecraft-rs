use std::time::Instant;

use crevice::std140::AsStd140;
use glam::{ Mat4, Vec4 };
use render_graph::RenderGraph;
use static_assertions as sa;

pub struct World {
    pub(crate) created_at: Instant,

    pub clear_color: Vec4,
    pub camera_transform: Mat4,
    pub camera_projection: Mat4,
}
sa::assert_impl_all!(World: Send, Sync);

impl World {
    pub(crate) fn update_render_graph(&self, render_graph: &mut RenderGraph) {
        render_graph.set_input::<crate::render_graph_nodes::RenderPassConfigResource>(crate::RenderPassConfig {
            clear_color: wgpu::Color {
                r: self.clear_color.x.into(),
                g: self.clear_color.y.into(),
                b: self.clear_color.z.into(),
                a: self.clear_color.w.into(),
            },
        });
    }
}

impl Default for World {
    fn default() -> Self {
        Self {
            created_at: Instant::now(),

            clear_color: Vec4::new(0., 0., 0., 1.),
            camera_transform: Mat4::IDENTITY,
            camera_projection: Mat4::IDENTITY,
        }
    }
}

#[derive(Debug, Copy, Clone, AsStd140)]
pub struct WorldUniform {
    pub view_projection_matrix: Mat4,
    pub time: f32,
}
