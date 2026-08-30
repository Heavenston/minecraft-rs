use glam::Vec4;

use crate::{ContructibleMiddleware, RenderMiddleware, RenderStage, RenderWorld};

const SHADER: &'static str = include_str!("./clear.wgsl");

pub struct Clear {
    color: Vec4,
    render_pipeline: wgpu::RenderPipeline,
}

impl Clear {
    pub fn set_color(&mut self, new_color: Vec4) {
        self.color = new_color;
    }
}

impl ContructibleMiddleware for Clear {
    fn new(world: &mut RenderWorld) -> (RenderStage, Self) {
        let shader_module = world.device().create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clear shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER)),
        });

        let render_pipeline = world.device().create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Clear Render Pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: world.render_target_format(),
                        blend: None,
                        write_mask: wgpu::ColorWrites::all(),
                    })
                ],
            }),
            multiview_mask: None,
            cache: None,
        });

        let this = Self {
            color: Vec4::new(0., 0.2, 0.2, 1.),
            render_pipeline,
        };
        (RenderStage::Background, this)
    }
}

impl RenderMiddleware for Clear {
    fn render(&mut self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_immediates(0, bytemuck::bytes_of(&self.color));
        render_pass.draw(0..4, 0..1);
    }
}
