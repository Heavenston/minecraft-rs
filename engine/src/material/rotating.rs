use glam::Vec4;

use crate::{ResumeCtx, render_graph_nodes as engine_graph};
use crate::material::Material;

const SHADER_CODE: &'static str = include_str!("rotating.wgsl");

pub struct RotatingConfig {
    pub color: Vec4,
    pub speed: f64,
}

pub struct Rotating {
    render_pipeline: wgpu::RenderPipeline,
}

impl Rotating {
    pub fn new(ctx: &mut ResumeCtx<'_>, _data: RotatingConfig) -> Self {
        let shader_module = ctx.renderer.device().create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rotating shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_CODE)),
        });
        let render_pipeline_layout = ctx.renderer.device().create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rotating pipeline layout"),
            bind_group_layouts: &[Some(&ctx.gpu_world.world_bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = ctx.renderer.device().create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rotating pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: None,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: None,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: ctx.renderer.surface_format(),
                        blend: None,
                        write_mask: wgpu::ColorWrites::all(),
                    })
                ],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            render_pipeline,
        }
    }
}

impl Material for Rotating {
    fn register(&mut self, render_graph: &mut super::RenderGraphWrapper<'_>) {
        render_graph.push_node(Draw {
            render_pipeline: self.render_pipeline.clone(),
        });
    }
}

struct Draw {
    render_pipeline: wgpu::RenderPipeline,
}
impl render_graph::GraphNode for Draw {
    render_graph::declare_graph_deps!((engine_graph::RenderPass,) -> (engine_graph::RenderPass,));
    fn run(&mut self, (mut render_pass,): Self::Inputs<'_>) -> Self::Outputs {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.draw(0..3, 0..1);
        (render_pass,)
    }
}
