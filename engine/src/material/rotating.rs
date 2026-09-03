use glam::Vec4;

use crate::{ResumeCtx, render_graph_nodes as engine_graph};
use crate::material::Material;
use harness::renderer::resources as render_res;

const SHADER_CODE: &'static str = include_str!("rotating.wgsl");

pub struct RotatingConfig {
    pub color: Vec4,
    pub speed: f64,
}

pub struct Rotating {
}

impl Rotating {
    pub fn new(_ctx: &mut ResumeCtx<'_>, _data: RotatingConfig) -> Self {
        Self { }
    }
}

impl Material for Rotating {
    fn register(&mut self, render_graph: &mut super::RenderGraphWrapper<'_>) {
        render_graph.push_node(CreateShaderModule);
        render_graph.push_node(CreateRenderPipelineLayout);
        render_graph.push_node(CreateRenderPipeline);
        render_graph.push_node(Draw);
    }
}

render_graph::graph_resource!(struct ShaderModule(wgpu::ShaderModule); permanent);
render_graph::graph_resource!(struct RenderPipelineLayout(wgpu::PipelineLayout); permanent);
render_graph::graph_resource!(struct RenderPipeline(wgpu::RenderPipeline); permanent);

struct CreateShaderModule;
impl render_graph::GraphNode for CreateShaderModule {
    render_graph::declare_graph_deps!((ref render_res::Device,) -> (ShaderModule,));

    fn run(&mut self, (device,): Self::Inputs<'_>) -> Self::Outputs {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rotating shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_CODE)),
        });
        (shader_module,)
    }
}

struct CreateRenderPipelineLayout;
impl render_graph::GraphNode for CreateRenderPipelineLayout {
    render_graph::declare_graph_deps!((ref render_res::Device,ref engine_graph::WorldBindGroupLayout,) -> (RenderPipelineLayout,));

    fn run(&mut self, (device,world_bind_group_layout,): Self::Inputs<'_>) -> Self::Outputs {
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rotating pipeline layout"),
            bind_group_layouts: &[Some(world_bind_group_layout)],
            immediate_size: 0,
        });
        (render_pipeline_layout ,)
    }
}

struct CreateRenderPipeline;
impl render_graph::GraphNode for CreateRenderPipeline {
    render_graph::declare_graph_deps!((ref render_res::Device,ref ShaderModule,ref RenderPipelineLayout,) -> (RenderPipeline,));

    fn run(&mut self, (device,shader_module,render_pipeline_layout,): Self::Inputs<'_>) -> Self::Outputs {
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        format: wgpu::TextureFormat::Bgra8UnormSrgb,
                        blend: None,
                        write_mask: wgpu::ColorWrites::all(),
                    })
                ],
            }),
            multiview_mask: None,
            cache: None,
        });
        (render_pipeline ,)
    }
}

struct Draw;
impl render_graph::GraphNode for Draw {
    render_graph::declare_graph_deps!((engine_graph::RenderPass,ref RenderPipeline,) -> (engine_graph::RenderPass,));
    fn run(&mut self, (mut render_pass,render_pipeline,): Self::Inputs<'_>) -> Self::Outputs {
        render_pass.set_pipeline(&render_pipeline);
        render_pass.draw(0..3, 0..1);
        (render_pass,)
    }
}
